//! Manifest-driven interop **matrix** writer — the broad cross-check that
//! complements the lean canonical [`interop`](interop.rs) harness.
//!
//!   cargo run -p oxiroot --example interop_matrix -- write <dir>
//!
//! Writes one small `.root` file per case (every histogram precision/dimension,
//! TProfile, Sumw2, variable bins, multi-object/subdirs/append, every RNTuple
//! scalar+vector field type + multi-cluster, every TTree branch kind + scalar
//! width + split `std::vector<Struct>`) **plus `manifest.json`** describing each
//! case and its expected values. The oracle readers
//! `scripts/interop_matrix_root.cpp` (ROOT C++) and
//! `scripts/interop_matrix_uproot.py` (uproot) consume `manifest.json` and assert
//! their parse matches — so the expected values live in exactly one place.
//!
//! `cargo run … -- read-big <file>` reads back a >2 GiB oracle file (the `--big`
//! path of `scripts/interop_local.sh`).

use std::fmt::Write as _;
use std::path::Path;
use std::process::exit;

use oxiroot::prelude::*;
use oxiroot::tree::BranchValues;

// ---------------------------------------------------------------------------
// Minimal JSON value + emitter (the workspace pulls in no serde).
// ---------------------------------------------------------------------------

enum J {
    Bool(bool),
    Int(i64),
    Flt(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(&'static str, J)>),
}

impl J {
    fn render(&self, out: &mut String) {
        match self {
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Int(i) => {
                let _ = write!(out, "{i}");
            }
            J::Flt(x) => {
                if x.is_finite() {
                    // `{:?}` prints the shortest string that round-trips an f64.
                    let _ = write!(out, "{x:?}");
                } else {
                    out.push_str("null");
                }
            }
            J::Str(s) => json_string(s, out),
            J::Arr(v) => {
                out.push('[');
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    e.render(out);
                }
                out.push(']');
            }
            J::Obj(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    json_string(k, out);
                    out.push(':');
                    v.render(out);
                }
                out.push('}');
            }
        }
    }
}

fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// JSON array builders.
fn jints(v: &[i64]) -> J {
    J::Arr(v.iter().map(|&x| J::Int(x)).collect())
}
fn jflts(v: &[f64]) -> J {
    J::Arr(v.iter().map(|&x| J::Flt(x)).collect())
}
fn jflts2(v: &[Vec<f64>]) -> J {
    J::Arr(v.iter().map(|r| jflts(r)).collect())
}
fn jflts3(v: &[Vec<Vec<f64>>]) -> J {
    J::Arr(v.iter().map(|r| jflts2(r)).collect())
}
fn jbools(v: &[bool]) -> J {
    J::Arr(v.iter().map(|&x| J::Bool(x)).collect())
}
fn jstrs(v: &[&str]) -> J {
    J::Arr(v.iter().map(|s| J::Str(s.to_string())).collect())
}
fn jvi64_2(v: &[Vec<i64>]) -> J {
    J::Arr(v.iter().map(|r| jints(r)).collect())
}

// ---------------------------------------------------------------------------
// Histogram builders (set bin contents directly so any precision/value works,
// including TH*L's 2^40-scale contents that are infeasible to reach via fill()).
// ---------------------------------------------------------------------------

fn th1_with(nbins: i32, xmin: f64, xmax: f64, contents: &[f64]) -> TH1 {
    let mut h = TH1::new("h", "m", nbins, xmin, xmax);
    for (i, &c) in contents.iter().enumerate() {
        h.contents[i + 1] = c; // [0] is underflow
    }
    h.entries = contents.iter().sum();
    h
}

fn th2_with(nx: i32, ny: i32, contents: &[Vec<f64>]) -> TH2 {
    let mut h = TH2::new("h", "m", nx, 0.0, nx as f64, ny, 0.0, ny as f64);
    // ROOT cell index = ix + (nx+2)*iy, in-range ix,iy in 1..=n.
    let stride = (nx + 2) as usize;
    let mut entries = 0.0;
    for (ix, col) in contents.iter().enumerate() {
        for (iy, &c) in col.iter().enumerate() {
            h.contents[(ix + 1) + stride * (iy + 1)] = c;
            entries += c;
        }
    }
    h.entries = entries;
    h
}

fn th3_with(n: i32, contents: &[Vec<Vec<f64>>]) -> TH3 {
    let mut h = TH3::new(
        "h", "m", n, 0.0, n as f64, n, 0.0, n as f64, n, 0.0, n as f64,
    );
    let s = (n + 2) as usize;
    let mut entries = 0.0;
    for (ix, plane) in contents.iter().enumerate() {
        for (iy, col) in plane.iter().enumerate() {
            for (iz, &c) in col.iter().enumerate() {
                h.contents[(ix + 1) + s * ((iy + 1) + s * (iz + 1))] = c;
                entries += c;
            }
        }
    }
    h.entries = entries;
    h
}

