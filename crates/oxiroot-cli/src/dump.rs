//! `oxroot dump` — print an object's data.

use clap::Args as ClapArgs;
use oxiroot::hist::{Histogram, ReadRoot, TGraph, TObjString, TParameter, TProfile, TH1, TH2, TH3};
use oxiroot::ntuple::{FieldValues, RNTuple};
use oxiroot::tree::{BranchValues, TTree};
use oxiroot::RFile;

use crate::util::{classify, locate_class, parse_spec, split_obj, CmdResult, Kind, Table};

/// Arguments for `oxroot dump`.
#[derive(ClapArgs)]
pub struct Args {
    /// The object to dump, as `file.root:object`.
    spec: String,
    /// Number of entries / points to print (trees, RNTuples, graphs).
    #[arg(short = 'n', long, default_value_t = 10)]
    entries: usize,
    /// Comma-separated branch/field subset to print (default: all).
    #[arg(short, long, value_delimiter = ',')]
    branches: Vec<String>,
}

/// Run `oxroot dump`.
pub fn run(args: Args) -> CmdResult {
    let (path, obj) = parse_spec(&args.spec);
    let obj = obj.ok_or("dump needs an object: `file.root:name`")?;
    let file = RFile::open(&path)?;
    let (subdir, name) = split_obj(&obj);
    let class = locate_class(&file, subdir, name)?;

    match classify(&class) {
        Kind::Tree if subdir.is_some() => {
            Err("dumping a TTree inside a subdirectory is not supported yet".into())
        }
        Kind::Tree => dump_tree(&file, name, &args),
        Kind::RNtuple => dump_rntuple(&file, subdir, name, &args),
        Kind::Hist1 => dump_th1(&read_obj::<TH1>(&file, subdir, name)?, name),
        Kind::Hist2 => dump_th2(&read_obj::<TH2>(&file, subdir, name)?, name),
        Kind::Hist3 => dump_th3(&read_obj::<TH3>(&file, subdir, name)?, name),
        Kind::Profile => dump_profile(&read_obj::<TProfile>(&file, subdir, name)?, name),
        Kind::Graph => dump_graph(
            &read_obj::<TGraph>(&file, subdir, name)?,
            name,
            args.entries,
        ),
        Kind::ObjString => {
            println!("{}", read_obj::<TObjString>(&file, subdir, name)?.value());
            Ok(())
        }
        Kind::Parameter => {
            let p = read_obj::<TParameter>(&file, subdir, name)?;
            println!("{name} = {:?}", p.value());
            Ok(())
        }
        Kind::Other => Err(format!("dump: reading class {class:?} is not supported").into()),
    }
}

/// Read a `ReadRoot` object from the top directory or a subdirectory.
fn read_obj<T: ReadRoot>(file: &RFile, subdir: Option<&str>, name: &str) -> oxiroot::Result<T> {
    match subdir {
        None => T::read_root(file, name),
        Some(dir) => T::read_root_in(file, dir, name),
    }
}

/// Print the first `n` entries of a `TTree` as a column table.
fn dump_tree(file: &RFile, name: &str, args: &Args) -> CmdResult {
    let tree = TTree::open(file, name)?;
    let n = args.entries.min(tree.num_entries() as usize);
    let selected: Vec<String> = if args.branches.is_empty() {
        tree.branch_names().iter().map(|s| s.to_string()).collect()
    } else {
        args.branches.clone()
    };
    let cols: Vec<(String, Option<BranchValues>)> = selected
        .iter()
        .map(|b| (b.clone(), tree.read_branch_range(file, b, 0, n as u64).ok()))
        .collect();

    let mut headers = vec!["#".to_string()];
    headers.extend(cols.iter().map(|(b, _)| b.clone()));
    let head_refs: Vec<&str> = headers.iter().map(String::as_str).collect();
    let mut table = Table::new(&head_refs).right_align(&[0]);
    for i in 0..n {
        let mut row = vec![i.to_string()];
        for (_, values) in &cols {
            row.push(
                values
                    .as_ref()
                    .map_or_else(String::new, |v| branch_cell(v, i)),
            );
        }
        table.row(row);
    }

    println!(
        "TTree {name:?}  ({} entries; showing {n})",
        tree.num_entries()
    );
    table.print();
    Ok(())
}

