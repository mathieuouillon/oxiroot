//! Generating the `TStreamerInfo` for the `TTree` class hierarchy.
//!
//! A written file embeds a `TList<TStreamerInfo>` describing every class it
//! stores, so it is self-describing (uproot relies on it; the streamer-driven
//! reader in [`crate::reader`] walks it). Rather than ship the two `TStreamerInfo`
//! blobs ROOT/uproot once produced as baked binaries, this module emits one
//! canonical list from a declarative table — the same class versions, checksums,
//! and member layouts ROOT writes (confirmed by reading the result back with
//! ROOT, uproot, and this crate).
//!
//! The serialization mirrors [`crate::reader`]'s parser in reverse: a
//! `TList<TStreamerInfo>`, each `TStreamerInfo` v10 wrapping a `TObjArray` v3 of
//! `TStreamerElement` subclasses (every object written with `kNewClassTag`).

use oxiroot_io_core::buffer::{Patch, WBuffer};
use oxiroot_io_core::streamer::{write_tnamed, write_tobject};

/// ROOT `fType` codes for an object/string member and the base-class slots.
const K_TOBJECT: i32 = 66;
const K_TNAMED: i32 = 67;
const K_OBJECT: i32 = 61; // an inline object member (e.g. `TObjArray fBranches`)
const K_ANY: i32 = 62; // an inline non-`TObject` member (e.g. `TIOFeatures`)
const K_OBJECT_PTR: i32 = 64; // an object pointer member
const K_TSTRING: i32 = 65;

/// One member (or base class) to serialize into a `TStreamerInfo`.
struct El {
    name: &'static str,
    /// ROOT `fType` code.
    ty: i32,
    /// In-memory size (`fSize`).
    size: i32,
    /// C++ type name (`fTypeName`); `"BASE"` for a base class.
    type_name: &'static str,
    kind: Ek,
}

