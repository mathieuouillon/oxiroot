//! Each written object brings the streamer info its class needs
//! (`WriteRoot::streamer_classes` / `streamer_blob`), and a file embeds exactly
//! what its objects bring — for types defined outside this workspace too.

use oxiroot_hist::{FileWriter, Hist, ObjList, ReadRoot, TObjString, TParameter, WriteRoot};
use oxiroot_io_core::buffer::WBuffer;
use oxiroot_io_core::streamer_gen::{base, basic, Cls};
use oxiroot_io_core::{Compression, FileReader, StreamerRegistry};

/// A class this workspace knows nothing about: `struct Point : TObject { double x, y; }`.
struct Point {
    name: String,
    x: f64,
    y: f64,
}

impl WriteRoot for Point {
    fn root_class(&self) -> String {
        "Point".into()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(1);
        oxiroot_io_core::write_tobject(&mut w, 0);
        w.be_f64(self.x);
        w.be_f64(self.y);
        w.end_object(obj);
        w.into_vec()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        vec![Cls {
            name: "Point".into(),
            version: 1,
            checksum: 0x1234_5678,
            elements: vec![
                base("TObject", 1),
                basic("x", 8, 8, "double"),
                basic("y", 8, 8, "double"),
            ],
        }]
    }
}

fn point(name: &str) -> Point {
    Point {
        name: name.into(),
        x: 1.5,
        y: -2.5,
    }
}

fn registry(path: &std::path::Path) -> StreamerRegistry {
    FileReader::open(path).unwrap().streamer_registry().unwrap()
}

#[test]
fn a_foreign_class_is_described_wherever_it_is_stored() {
    let dir = std::env::temp_dir();
    let mut h = Hist::reg(2, 0.0, 2.0).double().named("h");
    h.fill(0.5);

    let top = dir.join("oxiroot_sc_top.root");
    let subdir = dir.join("oxiroot_sc_dir.root");
    let list = dir.join("oxiroot_sc_list.root");
    FileWriter::create(&top)
        .add(&h)
        .add(&point("p"))
        .write(Compression::None)
        .unwrap();
    FileWriter::create(&subdir)
        .add(&h)
        .dir("d", |d| d.add(&point("p")))
        .write(Compression::None)
        .unwrap();
    FileWriter::create(&list)
        .add(&ObjList::list().named("l").add(&point("p")))
        .write(Compression::None)
        .unwrap();

    for path in [&top, &subdir, &list] {
        let registry = registry(path);
        let info = registry
            .get("Point")
            .unwrap_or_else(|| panic!("{}: Point is described", path.display()));
        assert_eq!(info.checksum, 0x1234_5678);
        assert_eq!(info.elements.len(), 3);
    }
    // Next to histograms, the captured histogram list is embedded too.
    assert!(registry(&top).get("TH1D").is_some());
    assert!(registry(&list).get("TH1D").is_none());
}

#[test]
fn a_file_without_histograms_does_not_carry_their_streamer_info() {
    let path = std::env::temp_dir().join("oxiroot_sc_param.root");
    FileWriter::create(&path)
        .add(&TParameter::f64("lumi", 1.5))
        .add(&TObjString::new("hello").named("s"))
        .write(Compression::None)
        .unwrap();
    let names: Vec<String> = registry(&path)
        .class_names()
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(names, ["TParameter<double>", "TObjString"]);
    assert!(
        std::fs::metadata(&path).unwrap().len() < 2_000,
        "no 38 KB histogram list"
    );

    // The same holds for the single-object shorthand.
    let single = std::env::temp_dir().join("oxiroot_sc_param_single.root");
    TParameter::i32("n", 3)
        .write_root(&single, Compression::None)
        .unwrap();
    let names = registry(&single);
    assert_eq!(names.class_names(), ["TParameter<int>"]);
}

#[test]
fn a_collection_read_back_still_describes_its_members() {
    let dir = std::env::temp_dir();
    let first = dir.join("oxiroot_sc_list_a.root");
    FileWriter::create(&first)
        .add(
            &ObjList::list()
                .named("l")
                .add(&TParameter::f32("cut", 0.5))
                .add(&TObjString::new("x")),
        )
        .write(Compression::None)
        .unwrap();

    // A list read from a file keeps only its members' bytes; writing it again
    // still describes the parameter class by name.
    let list = ObjList::read_root(&FileReader::open(&first).unwrap(), "l").unwrap();
    let second = dir.join("oxiroot_sc_list_b.root");
    FileWriter::create(&second)
        .add(&list)
        .write(Compression::None)
        .unwrap();
    let names = registry(&second);
    assert!(names.get("TParameter<float>").is_some());
    assert!(names.get("TObjString").is_some());
}
