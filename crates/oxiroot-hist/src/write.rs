//! Serializing a `TH1D` to ROOT's on-disk object layout.
//!
//! Reproduces the exact byte layout ROOT writes (validated by byte-comparison
//! against a ROOT-written fixture), filling the data-bearing members from a
//! [`TH1`] and the cosmetic/auxiliary members with ROOT's defaults.

use std::path::Path;

use oxiroot_io_core::buffer::WBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::streamer::{write_tnamed, write_tobject};
use oxiroot_io_core::{
    update_root_file, write_root_file_with_dirs, write_root_file_with_streamers, Compression,
    ObjectRecord, Subdir,
};

/// Derive the in-file name from `path`, build the file bytes, and write them,
/// returning the crate [`Result`]. Shared by every `write_*_file` entry point so
/// they agree on path handling, the default name, and the error type.
fn write_named(path: impl AsRef<Path>, build: impl FnOnce(&str) -> Vec<u8>) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(path, build(file_name))?;
    Ok(())
}

use crate::axis::TAxis;
use crate::th1::TH1;
use crate::th2::TH2;
use crate::th3::TH3;
use crate::tprofile::TProfile;
use crate::tprofile2d::TProfile2D;
use crate::tprofile3d::TProfile3D;

/// Write a single `TH1D` into a new ROOT file at `path`. `compression`
/// is e.g. `Compression::None` or `Compression::Zstd(5)`.
pub fn write_th1d_file(path: impl AsRef<Path>, h: &TH1, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH1D".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th1d_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Streamer info (`TList<TStreamerInfo>`) describing the writable histogram
/// hierarchy — `TH1/2/3{C,S,I,L,F,D}`, `TProfile`, and every base/member class —
/// at the exact class versions this module emits (the `L` types are version 0).
/// Embedded in every written file so it is self-describing. Sourced from a
/// ROOT-written file with one of each type, kept uncompressed.
const HIST_STREAMER_INFO: &[u8] = include_bytes!("histograms.streamerinfo.bin");

// `fBits` values ROOT writes for the embedded TObjects in a fresh histogram.
const HIST_BITS: u32 = 0x0300_0008;
const AXIS_BITS: u32 = 0x0300_0000;
const TLIST_BITS: u32 = 0x0301_0000;

/// How a histogram's data `TArray` base is serialized — one of `write_tarray{c,
/// s,i,l,f,d}`, picking the precision (`TArray{C,S,I,L64,F,D}`). Everything else
/// in the object is identical across precisions, so a `TH*X` reuses the `TH*D`
/// layout (only the outer class version differs: 0 for the Long64 `L` types).
type ArrayWriter = fn(&mut WBuffer, &[f64]);

/// Serialize a `TH1{D,F,C,S,I,L}` object (with its byte-count/version header)
/// into `w`, byte-for-byte as ROOT writes it. `version` is the class version
/// (3 for C/S/I/F/D, 0 for L) and `write_array` picks the precision.
fn write_th1_obj(w: &mut WBuffer, h: &TH1, version: u16, write_array: ArrayWriter) {
    let outer = w.begin_object(version);
    write_th1_base(w, h);
    write_array(w, &h.contents); // TArray{…} base: bin contents, inline
    w.end_object(outer);
}

/// Serialize a `TH1D` object (including its leading byte-count/version header)
/// into `w`, byte-for-byte as ROOT writes it.
pub fn write_th1d(w: &mut WBuffer, h: &TH1) {
    write_th1_obj(w, h, 3, write_tarrayd);
}

/// Serialize a `TH1F` object (the float-precision `TH1`) into `w`.
pub fn write_th1f(w: &mut WBuffer, h: &TH1) {
    write_th1_obj(w, h, 3, write_tarrayf);
}

/// Serialize a `TH1D` object to a fresh byte vector.
pub fn th1d_to_bytes(h: &TH1) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th1d(&mut w, h);
    w.into_vec()
}

/// Serialize a `TH1F` object to a fresh byte vector.
pub fn th1f_to_bytes(h: &TH1) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th1f(&mut w, h);
    w.into_vec()
}

