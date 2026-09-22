//! Axes and profiles written by older ROOT releases.
//!
//! ROOT's classes gained members over time, and a file keeps the class version
//! it was written with:
//! - `TAxis` added `fLabels` in class version 7 and `fBits2` in version 8.
//! - `TProfile` added `fTsumwy`/`fTsumwy2` in version 4 and `fBinSumw2` in 6.
//! - `TProfile2D` added `fTsumwz`/`fTsumwz2` in version 5 and `fBinSumw2` in 7.
//! - `TProfile3D` added `fBinSumw2` in version 7.
//!
//! The old profiles here are the current serialization with those trailing
//! members removed, stored with streamer info that describes the old version,
//! as the ROOT release that used it would have written them.

use oxiroot_hist::{Hist, TAxis, TProfile, TProfile2D, TProfile3D, WriteRoot, TH2};
use oxiroot_io_core::streamer_gen::{any, base, basic, Cls, El};
use oxiroot_io_core::{write_tnamed, Compression, Error, FileReader, RBuffer, ReadRoot, WBuffer};

// --- TAxis ------------------------------------------------------------------

/// A `TAxis` record at class `version`, laid out as ROOT wrote that version.
fn axis_record(version: u16, nbins: i32, xmin: f64, xmax: f64, edges: &[f64]) -> Vec<u8> {
    let mut w = WBuffer::new();
    let t = w.begin_object(version);
    write_tnamed(&mut w, 0x0300_0000, "xaxis", "");
    let att = w.begin_object(4); // TAttAxis
    w.be_i32(510); // fNdivisions
    for _ in 0..3 {
        w.be_i16(1); // fAxisColor, fLabelColor, fLabelFont
    }
    for _ in 0..5 {
        w.be_f32(0.035); // fLabelOffset … fTitleSize
    }
    w.be_i16(1); // fTitleColor
    w.be_i16(42); // fTitleFont
    w.end_object(att);
    w.be_i32(nbins);
    w.be_f64(xmin);
    w.be_f64(xmax);
    w.be_i32(edges.len() as i32); // fXbins
    for &edge in edges {
        w.be_f64(edge);
    }
    w.be_i32(0); // fFirst
    w.be_i32(0); // fLast
    if version >= 8 {
        w.be_u16(0); // fBits2
    }
    w.u8(0); // fTimeDisplay
    w.string(""); // fTimeFormat
    if version >= 7 {
        w.be_u32(0); // fLabels (null)
    }
    if version >= 10 {
        w.be_u32(0); // fModLabs (null)
    }
    w.end_object(t);
    w.into_vec()
}

#[test]
fn every_streamed_axis_version_reads() {
    for version in 6..=10 {
        // Two axes back to back: reading past the first one's members would
        // corrupt the second.
        let edges = [0.0, 0.5, 2.0, 3.0];
        let mut bytes = axis_record(version, 4, -1.0, 1.0, &[]);
        bytes.extend(axis_record(version, 3, 0.0, 3.0, &edges));
        let mut r = RBuffer::new(&bytes);

        let first = TAxis::read(&mut r).unwrap_or_else(|e| panic!("TAxis v{version}: {e}"));
        assert_eq!(
            (first.nbins, first.xmin, first.xmax),
            (4, -1.0, 1.0),
            "v{version}"
        );
        assert!(first.labels.is_empty(), "v{version}");

        let second = TAxis::read(&mut r).unwrap_or_else(|e| panic!("TAxis v{version}: {e}"));
        assert_eq!(
            (second.nbins, second.xbins.as_slice()),
            (3, &edges[..]),
            "v{version}"
        );
        assert_eq!(r.pos(), bytes.len(), "v{version} reads to the end");
    }
}

#[test]
fn an_axis_older_than_streamer_info_is_an_error() {
    let bytes = axis_record(5, 3, 0.0, 3.0, &[]);
    let Err(Error::Format(message)) = TAxis::read(&mut RBuffer::new(&bytes)) else {
        panic!("TAxis v5 read");
    };
    assert!(message.contains("TAxis class version 5"), "{message}");
}

// --- Profiles ---------------------------------------------------------------

/// An object stored at an older class version: its bytes and the streamer info
/// describing that version.
struct OldObject {
    class: &'static str,
    bytes: Vec<u8>,
    info: Cls<'static>,
}

