//! Writing an RNTuple into a ROOT file.
//!
//! [`write_rntuple_file`] writes a whole RNTuple in one shot, supporting scalar
//! (`bool`/`i32`/`i64`/`f32`/`f64`), `std::string`, and `std::vector<T>` fields
//! in a single cluster, with non-split column encodings and optional page
//! compression. [`NtupleWriter`] writes those same field types one cluster per
//! batch, so a large dataset need not be held in memory at once. The header/page/
//! page-list/footer envelopes are written as raw blobs at the offsets the anchor
//! (and the page locators) point to; only the anchor is a `TKey`. Validated by
//! reading the result back and by official ROOT / uproot.

use std::io::{Cursor, Seek, Write};
use std::path::Path;

use oxiroot_io_core::streamer_gen::Cls;
use oxiroot_io_core::{
    compress_if_smaller, Compression, ContainerWriter, DirId, Error, Result, WriteInto,
    KSTART_BIG_FILE,
};

use crate::anchor::ANCHOR_CLASS;

mod check;
mod classes;
mod envelopes;
mod fields;
mod lower;
mod streaming;

use classes::ntuple_classes;
use envelopes::{
    build_anchor, build_footer, build_footer_ext, build_header, build_page_list,
    build_page_list_offsets,
};
pub use fields::{Column, Field};
use lower::{lower, ColumnPlan, FieldPlan};
pub use streaming::NtupleWriter;

const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;

/// The stored bytes of one page: compressed when that makes it smaller (a reader
/// tells the two apart by comparing the stored size with the size the element
/// count implies, as ROOT does).
fn on_disk_page(page: &[u8], compression: u32) -> Vec<u8> {
    compress_if_smaller(page, compression).into_owned()
}

const ROLE_LEAF: u16 = 0;
const ROLE_COLLECTION: u16 = 1;
const ROLE_RECORD: u16 = 2;
const ROLE_VARIANT: u16 = 3;

/// Field flag: the field is a fixed-size array (`std::array`/`std::bitset`); an
/// element count (`u64`) trails the field record.
const FIELD_FLAG_ARRAY: u16 = 0x01;
/// Field flag: a type checksum (`u32`) trails the field record (user classes).
const FIELD_FLAG_CHECKSUM: u16 = 0x04;
/// The on-disk type version a checksummed user-class field carries (`-1`).
const CHECKSUM_TYPE_VERSION: u32 = 0xFFFF_FFFF;

/// Column flag: the descriptor carries an `(f64, f64)` value range (e.g. for a
/// quantized real column).
const COLUMN_FLAG_RANGE: u16 = 0x02;

/// Column flag: a deferred column (a schema-extension field added late) — the
/// descriptor carries an `i64` first-element index trailer.
const COLUMN_FLAG_DEFERRED: u16 = 0x01;

/// Build a complete ROOT file containing one RNTuple named `ntuple_name`,
/// optionally compressing pages (`compression` is e.g. `Compression::None` or
/// `Compression::Zstd(5)`). Automatically switches to ROOT's 64-bit ("big")
/// container form once the file would exceed 2 GiB.
pub fn rntuple_file_bytes(
    file_name: &str,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Result<Vec<u8>> {
    rntuple_file_bytes_threshold(file_name, ntuple_name, fields, compression, KSTART_BIG_FILE)
}

/// Like [`rntuple_file_bytes`] but with the big-file threshold injectable for
/// tests. Writes the small (32-bit) container first; only if that already
/// exceeds the threshold does it rewrite in the big (64-bit) form, so the
/// expensive page bytes are copied twice only for genuinely >2 GiB files.
fn rntuple_file_bytes_threshold(
    file_name: &str,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
    threshold: u64,
) -> Result<Vec<u8>> {
    // Lower and compress once; only the placement differs between the forms.
    let prep = prep_ntuple(ntuple_name, fields, compression.setting())?;
    let classes = ntuple_classes(fields);
    ContainerWriter::build(file_name, compression, threshold, |file| {
        write_one_rntuple(file, DirId::TOP, &prep)?;
        file.place_streamer_info(&[], &classes)
    })
}

/// One RNTuple's fully-lowered, page-encoded payload, ready to place into a file
/// at any offset: the header envelope (and its checksum), each column's on-disk
/// page bytes, the column plans, and the entry count. Independent of where in the
/// file it lands, so the same prep serves both container-form passes.
struct NtuplePrep {
    name: String,
    header_env: Vec<u8>,
    header_checksum: u64,
    disk_pages: Vec<Vec<u8>>,
    disk_sizes: Vec<usize>,
    cols: Vec<ColumnPlan>,
    n_entries: u32,
    /// Set when this is a schema-extended RNTuple: the late field/column
    /// descriptors (placed in the footer's extension record) and the per-column
    /// element offsets. `cols`/`disk_pages` already include the late columns.
    ext: Option<ExtPrep>,
}

/// The schema-extension half of a [`NtuplePrep`]: the late field descriptors, the
/// boundary between header and late columns, and each column's first-element
/// offset (`0` for header columns, the late field's first entry for late ones).
struct ExtPrep {
    ext_fields: Vec<FieldPlan>,
    base_col_count: usize,
    first_entries: Vec<i64>,
    element_offsets: Vec<i64>,
}

/// Lower one RNTuple's fields and encode its pages (the placement-independent
/// work), shared by every multi-RNTuple file pass.
fn prep_ntuple(name: &str, fields: &[Field], compression: u32) -> Result<NtuplePrep> {
    let (field_plans, cols, n_entries) = lower(fields)?;
    let header_env = build_header(name, &field_plans, &cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());
    let disk_pages: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| on_disk_page(&c.page, compression))
        .collect();
    let disk_sizes: Vec<usize> = disk_pages.iter().map(|p| p.len()).collect();
    Ok(NtuplePrep {
        name: name.to_string(),
        header_env,
        header_checksum,
        disk_pages,
        disk_sizes,
        cols,
        n_entries,
        ext: None,
    })
}

