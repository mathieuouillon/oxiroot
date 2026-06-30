//! The RNTuple footer envelope: cluster groups and their page-list locators.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;

use crate::envelope::{read_feature_flags, read_frame, read_locator, Locator};
use crate::header::{read_column_list, read_field_list, ColumnDescriptor, FieldDescriptor};

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
// Not `Eq`: extension `ColumnDescriptor`s may carry an `f64` value range.
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    /// XXH3-64 of the header envelope (cross-check).
    pub header_checksum: u64,
    /// Fields added after the header via the schema-extension record (the late
    /// fields of a schema-extended RNTuple); empty for an unextended one. Their
    /// field IDs continue the header's numbering.
    pub ext_fields: Vec<FieldDescriptor>,
    /// Columns added via the schema-extension record (deferred columns of the
    /// late fields); their IDs continue the header's numbering.
    pub ext_columns: Vec<ColumnDescriptor>,
    /// The cluster groups, in order.
    pub cluster_groups: Vec<ClusterGroup>,
}

impl Footer {
    /// Parse the footer from its envelope payload.
    pub fn parse(payload: &[u8]) -> Result<Footer> {
        let mut r = RBuffer::new(payload);

        read_feature_flags(&mut r)?;
        let header_checksum = r.le_u64()?;

        // Schema extension record frame: late-added field/column descriptors (a
        // record frame wrapping the same field + column lists as the header
        // schema). Empty for an unextended RNTuple.
        let ext = read_frame(&mut r)?;
        let (ext_fields, ext_columns) = if r.pos() < ext.end {
            let fields = read_field_list(&mut r)?;
            let columns = read_column_list(&mut r)?;
            (fields, columns)
        } else {
            (Vec::new(), Vec::new())
        };
        r.seek(ext.end)?;

        // Cluster group list frame. Cap the reservation at the remaining buffer
        // so a forged `n_items` can't drive a huge allocation (matching the
        // header/page-list parsers).
        let list = read_frame(&mut r)?;
        let mut cluster_groups = Vec::with_capacity((list.n_items as usize).min(r.remaining()));
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
            ext_fields,
            ext_columns,
            cluster_groups,
        })
    }
}
