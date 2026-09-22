//! A written function describes its own classes: ROOT's captured `TF1` with its
//! `TFormula` and bases, then the generated `TF2`/`TF3`, and none of the rest of
//! the histogram family.

use oxiroot_hist::FileWriter;
use oxiroot_hist_func::TF2;
use oxiroot_io_core::{Compression, FileReader};

#[test]
fn a_function_file_describes_the_function_classes_only() {
    let path = std::env::temp_dir().join("oxiroot_tf_streamer_info.root");
    let f = TF2::new("f2", "[0]*x*y", 0.0, 1.0, 0.0, 1.0).unwrap();
    FileWriter::create(&path)
        .add(&f)
        .write(Compression::None)
        .unwrap();
    let file = FileReader::open(&path).unwrap();
    let reg = file.streamer_registry().unwrap();
    let classes = reg.class_names();
    for class in [
        "TF2",
        "TF1",
        "TFormula",
        "TF1Parameters",
        "TAttLine",
        "TNamed",
    ] {
        assert!(classes.contains(&class), "no {class} in {classes:?}");
    }
    for class in ["TH2D", "TProfile", "TGraph", "TEfficiency"] {
        assert!(!classes.contains(&class), "{class} in {classes:?}");
    }
    // `TF1` is described once: the captured description, not a second one.
    assert_eq!(classes.iter().filter(|c| **c == "TF1").count(), 1);

    // Everything the function holds is described.
    let text = file.get_value("f2").unwrap().to_string();
    assert!(!text.contains("has no TStreamerInfo"), "{text}");
}
