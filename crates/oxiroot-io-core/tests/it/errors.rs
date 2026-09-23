//! The typed errors a caller can match on: a missing key or subdirectory, a key
//! holding another class, invalid input and an unsupported URL scheme.

use oxiroot_io_core::{
    object_bytes_any, Compression, Error, FileReader, FileWriter, ObjList, ParamValue, ReadRoot,
    TObjString, TParameter,
};

/// A file with a string `s` and a list `l` at the top, and a string `t` in the
/// subdirectory `sub`.
fn file() -> FileReader {
    let s = TObjString::new("hello").named("s");
    let l = ObjList::list()
        .named("l")
        .add(&TObjString::new("a").named("a"));
    let t = TObjString::new("there").named("t");
    let bytes = FileWriter::create("unused.root")
        .add(&s)
        .add(&l)
        .dir("sub", |d| d.add(&t))
        .to_bytes(Compression::None)
        .unwrap();
    FileReader::from_bytes(bytes).unwrap()
}

#[test]
fn a_missing_key_or_subdirectory_is_not_found() {
    let f = file();
    let err = object_bytes_any(&f, "nope").unwrap_err();
    assert_eq!(
        err,
        Error::NotFound {
            what: "key",
            name: "nope".into()
        }
    );
    assert_eq!(err.to_string(), "no key named \"nope\"");

    let err = f.subdir("elsewhere").unwrap_err();
    assert!(
        matches!(&err, Error::NotFound { what: "subdirectory", name } if name == "elsewhere"),
        "{err:?}"
    );

    // A key in a subdirectory is named by its path.
    let err = f.object_in("sub", "nope").unwrap_err();
    assert!(
        matches!(&err, Error::NotFound { what: "key", name } if name == "sub/nope"),
        "{err:?}"
    );
    assert_eq!(f.object_in("sub", "t").unwrap().0, "TObjString");
}

#[test]
fn a_key_holding_another_class_is_the_wrong_class() {
    let err = TObjString::read_root(&file(), "l").unwrap_err();
    assert_eq!(
        err,
        Error::WrongClass {
            name: "l".into(),
            found: "TList".into(),
            expected: "TObjString".into()
        }
    );
    assert_eq!(err.to_string(), "key \"l\" is a TList, not a TObjString");
}

#[test]
fn invalid_input_says_so() {
    let err = file().subdir("").unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");

    let err = FileWriter::create("unused.root")
        .add(&TObjString::new("no name"))
        .to_bytes(Compression::None)
        .unwrap_err();
    assert!(
        matches!(&err, Error::InvalidInput(m) if m.contains("unnamed")),
        "{err:?}"
    );
}

#[cfg(any(feature = "http", feature = "xrootd"))]
#[test]
fn a_url_scheme_without_a_reader_is_unsupported() {
    // Checked before any connection is made.
    let Err(err) = FileReader::open_url("ftp://example.org/f.root") else {
        panic!("an ftp:// URL opened");
    };
    assert!(
        matches!(&err, Error::Unsupported(m) if m.contains("ftp://")),
        "{err:?}"
    );
}

#[test]
fn context_prefixes_a_message_and_keeps_the_variant() {
    let err = Error::Format("frame size 3 too small".into()).context("reading \"n\"");
    assert_eq!(
        err,
        Error::Format("reading \"n\": frame size 3 too small".into())
    );
    let err = Error::SchemaChanged {
        detail: "input #1 is missing branch \"pt\"".into(),
    }
    .context("concat_trees");
    assert!(
        matches!(&err, Error::SchemaChanged { detail } if detail.starts_with("concat_trees: input #1")),
        "{err:?}"
    );

    // A structured error keeps its fields, untouched.
    let missing = Error::NotFound {
        what: "branch",
        name: "pt".into(),
    };
    assert_eq!(missing.clone().context("reading"), missing);
}

#[test]
fn typed_errors_display_what_went_wrong() {
    let cases = [
        (
            Error::UnsupportedVersion {
                class: "TAxis".into(),
                version: 5,
            },
            "TAxis class version 5 is not supported",
        ),
        (
            Error::MissingStreamerInfo {
                class: "TTree".into(),
            },
            "the file has no TStreamerInfo for TTree",
        ),
        (
            Error::ChecksumMismatch {
                what: "RNTuple page".into(),
                computed: 1,
                stored: 2,
            },
            "RNTuple page checksum mismatch: computed 0x0000000000000001, stored \
             0x0000000000000002",
        ),
        (
            Error::WrongClass {
                name: String::new(),
                found: "TList".into(),
                expected: "TMap".into(),
            },
            "the object is a TList, not a TMap",
        ),
    ];
    for (err, text) in cases {
        assert_eq!(err.to_string(), text);
    }
}

#[test]
fn parameters_of_different_types_do_not_add() {
    let mut lumi = TParameter::f64("lumi", 1.5);
    lumi.add(&TParameter::f64("lumi", 2.0)).unwrap();
    assert_eq!(lumi.value(), ParamValue::Double(3.5));

    let err = lumi.add(&TParameter::i32("n", 1)).unwrap_err();
    assert!(
        matches!(&err, Error::InvalidInput(m) if m.contains("TParameter<int>")),
        "{err:?}"
    );
    assert_eq!(lumi.value(), ParamValue::Double(3.5), "unchanged");
}