/// One histogram case: write `h` with the precision writer for `class`, return
/// the manifest entry. `values`/`edges` are pulled from the in-memory histogram.
fn hist1_case(id: &'static str, class: &str, comp: Compression, h: &TH1, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let path = dir.join(&file);
    match class {
        "TH1C" => write_th1c_file(&path, h, comp),
        "TH1S" => write_th1s_file(&path, h, comp),
        "TH1I" => write_th1i_file(&path, h, comp),
        "TH1L" => write_th1l_file(&path, h, comp),
        "TH1F" => write_th1f_file(&path, h, comp),
        "TH1D" => write_th1d_file(&path, h, comp),
        _ => unreachable!(),
    }
    .unwrap_or_else(|e| die(&format!("write {id}: {e}")));

    let mut fields = vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("hist".into())),
        ("file", J::Str(file)),
        ("name", J::Str("h".into())),
        ("class", J::Str(class.into())),
        ("dim", J::Int(1)),
        ("compression", comp_str(comp)),
        ("values", jflts(h.values())),
        ("edges", J::Arr(vec![jflts(&h.edges())])),
        ("entries", J::Flt(h.entries)),
        ("uproot_skip", J::Bool(false)),
    ];
    if !h.sumw2.is_empty() {
        let n = h.values().len();
        let errs: Vec<f64> = (1..=n).map(|b| h.bin_error(b)).collect();
        fields.push(("sumw2_error", jflts(&errs)));
    }
    J::Obj(fields)
}

fn hist2_case(id: &'static str, class: &str, comp: Compression, h: &TH2, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let path = dir.join(&file);
    match class {
        "TH2C" => write_th2c_file(&path, h, comp),
        "TH2S" => write_th2s_file(&path, h, comp),
        "TH2I" => write_th2i_file(&path, h, comp),
        "TH2L" => write_th2l_file(&path, h, comp),
        "TH2F" => write_th2f_file(&path, h, comp),
        "TH2D" => write_th2d_file(&path, h, comp),
        _ => unreachable!(),
    }
    .unwrap_or_else(|e| die(&format!("write {id}: {e}")));

    let mut fields = vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("hist".into())),
        ("file", J::Str(file)),
        ("name", J::Str("h".into())),
        ("class", J::Str(class.into())),
        ("dim", J::Int(2)),
        ("compression", comp_str(comp)),
        ("values", jflts2(&h.values())),
        (
            "edges",
            J::Arr(vec![jflts(&h.xaxis.edges()), jflts(&h.yaxis.edges())]),
        ),
        ("entries", J::Flt(h.entries)),
        ("uproot_skip", J::Bool(false)),
    ];
    if !h.sumw2.is_empty() {
        fields.push(("sumw2_error", jflts2(&th2_bin_errors(h))));
    }
    J::Obj(fields)
}

fn th2_bin_errors(h: &TH2) -> Vec<Vec<f64>> {
    let nx = h.values().len();
    let ny = if nx > 0 { h.values()[0].len() } else { 0 };
    let stride = nx + 2;
    (0..nx)
        .map(|ix| {
            (0..ny)
                .map(|iy| h.bin_error((ix + 1) + stride * (iy + 1)))
                .collect()
        })
        .collect()
}

fn hist3_case(id: &'static str, class: &str, comp: Compression, h: &TH3, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let path = dir.join(&file);
    match class {
        "TH3C" => write_th3c_file(&path, h, comp),
        "TH3S" => write_th3s_file(&path, h, comp),
        "TH3I" => write_th3i_file(&path, h, comp),
        "TH3L" => write_th3l_file(&path, h, comp),
        "TH3F" => write_th3f_file(&path, h, comp),
        "TH3D" => write_th3d_file(&path, h, comp),
        _ => unreachable!(),
    }
    .unwrap_or_else(|e| die(&format!("write {id}: {e}")));

    let mut fields = vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("hist".into())),
        ("file", J::Str(file)),
        ("name", J::Str("h".into())),
        ("class", J::Str(class.into())),
        ("dim", J::Int(3)),
        ("compression", comp_str(comp)),
        ("values", jflts3(&h.values())),
        (
            "edges",
            J::Arr(vec![
                jflts(&h.xaxis.edges()),
                jflts(&h.yaxis.edges()),
                jflts(&h.zaxis.edges()),
            ]),
        ),
        ("entries", J::Flt(h.entries)),
        ("uproot_skip", J::Bool(false)),
    ];
    if !h.sumw2.is_empty() {
        let v = h.values();
        let n = v.len();
        let s = n + 2;
        let errs: Vec<Vec<Vec<f64>>> = (0..n)
            .map(|ix| {
                (0..n)
                    .map(|iy| {
                        (0..n)
                            .map(|iz| h.bin_error((ix + 1) + s * ((iy + 1) + s * (iz + 1))))
                            .collect()
                    })
                    .collect()
            })
            .collect();
        fields.push(("sumw2_error", jflts3(&errs)));
    }
    J::Obj(fields)
}

