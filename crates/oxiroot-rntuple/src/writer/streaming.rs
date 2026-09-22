//! [`NtupleWriter`]: an RNTuple written one cluster per batch, so the whole
//! dataset need not be in memory.

use std::io::{Seek, Write};
use std::path::Path;

use oxiroot_io_core::{
    compress_if_smaller, Compression, ContainerWriter, DirId, Error, Result, KSTART_BIG_FILE,
};

use super::classes::ntuple_streamer_info;
use super::envelopes::{
    build_anchor, build_footer, build_header, check_page_limits, envelope, list_frame, record_frame,
};
use super::fields::Field;
use super::lower::{lower, ColumnPlan, FieldPlan};
use super::offset;
use crate::anchor::ANCHOR_CLASS;

/// One page's location for the page list (one page per column per cluster).
struct PageRec {
    offset: u64,
    disk_size: usize,
    n_elements: u32,
    element_offset: i64,
}

/// A batch's full lowered schema identity. Every batch must produce an equal
/// one, otherwise its pages would be appended under the first batch's header and
/// silently mis-described. Compares field identity (name, type, parent, role) as
/// well as physical columns — `(type, bits)` alone would let a field rename or
/// reorder slip through.
#[derive(PartialEq, Eq)]
struct SchemaSig {
    /// `(name, type_name, parent_id, role)` per field, in lowered order.
    fields: Vec<(String, String, u32, u16)>,
    /// `(column type, bit width, owning field id)` per physical column.
    columns: Vec<(u16, u16, u32)>,
}

fn schema_sig(field_plans: &[FieldPlan], cols: &[ColumnPlan]) -> SchemaSig {
    SchemaSig {
        fields: field_plans
            .iter()
            .map(|f| (f.name.clone(), f.type_name.clone(), f.parent_id, f.role))
            .collect(),
        columns: cols
            .iter()
            .map(|c| (c.column_type as u16, c.bits, c.field_id))
            .collect(),
    }
}

/// Schema + header bookkeeping, fixed once the first batch defines it.
struct HeaderState {
    seek: u64,
    len: usize,
    checksum: u64,
    /// The lowered schema the first batch committed — must match every batch.
    signature: SchemaSig,
    /// The file's streamer info, for the classes in the first batch's fields.
    streamer_info: Vec<u8>,
}

/// A streaming RNTuple writer: each [`write_batch`](NtupleWriter::write_batch)
/// flushes one *cluster* to the sink, so a large dataset can be written one
/// chunk at a time without ever holding it all in memory. Call
/// [`finish`](NtupleWriter::finish) to write the page list, footer, and anchor.
///
/// Handles the same field types as [`write_rntuple_file`](super::write_rntuple_file) — scalars,
/// `std::string`, and `std::vector<T>` — writing each batch's collection/string
/// index offsets relative to its own cluster, as the format requires.
#[doc(alias = "RNTupleWriter")]
pub struct NtupleWriter<W: Write + Seek> {
    file: ContainerWriter<W>,
    ntuple_name: String,
    // Set when the first batch defines the schema and writes the header.
    header: Option<HeaderState>,
    element_base: Vec<u64>,
    // Accumulated per-cluster metadata.
    total_entries: u64,
    summaries: Vec<(u64, u64)>,
    cluster_pages: Vec<Vec<PageRec>>,
}

impl NtupleWriter<std::fs::File> {
    /// Create a streaming RNTuple file at `path` (32-bit container; supports up
    /// to 2 GiB — [`finish`](NtupleWriter::finish) errors if that is exceeded).
    pub fn create(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::create_fmt(path, ntuple_name, compression, false)
    }

    /// Like [`create`](NtupleWriter::create), but writes the 64-bit ("big")
    /// container form so the file may exceed 2 GiB. Use this when the streamed
    /// dataset is expected to be large; small files are still valid, just stored
    /// in the wider form.
    pub fn create_large(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::create_fmt(path, ntuple_name, compression, true)
    }

    fn create_fmt(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
        big: bool,
    ) -> Result<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let file = std::fs::File::create(path)?;
        NtupleWriter::new_fmt(file, &file_name, ntuple_name, compression, big)
    }
}

impl<W: Write + Seek> NtupleWriter<W> {
    /// Begin writing into an arbitrary seekable sink (the TFile header and root
    /// directory are written immediately, with pointers to patch at the end).
    /// Small (32-bit) container — see [`new_large`](NtupleWriter::new_large) for
    /// the >2 GiB form.
    pub fn new(
        sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::new_fmt(sink, file_name, ntuple_name, compression, false)
    }

    /// Like [`new`](NtupleWriter::new), but writes the 64-bit ("big") container
    /// form so the streamed file may exceed 2 GiB.
    pub fn new_large(
        sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::new_fmt(sink, file_name, ntuple_name, compression, true)
    }

    fn new_fmt(
        sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
        big: bool,
    ) -> Result<Self> {
        Ok(NtupleWriter {
            file: ContainerWriter::new(sink, file_name, compression, big)?,
            ntuple_name: ntuple_name.to_string(),
            header: None,
            element_base: Vec::new(),
            total_entries: 0,
            summaries: Vec::new(),
            cluster_pages: Vec::new(),
        })
    }