/// Lower a schema-extended RNTuple: `base_fields` go in the header, and each
/// `(first_entry, late_field)` becomes a deferred field in the footer's extension
/// record whose data covers entries `first_entry..n_entries`. The late field's
/// IDs continue the base's. Late fields must be scalar leaves (one column each)
/// and supply exactly `n_entries - first_entry` values.
fn prep_ntuple_extended(
    name: &str,
    base_fields: &[Field],
    late: &[(u64, Field)],
    compression: u32,
) -> Result<NtuplePrep> {
    let (base_fields_plan, base_cols, n_entries) = lower(base_fields)?;
    let header_env = build_header(name, &base_fields_plan, &base_cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());

    let base_col_count = base_cols.len();
    let mut cols = base_cols;
    let mut ext_fields = Vec::new();
    let mut first_entries = Vec::new();
    let mut element_offsets = vec![0i64; base_col_count];

    for (first_entry, field) in late {
        let (mut late_fp, mut late_cols, late_n) = lower(std::slice::from_ref(field))?;
        if late_cols.len() != 1 || late_fp.len() != 1 {
            return Err(Error::Unsupported(format!(
                "late RNTuple field {:?} must be a scalar leaf",
                field.name
            )));
        }
        if *first_entry + late_n as u64 != u64::from(n_entries) {
            return Err(Error::LengthMismatch {
                what: format!(
                    "late RNTuple field {:?}, from entry {first_entry}",
                    field.name
                ),
                expected: u64::from(n_entries).saturating_sub(*first_entry) as usize,
                found: late_n as usize,
            });
        }
        // Continue the schema's field/column IDs past everything added so far.
        let id_base = (base_fields_plan.len() + ext_fields.len()) as u32;
        for f in &mut late_fp {
            f.parent_id += id_base;
        }
        for c in &mut late_cols {
            c.field_id += id_base;
        }
        ext_fields.append(&mut late_fp);
        first_entries.push(*first_entry as i64);
        element_offsets.push(*first_entry as i64);
        cols.append(&mut late_cols);
    }

    let disk_pages: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| on_disk_page(&c.page, compression))
        .collect();
    let disk_sizes: Vec<usize> = disk_pages.iter().map(|p| p.len()).collect();

    Ok(NtuplePrep {
        name: name.to_string(),
        header_env,
        header_checksum,
        disk_pages,
        disk_sizes,
        cols,
        n_entries,
        ext: Some(ExtPrep {
            ext_fields,
            base_col_count,
            first_entries,
            element_offsets,
        }),
    })
}

/// Build a complete ROOT file with one schema-extended RNTuple: `base_fields` in
/// the header plus late `(first_entry, field)` fields in the footer's extension
/// record (see [`prep_ntuple_extended`]).
fn extended_rntuple_file_bytes(
    file_name: &str,
    ntuple_name: &str,
    base_fields: &[Field],
    late: &[(u64, Field)],
    compression: Compression,
) -> Result<Vec<u8>> {
    let prep = prep_ntuple_extended(ntuple_name, base_fields, late, compression.setting())?;
    let classes = ntuple_classes(base_fields.iter().chain(late.iter().map(|(_, f)| f)));
    ContainerWriter::build(file_name, compression, KSTART_BIG_FILE, |file| {
        write_one_rntuple(file, DirId::TOP, &prep)?;
        file.place_streamer_info(&[], &classes)
    })
}