/// Which `TStreamerElement` subclass an [`El`] is, plus its subclass-specific
/// tail (a base class's referenced version; a `//[fCount]` pointer's counter).
enum Ek {
    Base(i32),
    Basic,
    Str,
    Object,
    Any,
    ObjectPtr,
    BasicPtr(&'static str),
}

/// One class's `TStreamerInfo`: name, on-disk version, ROOT checksum, members.
struct Cls {
    name: &'static str,
    version: i32,
    checksum: u32,
    elements: Vec<El>,
}

fn base(name: &'static str, base_version: i32) -> El {
    let ty = match name {
        "TObject" => K_TOBJECT,
        "TNamed" => K_TNAMED,
        _ => 0, // kBase
    };
    El {
        name,
        ty,
        size: 0,
        type_name: "BASE",
        kind: Ek::Base(base_version),
    }
}
fn basic(name: &'static str, ty: i32, size: i32, type_name: &'static str) -> El {
    El {
        name,
        ty,
        size,
        type_name,
        kind: Ek::Basic,
    }
}
fn strf(name: &'static str) -> El {
    El {
        name,
        ty: K_TSTRING,
        size: 24,
        type_name: "TString",
        kind: Ek::Str,
    }
}
fn object(name: &'static str, type_name: &'static str) -> El {
    El {
        name,
        ty: K_OBJECT,
        size: 64,
        type_name,
        kind: Ek::Object,
    }
}
fn any(name: &'static str, size: i32, type_name: &'static str) -> El {
    El {
        name,
        ty: K_ANY,
        size,
        type_name,
        kind: Ek::Any,
    }
}
fn objptr(name: &'static str, type_name: &'static str) -> El {
    El {
        name,
        ty: K_OBJECT_PTR,
        size: 8,
        type_name,
        kind: Ek::ObjectPtr,
    }
}
fn basicptr(
    name: &'static str,
    ty: i32,
    size: i32,
    type_name: &'static str,
    count: &'static str,
) -> El {
    El {
        name,
        ty,
        size,
        type_name,
        kind: Ek::BasicPtr(count),
    }
}

/// A `TLeaf<T>` subclass: the `TLeaf` base then `fMinimum`/`fMaximum` of the
/// element type. `min_ty`/`min_size`/`min_tn` describe that element type.
fn leaf_subclass(
    name: &'static str,
    checksum: u32,
    min_ty: i32,
    min_size: i32,
    min_tn: &'static str,
) -> Cls {
    Cls {
        name,
        version: 1,
        checksum,
        elements: vec![
            base("TLeaf", 2),
            basic("fMinimum", min_ty, min_size, min_tn),
            basic("fMaximum", min_ty, min_size, min_tn),
        ],
    }
}

/// The canonical class list, in dependency order (bases before the classes that
/// use them, as ROOT writes). Checksums and versions are ROOT's own values.
fn classes() -> Vec<Cls> {
    vec![
        Cls {
            name: "TObject",
            version: 1,
            checksum: 2417737773,
            elements: vec![
                basic("fUniqueID", 13, 4, "unsigned int"),
                basic("fBits", 15, 4, "unsigned int"),
            ],
        },
        Cls {
            name: "TString",
            version: 2,
            checksum: 95257,
            elements: vec![],
        },
        Cls {
            name: "TNamed",
            version: 1,
            checksum: 3753331260,
            elements: vec![base("TObject", 1), strf("fName"), strf("fTitle")],
        },
        Cls {
            name: "TCollection",
            version: 3,
            checksum: 1474546588,
            elements: vec![
                base("TObject", 1),
                strf("fName"),
                basic("fSize", 3, 4, "int"),
            ],
        },
        Cls {
            name: "TSeqCollection",
            version: 0,
            checksum: 4234951622,
            elements: vec![base("TCollection", 3)],
        },
        Cls {
            name: "TObjArray",
            version: 3,
            checksum: 2845730130,
            elements: vec![
                base("TSeqCollection", 0),
                basic("fLowerBound", 3, 4, "int"),
                basic("fLast", 3, 4, "int"),
            ],
        },
        Cls {
            name: "TList",
            version: 5,
            checksum: 1774568379,
            elements: vec![base("TSeqCollection", 0)],
        },
        Cls {
            name: "TAttLine",
            version: 2,
            checksum: 2483504457,
            elements: vec![
                basic("fLineColor", 2, 2, "short"),
                basic("fLineStyle", 2, 2, "short"),
                basic("fLineWidth", 2, 2, "short"),
            ],
        },
        Cls {
            name: "TAttFill",
            version: 2,
            checksum: 4292422290,
            elements: vec![
                basic("fFillColor", 2, 2, "short"),
                basic("fFillStyle", 2, 2, "short"),
            ],
        },
        Cls {
            name: "TAttMarker",
            version: 2,
            checksum: 689802220,
            elements: vec![
                basic("fMarkerColor", 2, 2, "short"),
                basic("fMarkerStyle", 2, 2, "short"),
                basic("fMarkerSize", 5, 4, "float"),
            ],
        },
        Cls {
            name: "ROOT::TIOFeatures",
            version: 1,
            checksum: 446770960,
            elements: vec![basic("fIOBits", 11, 1, "unsigned char")],
        },
        Cls {
            name: "TLeaf",
            version: 2,
            checksum: 1830715730,
            elements: vec![
                base("TNamed", 1),
                basic("fLen", 3, 4, "int"),
                basic("fLenType", 3, 4, "int"),
                basic("fOffset", 3, 4, "int"),
                basic("fIsRange", 18, 1, "bool"),
                basic("fIsUnsigned", 18, 1, "bool"),
                objptr("fLeafCount", "TLeaf*"),
            ],
        },
        leaf_subclass("TLeafO", 44976339, 18, 1, "bool"),
        leaf_subclass("TLeafB", 253643614, 1, 1, "char"),
        leaf_subclass("TLeafS", 353169103, 2, 2, "short"),
        leaf_subclass("TLeafI", 2120920601, 3, 4, "int"),
        leaf_subclass("TLeafL", 3727820898, 16, 8, "Long64_t"),
        leaf_subclass("TLeafF", 987602290, 5, 4, "float"),
        leaf_subclass("TLeafD", 294553462, 8, 8, "double"),
        leaf_subclass("TLeafC", 4226003699, 3, 4, "int"),
        Cls {
            name: "TLeafElement",
            version: 1,
            checksum: 2689566867,
            elements: vec![
                base("TLeaf", 2),
                basic("fID", 3, 4, "int"),
                basic("fType", 3, 4, "int"),
            ],
        },
        Cls {
            name: "TBranch",
            version: 13,
            checksum: 278366892,
            elements: vec![
                base("TNamed", 1),
                base("TAttFill", 2),
                basic("fCompress", 3, 4, "int"),
                basic("fBasketSize", 3, 4, "int"),
                basic("fEntryOffsetLen", 3, 4, "int"),
                basic("fWriteBasket", 3, 4, "int"),
                basic("fEntryNumber", 16, 8, "Long64_t"),
                any("fIOFeatures", 1, "ROOT::TIOFeatures"),
                basic("fOffset", 3, 4, "int"),
                basic("fMaxBaskets", 6, 4, "int"),
                basic("fSplitLevel", 3, 4, "int"),
                basic("fEntries", 16, 8, "Long64_t"),
                basic("fFirstEntry", 16, 8, "Long64_t"),
                basic("fTotBytes", 16, 8, "Long64_t"),
                basic("fZipBytes", 16, 8, "Long64_t"),
                object("fBranches", "TObjArray"),
                object("fLeaves", "TObjArray"),
                object("fBaskets", "TObjArray"),
                basicptr("fBasketBytes", 43, 4, "int*", "fMaxBaskets"),
                basicptr("fBasketEntry", 56, 8, "Long64_t*", "fMaxBaskets"),
                basicptr("fBasketSeek", 56, 8, "Long64_t*", "fMaxBaskets"),
                strf("fFileName"),
            ],
        },
        Cls {
            name: "TBranchElement",
            version: 10,
            checksum: 3880738403,
            elements: vec![
                base("TBranch", 13),
                strf("fClassName"),
                strf("fParentName"),
                strf("fClonesName"),
                basic("fCheckSum", 13, 4, "unsigned int"),
                basic("fClassVersion", 2, 2, "short"),
                basic("fID", 3, 4, "int"),
                basic("fType", 3, 4, "int"),
                basic("fStreamerType", 3, 4, "int"),
                basic("fMaximum", 3, 4, "int"),
                objptr("fBranchCount", "TBranchElement*"),
                objptr("fBranchCount2", "TBranchElement*"),
            ],
        },
        Cls {
            name: "TTree",
            version: 20,
            checksum: 1919213695,
            elements: vec![
                base("TNamed", 1),
                base("TAttLine", 2),
                base("TAttFill", 2),
                base("TAttMarker", 2),
                basic("fEntries", 16, 8, "Long64_t"),
                basic("fTotBytes", 16, 8, "Long64_t"),
                basic("fZipBytes", 16, 8, "Long64_t"),
                basic("fSavedBytes", 16, 8, "Long64_t"),
                basic("fFlushedBytes", 16, 8, "Long64_t"),
                basic("fWeight", 8, 8, "double"),
                basic("fTimerInterval", 3, 4, "int"),
                basic("fScanField", 3, 4, "int"),
                basic("fUpdate", 3, 4, "int"),
                basic("fDefaultEntryOffsetLen", 3, 4, "int"),
                basic("fNClusterRange", 6, 4, "int"),
                basic("fMaxEntries", 16, 8, "Long64_t"),
                basic("fMaxEntryLoop", 16, 8, "Long64_t"),
                basic("fMaxVirtualSize", 16, 8, "Long64_t"),
                basic("fAutoSave", 16, 8, "Long64_t"),
                basic("fAutoFlush", 16, 8, "Long64_t"),
                basic("fEstimate", 16, 8, "Long64_t"),
                basicptr("fClusterRangeEnd", 56, 8, "Long64_t*", "fNClusterRange"),
                basicptr("fClusterSize", 56, 8, "Long64_t*", "fNClusterRange"),
                any("fIOFeatures", 1, "ROOT::TIOFeatures"),
                object("fBranches", "TObjArray"),
                object("fLeaves", "TObjArray"),
                objptr("fAliases", "TList*"),
                any("fIndexValues", 24, "TArrayD"),
                any("fIndex", 24, "TArrayI"),
                objptr("fTreeIndex", "TVirtualIndex*"),
                objptr("fFriends", "TList*"),
                objptr("fUserInfo", "TList*"),
                objptr("fBranchRef", "TBranchRef*"),
            ],
        },
    ]
}

/// `fBits` ROOT writes for the embedded `TStreamerInfo`'s `TNamed`.
const SI_BITS: u32 = 0x0001_0000;

/// The byte-count + `kNewClassTag` + class-name header ROOT writes before each
/// streamed object (`TStreamerInfo`, `TObjArray`, every `TStreamerElement`).
fn begin_object_any(w: &mut WBuffer, class: &str) -> Patch {
    let bc = w.reserve(4);
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0); // NUL terminator
    bc
}
fn end_object_any(w: &mut WBuffer, bc: Patch) {
    let inner = (w.len() - w.patch_offset(bc) - 4) as u32;
    w.patch_be_u32(bc, inner | 0x4000_0000);
}