    /// Append one cluster holding the entries in `fields`. All batches must share
    /// the same field schema; the first batch fixes it and writes the header.
    pub fn write_batch(&mut self, fields: &[Field]) -> Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let (field_plans, cols, n_entries) = lower(fields)?;
        if n_entries == 0 {
            return Ok(());
        }
        let signature = schema_sig(&field_plans, &cols);

        match self.header.as_ref().map(|h| h.signature == signature) {
            Some(true) => {} // schema matches; header already written
            Some(false) => {
                return Err(Error::SchemaChanged {
                    detail: "this batch's field schema differs from the first batch's".into(),
                })
            }
            None => {
                // First batch fixes the schema and writes the header.
                let header_env = build_header(&self.ntuple_name, &field_plans, &cols);
                let checksum =
                    u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());
                let seek = self.file.place_blob(&header_env)?;
                self.element_base = vec![0u64; cols.len()];
                self.header = Some(HeaderState {
                    seek,
                    len: header_env.len(),
                    checksum,
                    signature,
                    streamer_info: ntuple_streamer_info(fields),
                });
            }
        }

        let first_entry = self.total_entries;
        let mut recs = Vec::with_capacity(cols.len());
        for (i, c) in cols.iter().enumerate() {
            let disk = compress_if_smaller(&c.page, self.file.compression_setting());
            let element_offset = self.element_base[i] as i64;
            let offset = self.file.place_blob(&disk)?;
            recs.push(PageRec {
                offset,
                disk_size: disk.len(),
                n_elements: c.n,
                element_offset,
            });
            self.element_base[i] += c.n as u64;
        }
        self.cluster_pages.push(recs);
        self.summaries.push((first_entry, n_entries as u64));
        self.total_entries += n_entries as u64;
        Ok(())
    }

    /// Finish the file: write the page list (all clusters), footer, anchor key,
    /// and key list, then patch the header pointers.
    pub fn finish(mut self) -> Result<()> {
        let header = self
            .header
            .take()
            .ok_or_else(|| Error::Format("NtupleWriter finished with no batches written".into()))?;
        let num_clusters = self.summaries.len() as u32;

        let compression = self.file.compression_setting();
        let page_list_env = build_page_list_multi(
            &self.summaries,
            &self.cluster_pages,
            compression,
            header.checksum,
        )?;
        let page_list_offset = self.file.place_blob(&page_list_env)?;

        let footer_env = build_footer(
            self.total_entries as u32,
            num_clusters,
            offset(page_list_offset)?,
            page_list_env.len(),
            header.checksum,
        );
        let seek_footer = self.file.place_blob(&footer_env)?;

        // A small (32-bit) container cannot address past 2 GiB. Fail loudly
        // rather than truncating the anchor / key-list seek pointers into a
        // corrupt file; the caller can re-run with `create_large`/`new_large`.
        let pos = self.file.position();
        if !self.file.is_big() && pos > KSTART_BIG_FILE {
            return Err(Error::FileTooLarge { size: pos });
        }

        let anchor = build_anchor(
            offset(header.seek)?,
            header.len,
            offset(seek_footer)?,
            footer_env.len(),
        );
        self.file.place_key_uncompressed(
            DirId::TOP,
            ANCHOR_CLASS,
            &self.ntuple_name,
            "",
            &anchor,
        )?;
        self.file.place_streamer_info(&header.streamer_info, &[])?;
        self.file.finish()?;
        Ok(())
    }
}

/// Build the page-list envelope for any number of clusters: cluster summaries,
/// then page locations nested clusters → columns → (one) page.
fn build_page_list_multi(
    summaries: &[(u64, u64)],
    cluster_pages: &[Vec<PageRec>],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let summary_frames: Vec<Vec<u8>> = summaries
        .iter()
        .map(|&(first, n)| {
            let mut s = Vec::new();
            s.extend_from_slice(&first.to_le_bytes());
            s.extend_from_slice(&n.to_le_bytes()); // high byte = flags (0)
            record_frame(&s)
        })
        .collect();
    p.extend_from_slice(&list_frame(&summary_frames));

    let mut cluster_frames: Vec<Vec<u8>> = Vec::with_capacity(cluster_pages.len());
    for cols in cluster_pages {
        let mut col_frames: Vec<Vec<u8>> = Vec::with_capacity(cols.len());
        for pr in cols {
            check_page_limits(pr.n_elements, pr.disk_size)?;
            let mut page = Vec::new();
            page.extend_from_slice(&(pr.n_elements as i32).to_le_bytes()); // no checksum
            page.extend_from_slice(&(pr.disk_size as i32).to_le_bytes()); // on-disk size
            page.extend_from_slice(&pr.offset.to_le_bytes()); // locator offset
            let mut body = Vec::new();
            body.extend_from_slice(&1u32.to_le_bytes()); // one page
            body.extend_from_slice(&page);
            body.extend_from_slice(&pr.element_offset.to_le_bytes());
            body.extend_from_slice(&compression.to_le_bytes());
            let size = (8 + body.len()) as i64;
            let mut frame = (-size).to_le_bytes().to_vec();
            frame.extend_from_slice(&body);
            col_frames.push(frame);
        }
        cluster_frames.push(list_frame(&col_frames));
    }
    p.extend_from_slice(&list_frame(&cluster_frames));

    Ok(envelope(0x03, &p))
}
