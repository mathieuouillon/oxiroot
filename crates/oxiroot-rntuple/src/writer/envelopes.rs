//! The header, page-list and footer envelopes, and the anchor.

use oxiroot_io_core::{Error, Result};

use super::lower::{ColumnPlan, FieldPlan};
use super::{CHECKSUM_TYPE_VERSION, COLUMN_FLAG_DEFERRED, COLUMN_FLAG_RANGE, K_BYTE_COUNT_MASK};

fn rstr(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

pub(super) fn envelope(type_id: u16, payload: &[u8]) -> Vec<u8> {
    let length = (8 + payload.len() + 8) as u64;
    let word = (type_id as u64) | (length << 16);
    let mut out = word.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    let checksum = xxhash_rust::xxh3::xxh3_64(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    out
}

pub(super) fn record_frame(payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()) as i64;
    let mut out = size.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    out
}

pub(super) fn list_frame(items: &[Vec<u8>]) -> Vec<u8> {
    let body_len: usize = items.iter().map(|i| i.len()).sum();
    let size = (8 + 4 + body_len) as i64;
    let mut out = (-size).to_le_bytes().to_vec();
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

// --- envelope builders ------------------------------------------------------

/// One field descriptor record (shared by the header and the footer's
/// schema-extension record).
fn field_record(f: &FieldPlan) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&0u32.to_le_bytes()); // field version
                                              // A checksummed (user-class) field records type version -1.
    let type_version = if f.type_checksum.is_some() {
        CHECKSUM_TYPE_VERSION
    } else {
        0
    };
    r.extend_from_slice(&type_version.to_le_bytes());
    r.extend_from_slice(&f.parent_id.to_le_bytes());
    r.extend_from_slice(&f.role.to_le_bytes()); // struct role
    r.extend_from_slice(&f.flags.to_le_bytes()); // flags
    r.extend_from_slice(&rstr(&f.name));
    r.extend_from_slice(&rstr(&f.type_name));
    r.extend_from_slice(&rstr("")); // type alias
    r.extend_from_slice(&rstr("")); // description
                                    // Flag-gated trailers (reader order: array, projection source, checksum).
    if let Some(size) = f.array_size {
        r.extend_from_slice(&size.to_le_bytes());
    }
    if let Some(checksum) = f.type_checksum {
        r.extend_from_slice(&checksum.to_le_bytes());
    }
    record_frame(&r)
}

/// One column descriptor record. `first_element_index` is `Some` for a deferred
/// (schema-extension) column — its data starts at that global element index — and
/// sets the [`COLUMN_FLAG_DEFERRED`] flag plus the index trailer (before the value
/// range, matching the reader's order).
fn column_record(c: &ColumnPlan, first_element_index: Option<i64>) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&(c.column_type as u16).to_le_bytes());
    r.extend_from_slice(&c.bits.to_le_bytes());
    r.extend_from_slice(&c.field_id.to_le_bytes());
    let mut flags = 0u16;
    if first_element_index.is_some() {
        flags |= COLUMN_FLAG_DEFERRED;
    }
    if c.value_range.is_some() {
        flags |= COLUMN_FLAG_RANGE;
    }
    r.extend_from_slice(&flags.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes()); // representation index
    if let Some(first) = first_element_index {
        r.extend_from_slice(&first.to_le_bytes());
    }
    if let Some((min, max)) = c.value_range {
        r.extend_from_slice(&min.to_le_bytes());
        r.extend_from_slice(&max.to_le_bytes());
    }
    record_frame(&r)
}

pub(super) fn build_header(name: &str, fields: &[FieldPlan], cols: &[ColumnPlan]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&rstr(name));
    p.extend_from_slice(&rstr("")); // description
    p.extend_from_slice(&rstr("oxiroot")); // writer

    let field_records: Vec<Vec<u8>> = fields.iter().map(field_record).collect();
    p.extend_from_slice(&list_frame(&field_records));

    let column_records: Vec<Vec<u8>> = cols.iter().map(|c| column_record(c, None)).collect();
    p.extend_from_slice(&list_frame(&column_records));

    p.extend_from_slice(&list_frame(&[])); // alias columns
    p.extend_from_slice(&list_frame(&[])); // extra type info

    envelope(0x01, &p)
}

/// A page-list entry stores a page's element count and on-disk size as signed
/// 32-bit fields — the element count's sign bit flags a trailing per-page
/// checksum — so a single page holds at most `i32::MAX` elements and `i32::MAX`
/// on-disk bytes. Reject anything larger rather than letting the `as i32` cast
/// wrap into a negative value that mislabels the page (a corrupt file). A genuine
/// page that big would need to be split across more clusters by the caller.
pub(super) fn check_page_limits(n_elements: u32, disk_size: usize) -> Result<()> {
    if n_elements > i32::MAX as u32 {
        return Err(Error::InvalidInput(format!(
            "RNTuple page has {n_elements} elements, over the per-page limit of {} \
             (write fewer entries per cluster)",
            i32::MAX
        )));
    }
    if disk_size > i32::MAX as usize {
        return Err(Error::InvalidInput(format!(
            "RNTuple page on-disk size {disk_size} exceeds the per-page limit of {} bytes",
            i32::MAX
        )));
    }
    Ok(())
}

pub(super) fn build_page_list(
    n_entries: u32,
    page_offsets: &[usize],
    disk_sizes: &[usize],
    cols: &[ColumnPlan],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    // A normal single cluster: every column starts at element 0.
    let element_offsets = vec![0i64; cols.len()];
    build_page_list_offsets(
        n_entries,
        page_offsets,
        disk_sizes,
        cols,
        &element_offsets,
        compression,
        header_checksum,
    )
}