/// Write the `TStreamerElement` v4 base common to every element subclass.
fn write_element_base(w: &mut WBuffer, el: &El) {
    let se = w.begin_object(4); // TStreamerElement v4
    write_tnamed(w, 0, el.name, "");
    w.be_i32(el.ty); // fType
    w.be_i32(el.size); // fSize
    w.be_i32(0); // fArrayLength
    w.be_i32(0); // fArrayDim
    for _ in 0..5 {
        w.be_i32(0); // fMaxIndex[5]
    }
    w.string(el.type_name); // fTypeName
    w.end_object(se);
}

/// Write one element as its `TStreamerElement` subclass (`TStreamerBase`,
/// `TStreamerBasicType`, …), wrapping the common base with the subclass tail.
/// `owner`/`owner_version` name the class that declares the element (used for a
/// `//[fCount]` pointer's `fCountClass`/`fCountVersion`).
fn write_element(w: &mut WBuffer, el: &El, owner: &str, owner_version: i32) {
    let (class, version) = match el.kind {
        Ek::Base(_) => ("TStreamerBase", 3),
        Ek::Basic => ("TStreamerBasicType", 2),
        Ek::Str => ("TStreamerString", 2),
        Ek::Object => ("TStreamerObject", 2),
        Ek::Any => ("TStreamerObjectAny", 2),
        Ek::ObjectPtr => ("TStreamerObjectPointer", 2),
        Ek::BasicPtr(_) => ("TStreamerBasicPointer", 2),
    };
    let bc = begin_object_any(w, class);
    let sub = w.begin_object(version);
    write_element_base(w, el);
    match el.kind {
        Ek::Base(base_version) => w.be_i32(base_version), // fBaseVersion
        Ek::BasicPtr(count_name) => {
            w.be_i32(owner_version); // fCountVersion
            w.string(count_name); // fCountName
            w.string(owner); // fCountClass
        }
        _ => {}
    }
    w.end_object(sub);
    end_object_any(w, bc);
}

