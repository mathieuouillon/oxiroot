//! The scikit-hep `hist`-style quick-construction builder + accessors, verified
//! to stay ROOT-compatible (the builder output round-trips through the writer
//! and ROOT C++ reads the axis label — checked out of band).

use oxiroot_hist::{Hist, ReadRoot, WriteRoot, TH1};
use oxiroot_io_core::{Compression, RFile};

#[test]
fn builder_maps_storage_to_root_classes() {
    assert_eq!(Hist::reg(4, 0.0, 4.0).double().class_name(), "TH1D");
    assert_eq!(Hist::reg(4, 0.0, 4.0).float().class_name(), "TH1F");
    assert_eq!(Hist::reg(4, 0.0, 4.0).int64().class_name(), "TH1L");
    assert_eq!(Hist::reg(4, 0.0, 4.0).weight().class_name(), "TH1D"); // + Sumw2

    // 2-D and 3-D, and a variable axis.
    assert_eq!(
        Hist::reg(4, 0., 4.).reg(3, 0., 3.).weight().class_name(),
        "TH2D"
    );
    assert_eq!(
        Hist::reg(4, 0., 4.)
            .reg(3, 0., 3.)
            .reg(2, 0., 2.)
            .double()
            .class_name(),
        "TH3D"
    );
    assert_eq!(Hist::var(&[0.0, 1.0, 4.0, 10.0]).double().values().len(), 3);
}

#[test]
fn builder_sets_name_title_and_axis_labels() {
    let h = Hist::reg(10, 0.0, 100.0)
        .name("pt")
        .title("transverse momentum")
        .label("$p_T$ [GeV]")
        .double();
    assert_eq!(h.name(), "pt");
    assert_eq!(h.title(), "transverse momentum");
    assert_eq!(h.x_label(), "$p_T$ [GeV]");

    // 2-D: each `.label()` names the most recently added axis.
    let h2 = Hist::reg(4, 0., 4.)
        .label("x [cm]")
        .reg(3, 0., 3.)
        .label("y [cm]")
        .double();
    assert_eq!(h2.xaxis.title, "x [cm]");
    assert_eq!(h2.yaxis.title, "y [cm]");
}

#[test]
fn hist_style_accessors() {
    let mut h = Hist::reg(4, 0.0, 4.0).weight();
    h.fill_weight(0.5, 2.0);
    h.fill_weight(1.5, 3.0);

    assert_eq!(h.values(), &[2.0, 3.0, 0.0, 0.0]);
    assert_eq!(h.variances(), vec![4.0, 9.0, 0.0, 0.0]); // Sumw2 = Σw²
    assert_eq!(h.errors(), vec![2.0, 3.0, 0.0, 0.0]); // √variance
    assert_eq!(h.counts(), vec![1.0, 1.0, 0.0, 0.0]); // effective entries
    assert_eq!(h.density(), vec![0.4, 0.6, 0.0, 0.0]); // Σ density·width = 1
    assert_eq!(h.at(0.5), 2.0); // bin content at a coordinate
    assert_eq!(h.at(1.5), 3.0);

    // An unweighted histogram: variances default to the bin content (Poisson).
    let mut p = TH1::new(3, 0.0, 3.0);
    p.fill(0.5);
    p.fill(0.5);
    assert_eq!(p.variances(), vec![2.0, 0.0, 0.0]);
    assert_eq!(p.counts(), vec![2.0, 0.0, 0.0]); // no weights → counts == values
}

#[test]
fn builder_output_round_trips_through_root() {
    let mut h = Hist::reg(4, 0.0, 4.0)
        .name("pt")
        .label("$p_T$ [GeV]")
        .weight();
    h.fill_weight(0.5, 2.0);
    h.fill_weight(1.5, 3.0);

    let out = std::env::temp_dir().join("oxiroot_quick_hist.root");
    h.write_root(&out, Compression::None).unwrap();

    let f = RFile::open(&out).unwrap();
    let back = TH1::read_root(&f, "pt").unwrap();
    assert_eq!(back.x_label(), "$p_T$ [GeV]"); // axis label survives ROOT
    assert_eq!(back.values(), &[2.0, 3.0, 0.0, 0.0]);
    assert_eq!(back.variances(), vec![4.0, 9.0, 0.0, 0.0]); // Sumw2 survives
    let _ = std::fs::remove_file(&out);
}
