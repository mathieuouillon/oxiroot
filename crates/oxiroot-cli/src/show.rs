//! `oxroot show` — the structure of a `TTree` or RNTuple.

use clap::Args as ClapArgs;
use oxiroot::ntuple::RNTuple;
use oxiroot::tree::{BranchValues, LeafType, TTree};
use oxiroot::RFile;

use crate::util::{
    classify, leaf_type_name, locate_class, parse_spec, split_obj, CmdResult, Kind, Table,
};

/// Arguments for `oxroot show`.
#[derive(ClapArgs)]
pub struct Args {
    /// The object to show, as `file.root:object`.
    spec: String,
}

/// Run `oxroot show`.
pub fn run(args: Args) -> CmdResult {
    let (path, obj) = parse_spec(&args.spec);
    let obj = obj.ok_or("show needs an object: `file.root:name`")?;
    let file = RFile::open(&path)?;
    let (subdir, name) = split_obj(&obj);
    let class = locate_class(&file, subdir, name)?;

    match classify(&class) {
        Kind::Tree if subdir.is_some() => {
            Err("showing a TTree inside a subdirectory is not supported yet".into())
        }
        Kind::Tree => show_tree(&file, name),
        Kind::RNtuple => show_rntuple(&file, subdir, name),
        _ => {
            println!("{name}  {class}");
            println!("(not a TTree or RNTuple — use `oxroot dump` to see its contents)");
            Ok(())
        }
    }
}

/// Print a `TTree`'s branches, their types, and any unreadable branches.
fn show_tree(file: &RFile, name: &str) -> CmdResult {
    let tree = TTree::open(file, name)?;
    println!(
        "TTree {name:?}  ({} entries, {} branches)",
        tree.num_entries(),
        tree.branch_names().len()
    );

    let mut table = Table::new(&["branch", "type"]);
    for branch in tree.branch_names() {
        table.row(vec![
            branch.to_string(),
            branch_type_label(&tree, file, branch),
        ]);
    }
    for (branch, reason) in tree.unsupported_branches() {
        table.row(vec![
            format!("! {branch}"),
            format!("unreadable ({reason})"),
        ]);
    }
    table.print();
    Ok(())
}

/// Print an RNTuple's top-level fields and their C++ types.
fn show_rntuple(file: &RFile, subdir: Option<&str>, name: &str) -> CmdResult {
    let ntuple = match subdir {
        None => RNTuple::open(file, name)?,
        Some(dir) => RNTuple::open_in(file, dir, name)?,
    };
    println!(
        "RNTuple {name:?}  ({} entries, {} fields)",
        ntuple.num_entries(),
        ntuple.field_names().len()
    );

    let fields = &ntuple.header().fields;
    let mut table = Table::new(&["field", "type"]);
    for field in ntuple.field_names() {
        let ty = fields
            .iter()
            .find(|fd| fd.name == field)
            .map(|fd| fd.type_name.clone())
            .unwrap_or_default();
        table.row(vec![field.to_string(), ty]);
    }
    table.print();
    Ok(())
}

/// The C++-ish type label of a branch: `double`, `double[3]` (fixed array),
/// `double[]` (variable), `char*`, or a nested/vector form. Peeks at the first
/// entry to tell a scalar from a collection when the title carries no shape.
fn branch_type_label(tree: &TTree, file: &RFile, name: &str) -> String {
    let elem = leaf_type_name(tree.branch_type(name).unwrap_or(LeafType::Str));

    let shape = tree.branch_shape(name).unwrap_or(&[]);
    if !shape.is_empty() {
        let dims: String = shape.iter().map(|n| format!("[{n}]")).collect();
        return format!("{elem}{dims}");
    }

    if tree.num_entries() == 0 {
        return elem.to_string();
    }
    match tree.read_branch_range(file, name, 0, 1) {
        Ok(values) => values_label(elem, &values),
        Err(_) => elem.to_string(),
    }
}

/// Turn the first entry's decoded value into a type label.
fn values_label(elem: &str, values: &BranchValues) -> String {
    use BranchValues::*;
    match values {
        Str(_) => "char*".to_string(),
        VecStr(_) => "std::vector<string>".to_string(),
        Nested { .. } => format!("std::vector<std::vector<{elem}>>"),
        VecBool(_) | VecI8(_) | VecU8(_) | VecI16(_) | VecU16(_) | VecI32(_) | VecU32(_)
        | VecI64(_) | VecU64(_) | VecF32(_) | VecF64(_) => format!("{elem}[]"),
        _ => elem.to_string(),
    }
}