fn hist_obj_entry(name: &str, class: &str, dim: i64, values: J, edges: J) -> J {
    J::Obj(vec![
        ("name", J::Str(name.into())),
        ("class", J::Str(class.into())),
        ("dim", J::Int(dim)),
        ("values", values),
        ("edges", edges),
    ])
}

fn comp_str(c: Compression) -> J {
    match c {
        Compression::None => J::Str("none".into()),
        Compression::Zstd(_) => J::Str("zstd".into()),
        Compression::Zlib(_) => J::Str("zlib".into()),
        Compression::Lz4(_) => J::Str("lz4".into()),
    }
}

// ---------------------------------------------------------------------------
// All cases.
// ---------------------------------------------------------------------------

fn write(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&format!("mkdir {}: {e}", dir.display())));
    let none = Compression::None;
    let zstd = Compression::Zstd(5);
    let mut cases: Vec<J> = Vec::new();

    // --- Histograms: every precision × dimension (direct-set contents). ---
    cases.push(hist1_case(
        "th1c",
        "TH1C",
        none,
        &th1_with(3, 0.0, 3.0, &[1.0, 2.0, 3.0]),
        dir,
    ));
    cases.push(hist1_case(
        "th1s",
        "TH1S",
        none,
        &th1_with(3, 0.0, 3.0, &[100.0, 200.0, 300.0]),
        dir,
    ));
    cases.push(hist1_case(
        "th1i",
        "TH1I",
        none,
        &th1_with(3, 0.0, 3.0, &[1e5, 2e5, 3e5]),
        dir,
    ));
    let big = (1i64 << 40) as f64;
    cases.push(hist1_case(
        "th1l",
        "TH1L",
        none,
        &th1_with(3, 0.0, 3.0, &[big, 2.0 * big, 3.0 * big]),
        dir,
    ));
    cases.push(hist1_case(
        "th1f",
        "TH1F",
        none,
        &th1_with(3, 0.0, 3.0, &[1.5, 2.5, 3.5]),
        dir,
    ));
    cases.push(hist1_case(
        "th1d",
        "TH1D",
        none,
        &th1_with(3, 0.0, 3.0, &[1.5, 2.5, 3.5]),
        dir,
    ));
    cases.push(hist1_case(
        "th1d_zstd",
        "TH1D",
        zstd,
        &th1_with(3, 0.0, 3.0, &[1.5, 2.5, 3.5]),
        dir,
    ));

    let g2 = |s: f64| {
        vec![
            vec![1.0 * s, 2.0 * s],
            vec![3.0 * s, 4.0 * s],
            vec![5.0 * s, 6.0 * s],
        ]
    };
    cases.push(hist2_case(
        "th2c",
        "TH2C",
        none,
        &th2_with(3, 2, &g2(1.0)),
        dir,
    ));
    cases.push(hist2_case(
        "th2s",
        "TH2S",
        none,
        &th2_with(3, 2, &g2(100.0)),
        dir,
    ));
    cases.push(hist2_case(
        "th2i",
        "TH2I",
        none,
        &th2_with(3, 2, &g2(10000.0)),
        dir,
    ));
    cases.push(hist2_case(
        "th2l",
        "TH2L",
        none,
        &th2_with(3, 2, &g2(big)),
        dir,
    ));
    cases.push(hist2_case(
        "th2f",
        "TH2F",
        none,
        &th2_with(3, 2, &g2(0.5)),
        dir,
    ));
    cases.push(hist2_case(
        "th2d",
        "TH2D",
        none,
        &th2_with(3, 2, &g2(0.25)),
        dir,
    ));

    let g3 = |s: f64| {
        let mut v = 0.0;
        let mut out = vec![vec![vec![0.0; 2]; 2]; 2];
        for plane in out.iter_mut() {
            for col in plane.iter_mut() {
                for c in col.iter_mut() {
                    v += 1.0;
                    *c = v * s;
                }
            }
        }
        out
    };
    cases.push(hist3_case(
        "th3c",
        "TH3C",
        none,
        &th3_with(2, &g3(1.0)),
        dir,
    ));
    cases.push(hist3_case(
        "th3s",
        "TH3S",
        none,
        &th3_with(2, &g3(100.0)),
        dir,
    ));
    cases.push(hist3_case(
        "th3i",
        "TH3I",
        none,
        &th3_with(2, &g3(10000.0)),
        dir,
    ));
    cases.push(hist3_case(
        "th3l",
        "TH3L",
        none,
        &th3_with(2, &g3(big)),
        dir,
    ));
    cases.push(hist3_case(
        "th3f",
        "TH3F",
        none,
        &th3_with(2, &g3(0.5)),
        dir,
    ));
    cases.push(hist3_case(
        "th3d",
        "TH3D",
        none,
        &th3_with(2, &g3(0.25)),
        dir,
    ));

    // --- Variable bin edges (TH1 + TH2 only; TH3 has no new_variable). ---
    {
        let mut h = TH1::new_variable("h", "m", &[0.0, 1.0, 4.0, 10.0]);
        for (i, &c) in [2.0, 1.0, 3.0].iter().enumerate() {
            h.contents[i + 1] = c;
        }
        h.entries = 6.0;
        cases.push(hist1_case("th1d_variable", "TH1D", none, &h, dir));
    }
    {
        let mut h = TH2::new_variable("h", "m", &[0.0, 1.0, 4.0], &[0.0, 2.0, 5.0]);
        let stride = 4; // nx+2 = 2+2
        for (ix, col) in [[1.0, 2.0], [3.0, 4.0]].iter().enumerate() {
            for (iy, &c) in col.iter().enumerate() {
                h.contents[(ix + 1) + stride * (iy + 1)] = c;
            }
        }
        h.entries = 10.0;
        cases.push(hist2_case("th2d_variable", "TH2D", none, &h, dir));
    }

    // --- Sumw2 (weighted errors), 1/2/3-D. ---
    {
        let mut h = TH1::new("h", "m", 3, 0.0, 3.0);
        h.sumw2();
        h.fill_weight(0.5, 2.0);
        h.fill_weight(0.5, 3.0); // bin1: content 5, sumw2 13
        h.fill_weight(1.5, 1.0); // bin2: content 1, sumw2 1
        cases.push(hist1_case("th1d_sumw2", "TH1D", none, &h, dir));
    }
    {
        let mut h = TH2::new("h", "m", 2, 0.0, 2.0, 2, 0.0, 2.0);
        h.sumw2();
        h.fill_weight(0.5, 0.5, 2.0);
        h.fill_weight(0.5, 0.5, 1.0); // (0,0): content 3, sumw2 5
        h.fill_weight(1.5, 1.5, 4.0); // (1,1): content 4, sumw2 16
        cases.push(hist2_case("th2d_sumw2", "TH2D", none, &h, dir));
    }
    {
        let mut h = TH3::new("h", "m", 2, 0.0, 2.0, 2, 0.0, 2.0, 2, 0.0, 2.0);
        h.sumw2();
        h.fill_weight(0.5, 0.5, 0.5, 2.0);
        h.fill_weight(0.5, 0.5, 0.5, 3.0); // (0,0,0): content 5, sumw2 13
        h.fill_weight(1.5, 1.5, 1.5, 1.0); // (1,1,1): content 1, sumw2 1
        cases.push(hist3_case("th3d_sumw2", "TH3D", none, &h, dir));
    }

    // --- TProfile. ---
    {
        let mut p = TProfile::new("p", "prof", 4, 0.0, 4.0);
        p.fill(0.5, 10.0);
        p.fill(0.5, 20.0); // bin1 mean 15
        p.fill(1.5, 5.0);
        p.fill(1.5, 10.0); // bin2 mean 7.5
        p.fill(2.5, 30.0); // bin3 mean 30
        let file = "m_tprofile.root".to_string();
        write_tprofile_file(dir.join(&file), &p, none)
            .unwrap_or_else(|e| die(&format!("tprofile: {e}")));
        cases.push(J::Obj(vec![
            ("id", J::Str("tprofile".into())),
            ("kind", J::Str("profile".into())),
            ("file", J::Str(file)),
            ("name", J::Str("p".into())),
            ("class", J::Str("TProfile".into())),
            ("compression", comp_str(none)),
            ("values", jflts(&p.values())),
            ("edges", J::Arr(vec![jflts(&p.xaxis.edges())])),
            ("uproot_skip", J::Bool(false)),
        ]));
    }

    // --- Multi-object file (write_histograms_file). ---
    {
        let ha = th1_with(2, 0.0, 2.0, &[1.0, 2.0]);
        let hb = th2_with(2, 2, &[vec![3.0, 4.0], vec![5.0, 6.0]]);
        // Multi-object writer renames keys; set names.
        let mut ha = ha;
        ha.name = "ha".into();
        let mut hb = hb;
        hb.name = "hb".into();
        let file = "m_multiobj.root".to_string();
        write_histograms_file(dir.join(&file), &[Hist::Th1(&ha), Hist::Th2(&hb)], none)
            .unwrap_or_else(|e| die(&format!("multiobj: {e}")));
        cases.push(J::Obj(vec![
            ("id", J::Str("multiobj".into())),
            ("kind", J::Str("hist_multi".into())),
            ("file", J::Str(file)),
            ("compression", comp_str(none)),
            (
                "objects",
                J::Arr(vec![
                    hist_obj_entry(
                        "ha",
                        "TH1D",
                        1,
                        jflts(ha.values()),
                        J::Arr(vec![jflts(&ha.edges())]),
                    ),
                    hist_obj_entry(
                        "hb",
                        "TH2D",
                        2,
                        jflts2(&hb.values()),
                        J::Arr(vec![jflts(&hb.xaxis.edges()), jflts(&hb.yaxis.edges())]),
                    ),
                ]),
            ),
            ("uproot_skip", J::Bool(false)),
        ]));
    }

    // --- Subdirectories (write_histograms_dirs). ---
    {
        let mut htop = th1_with(1, 0.0, 1.0, &[7.0]);
        htop.name = "htop".into();
        let mut ha = th1_with(2, 0.0, 2.0, &[2.0, 3.0]);
        ha.name = "h".into();
        let mut hb = th1_with(1, 0.0, 1.0, &[4.0]);
        hb.name = "h".into();
        let file = "m_subdirs.root".to_string();
        write_histograms_dirs(
            dir.join(&file),
            &[Hist::Th1(&htop)],
            &[
                ("regionA", &[Hist::Th1(&ha)]),
                ("regionB", &[Hist::Th1(&hb)]),
            ],
            none,
        )
        .unwrap_or_else(|e| die(&format!("subdirs: {e}")));
        cases.push(J::Obj(vec![
            ("id", J::Str("subdirs".into())),
            ("kind", J::Str("hist_dirs".into())),
            ("file", J::Str(file)),
            ("compression", comp_str(none)),
            (
                "root_objects",
                J::Arr(vec![hist_obj_entry(
                    "htop",
                    "TH1D",
                    1,
                    jflts(htop.values()),
                    J::Arr(vec![jflts(&htop.edges())]),
                )]),
            ),
            (
                "dirs",
                J::Arr(vec![
                    J::Obj(vec![
                        ("dir", J::Str("regionA".into())),
                        (
                            "objects",
                            J::Arr(vec![hist_obj_entry(
                                "h",
                                "TH1D",
                                1,
                                jflts(ha.values()),
                                J::Arr(vec![jflts(&ha.edges())]),
                            )]),
                        ),
                    ]),
                    J::Obj(vec![
                        ("dir", J::Str("regionB".into())),
                        (
                            "objects",
                            J::Arr(vec![hist_obj_entry(
                                "h",
                                "TH1D",
                                1,
                                jflts(hb.values()),
                                J::Arr(vec![jflts(&hb.edges())]),
                            )]),
                        ),
                    ]),
                ]),
            ),
            ("uproot_skip", J::Bool(false)),
        ]));
    }

    // --- Append / update mode. ---
    {
        let mut h1 = th1_with(1, 0.0, 1.0, &[1.0]);
        h1.name = "h1".into();
        let mut h2 = th1_with(2, 0.0, 2.0, &[2.0, 3.0]);
        h2.name = "h2".into();
        let file = "m_append.root".to_string();
        let path = dir.join(&file);
        write_histograms_file(&path, &[Hist::Th1(&h1)], none)
            .unwrap_or_else(|e| die(&format!("append base: {e}")));
        append_histograms_file(&path, &[Hist::Th1(&h2)], none)
            .unwrap_or_else(|e| die(&format!("append: {e}")));
        cases.push(J::Obj(vec![
            ("id", J::Str("append".into())),
            ("kind", J::Str("hist_multi".into())),
            ("file", J::Str(file)),
            ("compression", comp_str(none)),
            (
                "objects",
                J::Arr(vec![
                    hist_obj_entry(
                        "h1",
                        "TH1D",
                        1,
                        jflts(h1.values()),
                        J::Arr(vec![jflts(&h1.edges())]),
                    ),
                    hist_obj_entry(
                        "h2",
                        "TH1D",
                        1,
                        jflts(h2.values()),
                        J::Arr(vec![jflts(&h2.edges())]),
                    ),
                ]),
            ),
            ("uproot_skip", J::Bool(false)),
        ]));
    }

    // --- RNTuple: scalars, vectors, multi-cluster, large container. ---
    cases.push(rntuple_scalars("rntuple_scalars", none, dir));
    cases.push(rntuple_scalars("rntuple_scalars_zstd", zstd, dir));
    cases.push(rntuple_vectors("rntuple_vectors", zstd, dir));
    cases.push(rntuple_multicluster("rntuple_multicluster", false, dir));
    cases.push(rntuple_multicluster("rntuple_large", true, dir));

    // --- TTree: scalars (11 widths), arrays, stl vector, split struct. ---
    cases.push(tree_scalars("tree_scalars", none, dir));
    cases.push(tree_scalars("tree_scalars_zstd", zstd, dir));
    cases.push(tree_arrays("tree_arrays", zstd, dir));
    cases.push(tree_stl_vector("tree_stl_vector", dir));
    cases.push(tree_split("tree_split", dir));

    // --- Emit manifest.json. ---
    let manifest = J::Obj(vec![("version", J::Int(1)), ("cases", J::Arr(cases))]);
    let mut out = String::new();
    manifest.render(&mut out);
    std::fs::write(dir.join("manifest.json"), out)
        .unwrap_or_else(|e| die(&format!("manifest: {e}")));
    println!(
        "wrote interop matrix ({} cases) + manifest.json to {}",
        count_cases(&manifest),
        dir.display()
    );
}

