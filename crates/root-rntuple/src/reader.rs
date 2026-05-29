//! Opening an RNTuple from a ROOT file: anchor → header/footer envelopes →
//! page-list envelopes → on-demand column decoding.

use root_io_core::error::{Error, Result};
use root_io_core::RFile;

use crate::anchor::{RNTupleAnchor, ANCHOR_CLASS};
use crate::envelope::{read_envelope, ENVELOPE_FOOTER, ENVELOPE_HEADER, ENVELOPE_PAGELIST};
use crate::footer::Footer;
use crate::header::Header;
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

        let anchor_payload = &file.data()[key.payload_range()];
        let anchor_object = root_compress::decompress(anchor_payload, key.obj_len as usize)
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
        )
    }
}

/// Read and decompress an RBlob (header/footer/page list) at `seek`.
fn read_blob(data: &[u8], seek: u64, nbytes: u64, len: u64, what: &str) -> Result<Vec<u8>> {
    let start = seek as usize;
    let end = start
        .checked_add(nbytes as usize)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| Error::Format(format!("{what} blob at {seek} runs past end of file")))?;
    root_compress::decompress(&data[start..end], len as usize)
        .map_err(|e| Error::Format(format!("decompressing {what}: {e}")))
}