impl WriteRoot for OldObject {
    fn root_class(&self) -> String {
        self.class.to_string()
    }
    fn root_name(&self) -> &str {
        "p"
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
    // Only the old class version is described: ROOT knows the current versions
    // of the other classes (TH1D, TAxis, …) the profile contains.
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        vec![self.info.clone()]
    }
}

/// The member names a profile class uses for its profiled axis.
struct Members {
    class: &'static str,
    base: &'static str,
    base_version: i32,
    min: &'static str,
    max: &'static str,
    sum: &'static str,
    sum2: &'static str,
    /// The first class versions with the stored sums and with `fBinSumw2`.
    sums_since: u16,
    sumw2_since: u16,
}

const TPROFILE: Members = Members {
    class: "TProfile",
    base: "TH1D",
    base_version: 3,
    min: "fYmin",
    max: "fYmax",
    sum: "fTsumwy",
    sum2: "fTsumwy2",
    sums_since: 4,
    sumw2_since: 6,
};

const TPROFILE2D: Members = Members {
    class: "TProfile2D",
    base: "TH2D",
    base_version: 4,
    min: "fZmin",
    max: "fZmax",
    sum: "fTsumwz",
    sum2: "fTsumwz2",
    sums_since: 5,
    sumw2_since: 7,
};

const TPROFILE3D: Members = Members {
    class: "TProfile3D",
    base: "TH3D",
    base_version: 4,
    min: "fTmin",
    max: "fTmax",
    sum: "fTsumwt",
    sum2: "fTsumwt2",
    sums_since: 0,
    sumw2_since: 7,
};

/// Rewrite a profile's current serialization (`bytes`, whose trailing
/// `fBinSumw2` holds `n_sumw2` values) as class `version`: drop the trailing
/// members that version lacks and describe it in streamer info.
fn downgrade(m: &Members, mut bytes: Vec<u8>, n_sumw2: usize, version: u16) -> OldObject {
    let mut elements: Vec<El<'static>> = vec![
        base(m.base, m.base_version),
        any("fBinEntries", 24, "TArrayD"),
        basic("fErrorMode", 3, 4, "EErrorType"),
        basic(m.min, 8, 8, "double"),
        basic(m.max, 8, 8, "double"),
    ];
    let mut cut = 0;
    if version >= m.sumw2_since {
        elements.push(basic(m.sum, 8, 8, "double"));
        elements.push(basic(m.sum2, 8, 8, "double"));
        elements.push(any("fBinSumw2", 24, "TArrayD"));
    } else {
        cut += 4 + 8 * n_sumw2; // the fBinSumw2 TArrayD: a count, then values
        if version >= m.sums_since {
            elements.push(basic(m.sum, 8, 8, "double"));
            elements.push(basic(m.sum2, 8, 8, "double"));
        } else {
            cut += 16;
        }
    }
    let len = bytes.len() - cut;
    bytes.truncate(len);
    bytes[4..6].copy_from_slice(&version.to_be_bytes());
    let byte_count = u32::try_from(len - 4).unwrap() | 0x4000_0000;
    bytes[..4].copy_from_slice(&byte_count.to_be_bytes());
    OldObject {
        class: m.class,
        bytes,
        info: Cls {
            name: m.class.into(),
            version: i32::from(version),
            checksum: 0x0DD0_0000 | u32::from(version),
            elements,
        },
    }
}

/// Write `old` to a file and read it back as a `T`.
fn read_back<T: ReadRoot>(old: &OldObject, version: u16) -> Result<T, Error> {
    let path = std::env::temp_dir().join(format!("oxiroot_old_{}_v{version}.root", old.class));
    old.write_root(&path, Compression::None)?;
    let back = T::read_root(&FileReader::open(&path)?, "p");
    let _ = std::fs::remove_file(&path);
    back
}