fn count_cases(m: &J) -> usize {
    if let J::Obj(fields) = m {
        if let Some((_, J::Arr(c))) = fields.iter().find(|(k, _)| *k == "cases") {
            return c.len();
        }
    }
    0
}

// --- RNTuple cases ---

fn rntuple_scalars(id: &'static str, comp: Compression, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let bools = vec![true, false, true, false];
    let i32s = vec![1i32, 2, 3, 4];
    let i64s = vec![10i64, 20, 30, 40];
    let u32s = vec![1u32, 2, 3, 4_000_000_000];
    let u64s = vec![1u64, 2, 3, 4];
    let f32s = vec![0.5f32, 1.5, 2.5, 3.5];
    let f64s = vec![1.25f64, 2.5, 3.75, 5.0];
    let strs: Vec<String> = vec!["a".into(), "bb".into(), "ccc".into(), "dddd".into()];
    let fields = vec![
        Field::bools("fb", bools.clone()),
        Field::i32("fi32", i32s.clone()),
        Field::i64("fi64", i64s.clone()),
        Field::u32("fu32", u32s.clone()),
        Field::u64("fu64", u64s.clone()),
        Field::f32("ff32", f32s.clone()),
        Field::f64("ff64", f64s.clone()),
        Field::strings("fs", strs.clone()),
    ];
    write_rntuple_file(dir.join(&file), "ntpl", &fields, comp)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let mfields = J::Arr(vec![
        rn_field("fb", "bool", jbools(&bools)),
        rn_field(
            "fi32",
            "int32",
            jints(&i32s.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        rn_field("fi64", "int64", jints(&i64s)),
        rn_field(
            "fu32",
            "uint32",
            jints(&u32s.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        rn_field(
            "fu64",
            "uint64",
            jints(&u64s.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        rn_field(
            "ff32",
            "float32",
            jflts(&f32s.iter().map(|&x| x as f64).collect::<Vec<_>>()),
        ),
        rn_field("ff64", "float64", jflts(&f64s)),
        rn_field(
            "fs",
            "string",
            jstrs(&strs.iter().map(String::as_str).collect::<Vec<_>>()),
        ),
    ]);
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("rntuple".into())),
        ("file", J::Str(file)),
        ("name", J::Str("ntpl".into())),
        ("compression", comp_str(comp)),
        ("n_entries", J::Int(4)),
        ("fields", mfields),
        ("uproot_skip", J::Bool(true)),
    ])
}

fn rntuple_vectors(id: &'static str, comp: Compression, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let vb = vec![vec![true], vec![], vec![false, true]];
    let vi32 = vec![vec![1i32, 2], vec![], vec![3]];
    let vi64 = vec![vec![10i64], vec![20, 30], vec![]];
    let vf32 = vec![vec![0.5f32], vec![], vec![1.5, 2.5]];
    let vf64 = vec![vec![1.0f64, 2.0], vec![3.0], vec![]];
    let fields = vec![
        Field::vec_bool("vb", vb.clone()),
        Field::vec_i32("vi32", vi32.clone()),
        Field::vec_i64("vi64", vi64.clone()),
        Field::vec_f32("vf32", vf32.clone()),
        Field::vec_f64("vf64", vf64.clone()),
    ];
    write_rntuple_file(dir.join(&file), "ntpl", &fields, comp)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let mfields = J::Arr(vec![
        rn_field(
            "vb",
            "vector<bool>",
            J::Arr(vb.iter().map(|r| jbools(r)).collect()),
        ),
        rn_field(
            "vi32",
            "vector<int32>",
            jvi64_2(
                &vi32
                    .iter()
                    .map(|r| r.iter().map(|&x| x as i64).collect())
                    .collect::<Vec<_>>(),
            ),
        ),
        rn_field("vi64", "vector<int64>", jvi64_2(&vi64)),
        rn_field(
            "vf32",
            "vector<float32>",
            jflts2(
                &vf32
                    .iter()
                    .map(|r| r.iter().map(|&x| x as f64).collect())
                    .collect::<Vec<_>>(),
            ),
        ),
        rn_field("vf64", "vector<float64>", jflts2(&vf64)),
    ]);
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("rntuple".into())),
        ("file", J::Str(file)),
        ("name", J::Str("ntpl".into())),
        ("compression", comp_str(comp)),
        ("n_entries", J::Int(3)),
        ("fields", mfields),
        ("uproot_skip", J::Bool(true)),
    ])
}

