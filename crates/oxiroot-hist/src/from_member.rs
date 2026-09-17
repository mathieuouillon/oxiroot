//! [`FromMember`] for the histogram-family types, so
//! [`ObjList::items`](oxiroot_io_core::ObjList::items) and
//! [`TMap::get`](oxiroot_io_core::TMap::get) can pull histograms and graphs out
//! of a collection.

use oxiroot_io_core::error::Result;
use oxiroot_io_core::FromMember;

use crate::graph::{decode_tgraph, TGraph};
use crate::th1::{decode_th1, TH1};
use crate::th2::{decode_th2, TH2};
use crate::th3::{decode_th3, TH3};

impl FromMember for TH1 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        class
            .starts_with("TH1")
            .then(|| decode_th1((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TH2 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class.starts_with("TH2") && class != "TH2Poly")
            .then(|| decode_th2((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TH3 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        class
            .starts_with("TH3")
            .then(|| decode_th3((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TGraph {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        matches!(class, "TGraph" | "TGraphErrors" | "TGraphAsymmErrors")
            .then(|| decode_tgraph(class, class, bytes))
    }
}
