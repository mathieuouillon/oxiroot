#![no_main]
//! Fuzz the typed object readers (histograms, profiles, graphs, functions,
//! collections): every reader must reject malformed bytes, never panic.
use libfuzzer_sys::fuzz_target;
use oxiroot_hist::{
    ObjList, ReadRoot, TEfficiency, TGraph, TGraph2D, TGraphMultiErrors, TH2Poly, THStack,
    THnSparse, TMap, TMultiGraph, TObjString, TParameter, TProfile, TProfile2D, TProfile3D, TH1,
    TH2, TH3,
};
use oxiroot_hist_func::{TF1, TF2, TF3};
use oxiroot_io_core::RFile;

/// Try every typed reader on `name`; each checks the key's class first, so the
/// matching one does the real parsing.
fn read_all(f: &RFile, name: &str) {
    macro_rules! try_read {
        ($($t:ty),+ $(,)?) => {$( let _ = <$t>::read_root(f, name); )+};
    }
    try_read!(
        TH1, TH2, TH3, TProfile, TProfile2D, TProfile3D, TEfficiency, THnSparse, TH2Poly,
        TGraph, TGraph2D, TGraphMultiErrors, TF1, TF2, TF3, ObjList, TMap, THStack, TMultiGraph,
        TObjString, TParameter,
    );
}

fuzz_target!(|data: &[u8]| {
    let Ok(f) = RFile::from_bytes(data.to_vec()) else {
        return;
    };
    let keys: Vec<(String, String)> = f
        .keys()
        .iter()
        .take(8)
        .map(|k| (k.name.clone(), k.class_name.clone()))
        .collect();
    for (name, class) in &keys {
        read_all(&f, name);
        if class == "TDirectory" || class == "TDirectoryFile" {
            let _ = TH1::read_root_in(&f, name, "h");
        }
    }
});