fn rntuple_multicluster(id: &'static str, large: bool, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let path = dir.join(&file);
    let mut w = if large {
        RNTupleWriter::create_large(&path, "ntpl", Compression::None)
    } else {
        RNTupleWriter::create(&path, "ntpl", Compression::None)
    }
    .unwrap_or_else(|e| die(&format!("{id} create: {e}")));
    // Two batches → two clusters.
    w.write_batch(&[
        Field::i32("x", vec![0, 1, 2]),
        Field::f64("y", vec![0.0, 1.0, 2.0]),
    ])
    .unwrap_or_else(|e| die(&format!("{id} batch1: {e}")));
    w.write_batch(&[
        Field::i32("x", vec![3, 4, 5]),
        Field::f64("y", vec![3.0, 4.0, 5.0]),
    ])
    .unwrap_or_else(|e| die(&format!("{id} batch2: {e}")));
    w.finish()
        .unwrap_or_else(|e| die(&format!("{id} finish: {e}")));
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("rntuple_stream".into())),
        ("file", J::Str(file)),
        ("name", J::Str("ntpl".into())),
        ("compression", comp_str(Compression::None)),
        ("n_entries", J::Int(6)),
        (
            "fields",
            J::Arr(vec![
                rn_field("x", "int32", jints(&[0, 1, 2, 3, 4, 5])),
                rn_field("y", "float64", jflts(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0])),
            ]),
        ),
        ("uproot_skip", J::Bool(true)),
    ])
}

