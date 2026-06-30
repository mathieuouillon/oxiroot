//! The `TStreamerInfo` for the `TTree` class hierarchy.
//!
//! A written file embeds a `TList<TStreamerInfo>` describing every class it
//! stores, so it is self-describing (uproot relies on it; the streamer-driven
//! reader in [`crate::reader`] walks it). The generic serializer lives in
//! [`oxiroot_io_core::streamer_gen`]; this module only supplies the declarative
//! class table for `TTree` and its members (the same class versions, checksums,
//! and member layouts ROOT writes — confirmed by reading the result back with
//! ROOT, uproot, and this crate).

use oxiroot_io_core::streamer_gen::{
    any, base, basic, basicptr, object, objptr, streamer_info_list, strf, Cls,
};

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

/// Serialize the canonical `TList<TStreamerInfo>` object body (no key header),
/// describing the whole `TTree` class hierarchy. Every written tree embeds it so
/// the file is self-describing.
pub(crate) fn tree_streamer_info() -> Vec<u8> {
    streamer_info_list(&classes())
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
