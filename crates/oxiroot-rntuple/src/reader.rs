//! Opening an RNTuple from a ROOT file: anchor → header/footer envelopes →
//! page-list envelopes → on-demand column decoding.

use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::RFile;

use crate::anchor::{RNTupleAnchor, ANCHOR_CLASS};
use crate::envelope::{read_envelope, ENVELOPE_FOOTER, ENVELOPE_HEADER, ENVELOPE_PAGELIST};
use crate::field::{self, FieldValues};
use crate::footer::Footer;
use crate::header::{Header, StructRole};
use crate::page::{read_column, ColumnValues};
use crate::pagelist::{ClusterPages, ClusterSummary, PageInfo, PageList};

/// An opened RNTuple: verified anchor, parsed schema, cluster summaries, and
/// per-cluster page locations. Column data is decoded on demand.
pub struct RNTuple {
    anchor: RNTupleAnchor,
    header: Header,
    footer: Footer,
    summaries: Vec<ClusterSummary>,
    page_clusters: Vec<ClusterPages>,
    header_bytes: Vec<u8>,
    footer_bytes: Vec<u8>,
}

impl RNTuple {
    /// Open the RNTuple named `name` from an open ROOT file.
    pub fn open(file: &RFile, name: &str) -> Result<RNTuple> {
        let key = file
            .key(name)
            .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
        if key.class_name != ANCHOR_CLASS {
            return Err(Error::Format(format!(
                "key {name:?} is a {}, not {ANCHOR_CLASS}",
                key.class_name
            )));
        }

        let anchor_payload = key.payload(file.data())?;
        let anchor_object = oxiroot_compress::decompress(anchor_payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing anchor: {e}")))?;
        let anchor = RNTupleAnchor::read(&anchor_object)?;

        let header_bytes = read_blob(
            file.data(),
            anchor.seek_header,
            anchor.nbytes_header,
            anchor.len_header,
            "header",
        )?;
        let footer_bytes = read_blob(
            file.data(),
            anchor.seek_footer,
            anchor.nbytes_footer,
            anchor.len_footer,
            "footer",
        )?;

        let h = read_envelope(&header_bytes)?;
        if h.type_id != ENVELOPE_HEADER {
            return Err(Error::Format(format!(
                "bad header envelope type {:#x}",
                h.type_id
            )));
        }
        let header = Header::parse(h.payload)?;

        let f = read_envelope(&footer_bytes)?;
        if f.type_id != ENVELOPE_FOOTER {
            return Err(Error::Format(format!(
                "bad footer envelope type {:#x}",
                f.type_id
            )));
        }
        let footer = Footer::parse(f.payload)?;

        // Read each cluster group's page-list envelope.
        let mut summaries = Vec::new();
        let mut page_clusters = Vec::new();
        for group in &footer.cluster_groups {
            let blob = read_blob(
                file.data(),
                group.page_list.offset,
                group.page_list.size as u64,
                group.page_list_len,
                "page list",
            )?;
            let env = read_envelope(&blob)?;
            if env.type_id != ENVELOPE_PAGELIST {
                return Err(Error::Format(format!(
                    "bad page-list envelope type {:#x}",
                    env.type_id
                )));
            }
            let page_list = PageList::parse(env.payload)?;
            summaries.extend(page_list.summaries);
            page_clusters.extend(page_list.clusters);
        }

        Ok(RNTuple {
            anchor,
            header,
            footer,
            summaries,
            page_clusters,
            header_bytes,
            footer_bytes,
        })
    }

    /// The verified anchor.
    pub fn anchor(&self) -> &RNTupleAnchor {
        &self.anchor
    }

    /// The parsed schema (fields and columns).
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The parsed footer (cluster groups).
    pub fn footer(&self) -> &Footer {
        &self.footer
    }

    /// The decompressed header envelope bytes.
    pub fn header_envelope(&self) -> &[u8] {
        &self.header_bytes
    }

    /// The decompressed footer envelope bytes.
    pub fn footer_envelope(&self) -> &[u8] {
        &self.footer_bytes
    }

    /// Total number of entries across all clusters.
    pub fn num_entries(&self) -> u64 {
        self.summaries.iter().map(|s| s.num_entries).sum()
    }

    /// Decode physical column `column_index` across all clusters.
    pub fn read_column(&self, file: &RFile, column_index: usize) -> Result<ColumnValues> {
        let descriptor = self
            .header
            .columns
            .get(column_index)
            .ok_or_else(|| Error::Format(format!("no column {column_index}")))?;

        let mut pages: Vec<PageInfo> = Vec::new();
        for cluster in &self.page_clusters {
            let column = cluster
                .columns
                .get(column_index)
                .ok_or_else(|| Error::Format(format!("cluster missing column {column_index}")))?;
            pages.extend_from_slice(&column.pages);
        }

        read_column(
            file.data(),
            descriptor.column_type,
            descriptor.bits_on_storage,
            &pages,
            descriptor.value_range,
        )
    }

    /// Names of the top-level fields, in schema order.
    pub fn field_names(&self) -> Vec<&str> {
        self.header
            .fields
            .iter()
            .enumerate()
            .filter(|(i, f)| f.parent_field_id as usize == *i)
            .map(|(_, f)| f.name.as_str())
            .collect()
    }

    /// Read a top-level field by name, reconstructing its per-entry values.
    /// Supports scalar leaves, `std::string`, `std::vector<T>`, and arbitrarily
    /// nested collections / records — `std::vector<std::string>`,
    /// `std::vector<std::vector<T>>`, and `std::vector<MyStruct>` — by walking
    /// the field tree (see [`FieldValues`]).
    pub fn read_field(&self, file: &RFile, name: &str) -> Result<FieldValues> {
        let idx = self
            .header
            .fields
            .iter()
            .enumerate()
            .position(|(i, f)| f.name == name && f.parent_field_id as usize == i)
            .ok_or_else(|| Error::Format(format!("no top-level field named {name:?}")))?;

        let values = self.read_field_tree(file, idx)?;
        // A top-level field carries exactly one element per entry; a mismatch
        // means a truncated or corrupt index/leaf column.
        if values.len() as u64 != self.num_entries() {
            return Err(Error::Format(format!(
                "field {name:?}: decoded {} entries, expected {}",
                values.len(),
                self.num_entries()
            )));
        }
        Ok(values)
    }

    /// Recursively reconstruct the values of field `field_idx` (a leaf, string,
    /// collection, or record) as flattened-at-this-level [`FieldValues`].
    fn read_field_tree(&self, file: &RFile, field_idx: usize) -> Result<FieldValues> {
        let fld = self
            .header
            .fields
            .get(field_idx)
            .ok_or_else(|| Error::Format(format!("no field {field_idx}")))?;
        let columns = self.field_columns(field_idx);

        match fld.struct_role {
            StructRole::Leaf if fld.type_name == "std::string" => {
                let index_ci = self.index_column(&columns).ok_or_else(|| {
                    Error::Format(format!("string field {:?} has no index column", fld.name))
                })?;
                let char_ci = *columns
                    .iter()
                    .find(|&&ci| !self.header.columns[ci].column_type.is_index())
                    .ok_or_else(|| {
                        Error::Format(format!("string field {:?} has no char column", fld.name))
                    })?;
                let offsets = self.read_offsets(file, index_ci)?;
                let bytes = match self.read_column(file, char_ci)? {
                    ColumnValues::Bytes(v) => v,
                    other => {
                        return Err(Error::Format(format!("string chars decoded as {other:?}")))
                    }
                };
                field::strings(&offsets, &bytes)
            }
            StructRole::Leaf => {
                let ci = *columns
                    .first()
                    .ok_or_else(|| Error::Format(format!("field {:?} has no column", fld.name)))?;
                field::scalar(self.read_column(file, ci)?)
            }
            StructRole::Collection => {
                let index_ci = self.index_column(&columns).ok_or_else(|| {
                    Error::Format(format!(
                        "collection field {:?} has no index column",
                        fld.name
                    ))
                })?;
                let offsets = self.read_offsets(file, index_ci)?;
                let child = *self.child_fields(field_idx).first().ok_or_else(|| {
                    Error::Format(format!(
                        "collection field {:?} has no element field",
                        fld.name
                    ))
                })?;
                let items = self.read_field_tree(file, child)?;
                field::collect(offsets, items)
            }
            StructRole::Record => {
                let children = self.child_fields(field_idx);
                if children.is_empty() {
                    return Err(Error::Format(format!(
                        "record field {:?} has no sub-fields",
                        fld.name
                    )));
                }
                let mut out = Vec::with_capacity(children.len());
                for c in children {
                    let sub_name = self.header.fields[c].name.clone();
                    out.push((sub_name, self.read_field_tree(file, c)?));
                }
                Ok(FieldValues::Record(out))
            }
            StructRole::Variant => {
                // The variant field carries one Switch column of (index, tag);
                // each sub-field is one densely-packed alternative.
                let switch_ci = *columns.first().ok_or_else(|| {
                    Error::Format(format!("variant field {:?} has no switch column", fld.name))
                })?;
                let (tags, indices) = match self.read_column(file, switch_ci)? {
                    ColumnValues::Switch(v) => (
                        v.iter().map(|&(_, t)| t).collect::<Vec<_>>(),
                        v.iter().map(|&(i, _)| i).collect::<Vec<_>>(),
                    ),
                    other => {
                        return Err(Error::Format(format!(
                            "variant switch column decoded as {other:?}"
                        )))
                    }
                };
                let children = self.child_fields(field_idx);
                if children.is_empty() {
                    return Err(Error::Format(format!(
                        "variant field {:?} has no alternatives",
                        fld.name
                    )));
                }
                let mut alternatives = Vec::with_capacity(children.len());
                for c in children {
                    let sub_name = self.header.fields[c].name.clone();
                    alternatives.push((sub_name, self.read_field_tree(file, c)?));
                }
                Ok(FieldValues::Variant {
                    alternatives,
                    tags,
                    indices,
                })
            }
            other => Err(Error::Format(format!(
                "field role {other:?} is not supported"
            ))),
        }
    }

    /// Column indices belonging to field `field_idx`, in column order.
    fn field_columns(&self, field_idx: usize) -> Vec<usize> {
        self.header
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.field_id as usize == field_idx)
            .map(|(ci, _)| ci)
            .collect()
    }