fn rn_field(name: &str, ty: &str, values: J) -> J {
    J::Obj(vec![
        ("name", J::Str(name.into())),
        ("type", J::Str(ty.into())),
        ("values", values),
    ])
}

// --- TTree cases ---

fn tree_scalars(id: &'static str, comp: Compression, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let bb = vec![true, false, true];
    let bi8 = vec![-1i8, 0, 1];
    let bu8 = vec![1u8, 2, 3];
    let bi16 = vec![-100i16, 0, 100];
    let bu16 = vec![1u16, 2, 3];
    let bi32 = vec![1i32, 2, 3];
    let bu32 = vec![1u32, 2, 4_000_000_000];
    let bi64 = vec![10i64, 11, 12];
    let bu64 = vec![1u64, 2, 3];
    let bf32 = vec![0.5f32, 1.5, 2.5];
    let bf64 = vec![0.25f64, 1.25, 2.25];
    let branches = vec![
        Branch::bools("bb", bb.clone()),
        Branch::i8("bi8", bi8.clone()),
        Branch::u8("bu8", bu8.clone()),
        Branch::i16("bi16", bi16.clone()),
        Branch::u16("bu16", bu16.clone()),
        Branch::i32("bi32", bi32.clone()),
        Branch::u32("bu32", bu32.clone()),
        Branch::i64("bi64", bi64.clone()),
        Branch::u64("bu64", bu64.clone()),
        Branch::f32("bf32", bf32.clone()),
        Branch::f64("bf64", bf64.clone()),
    ];
    write_tree_file(dir.join(&file), "T", &branches, comp)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let scal = |name: &str, ty: &str, values: J| {
        J::Obj(vec![
            ("name", J::Str(name.into())),
            ("leaf", J::Str("scalar".into())),
            ("type", J::Str(ty.into())),
            ("values", values),
        ])
    };
    let mb = J::Arr(vec![
        scal("bb", "bool", jbools(&bb)),
        scal(
            "bi8",
            "int8",
            jints(&bi8.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bu8",
            "uint8",
            jints(&bu8.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bi16",
            "int16",
            jints(&bi16.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bu16",
            "uint16",
            jints(&bu16.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bi32",
            "int32",
            jints(&bi32.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bu32",
            "uint32",
            jints(&bu32.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal("bi64", "int64", jints(&bi64)),
        scal(
            "bu64",
            "uint64",
            jints(&bu64.iter().map(|&x| x as i64).collect::<Vec<_>>()),
        ),
        scal(
            "bf32",
            "float32",
            jflts(&bf32.iter().map(|&x| x as f64).collect::<Vec<_>>()),
        ),
        scal("bf64", "float64", jflts(&bf64)),
    ]);
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("tree".into())),
        ("file", J::Str(file)),
        ("name", J::Str("T".into())),
        ("compression", comp_str(comp)),
        ("n_entries", J::Int(3)),
        ("branches", mb),
        ("uproot_skip", J::Bool(false)),
    ])
}

fn tree_arrays(id: &'static str, comp: Compression, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let fx = vec![
        vec![1.0f64, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
    ];
    let jy = vec![vec![1.0f64], vec![2.0, 3.0], vec![]];
    let bs = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];
    let branches = vec![
        Branch::vec_f64("fx", fx.clone()),
        Branch::jagged_f64("jy", jy.clone()),
        Branch::strings("bs", bs.clone()),
    ];
    write_tree_file(dir.join(&file), "T", &branches, comp)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let mb = J::Arr(vec![
        J::Obj(vec![
            ("name", J::Str("fx".into())),
            ("leaf", J::Str("fixed".into())),
            ("type", J::Str("float64".into())),
            ("dim", J::Int(3)),
            ("values", jflts2(&fx)),
        ]),
        J::Obj(vec![
            ("name", J::Str("jy".into())),
            ("leaf", J::Str("jagged".into())),
            ("type", J::Str("float64".into())),
            ("count", J::Str("njy".into())),
            ("values", jflts2(&jy)),
        ]),
        J::Obj(vec![
            ("name", J::Str("bs".into())),
            ("leaf", J::Str("string".into())),
            ("type", J::Str("string".into())),
            (
                "values",
                jstrs(&bs.iter().map(String::as_str).collect::<Vec<_>>()),
            ),
        ]),
    ]);
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("tree".into())),
        ("file", J::Str(file)),
        ("name", J::Str("T".into())),
        ("compression", comp_str(comp)),
        ("n_entries", J::Int(3)),
        ("branches", mb),
        ("uproot_skip", J::Bool(false)),
    ])
}