/// Write one RNTuple's blobs (header, pages, page list, footer) at the end of
/// `file`, then its anchor key in directory `dir`.
fn write_one_rntuple<W: Write + Seek>(
    file: &mut ContainerWriter<W>,
    dir: DirId,
    p: &NtuplePrep,
) -> Result<()> {
    let compression = file.compression_setting();
    let seek_header = file.place_blob(&p.header_env)?;
    let mut page_offsets = Vec::with_capacity(p.cols.len());
    for dp in &p.disk_pages {
        page_offsets.push(offset(file.place_blob(dp)?)?);
    }
    let page_list_offset = offset(file.position())?;
    let footer_env = if let Some(ext) = &p.ext {
        // Schema-extended: the late columns' pages start at their first element
        // index (the page list records the offset), and the late field/column
        // descriptors go in the footer's schema-extension record.
        let page_list_env = build_page_list_offsets(
            p.n_entries,
            &page_offsets,
            &p.disk_sizes,
            &p.cols,
            &ext.element_offsets,
            compression,
            p.header_checksum,
        )?;
        file.place_blob(&page_list_env)?;
        build_footer_ext(
            p.n_entries,
            1,
            page_list_offset,
            page_list_env.len(),
            p.header_checksum,
            &ext.ext_fields,
            &p.cols[ext.base_col_count..],
            &ext.first_entries,
        )
    } else {
        let page_list_env = build_page_list(
            p.n_entries,
            &page_offsets,
            &p.disk_sizes,
            &p.cols,
            compression,
            p.header_checksum,
        )?;
        file.place_blob(&page_list_env)?;
        build_footer(
            p.n_entries,
            1,
            page_list_offset,
            page_list_env.len(),
            p.header_checksum,
        )
    };
    let seek_footer = file.place_blob(&footer_env)?;

    let anchor = build_anchor(
        offset(seek_header)?,
        p.header_env.len(),
        offset(seek_footer)?,
        footer_env.len(),
    );
    file.place_key_uncompressed(dir, ANCHOR_CLASS, &p.name, "", &anchor)?;
    Ok(())
}

/// A file offset as the `usize` the envelope builders take.
fn offset(seek: u64) -> Result<usize> {
    usize::try_from(seek)
        .map_err(|_| Error::Unsupported(format!("file offset {seek} does not fit this platform")))
}