/// Write a single `TH1F` (float-precision histogram) into a new ROOT file.
pub fn write_th1f_file(path: impl AsRef<Path>, h: &TH1, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH1F".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th1f_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Write a single `TH2D` into a new ROOT file at `path`. `compression`
/// is e.g. `Compression::None` or `Compression::Zstd(5)`.
pub fn write_th2d_file(path: impl AsRef<Path>, h: &TH2, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH2D".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th2d_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Serialize a `TH2D`/`TH2F` object (with its byte-count/version header) into
/// `w`, byte-for-byte as ROOT writes it. Layout: `TH2{D,F}{ TH2{ TH1{…},
/// fScalefactor, fTsumwy, fTsumwy2, fTsumwxy }, TArray{D,F} }`.
fn write_th2_obj(w: &mut WBuffer, h: &TH2, version: u16, write_array: ArrayWriter) {
    let outer = w.begin_object(version); // TH2{D,F,C,S,I}=4, TH2L=0
    let th2 = w.begin_object(5); // TH2 version 5
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &h.yaxis, &h.zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumw2,
    );
    w.be_f64(1.0); // fScalefactor (ROOT default)
    w.be_f64(h.tsumwy);
    w.be_f64(h.tsumwy2);
    w.be_f64(h.tsumwxy);
    w.end_object(th2);
    write_array(w, &h.contents); // TArray{D,F} base: bin contents, inline
    w.end_object(outer);
}

/// Serialize a `TH2D` object (including its leading byte-count/version header)
/// into `w`, byte-for-byte as ROOT writes it.
pub fn write_th2d(w: &mut WBuffer, h: &TH2) {
    write_th2_obj(w, h, 4, write_tarrayd);
}

/// Serialize a `TH2F` object (the float-precision `TH2`) into `w`.
pub fn write_th2f(w: &mut WBuffer, h: &TH2) {
    write_th2_obj(w, h, 4, write_tarrayf);
}

/// Serialize a `TH2D` object to a fresh byte vector.
pub fn th2d_to_bytes(h: &TH2) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th2d(&mut w, h);
    w.into_vec()
}

/// Serialize a `TH2F` object to a fresh byte vector.
pub fn th2f_to_bytes(h: &TH2) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th2f(&mut w, h);
    w.into_vec()
}

/// Write a single `TH2F` (float-precision histogram) into a new ROOT file.
pub fn write_th2f_file(path: impl AsRef<Path>, h: &TH2, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH2F".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th2f_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Write a single `TH3D` into a new ROOT file at `path`. `compression`
/// is e.g. `Compression::None` or `Compression::Zstd(5)`.
pub fn write_th3d_file(path: impl AsRef<Path>, h: &TH3, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH3D".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th3d_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Serialize a `TH3D`/`TH3F` object (with its byte-count/version header) into
/// `w`, byte-for-byte as ROOT writes it. Layout: `TH3{D,F}{ TH3{ TH1{…}, TAtt3D,
/// fTsumwy, fTsumwy2, fTsumwxy, fTsumwz, fTsumwz2, fTsumwxz, fTsumwyz },
/// TArray{D,F} }`.
fn write_th3_obj(w: &mut WBuffer, h: &TH3, version: u16, write_array: ArrayWriter) {
    let outer = w.begin_object(version); // TH3{D,F,C,S,I}=4, TH3L=0
    let th3 = w.begin_object(6); // TH3 version 6
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &h.yaxis, &h.zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumw2,
    );
    let att3d = w.begin_object(1); // TAtt3D version 1 (empty base)
    w.end_object(att3d);
    w.be_f64(h.tsumwy);
    w.be_f64(h.tsumwy2);
    w.be_f64(h.tsumwxy);
    w.be_f64(h.tsumwz);
    w.be_f64(h.tsumwz2);
    w.be_f64(h.tsumwxz);
    w.be_f64(h.tsumwyz);
    w.end_object(th3);
    write_array(w, &h.contents); // TArray{D,F} base: bin contents, inline
    w.end_object(outer);
}

/// Serialize a `TH3D` object (including its leading byte-count/version header)
/// into `w`, byte-for-byte as ROOT writes it.
pub fn write_th3d(w: &mut WBuffer, h: &TH3) {
    write_th3_obj(w, h, 4, write_tarrayd);
}

/// Serialize a `TH3F` object (the float-precision `TH3`) into `w`.
pub fn write_th3f(w: &mut WBuffer, h: &TH3) {
    write_th3_obj(w, h, 4, write_tarrayf);
}

/// Serialize a `TH3D` object to a fresh byte vector.
pub fn th3d_to_bytes(h: &TH3) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th3d(&mut w, h);
    w.into_vec()
}