/// Like [`build_page_list`] but with each column's first-element offset given
/// explicitly: a deferred (schema-extension) column's page starts at that global
/// element index, so the reader knows the earlier entries are defaulted.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_page_list_offsets(
    n_entries: u32,
    page_offsets: &[usize],
    disk_sizes: &[usize],
    cols: &[ColumnPlan],
    element_offsets: &[i64],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let mut summary = Vec::new();
    summary.extend_from_slice(&0u64.to_le_bytes()); // first entry
    summary.extend_from_slice(&(n_entries as u64).to_le_bytes()); // num entries (flags=0)
    p.extend_from_slice(&list_frame(&[record_frame(&summary)]));

    let mut column_frames: Vec<Vec<u8>> = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        check_page_limits(c.n, disk_sizes[i])?;
        let mut page = Vec::new();
        page.extend_from_slice(&(c.n as i32).to_le_bytes()); // positive: no checksum
        page.extend_from_slice(&(disk_sizes[i] as i32).to_le_bytes()); // on-disk locator size
        page.extend_from_slice(&(page_offsets[i] as u64).to_le_bytes()); // locator offset
        let mut body = Vec::new();
        body.extend_from_slice(&1u32.to_le_bytes()); // one page
        body.extend_from_slice(&page);
        body.extend_from_slice(&element_offsets[i].to_le_bytes()); // element offset
        body.extend_from_slice(&compression.to_le_bytes()); // compression settings
        let size = (8 + body.len()) as i64;
        let mut frame = (-size).to_le_bytes().to_vec();
        frame.extend_from_slice(&body);
        column_frames.push(frame);
    }
    let inner = list_frame(&column_frames); // over columns
    p.extend_from_slice(&list_frame(&[inner])); // over clusters

    Ok(envelope(0x03, &p))
}

pub(super) fn build_footer(
    n_entries: u32,
    num_clusters: u32,
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
) -> Vec<u8> {
    build_footer_ext(
        n_entries,
        num_clusters,
        page_list_offset,
        page_list_len,
        header_checksum,
        &[],
        &[],
        &[],
    )
}

/// Like [`build_footer`] but with a non-empty schema-extension record: the late
/// `ext_fields` and their deferred `ext_cols` (each paired with its first-element
/// index) go into the header-extension record so readers merge them and back-fill.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_footer_ext(
    n_entries: u32,
    num_clusters: u32,
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
    ext_fields: &[FieldPlan],
    ext_cols: &[ColumnPlan],
    first_entries: &[i64],
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&header_checksum.to_le_bytes());

    // Header-extension record: late field list, late column list, then the two
    // empty trailers (alias columns, extra type info).
    let mut ext = Vec::new();
    let field_records: Vec<Vec<u8>> = ext_fields.iter().map(field_record).collect();
    ext.extend_from_slice(&list_frame(&field_records));
    let column_records: Vec<Vec<u8>> = ext_cols
        .iter()
        .zip(first_entries)
        .map(|(c, &first)| column_record(c, Some(first)))
        .collect();
    ext.extend_from_slice(&list_frame(&column_records));
    ext.extend_from_slice(&list_frame(&[])); // alias columns
    ext.extend_from_slice(&list_frame(&[])); // extra type info
    p.extend_from_slice(&record_frame(&ext));

    // One cluster group spanning every cluster; it links to the single page-list
    // envelope that details all clusters' pages.
    let mut cg = Vec::new();
    cg.extend_from_slice(&0u64.to_le_bytes()); // min entry
    cg.extend_from_slice(&(n_entries as u64).to_le_bytes()); // entry span
    cg.extend_from_slice(&num_clusters.to_le_bytes()); // num clusters
    cg.extend_from_slice(&(page_list_len as u64).to_le_bytes()); // envelope link: uncompressed len
    cg.extend_from_slice(&(page_list_len as i32).to_le_bytes()); // locator size
    cg.extend_from_slice(&(page_list_offset as u64).to_le_bytes()); // locator offset
    p.extend_from_slice(&list_frame(&[record_frame(&cg)]));

    // Linked attribute sets (RNTuple format >= 1.0.1.0); empty here.
    p.extend_from_slice(&list_frame(&[]));

    envelope(0x02, &p)
}

pub(super) fn build_anchor(
    seek_header: usize,
    len_header: usize,
    seek_footer: usize,
    len_footer: usize,
) -> Vec<u8> {
    let mut fields = Vec::with_capacity(64);
    fields.extend_from_slice(&1u16.to_be_bytes()); // epoch
    fields.extend_from_slice(&0u16.to_be_bytes()); // major
    fields.extend_from_slice(&1u16.to_be_bytes()); // minor
    fields.extend_from_slice(&1u16.to_be_bytes()); // patch
    fields.extend_from_slice(&(seek_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(seek_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&0x4000_0000u64.to_be_bytes()); // max key size

    let checksum = xxhash_rust::xxh3::xxh3_64(&fields);

    let mut obj = Vec::new();
    obj.extend_from_slice(&((66u32) | K_BYTE_COUNT_MASK).to_be_bytes());
    obj.extend_from_slice(&2u16.to_be_bytes()); // class version
    obj.extend_from_slice(&fields);
    obj.extend_from_slice(&checksum.to_be_bytes());
    obj
}