/// Print the first `n` entries of an RNTuple as a column table.
fn dump_rntuple(file: &RFile, subdir: Option<&str>, name: &str, args: &Args) -> CmdResult {
    let ntuple = match subdir {
        None => RNTuple::open(file, name)?,
        Some(dir) => RNTuple::open_in(file, dir, name)?,
    };
    let n = args.entries.min(ntuple.num_entries() as usize);
    let selected: Vec<String> = if args.branches.is_empty() {
        ntuple.field_names().iter().map(|s| s.to_string()).collect()
    } else {
        args.branches.clone()
    };
    let cols: Vec<(String, Option<FieldValues>)> = selected
        .iter()
        .map(|f| (f.clone(), ntuple.read_field(file, f).ok()))
        .collect();

    let mut headers = vec!["#".to_string()];
    headers.extend(cols.iter().map(|(f, _)| f.clone()));
    let head_refs: Vec<&str> = headers.iter().map(String::as_str).collect();
    let mut table = Table::new(&head_refs).right_align(&[0]);
    for i in 0..n {
        let mut row = vec![i.to_string()];
        for (_, values) in &cols {
            row.push(
                values
                    .as_ref()
                    .map_or_else(String::new, |v| field_cell(v, i)),
            );
        }
        table.row(row);
    }

    println!(
        "RNTuple {name:?}  ({} entries; showing {n})",
        ntuple.num_entries()
    );
    table.print();
    Ok(())
}

/// Bins, contents, errors, and summary stats of a `TH1`.
fn dump_th1(hist: &TH1, name: &str) -> CmdResult {
    println!(
        "TH1 {name:?}  entries {}  mean {}  std {}  integral {}",
        num(hist.entries()),
        num(hist.mean()),
        num(hist.std_dev()),
        num(hist.integral()),
    );
    let edges = hist.edges();
    let errors = hist.errors();
    let mut table =
        Table::new(&["bin", "low", "high", "content", "error"]).right_align(&[0, 1, 2, 3, 4]);
    for (i, &content) in hist.values().iter().enumerate() {
        table.row(vec![
            (i + 1).to_string(),
            num(edges.get(i).copied().unwrap_or(f64::NAN)),
            num(edges.get(i + 1).copied().unwrap_or(f64::NAN)),
            num(content),
            num(errors.get(i).copied().unwrap_or(0.0)),
        ]);
    }
    table.print();
    Ok(())
}

/// A summary of a `TH2` (its full grid is not printed).
fn dump_th2(hist: &TH2, name: &str) -> CmdResult {
    let values = hist.values();
    let ny = values.len();
    let nx = values.first().map_or(0, Vec::len);
    println!("TH2 {name:?}  {nx} x {ny} bins");
    println!(
        "entries {}  integral {}  mean_x {}  mean_y {}",
        num(hist.entries()),
        num(hist.integral()),
        num(hist.mean_x()),
        num(hist.mean_y()),
    );
    Ok(())
}

/// A summary of a `TH3` (its full grid is not printed).
fn dump_th3(hist: &TH3, name: &str) -> CmdResult {
    let values = hist.values();
    let nz = values.len();
    let ny = values.first().map_or(0, Vec::len);
    let nx = values.first().and_then(|p| p.first()).map_or(0, Vec::len);
    println!("TH3 {name:?}  {nx} x {ny} x {nz} bins");
    println!(
        "entries {}  integral {}  mean_x {}  mean_y {}",
        num(hist.entries()),
        num(hist.integral()),
        num(hist.mean_x()),
        num(hist.mean_y()),
    );
    Ok(())
}

/// Per-bin mean-y of a `TProfile`.
fn dump_profile(profile: &TProfile, name: &str) -> CmdResult {
    println!("TProfile {name:?}");
    let edges = profile.edges();
    let mut table = Table::new(&["bin", "low", "high", "mean-y"]).right_align(&[0, 1, 2, 3]);
    for (i, &value) in profile.values().iter().enumerate() {
        table.row(vec![
            (i + 1).to_string(),
            num(edges.get(i).copied().unwrap_or(f64::NAN)),
            num(edges.get(i + 1).copied().unwrap_or(f64::NAN)),
            num(value),
        ]);
    }
    table.print();
    Ok(())
}