/// Serialize a `TH3F` object to a fresh byte vector.
pub fn th3f_to_bytes(h: &TH3) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th3f(&mut w, h);
    w.into_vec()
}

/// Write a single `TH3F` (float-precision histogram) into a new ROOT file.
pub fn write_th3f_file(path: impl AsRef<Path>, h: &TH3, compression: Compression) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TH3F".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: th3f_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Generate the `write_*`/`*_to_bytes`/`write_*_file` trio for one integer
/// histogram precision (`TH1C`/`TH2S`/`TH3I`/`TH1L`/…). The object layout is
/// identical to the same-dimension `TH*D`/`TH*F` apart from the class version
/// `$ver` (3/4 for C/S/I, 0 for the Long64 `L`) and the data `TArray` (`$array`).
/// The in-memory `f64` bin contents are narrowed to the integer type.
macro_rules! int_hist {
    ($write:ident, $bytes:ident, $file:ident, $class:literal, $htype:ty, $obj:ident, $ver:literal, $array:ident) => {
        #[doc = concat!("Serialize a `", $class, "` object (with its byte-count/version header) into `w`.")]
        pub fn $write(w: &mut WBuffer, h: &$htype) {
            $obj(w, h, $ver, $array);
        }
        #[doc = concat!("Serialize a `", $class, "` object to a fresh byte vector.")]
        pub fn $bytes(h: &$htype) -> Vec<u8> {
            let mut w = WBuffer::new();
            $write(&mut w, h);
            w.into_vec()
        }
        #[doc = concat!("Write a single `", $class, "` (integer-precision histogram) into a new ROOT file.")]
        pub fn $file(path: impl AsRef<Path>, h: &$htype, compression: Compression) -> Result<()> {
            write_named(path, |file_name| {
                let record = ObjectRecord {
                    class_name: $class.to_string(),
                    name: h.name.clone(),
                    title: h.title.clone(),
                    object: $bytes(h),
                };
                write_root_file_with_streamers(
                    file_name,
                    &[record],
                    compression.setting(),
                    Some(HIST_STREAMER_INFO),
                )
            })
        }
    };
}

int_hist!(
    write_th1c,
    th1c_to_bytes,
    write_th1c_file,
    "TH1C",
    TH1,
    write_th1_obj,
    3,
    write_tarrayc
);
int_hist!(
    write_th1s,
    th1s_to_bytes,
    write_th1s_file,
    "TH1S",
    TH1,
    write_th1_obj,
    3,
    write_tarrays
);
int_hist!(
    write_th1i,
    th1i_to_bytes,
    write_th1i_file,
    "TH1I",
    TH1,
    write_th1_obj,
    3,
    write_tarrayi
);
int_hist!(
    write_th1l,
    th1l_to_bytes,
    write_th1l_file,
    "TH1L",
    TH1,
    write_th1_obj,
    0,
    write_tarrayl
);
int_hist!(
    write_th2c,
    th2c_to_bytes,
    write_th2c_file,
    "TH2C",
    TH2,
    write_th2_obj,
    4,
    write_tarrayc
);
int_hist!(
    write_th2s,
    th2s_to_bytes,
    write_th2s_file,
    "TH2S",
    TH2,
    write_th2_obj,
    4,
    write_tarrays
);
int_hist!(
    write_th2i,
    th2i_to_bytes,
    write_th2i_file,
    "TH2I",
    TH2,
    write_th2_obj,
    4,
    write_tarrayi
);
int_hist!(
    write_th2l,
    th2l_to_bytes,
    write_th2l_file,
    "TH2L",
    TH2,
    write_th2_obj,
    0,
    write_tarrayl
);
int_hist!(
    write_th3c,
    th3c_to_bytes,
    write_th3c_file,
    "TH3C",
    TH3,
    write_th3_obj,
    4,
    write_tarrayc
);
int_hist!(
    write_th3s,
    th3s_to_bytes,
    write_th3s_file,
    "TH3S",
    TH3,
    write_th3_obj,
    4,
    write_tarrays
);
int_hist!(
    write_th3i,
    th3i_to_bytes,
    write_th3i_file,
    "TH3I",
    TH3,
    write_th3_obj,
    4,
    write_tarrayi
);
int_hist!(
    write_th3l,
    th3l_to_bytes,
    write_th3l_file,
    "TH3L",
    TH3,
    write_th3_obj,
    0,
    write_tarrayl
);