/// Serialize the canonical `TList<TStreamerInfo>` object body (no key header),
/// describing the whole `TTree` class hierarchy. This replaces the baked blobs;
/// every written tree embeds it so the file is self-describing.
pub(crate) fn tree_streamer_info() -> Vec<u8> {
    let classes = classes();
    let mut w = WBuffer::new();

    let list = w.begin_object(5); // TList v5
    write_tobject(&mut w, 0);
    w.string(""); // fName
    w.be_i32(classes.len() as i32); // nobjects

    for cls in &classes {
        let info_bc = begin_object_any(&mut w, "TStreamerInfo");
        let si = w.begin_object(10); // TStreamerInfo v10
        write_tnamed(&mut w, SI_BITS, cls.name, "");
        w.be_u32(cls.checksum);
        w.be_i32(cls.version);

        let oa_bc = begin_object_any(&mut w, "TObjArray");
        let oa = w.begin_object(3); // TObjArray v3
        write_tobject(&mut w, 0);
        w.string(""); // fName
        w.be_i32(cls.elements.len() as i32);
        w.be_i32(0); // fLowerBound
        for el in &cls.elements {
            write_element(&mut w, el, cls.name, cls.version);
        }
        w.end_object(oa);
        end_object_any(&mut w, oa_bc);

        w.end_object(si);
        end_object_any(&mut w, info_bc);
        w.string(""); // the TList option string for this entry
    }

    w.end_object(list);
    w.into_vec()
}

#[cfg(test)]
mod tests {
    use oxiroot_io_core::streamer_info::parse_streamer_info;

    /// The generated bytes parse back (via the same parser the reader uses) to a
    /// registry with the right classes, versions, checksums, and member layouts —
    /// the contract the embedded streamer info must satisfy. (No back-references
    /// are emitted, so the key length is irrelevant here.)
    #[test]
    fn generated_streamer_info_round_trips() {
        let bytes = super::tree_streamer_info();
        let reg = parse_streamer_info(&bytes, 0).expect("parse generated streamer info");
        assert_eq!(reg.infos().len(), 24, "class count");

        let tree = reg.get("TTree").expect("TTree");
        assert_eq!(tree.class_version, 20);
        assert_eq!(tree.checksum, 1919213695);
        assert_eq!(tree.elements.len(), 33);

        let branch = reg.get("TBranch").expect("TBranch");
        assert_eq!(branch.class_version, 13);
        assert_eq!(branch.checksum, 278366892);
        // The counted basket arrays carry their counter name, so the reader can
        // size them.
        let seek = branch
            .elements
            .iter()
            .find(|e| e.name == "fBasketSeek")
            .expect("fBasketSeek");
        assert_eq!(seek.el_type, 56);
        assert_eq!(seek.count_name.as_deref(), Some("fMaxBaskets"));

        // The TNamed base of TBranch is recorded with its base version.
        let named_base = &branch.elements[0];
        assert_eq!(named_base.name, "TNamed");
        assert_eq!(named_base.base_version, Some(1));

        let be = reg.get("TBranchElement").expect("TBranchElement");
        assert_eq!(be.class_version, 10);
        assert_eq!(be.checksum, 3880738403);
        assert_eq!(be.elements[0].name, "TBranch"); // its base
        assert_eq!(be.elements[1].name, "fClassName");

        for leaf in ["TLeafI", "TLeafD", "TLeafC", "TLeafElement"] {
            assert!(reg.get(leaf).is_some(), "missing {leaf}");
        }
    }
}