    /// Sub-field ids of `field_idx` (children whose parent is `field_idx`),
    /// excluding the field itself (a top-level field is its own parent), in
    /// declaration order.
    fn child_fields(&self, field_idx: usize) -> Vec<usize> {
        self.header
            .fields
            .iter()
            .enumerate()
            .filter(|(ci, f)| *ci != field_idx && f.parent_field_id as usize == field_idx)
            .map(|(ci, _)| ci)
            .collect()
    }

    fn index_column(&self, columns: &[usize]) -> Option<usize> {
        columns
            .iter()
            .copied()
            .find(|&ci| self.header.columns[ci].column_type.is_index())
    }

    /// Read a collection/string index column as globally cumulative element
    /// offsets. Index offsets are stored relative to each cluster, so we decode
    /// the column one cluster at a time and shift each cluster's values by the
    /// number of elements in all preceding clusters. (This is a no-op for a
    /// single cluster, and matches a flat decode for delta-encoded indices.)
    ///
    /// The decoded count is one offset per element of the *enclosing* level —
    /// the entry count for a top-level collection, but the parent-element count
    /// for a nested one — so it is validated by the caller, not here.
    fn read_offsets(&self, file: &RFile, column_index: usize) -> Result<Vec<u64>> {
        let descriptor = self
            .header
            .columns
            .get(column_index)
            .ok_or_else(|| Error::Format(format!("no column {column_index}")))?;

        let mut out = Vec::new();
        let mut base = 0u64;
        for cluster in &self.page_clusters {
            let column = cluster
                .columns
                .get(column_index)
                .ok_or_else(|| Error::Format(format!("cluster missing column {column_index}")))?;
            let local = match read_column(
                file.data(),
                descriptor.column_type,
                descriptor.bits_on_storage,
                &column.pages,
                descriptor.value_range,
            )? {
                ColumnValues::U64(v) => v,
                other => {
                    return Err(Error::Format(format!(
                        "expected index offsets, got {other:?}"
                    )))
                }
            };
            // Offsets are decoded from the file; use wrapping arithmetic (as
            // delta_decode does) so a corrupt index column cannot panic in debug.
            let cluster_total = local.last().copied().unwrap_or(0);
            out.extend(local.into_iter().map(|v| v.wrapping_add(base)));
            base = base.wrapping_add(cluster_total);
        }
        Ok(out)
    }
}

/// Read and decompress an RBlob (header/footer/page list) at `seek`.
fn read_blob(data: &[u8], seek: u64, nbytes: u64, len: u64, what: &str) -> Result<Vec<u8>> {
    let start = seek as usize;
    let end = start
        .checked_add(nbytes as usize)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| Error::Format(format!("{what} blob at {seek} runs past end of file")))?;
    oxiroot_compress::decompress(&data[start..end], len as usize)
        .map_err(|e| Error::Format(format!("decompressing {what}: {e}")))
}