/// Write a single `TProfile` into a new ROOT file at `path`. `compression`
/// is e.g. `Compression::None` or `Compression::Zstd(5)`.
pub fn write_tprofile_file(
    path: impl AsRef<Path>,
    h: &TProfile,
    compression: Compression,
) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TProfile".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: tprofile_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Serialize a `TProfile` object (including its leading byte-count/version
/// header) into `w`. Layout: `TProfile{ TH1D{ TH1{…, fSumw2=Σwy²}, TArrayD=Σwy },
/// fBinEntries, fErrorMode, fYmin, fYmax, fTsumwy, fTsumwy2, fBinSumw2 }`.
pub fn write_tprofile(w: &mut WBuffer, h: &TProfile) {
    // A 1-D profile keeps degenerate y/z axes, as ROOT's TH1 constructor does.
    let yaxis = TAxis::new("yaxis", 1, 0.0, 1.0);
    let zaxis = TAxis::new("zaxis", 1, 0.0, 1.0);

    let tp = w.begin_object(7); // TProfile version 7
    let th1d = w.begin_object(3); // TH1D version 3
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &yaxis, &zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumy2,
    );
    write_tarrayd(w, &h.sums); // TH1D TArrayD base: per-bin sum of w*y
    w.end_object(th1d);
    write_tarrayd(w, &h.bin_entries); // fBinEntries
    w.be_i32(h.error_mode);
    w.be_f64(h.ymin);
    w.be_f64(h.ymax);
    w.be_f64(h.tsumwy);
    w.be_f64(h.tsumwy2);
    write_tarrayd(w, &h.bin_sumw2); // fBinSumw2
    w.end_object(tp);
}

/// Serialize a `TProfile` object to a fresh byte vector.
pub fn tprofile_to_bytes(h: &TProfile) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_tprofile(&mut w, h);
    w.into_vec()
}

/// Write a single `TProfile2D` into a new ROOT file at `path`.
pub fn write_tprofile2d_file(
    path: impl AsRef<Path>,
    h: &TProfile2D,
    compression: Compression,
) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TProfile2D".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: tprofile2d_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Serialize a `TProfile2D` object (with its byte-count/version header) into `w`.
/// Layout: `TProfile2D{ TH2D{ TH2{ TH1{…, fSumw2=Σwz²}, fScalefactor, fTsumwy,
/// fTsumwy2, fTsumwxy }, TArrayD=Σwz }, fBinEntries, fErrorMode, fZmin, fZmax,
/// fTsumwz, fTsumwz2, fBinSumw2 }`.
pub fn write_tprofile2d(w: &mut WBuffer, h: &TProfile2D) {
    // A 2-D profile keeps a degenerate z axis, as ROOT's TH2 constructor does.
    let zaxis = TAxis::new("zaxis", 1, 0.0, 1.0);

    let tp = w.begin_object(8); // TProfile2D version 8
    let th2d = w.begin_object(4); // TH2D version 4
    let th2 = w.begin_object(5); // TH2 version 5
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &h.yaxis, &zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumz2,
    );
    w.be_f64(1.0); // fScalefactor (ROOT default)
    w.be_f64(h.tsumwy);
    w.be_f64(h.tsumwy2);
    w.be_f64(h.tsumwxy);
    w.end_object(th2);
    write_tarrayd(w, &h.sums); // TH2D TArrayD base: per-cell Σ(w·z)
    w.end_object(th2d);
    write_tarrayd(w, &h.bin_entries); // fBinEntries
    w.be_i32(h.error_mode);
    w.be_f64(h.zmin);
    w.be_f64(h.zmax);
    w.be_f64(h.tsumwz);
    w.be_f64(h.tsumwz2);
    write_tarrayd(w, &h.bin_sumw2); // fBinSumw2
    w.end_object(tp);
}

