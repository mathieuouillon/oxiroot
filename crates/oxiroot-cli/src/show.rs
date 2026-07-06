//! `oxroot show` — the structure of a `TTree` or RNTuple.

use clap::Args as ClapArgs;
use oxiroot::ntuple::RNTuple;
use oxiroot::tree::{BranchValues, LeafType, TTree};
use oxiroot::RFile;

use crate::json::Json;
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
pub fn run(args: Args, json: bool) -> CmdResult {
    let (path, obj) = parse_spec(&args.spec);
    let obj = obj.ok_or("show needs an object: `file.root:name`")?;
    let file = crate::util::open_root(&path)?;
    let (subdir, name) = split_obj(&obj);
    let class = locate_class(&file, subdir, name)?;

    match classify(&class) {
        Kind::Tree => show_tree(&file, subdir, name, json),
        Kind::RNtuple => show_rntuple(&file, subdir, name, json),
        _ => {
            if json {
                let out = Json::Object(vec![("name", Json::s(name)), ("class", Json::s(class))]);
                println!("{}", out.render());
            } else {
                println!("{name}  {class}");
                println!("(not a TTree or RNTuple — use `oxroot dump` to see its contents)");
            }
            Ok(())
        }
    }
}

/// Show a `TTree`'s branches, their types, and any unreadable branches.
fn show_tree(file: &RFile, subdir: Option<&str>, name: &str, json: bool) -> CmdResult {
    let tree = match subdir {
        None => TTree::open(file, name)?,
        Some(dir) => TTree::open_in(file, dir, name)?,
    };
    let mut branches = Vec::new();
    for b in tree.branch_names() {
        branches.push((b.to_string(), branch_type_label(&tree, file, b)));
    }
    let unreadable: Vec<(String, String)> = tree
        .unsupported_branches()
        .iter()
        .map(|(n, r)| ((*n).to_string(), (*r).to_string()))
        .collect();

    if json {
        let out = Json::Object(vec![
            ("name", Json::s(name)),
            ("class", Json::s("TTree")),
            ("entries", Json::Int(tree.num_entries() as i64)),
            ("branches", named_type_array(&branches)),
            (
                "unreadable",
                Json::Array(
                    unreadable
                        .iter()
                        .map(|(n, r)| {
                            Json::Object(vec![
                                ("name", Json::s(n.clone())),
                                ("reason", Json::s(r.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        println!("{}", out.render());
        return Ok(());
    }

    println!(
        "TTree {name:?}  ({} entries, {} branches)",
        tree.num_entries(),
        branches.len()
    );
    let mut table = Table::new(&["branch", "type"]);
    for (n, t) in &branches {
        table.row(vec![n.clone(), t.clone()]);
    }
    for (n, r) in &unreadable {
        table.row(vec![format!("! {n}"), format!("unreadable ({r})")]);
    }
    table.print();
    Ok(())
}

/// Show an RNTuple's top-level fields and their C++ types.
fn show_rntuple(file: &RFile, subdir: Option<&str>, name: &str, json: bool) -> CmdResult {
    let ntuple = match subdir {
        None => RNTuple::open(file, name)?,
        Some(dir) => RNTuple::open_in(file, dir, name)?,
    };
    let descriptors = &ntuple.header().fields;
    let mut fields = Vec::new();
    for f in ntuple.field_names() {
        let ty = descriptors
            .iter()
            .find(|fd| fd.name == f)
            .map(|fd| fd.type_name.clone())
            .unwrap_or_default();
        fields.push((f.to_string(), ty));
    }

    if json {
        let out = Json::Object(vec![
            ("name", Json::s(name)),
            ("class", Json::s("RNTuple")),
            ("entries", Json::Int(ntuple.num_entries() as i64)),
            ("fields", named_type_array(&fields)),
        ]);
        println!("{}", out.render());
        return Ok(());
    }

    println!(
        "RNTuple {name:?}  ({} entries, {} fields)",
        ntuple.num_entries(),
        fields.len()
    );
    let mut table = Table::new(&["field", "type"]);
    for (n, t) in &fields {
        table.row(vec![n.clone(), t.clone()]);
    }
    table.print();
    Ok(())
}

/// A JSON array of `{name, type}` objects.
fn named_type_array(items: &[(String, String)]) -> Json {
    Json::Array(
        items
            .iter()
            .map(|(n, t)| {
                Json::Object(vec![
                    ("name", Json::s(n.clone())),
                    ("type", Json::s(t.clone())),
                ])
            })
            .collect(),
    )
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