fn tree_stl_vector(id: &'static str, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let vw = vec![vec![10.0f64, 20.0], vec![], vec![30.0]];
    let branches = vec![Branch::vector_f64("vw", vw.clone())];
    write_tree_file(dir.join(&file), "T", &branches, Compression::None)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let mb = J::Arr(vec![J::Obj(vec![
        ("name", J::Str("vw".into())),
        ("leaf", J::Str("stl_vector".into())),
        ("type", J::Str("vector<double>".into())),
        ("values", jflts2(&vw)),
    ])]);
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("tree".into())),
        ("file", J::Str(file)),
        ("name", J::Str("T".into())),
        ("compression", comp_str(Compression::None)),
        ("n_entries", J::Int(3)),
        ("branches", mb),
        ("uproot_skip", J::Bool(true)),
    ])
}

fn tree_split(id: &'static str, dir: &Path) -> J {
    let file = format!("m_{id}.root");
    let x = vec![vec![1.0f32], vec![], vec![2.0, 3.0]];
    let y = vec![vec![1.5f32], vec![], vec![2.5, 3.5]];
    let idm = vec![vec![1i32], vec![], vec![2, 3]];
    let branch = Branch::split_vector(
        "hits",
        "Hit",
        vec![
            SplitMember::f32("x", x.clone()),
            SplitMember::f32("y", y.clone()),
            SplitMember::i32("id", idm.clone()),
        ],
    );
    write_tree_file(dir.join(&file), "T", &[branch], Compression::None)
        .unwrap_or_else(|e| die(&format!("{id}: {e}")));
    let mem = |name: &str, ty: &str, values: J| {
        J::Obj(vec![
            ("name", J::Str(name.into())),
            ("type", J::Str(ty.into())),
            ("values", values),
        ])
    };
    J::Obj(vec![
        ("id", J::Str(id.into())),
        ("kind", J::Str("tree".into())),
        ("file", J::Str(file)),
        ("name", J::Str("T".into())),
        ("compression", comp_str(Compression::None)),
        ("n_entries", J::Int(3)),
        (
            "split",
            J::Obj(vec![
                ("branch", J::Str("hits".into())),
                ("class", J::Str("Hit".into())),
                (
                    "members",
                    J::Arr(vec![
                        mem(
                            "x",
                            "float32",
                            jflts2(
                                &x.iter()
                                    .map(|r| r.iter().map(|&v| v as f64).collect())
                                    .collect::<Vec<_>>(),
                            ),
                        ),
                        mem(
                            "y",
                            "float32",
                            jflts2(
                                &y.iter()
                                    .map(|r| r.iter().map(|&v| v as f64).collect())
                                    .collect::<Vec<_>>(),
                            ),
                        ),
                        mem(
                            "id",
                            "int32",
                            jvi64_2(
                                &idm.iter()
                                    .map(|r| r.iter().map(|&v| v as i64).collect())
                                    .collect::<Vec<_>>(),
                            ),
                        ),
                    ]),
                ),
            ]),
        ),
        ("uproot_skip", J::Bool(false)),
    ])
}

