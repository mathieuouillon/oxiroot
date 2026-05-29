//! The RNTuple footer envelope: cluster groups and their page-list locators.

use root_io_core::buffer::RBuffer;
use root_io_core::error::Result;

use crate::envelope::{read_feature_flags, read_frame, read_locator, Locator};

/// A cluster group: a contiguous range of entries whose page locations live in
/// one page-list envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterGroup {
    /// First entry number covered by this group.
    pub min_entry: u64,
    /// Number of entries spanned by this group.
    pub entry_span: u64,
    /// Number of clusters in this group.
    pub num_clusters: u32,
    /// Locator of this group's page-list envelope.
    pub page_list: Locator,
    /// Uncompressed size of the page-list envelope.
    pub page_list_len: u64,
}

/// The parsed footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    /// XXH3-64 of the header envelope (cross-check).
    pub header_checksum: u64,
    /// The cluster groups, in order.
    pub cluster_groups: Vec<ClusterGroup>,
}

impl Footer {
    /// Parse the footer from its envelope payload.
    pub fn parse(payload: &[u8]) -> Result<Footer> {
        let mut r = RBuffer::new(payload);

        read_feature_flags(&mut r)?;
        let header_checksum = r.le_u64()?;

        // Schema extension record frame (late-added fields/columns) — skipped.
        let ext = read_frame(&mut r)?;
        r.seek(ext.end)?;

        // Cluster group list frame.
        let list = read_frame(&mut r)?;
        let mut cluster_groups = Vec::with_capacity(list.n_items as usize);
        for _ in 0..list.n_items {
            let frame = read_frame(&mut r)?;
            let min_entry = r.le_u64()?;
            let entry_span = r.le_u64()?;
            let num_clusters = r.le_u32()?;
            // The page-list locator is an "envelope link": its uncompressed
            // length (u64) precedes the locator.
            let page_list_len = r.le_u64()?;
            let page_list = read_locator(&mut r)?;
            r.seek(frame.end)?;
            cluster_groups.push(ClusterGroup {
                min_entry,
                entry_span,
                num_clusters,
                page_list,
                page_list_len,
            });
        }
        r.seek(list.end)?;

        Ok(Footer {
            header_checksum,
            cluster_groups,
        })
    }
}
