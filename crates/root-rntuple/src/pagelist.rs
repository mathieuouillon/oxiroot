//! The RNTuple page-list envelope: cluster summaries and page locations.
//!
//! Page locations are a triple-nested list: clusters → columns → pages. Each
//! per-column frame ends with the column's element offset and compression
//! settings (inside the frame).

use root_io_core::buffer::RBuffer;
use root_io_core::error::Result;

use crate::envelope::{read_frame, read_locator, Locator};

/// Summary of one cluster: where its entries start and how many it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterSummary {
    /// First entry number in this cluster.
    pub first_entry: u64,
    /// Number of entries in this cluster.
    pub num_entries: u64,
}

/// Location of one page of a column within a cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageInfo {
    /// Number of elements stored in the page.
    pub num_elements: u32,
    /// Whether an 8-byte XXH3 checksum follows the page data on disk.
    pub has_checksum: bool,
    /// Where the (compressed) page bytes live.
    pub locator: Locator,
}

/// The pages of one column within one cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnPages {
    /// The pages, in order.
    pub pages: Vec<PageInfo>,
    /// Index of the first element (negative ⇒ suppressed column).
    pub element_offset: i64,
    /// Compression settings for this column's pages (None if suppressed).
    pub compression: Option<u32>,
}

/// The page locations for one cluster: one [`ColumnPages`] per column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterPages {
    /// Per-column pages, indexed by physical column id.
    pub columns: Vec<ColumnPages>,
}

/// A parsed page-list envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageList {
    /// Per-cluster entry summaries.
    pub summaries: Vec<ClusterSummary>,
    /// Per-cluster page locations.
    pub clusters: Vec<ClusterPages>,
}

impl PageList {
    /// Parse a page-list envelope payload.
    pub fn parse(payload: &[u8]) -> Result<PageList> {
        let mut r = RBuffer::new(payload);
        let _header_checksum = r.le_u64()?;

        // Cluster summaries: a list of record frames.
        let summary_list = read_frame(&mut r)?;
        let mut summaries = Vec::with_capacity(summary_list.n_items as usize);
        for _ in 0..summary_list.n_items {
            let frame = read_frame(&mut r)?;
            let first_entry = r.le_u64()?;
            let num_entries = r.le_u64()? & 0x00FF_FFFF_FFFF_FFFF; // high byte = flags
            r.seek(frame.end)?;
            summaries.push(ClusterSummary {
                first_entry,
                num_entries,
            });
        }
        r.seek(summary_list.end)?;

        // Page locations: outer list (clusters) of inner list (columns).
        let cluster_list = read_frame(&mut r)?;
        let mut clusters = Vec::with_capacity(cluster_list.n_items as usize);
        for _ in 0..cluster_list.n_items {
            let column_list = read_frame(&mut r)?;
            let mut columns = Vec::with_capacity(column_list.n_items as usize);
            for _ in 0..column_list.n_items {
                columns.push(read_column_pages(&mut r)?);
            }
            r.seek(column_list.end)?;
            clusters.push(ClusterPages { columns });
        }
        r.seek(cluster_list.end)?;

        Ok(PageList {
            summaries,
            clusters,
        })
    }
}

/// Read one column's pages: a list frame of page descriptions, then the
/// column's element offset and (unless suppressed) compression settings — all
/// inside the frame.
fn read_column_pages(r: &mut RBuffer) -> Result<ColumnPages> {
    let frame = read_frame(r)?;
    let mut pages = Vec::with_capacity(frame.n_items as usize);
    for _ in 0..frame.n_items {
        let raw = r.le_i32()?;
        let has_checksum = raw < 0;
        let num_elements = raw.unsigned_abs();
        let locator = read_locator(r)?;
        pages.push(PageInfo {
            num_elements,
            has_checksum,
            locator,
        });
    }
    let element_offset = r.le_i64()?;
    let compression = if element_offset >= 0 {
        Some(r.le_u32()?)
    } else {
        None
    };
    r.seek(frame.end)?;
    Ok(ColumnPages {
        pages,
        element_offset,
        compression,
    })
}