#[test]
fn every_streamed_tprofile_version_reads() {
    let mut p = Hist::reg(4, 0.0, 4.0).profile().named("p");
    p.fill(0.5, 1.0);
    p.fill(0.5, 3.0);
    p.fill_weight(1.5, 2.0, 2.0); // a weight: fBinSumw2 is tracked
    p.fill(2.5, 4.5);
    assert!(!p.bin_sumw2.is_empty());

    for version in 2..=7 {
        let old = downgrade(&TPROFILE, p.to_root_bytes(), p.bin_sumw2.len(), version);
        let back: TProfile =
            read_back(&old, version).unwrap_or_else(|e| panic!("TProfile v{version}: {e}"));
        // Versions without fBinSumw2 did not track weights; the sums missing
        // before version 4 are taken from the bins.
        let mut expected = p.clone();
        if version < 6 {
            expected.bin_sumw2.clear();
        }
        assert_eq!(back, expected, "TProfile v{version}");
    }
}

#[test]
fn every_streamed_tprofile2d_version_reads() {
    let mut p = Hist::reg(2, 0.0, 2.0).reg(3, 0.0, 3.0).profile().named("p");
    p.fill(0.5, 0.5, 10.0);
    p.fill(0.5, 0.5, 20.0);
    p.fill_weight(1.5, 2.5, 5.0, 0.5);
    p.fill(1.5, 1.5, 30.0);

    for version in 2..=8 {
        let old = downgrade(&TPROFILE2D, p.to_root_bytes(), p.bin_sumw2.len(), version);
        let back: TProfile2D =
            read_back(&old, version).unwrap_or_else(|e| panic!("TProfile2D v{version}: {e}"));
        let mut expected = p.clone();
        if version < 7 {
            expected.bin_sumw2.clear();
        }
        assert_eq!(back, expected, "TProfile2D v{version}");
    }
}

#[test]
fn every_streamed_tprofile3d_version_reads() {
    let mut p = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .profile()
        .named("p");
    p.fill(0.5, 0.5, 0.5, 10.0);
    p.fill_weight(1.5, 1.5, 1.5, 7.0, 3.0);

    for version in 6..=8 {
        let old = downgrade(&TPROFILE3D, p.to_root_bytes(), p.bin_sumw2.len(), version);
        let back: TProfile3D =
            read_back(&old, version).unwrap_or_else(|e| panic!("TProfile3D v{version}: {e}"));
        let mut expected = p.clone();
        if version < 7 {
            expected.bin_sumw2.clear();
        }
        assert_eq!(back, expected, "TProfile3D v{version}");
    }
}

#[test]
fn profiles_older_than_streamer_info_are_an_error() {
    let p1 = Hist::reg(2, 0.0, 2.0).profile().named("p");
    let old = downgrade(&TPROFILE, p1.to_root_bytes(), 0, 1);
    let Err(Error::Format(message)) = read_back::<TProfile>(&old, 1) else {
        panic!("TProfile v1 read");
    };
    assert!(message.contains("TProfile class version 1"), "{message}");

    let p3 = Hist::reg(1, 0.0, 1.0)
        .reg(1, 0.0, 1.0)
        .reg(1, 0.0, 1.0)
        .profile()
        .named("p");
    let old = downgrade(&TPROFILE3D, p3.to_root_bytes(), 0, 5);
    assert!(matches!(
        read_back::<TProfile3D>(&old, 5),
        Err(Error::Format(_))
    ));

    // A TH1 base at class version 1 stored floats where later ones store
    // doubles. It sits after the TProfile and TH1D headers (6 bytes each).
    let mut bytes = p1.to_root_bytes();
    bytes[16..18].copy_from_slice(&1u16.to_be_bytes());
    let old = OldObject {
        class: "TProfile",
        bytes,
        info: downgrade(&TPROFILE, p1.to_root_bytes(), 0, 7).info,
    };
    let Err(Error::Format(message)) = read_back::<TProfile>(&old, 71) else {
        panic!("TH1 v1 read");
    };
    assert!(message.contains("TH1 class version 1"), "{message}");
}

#[test]
fn a_root_1_th2d_is_an_error() {
    // Class version 1 had no TH2 record: the TH1 base, the bins, then the TH2
    // members. Read as the later layout it would be garbage.
    let h = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).double().named("p");
    let mut bytes = h.to_root_bytes();
    bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
    let old = OldObject {
        class: "TH2D",
        bytes,
        info: Cls {
            name: "TH2D".into(),
            version: 1,
            checksum: 1,
            elements: Vec::new(),
        },
    };
    let Err(Error::Format(message)) = read_back::<TH2>(&old, 1) else {
        panic!("TH2D v1 read");
    };
    assert!(message.contains("TH2 class version 1"), "{message}");
}
