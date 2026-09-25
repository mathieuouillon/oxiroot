//! Each written object brings the streamer info its class needs
//! (`WriteRoot::streamer_classes`), and a file embeds exactly what its objects
//! bring — for types defined outside this workspace too.

use oxiroot_hist::{
    hist_streamer_classes, Efficiency, FileWriter, Graph, GraphFunction, GraphStack, Hist,
    HistStack, ObjList, ObjString, Parameter, PolyHist, ReadRoot, SparseHist, WriteRoot,
};
use oxiroot_io_core::streamer_gen::{base, basic, Cls};
use oxiroot_io_core::{Compression, FileReader, StreamerRegistry, WBuffer};

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
        oxiroot_io_core::write_object_base(&mut w, 0);
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
        .add(&Parameter::f64("lumi", 1.5))
        .add(&ObjString::new("hello").named("s"))
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
    Parameter::i32("n", 3)
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
                .add(&Parameter::f32("cut", 0.5))
                .add(&ObjString::new("x")),
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

/// The histogram-family objects, each holding what it can hold: labels, bins,
/// functions, members.
fn family() -> Vec<(&'static str, Box<dyn WriteRoot>)> {
    let mut h = Hist::reg(3, 0.0, 3.0).double().named("h");
    h.xaxis.set_label(1, "a");
    let p = Hist::reg(2, 0.0, 2.0).profile().named("p");
    let mut e = Efficiency::new(2, 0.0, 2.0).named("e");
    e.fill(true, 0.5);
    let mut sp = SparseHist::new(&[(4, 0.0, 4.0)]).named("sp");
    sp.fill(&[1.5]).unwrap();
    let mut poly = PolyHist::new(0.0, 2.0, 0.0, 2.0);
    poly.add_bin_rect(0.0, 0.0, 1.0, 1.0);
    poly.name = "poly".into();
    let line = GraphFunction::new("line", "[0]+[1]*x", vec![1.0, 2.0], 0.0, 3.0);
    let g = Graph::new(vec![1.0, 2.0], vec![3.0, 4.0])
        .unwrap()
        .named("g")
        .with_function(line);
    let st = HistStack::new()
        .named("st")
        .add(Hist::reg(2, 0.0, 2.0).float().named("m"));
    let mg = GraphStack::new().named("mg").add(g.clone());
    vec![
        ("h", Box::new(h)),
        ("p", Box::new(p)),
        ("e", Box::new(e)),
        ("sp", Box::new(sp)),
        ("poly", Box::new(poly)),
        ("g", Box::new(g)),
        ("st", Box::new(st)),
        ("mg", Box::new(mg)),
    ]
}

#[test]
fn a_histogram_family_object_describes_only_its_classes() {
    // What each object holds, and a class of the family it must not describe.
    let expected: [(&str, &[&str], &[&str]); 8] = [
        (
            "h",
            &["TH1D", "TH1", "TAxis", "THashList", "TObjString"],
            &["TH2D", "TProfile"],
        ),
        (
            "p",
            &["TProfile", "TH1D", "TH1", "TAxis"],
            &["TH2D", "TObjString"],
        ),
        ("e", &["TEfficiency", "TH1D", "TH1"], &["TH2D", "TGraph"]),
        (
            "sp",
            &[
                "THnSparseT<TArrayD>",
                "THnSparse",
                "TAxis",
                "THnSparseArrayChunk",
                "TArrayD",
            ],
            &["TH1D", "TGraph"],
        ),
        (
            "poly",
            &["TH2Poly", "TH2PolyBin", "TGraph", "TList"],
            &["TH1D", "TProfile"],
        ),
        (
            "g",
            &["TGraph", "TF1", "TFormula", "TList"],
            &["TH2D", "TProfile"],
        ),
        ("st", &["THStack", "TH1F", "TList"], &["TH2D", "TGraph"]),
        (
            "mg",
            &["TMultiGraph", "TGraph", "TF1"],
            &["TH2D", "TProfile"],
        ),
    ];
    let dir = std::env::temp_dir();
    for ((name, object), (key, holds, lacks)) in family().iter().zip(expected) {
        assert_eq!(*name, key);
        let path = dir.join(format!("oxiroot_sc_family_{name}.root"));
        FileWriter::create(&path)
            .add(&**object)
            .write(Compression::None)
            .unwrap();
        let reg = registry(&path);
        let classes = reg.class_names();
        for class in holds {
            assert!(classes.contains(class), "{name}: no {class} in {classes:?}");
        }
        for class in lacks {
            assert!(!classes.contains(class), "{name}: {class} in {classes:?}");
        }
        assert!(
            std::fs::metadata(&path).unwrap().len() < 36_000,
            "{name}: the whole 38 KB family list"
        );
    }
}

#[test]
fn nothing_a_written_object_holds_goes_undescribed() {
    // The generic reader decodes an object from the file's streamer info alone,
    // so an undescribed class surfaces as an unsupported member.
    let dir = std::env::temp_dir();
    for (name, object) in family() {
        let path = dir.join(format!("oxiroot_sc_decode_{name}.root"));
        FileWriter::create(&path)
            .add(&*object)
            .write(Compression::None)
            .unwrap();
        let value = FileReader::open(&path).unwrap().get_value(name).unwrap();
        let text = value.to_string();
        assert!(!text.contains("has no TStreamerInfo"), "{name}: {text}");
    }
}

#[test]
fn the_captured_list_gives_a_class_after_its_dependencies() {
    let classes = hist_streamer_classes(&["TH1D"]);
    let names: Vec<&str> = classes.iter().map(|c| &*c.name).collect();
    let at = |class: &str| names.iter().position(|n| *n == class).unwrap();
    assert_eq!(names.last(), Some(&"TH1D"));
    assert!(at("TObject") < at("TNamed") && at("TNamed") < at("TH1"));
    assert!(at("TAxis") < at("TH1"));

    // A class the list does not describe gives nothing, and a class two
    // requests share is described once.
    assert!(hist_streamer_classes(&["NoSuchClass"]).is_empty());
    let both = hist_streamer_classes(&["TH1D", "TH1F"]);
    assert_eq!(both.iter().filter(|c| c.name == "TH1").count(), 1);
}