// ---------------------------------------------------------------------------
// `read-big`: read back a >2 GiB oracle file (the --big path). It contains a
// TTree "T" with one i32 scalar branch `n` whose value equals the entry index;
// we spot-check a few entries without holding everything in memory.
// ---------------------------------------------------------------------------

fn read_big(path: &Path) {
    let f = RFile::open(path).unwrap_or_else(|e| die(&format!("open big: {e}")));
    if !f.header().is_big() {
        die("big file did not parse as 64-bit format (fEND ≤ 2 GiB?)");
    }
    let t = TTree::open(&f, "T").unwrap_or_else(|e| die(&format!("open tree: {e}")));
    let n = t.num_entries();
    if n < 1 {
        die("big tree has no entries");
    }
    match t
        .read_branch(&f, "n")
        .unwrap_or_else(|e| die(&format!("read n: {e}")))
    {
        BranchValues::I32(v) => {
            if v.len() as u64 != n {
                die(&format!("big: read {} values for {n} entries", v.len()));
            }
            for (i, &x) in v.iter().enumerate() {
                if x != i as i32 {
                    die(&format!("big: entry {i} = {x}"));
                }
            }
        }
        other => die(&format!("big: expected I32, got {other:?}")),
    }
    println!("read-big OK: {n} entries verified");
}

fn die(msg: &str) -> ! {
    eprintln!("interop_matrix ERROR: {msg}");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = || -> ! {
        eprintln!("usage: interop_matrix <write <dir> | read-big <file>>");
        exit(2);
    };
    if args.len() != 3 {
        usage();
    }
    match args[1].as_str() {
        "write" => write(Path::new(&args[2])),
        "read-big" => read_big(Path::new(&args[2])),
        _ => usage(),
    }
}