/// Serialize a `TProfile2D` object to a fresh byte vector.
pub fn tprofile2d_to_bytes(h: &TProfile2D) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_tprofile2d(&mut w, h);
    w.into_vec()
}

/// Write a single `TProfile3D` into a new ROOT file at `path`.
pub fn write_tprofile3d_file(
    path: impl AsRef<Path>,
    h: &TProfile3D,
    compression: Compression,
) -> Result<()> {
    write_named(path, |file_name| {
        let record = ObjectRecord {
            class_name: "TProfile3D".to_string(),
            name: h.name.clone(),
            title: h.title.clone(),
            object: tprofile3d_to_bytes(h),
        };
        write_root_file_with_streamers(
            file_name,
            &[record],
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Serialize a `TProfile3D` object (with its byte-count/version header) into `w`.
/// Layout: `TProfile3D{ TH3D{ TH3{ TH1{…, fSumw2=Σwt²}, TAtt3D, fTsumwy…fTsumwyz },
/// TArrayD=Σwt }, fBinEntries, fErrorMode, fTmin, fTmax, fTsumwt, fTsumwt2,
/// fBinSumw2 }`.
pub fn write_tprofile3d(w: &mut WBuffer, h: &TProfile3D) {
    let tp = w.begin_object(8); // TProfile3D version 8
    let th3d = w.begin_object(4); // TH3D version 4
    let th3 = w.begin_object(6); // TH3 version 6
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &h.yaxis, &h.zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumt2,
    );
    let att3d = w.begin_object(1); // TAtt3D version 1 (empty base)
    w.end_object(att3d);
    w.be_f64(h.tsumwy);
    w.be_f64(h.tsumwy2);
    w.be_f64(h.tsumwxy);
    w.be_f64(h.tsumwz);
    w.be_f64(h.tsumwz2);
    w.be_f64(h.tsumwxz);
    w.be_f64(h.tsumwyz);
    w.end_object(th3);
    write_tarrayd(w, &h.sums); // TH3D TArrayD base: per-cell Σ(w·t)
    w.end_object(th3d);
    write_tarrayd(w, &h.bin_entries); // fBinEntries
    w.be_i32(h.error_mode);
    w.be_f64(h.tmin);
    w.be_f64(h.tmax);
    w.be_f64(h.tsumwt);
    w.be_f64(h.tsumwt2);
    write_tarrayd(w, &h.bin_sumw2); // fBinSumw2
    w.end_object(tp);
}

/// Serialize a `TProfile3D` object to a fresh byte vector.
pub fn tprofile3d_to_bytes(h: &TProfile3D) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_tprofile3d(&mut w, h);
    w.into_vec()
}

/// A histogram to store in a multi-object file via [`write_histograms_file`].
pub enum Hist<'a> {
    /// A 1-D histogram (written as `TH1D`).
    Th1(&'a TH1),
    /// A 2-D histogram (written as `TH2D`).
    Th2(&'a TH2),
    /// A 3-D histogram (written as `TH3D`).
    Th3(&'a TH3),
}

impl Hist<'_> {
    fn record(&self) -> ObjectRecord {
        match self {
            Hist::Th1(h) => ObjectRecord {
                class_name: "TH1D".to_string(),
                name: h.name.clone(),
                title: h.title.clone(),
                object: th1d_to_bytes(h),
            },
            Hist::Th2(h) => ObjectRecord {
                class_name: "TH2D".to_string(),
                name: h.name.clone(),
                title: h.title.clone(),
                object: th2d_to_bytes(h),
            },
            Hist::Th3(h) => ObjectRecord {
                class_name: "TH3D".to_string(),
                name: h.name.clone(),
                title: h.title.clone(),
                object: th3d_to_bytes(h),
            },
        }
    }
}

/// Write several histograms into one ROOT file at `path` (each becomes a key in
/// the root directory). `compression` is e.g. `Compression::None` or
/// `Compression::Zstd(5)`.
pub fn write_histograms_file(
    path: impl AsRef<Path>,
    hists: &[Hist],
    compression: Compression,
) -> Result<()> {
    write_named(path, |file_name| {
        let records: Vec<ObjectRecord> = hists.iter().map(Hist::record).collect();
        write_root_file_with_streamers(
            file_name,
            &records,
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Write histograms organized into subdirectories: `root` goes in the file's
/// top directory, and each `(name, hists)` in `subdirs` becomes a `TDirectory`
/// holding its own histograms (e.g. one directory per analysis region).
pub fn write_histograms_dirs(
    path: impl AsRef<Path>,
    root: &[Hist],
    subdirs: &[(&str, &[Hist])],
    compression: Compression,
) -> Result<()> {
    write_named(path, |file_name| {
        let root_objects: Vec<ObjectRecord> = root.iter().map(Hist::record).collect();
        let dirs: Vec<Subdir> = subdirs
            .iter()
            .map(|(name, hists)| Subdir {
                name: name.to_string(),
                objects: hists.iter().map(Hist::record).collect(),
            })
            .collect();
        write_root_file_with_dirs(
            file_name,
            &root_objects,
            &dirs,
            compression.setting(),
            Some(HIST_STREAMER_INFO),
        )
    })
}

/// Append histograms to an existing ROOT file at `path`, rewriting it with the
/// existing objects plus the new ones (each becomes a key). A new histogram
/// whose name matches an existing one is stored at a higher cycle, as ROOT does.
/// Errors if the file contains an RNTuple (see [`update_root_file`]).
pub fn append_histograms_file(
    path: impl AsRef<Path>,
    hists: &[Hist],
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    let existing = std::fs::read(path)?;
    let records: Vec<ObjectRecord> = hists.iter().map(Hist::record).collect();
    let bytes = update_root_file(
        &existing,
        file_name,
        &records,
        compression.setting(),
        Some(HIST_STREAMER_INFO),
    )?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn write_th1_base(w: &mut WBuffer, h: &TH1) {
    write_th1_core(
        w, &h.name, &h.title, &h.xaxis, &h.yaxis, &h.zaxis, h.ncells, h.entries, h.tsumw, h.tsumw2,
        h.tsumwx, h.tsumwx2, &h.sumw2,
    );
}

/// Write the shared `TH1` base object (version 8) used by every histogram
/// class. The dimension-specific stat sums (y/z) and the data `TArray` are
/// written by the caller after this returns.
#[allow(clippy::too_many_arguments)]
fn write_th1_core(
    w: &mut WBuffer,
    name: &str,
    title: &str,
    xaxis: &TAxis,
    yaxis: &TAxis,
    zaxis: &TAxis,
    ncells: i32,
    entries: f64,
    tsumw: f64,
    tsumw2: f64,
    tsumwx: f64,
    tsumwx2: f64,
    fsumw2: &[f64],
) {
    let th1 = w.begin_object(8); // TH1 version 8

    write_tnamed(w, HIST_BITS, name, title);
    write_attline(w);
    write_attfill(w);
    write_attmarker(w);

    w.be_i32(ncells);
    write_taxis(w, xaxis);
    write_taxis(w, yaxis);
    write_taxis(w, zaxis);
    w.be_i16(0); // fBarOffset
    w.be_i16(1000); // fBarWidth
    w.be_f64(entries);
    w.be_f64(tsumw);
    w.be_f64(tsumw2);
    w.be_f64(tsumwx);
    w.be_f64(tsumwx2);
    w.be_f64(-1111.0); // fMaximum
    w.be_f64(-1111.0); // fMinimum
    w.be_f64(0.0); // fNormFactor
    write_tarrayd(w, &[]); // fContour
    write_tarrayd(w, fsumw2); // fSumw2 (per-bin sum of squared weights)
    w.string(""); // fOption
    write_empty_tlist(w); // fFunctions
    w.be_i32(0); // fBufferSize
    w.u8(0); // fBuffer (null pointer-to-array marker)
    w.be_i32(0); // fBinStatErrOpt
    w.be_i32(2); // fStatOverflows

    w.end_object(th1);
}

fn write_attline(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(602); // fLineColor
    w.be_i16(1); // fLineStyle
    w.be_i16(1); // fLineWidth
    w.end_object(t);
}

fn write_attfill(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(0); // fFillColor
    w.be_i16(1001); // fFillStyle
    w.end_object(t);
}

fn write_attmarker(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(1); // fMarkerColor
    w.be_i16(1); // fMarkerStyle
    w.be_f32(1.0); // fMarkerSize
    w.end_object(t);
}

fn write_taxis(w: &mut WBuffer, ax: &TAxis) {
    let t = w.begin_object(10); // TAxis version 10
    write_tnamed(w, AXIS_BITS, &ax.name, &ax.title);

    // TAttAxis base (drawing defaults).
    let att = w.begin_object(4);
    w.be_i32(510); // fNdivisions
    w.be_i16(1); // fAxisColor
    w.be_i16(1); // fLabelColor
    w.be_i16(42); // fLabelFont
    w.be_f32(0.005); // fLabelOffset
    w.be_f32(0.035); // fLabelSize
    w.be_f32(0.03); // fTickLength
    w.be_f32(1.0); // fTitleOffset
    w.be_f32(0.035); // fTitleSize
    w.be_i16(1); // fTitleColor
    w.be_i16(42); // fTitleFont
    w.end_object(att);

    w.be_i32(ax.nbins);
    w.be_f64(ax.xmin);
    w.be_f64(ax.xmax);
    write_tarrayd(w, &ax.xbins); // fXbins
    w.be_i32(0); // fFirst
    w.be_i32(0); // fLast
    w.be_u16(0); // fBits2
    w.u8(0); // fTimeDisplay
    w.string(""); // fTimeFormat
    w.be_u32(0); // fLabels (null THashList*)
    w.be_u32(0); // fModLabs (null TList*)
    w.end_object(t);
}

fn write_empty_tlist(w: &mut WBuffer) {
    let t = w.begin_object(5); // TList version 5
    write_tobject(w, TLIST_BITS);
    w.string(""); // fName
    w.be_i32(0); // fSize
    w.end_object(t);
}

/// Write a `TArrayD` base inline (a count followed by that many doubles).
fn write_tarrayd(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_f64(d);
    }
}

/// Write a `TArrayF` base inline (a count followed by that many floats). The
/// in-memory bin contents are `f64`; a `TH*F` narrows them to `f32`, as ROOT does.
fn write_tarrayf(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_f32(d as f32);
    }
}

/// Write a `TArrayC` base inline (`Char_t`/`i8` bin contents; `TH*C`).
fn write_tarrayc(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.u8(d as i8 as u8);
    }
}

/// Write a `TArrayS` base inline (`Short_t`/`i16` bin contents; `TH*S`).
fn write_tarrays(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_i16(d as i16);
    }
}

/// Write a `TArrayI` base inline (`Int_t`/`i32` bin contents; `TH*I`).
fn write_tarrayi(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_i32(d as i32);
    }
}

/// Write a `TArrayL64` base inline (`Long64_t`/`i64` bin contents; `TH*L`).
fn write_tarrayl(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_i64(d as i64);
    }
}
