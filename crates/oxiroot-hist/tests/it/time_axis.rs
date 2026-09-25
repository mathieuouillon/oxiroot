//! Time axes (`Axis::fTimeDisplay` / `fTimeFormat`), which ROOT draws as dates
//! and clock times rather than numbers.

use oxiroot_hist::{FileWriter, Graph, Hist, Hist1D, ReadRoot, WriteRoot};
use oxiroot_io_core::{Compression, FileReader};

/// ROOT's own spelling: a `strftime` format, then the epoch after `%F`.
const FORMAT: &str = "%H:%M%F2024-01-01 00:00:00";

#[test]
fn a_time_axis_survives_a_file() {
    let mut h = Hist::reg(24, 0.0, 86_400.0)
        .double()
        .named("rate")
        .titled("hits per hour");
    h.xaxis.set_time_format(FORMAT);
    h.fill(3_600.0);
    assert!(h.xaxis.time_display);

    // A graph carries its axes on its display frame, as ROOT stores them.
    let mut frame = Hist::reg(2, 0.0, 7_200.0).float().named("Graph");
    frame.xaxis.set_time_format(FORMAT);
    let mut g = Graph::new(vec![0.0, 3_600.0], vec![1.0, 2.0])
        .unwrap()
        .named("trend");
    g.histogram = Some(frame);

    let out = std::env::temp_dir().join("oxiroot_time_axis.root");
    FileWriter::create(&out)
        .add(&h)
        .add(&g)
        .write(Compression::None)
        .expect("write");

    let f = FileReader::open(&out).expect("reopen");
    let read = Hist1D::read_root(&f, "rate").expect("read hist");
    assert!(read.xaxis.time_display);
    assert_eq!(read.xaxis.time_format, FORMAT);
    assert_eq!(read, h);

    let read = Graph::read_root(&f, "trend").expect("read graph");
    let frame = read.histogram.as_ref().expect("display frame");
    assert!(frame.xaxis.time_display);
    assert_eq!(frame.xaxis.time_format, FORMAT);
}

#[test]
fn an_ordinary_axis_stays_numeric() {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("plain");
    h.fill(1.5);
    assert!(!h.xaxis.time_display);
    assert_eq!(h.xaxis.time_format, "");

    let out = std::env::temp_dir().join("oxiroot_plain_axis.root");
    h.write_root(&out, Compression::None).expect("write");
    let read = Hist1D::read_root(&FileReader::open(&out).unwrap(), "plain").unwrap();
    assert!(!read.xaxis.time_display);
    assert_eq!(read.xaxis.time_format, "");

    // Setting and clearing leaves an axis as it started.
    let mut axis = read.xaxis.clone();
    axis.set_time_format("%H:%M");
    axis.clear_time_format();
    assert_eq!(axis, read.xaxis);
}