/// Write a one-RNTuple ROOT file to `path`, optionally compressing pages
/// (`compression` is e.g. `Compression::None` or `Compression::Zstd(5)`).
pub fn write_rntuple_file(
    path: impl AsRef<Path>,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    let bytes = rntuple_file_bytes(file_name, ntuple_name, fields, compression)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// An RNTuple to write: a name and its [`Field`]s. The method-based,
/// write-side counterpart to the free [`write_rntuple_file`] function (and to
/// the read-only [`NtupleReader`](crate::NtupleReader)) — build one, then call
/// [`write_root`](Ntuple::write_root), mirroring `hist.write_root`:
///
/// ```no_run
/// use oxiroot_rntuple::{Field, Ntuple};
/// use oxiroot_io_core::Compression;
///
/// let fields = vec![
///     Field::f64("mass", vec![91.2, 125.0]),
///     Field::i32("charge", vec![0, -1]),
/// ];
/// Ntuple::new("events", fields).write_root("data.root", Compression::None)?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
pub struct Ntuple {
    name: String,
    fields: Vec<Field>,
}

impl Ntuple {
    /// Create a writable RNTuple from a name and its fields.
    pub fn new(name: impl Into<String>, fields: Vec<Field>) -> Ntuple {
        Ntuple {
            name: name.into(),
            fields,
        }
    }

    /// The RNTuple's name (the in-file key).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The RNTuple's fields.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Write this RNTuple as a new one-RNTuple ROOT file, optionally compressing
    /// pages. The method form of [`write_rntuple_file`].
    pub fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()> {
        write_rntuple_file(path, &self.name, &self.fields, compression)
    }

    /// The complete ROOT-file bytes for this RNTuple (the method form of
    /// [`rntuple_file_bytes`]); `file_name` is the `TFile` name recorded in the
    /// file header.
    pub fn to_root_bytes(&self, file_name: &str, compression: Compression) -> Result<Vec<u8>> {
        rntuple_file_bytes(file_name, &self.name, &self.fields, compression)
    }

    /// Write this RNTuple **with a schema late extension**: this RNTuple's fields
    /// go in the header, and each `(first_entry, field)` in `late` is added as a
    /// deferred field via the footer's schema-extension record — its data covers
    /// entries `first_entry..N`, and the earlier entries default. ROOT reads the
    /// result as a schema-extended RNTuple (as if the field had been added
    /// mid-writing with a model updater). Late fields must be scalar leaves and
    /// supply exactly `N - first_entry` values.
    ///
    /// ```no_run
    /// use oxiroot_rntuple::{Field, Ntuple};
    /// use oxiroot_io_core::Compression;
    /// // 4 entries of `x`; `y` added late, covering only entries 2 and 3.
    /// Ntuple::new("events", vec![Field::i32("x", vec![1, 2, 3, 4])])
    ///     .write_root_extended(
    ///         "ext.root",
    ///         &[(2, Field::f32("y", vec![3.5, 4.5]))],
    ///         Compression::None,
    ///     )?;
    /// # Ok::<(), oxiroot_io_core::Error>(())
    /// ```
    pub fn write_root_extended(
        &self,
        path: impl AsRef<Path>,
        late: &[(u64, Field)],
        compression: Compression,
    ) -> Result<()> {
        let file_name = path
            .as_ref()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let bytes =
            extended_rntuple_file_bytes(&file_name, &self.name, &self.fields, late, compression)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

/// An `Ntuple` goes into a [`FileWriter`](oxiroot_io_core::FileWriter) with
/// [`put`](oxiroot_io_core::FileWriter::put): several RNTuples in one file, inside
/// `TDirectory` subdirectories, or next to histograms and trees. ROOT and uproot
/// navigate the result natively.
///
/// ```no_run
/// use oxiroot_io_core::{Compression, FileWriter};
/// use oxiroot_rntuple::{Field, Ntuple};
///
/// FileWriter::create("multi.root")
///     .put(Ntuple::new("events", vec![Field::i32("x", vec![1, 2, 3])]))
///     .put(Ntuple::new("runs", vec![Field::i32("run", vec![7])]))
///     .dir("cal", |d| d.put(Ntuple::new("pedestals", vec![Field::f64("p", vec![0.5])])))
///     .write(Compression::None)?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
impl WriteInto for Ntuple {
    fn root_class(&self) -> String {
        ANCHOR_CLASS.to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn write_into(&self, file: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()> {
        let prep = prep_ntuple(&self.name, &self.fields, file.compression_setting())?;
        write_one_rntuple(file, dir, &prep)
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        ntuple_classes(&self.fields)
            .into_iter()
            .map(Cls::into_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::envelopes::check_page_limits;
    use super::*;
    use crate::{FieldValues, NtupleReader};
    use oxiroot_io_core::FileReader;

    #[test]
    fn one_shot_writes_and_reads_big_format() {
        let fields = vec![
            Field::i32("x", vec![1, 2, 3, 4]),
            Field::f64("y", vec![1.5, 2.5, 3.5, 4.5]),
        ];

        // Tiny file, but forced into the 64-bit container form via a low
        // threshold — it must still parse and yield identical values.
        let bytes =
            rntuple_file_bytes_threshold("t.root", "ntpl", &fields, Compression::None, 64).unwrap();
        // Also drop it to a temp file so an external reader (uproot) can be run
        // against the one-shot big-format output out of band.
        let _ = std::fs::write("/tmp/rootrs_oneshot_big.root", &bytes);
        let f = FileReader::from_bytes(bytes).unwrap();
        assert!(f.header().is_big(), "forced into big-format container");
        let ntpl = NtupleReader::open(&f, "ntpl").unwrap();
        assert_eq!(ntpl.num_entries(), 4);
        assert_eq!(
            ntpl.read_field(&f, "x").unwrap(),
            FieldValues::I32(vec![1, 2, 3, 4])
        );
        assert_eq!(
            ntpl.read_field(&f, "y").unwrap(),
            FieldValues::F64(vec![1.5, 2.5, 3.5, 4.5])
        );

        // The same data under the real threshold stays in small (32-bit) form.
        let small = rntuple_file_bytes("t.root", "ntpl", &fields, Compression::None).unwrap();
        let fs = FileReader::from_bytes(small).unwrap();
        assert!(!fs.header().is_big());
        let ntpl = NtupleReader::open(&fs, "ntpl").unwrap();
        assert_eq!(
            ntpl.read_field(&fs, "x").unwrap(),
            FieldValues::I32(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn page_limits_reject_oversized_counts_and_sizes() {
        // In range, including exactly at the boundary, is accepted.
        assert!(check_page_limits(1_000_000, 1_000_000).is_ok());
        assert!(check_page_limits(i32::MAX as u32, i32::MAX as usize).is_ok());
        // One element past the limit would flip the count's i32 sign bit, which
        // the format reads as "this page has a trailing checksum" — rejected.
        assert!(check_page_limits(i32::MAX as u32 + 1, 0).is_err());
        // One byte past the on-disk-size limit would flip the locator size sign.
        assert!(check_page_limits(0, i32::MAX as usize + 1).is_err());
    }
}