/// The first `n` points of a `TGraph`.
fn dump_graph(graph: &TGraph, name: &str, n: usize) -> CmdResult {
    println!("TGraph {name:?}  ({} points)", graph.x.len());
    let mut table = Table::new(&["#", "x", "y"]).right_align(&[0, 1, 2]);
    for i in 0..graph.x.len().min(n) {
        table.row(vec![i.to_string(), num(graph.x[i]), num(graph.y[i])]);
    }
    table.print();
    Ok(())
}

/// One `BranchValues` entry as a cell string.
fn branch_cell(values: &BranchValues, i: usize) -> String {
    use BranchValues::*;
    macro_rules! scalar {
        ($v:expr) => {
            $v.get(i).map(ToString::to_string).unwrap_or_default()
        };
    }
    match values {
        Bool(v) => scalar!(v),
        I8(v) => scalar!(v),
        U8(v) => scalar!(v),
        I16(v) => scalar!(v),
        U16(v) => scalar!(v),
        I32(v) => scalar!(v),
        U32(v) => scalar!(v),
        I64(v) => scalar!(v),
        U64(v) => scalar!(v),
        F32(v) => v.get(i).map_or_else(String::new, |x| num(f64::from(*x))),
        F64(v) => v.get(i).map_or_else(String::new, |x| num(*x)),
        Str(v) => v.get(i).cloned().unwrap_or_default(),
        VecBool(v) => list_cell(v, i),
        VecI8(v) => list_cell(v, i),
        VecU8(v) => list_cell(v, i),
        VecI16(v) => list_cell(v, i),
        VecU16(v) => list_cell(v, i),
        VecI32(v) => list_cell(v, i),
        VecU32(v) => list_cell(v, i),
        VecI64(v) => list_cell(v, i),
        VecU64(v) => list_cell(v, i),
        VecF32(v) => list_cell(v, i),
        VecF64(v) => list_cell(v, i),
        VecStr(v) => v.get(i).map_or_else(String::new, |row| format!("{row:?}")),
        Nested { .. } => "...".to_string(),
        _ => "...".to_string(),
    }
}

/// One `FieldValues` entry as a cell string.
fn field_cell(values: &FieldValues, i: usize) -> String {
    use FieldValues::*;
    macro_rules! scalar {
        ($v:expr) => {
            $v.get(i).map(ToString::to_string).unwrap_or_default()
        };
    }
    match values {
        Bool(v) => scalar!(v),
        I8(v) => scalar!(v),
        U8(v) => scalar!(v),
        I16(v) => scalar!(v),
        U16(v) => scalar!(v),
        I32(v) => scalar!(v),
        U32(v) => scalar!(v),
        I64(v) => scalar!(v),
        U64(v) => scalar!(v),
        F32(v) => v.get(i).map_or_else(String::new, |x| num(f64::from(*x))),
        F64(v) => v.get(i).map_or_else(String::new, |x| num(*x)),
        Str(v) => v.get(i).cloned().unwrap_or_default(),
        VecBool(v) => list_cell(v, i),
        VecI8(v) => list_cell(v, i),
        VecU8(v) => list_cell(v, i),
        VecI16(v) => list_cell(v, i),
        VecU16(v) => list_cell(v, i),
        VecI32(v) => list_cell(v, i),
        VecU32(v) => list_cell(v, i),
        VecI64(v) => list_cell(v, i),
        VecU64(v) => list_cell(v, i),
        VecF32(v) => list_cell(v, i),
        VecF64(v) => list_cell(v, i),
        VecStr(v) => v.get(i).map_or_else(String::new, |row| format!("{row:?}")),
        _ => "...".to_string(),
    }
}

/// Entry `i` of a per-entry list column as `[a, b, c]`.
fn list_cell<T: ToString>(rows: &[Vec<T>], i: usize) -> String {
    rows.get(i).map_or_else(String::new, |row| {
        let parts: Vec<String> = row.iter().map(ToString::to_string).collect();
        format!("[{}]", parts.join(", "))
    })
}

/// Format a float compactly: integers without a fraction, otherwise up to six
/// decimals with trailing zeros trimmed.
fn num(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return (x as i64).to_string();
    }
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
