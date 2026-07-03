//! Shared helpers: path-spec parsing, class classification, type-name mapping,
//! and a minimal aligned-column table.

use std::error::Error;
use std::path::Path;

use oxiroot::tree::LeafType;
use oxiroot::RFile;

/// Every command returns this: `Ok(())` on success, else a boxed error printed
/// by `main`.
pub type CmdResult = Result<(), Box<dyn Error>>;

/// Split a `file.root:dir/object` spec into the file path and an optional
/// in-file object path.
///
/// The object path is whatever follows the last `:` — unless that would name a
/// file which doesn't exist while the whole spec does (a `:` inside the
/// filename), in which case the whole spec is treated as the file.
pub fn parse_spec(spec: &str) -> (String, Option<String>) {
    if let Some((file, obj)) = spec.rsplit_once(':') {
        let split_names_a_file = Path::new(file).exists() || !Path::new(spec).exists();
        if !file.is_empty() && !obj.is_empty() && split_names_a_file {
            return (file.to_string(), Some(obj.to_string()));
        }
    }
    (spec.to_string(), None)
}

/// Split an in-file object path into an optional single subdirectory and the
/// object name (`dir/obj` → `(Some("dir"), "obj")`, `obj` → `(None, "obj")`).
pub fn split_obj(obj: &str) -> (Option<&str>, &str) {
    match obj.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, obj),
    }
}

/// The class name of the object at `(subdir, name)`, or an error if there is no
/// such key.
pub fn locate_class(
    file: &RFile,
    subdir: Option<&str>,
    name: &str,
) -> Result<String, Box<dyn Error>> {
    match subdir {
        None => Ok(file
            .key(name)
            .ok_or_else(|| format!("no object named {name:?} in the file"))?
            .class_name
            .clone()),
        Some(dir) => {
            let d = file.subdir(dir)?;
            let key = d
                .keys
                .iter()
                .filter(|k| k.name == name && !k.is_deleted())
                .max_by_key(|k| k.cycle)
                .ok_or_else(|| format!("no object named {name:?} in subdirectory {dir:?}"))?;
            Ok(key.class_name.clone())
        }
    }
}

/// A coarse object category derived from a ROOT class name — the CLI dispatches
/// `show`/`dump` on this.
pub enum Kind {
    Tree,
    RNtuple,
    Hist1,
    Hist2,
    Hist3,
    Profile,
    Graph,
    ObjString,
    Parameter,
    Other,
}

/// Classify a ROOT class name into a [`Kind`].
pub fn classify(class: &str) -> Kind {
    match class {
        "TTree" | "TNtuple" | "TNtupleD" => Kind::Tree,
        "ROOT::RNTuple" => Kind::RNtuple,
        "TProfile" => Kind::Profile,
        "TObjString" => Kind::ObjString,
        "TGraph" | "TGraphErrors" | "TGraphAsymmErrors" => Kind::Graph,
        c if c.starts_with("TParameter") => Kind::Parameter,
        c if is_th(c, b'1') => Kind::Hist1,
        c if is_th(c, b'2') && c != "TH2Poly" => Kind::Hist2,
        c if is_th(c, b'3') => Kind::Hist3,
        _ => Kind::Other,
    }
}

/// A summable histogram class name `TH{dim}{precision}` (e.g. `TH1D`).
fn is_th(class: &str, dim: u8) -> bool {
    let b = class.as_bytes();
    b.len() == 4
        && &b[..2] == b"TH"
        && b[2] == dim
        && matches!(b[3], b'C' | b'S' | b'I' | b'F' | b'D' | b'L')
}

/// The C++ element type name for a leaf type.
pub fn leaf_type_name(t: LeafType) -> &'static str {
    match t {
        LeafType::Bool => "bool",
        LeafType::I8 => "int8_t",
        LeafType::U8 => "uint8_t",
        LeafType::I16 => "int16_t",
        LeafType::U16 => "uint16_t",
        LeafType::I32 => "int32_t",
        LeafType::U32 => "uint32_t",
        LeafType::I64 => "int64_t",
        LeafType::U64 => "uint64_t",
        LeafType::F32 => "float",
        LeafType::F64 => "double",
        LeafType::Str => "char*",
        _ => "?",
    }
}

/// Decode ROOT's `fCompress` (`algorithm * 100 + level`) into a label.
pub fn compression_label(compress: u32) -> String {
    let (alg, level) = (compress / 100, compress % 100);
    let name = match alg {
        0 => return "none".to_string(),
        1 => "zlib",
        2 => "lzma",
        4 => "lz4",
        5 => "zstd",
        _ => "unknown",
    };
    format!("{name} (level {level})")
}

/// Format a ROOT file-version integer (`62400`) as `6.24/00`.
pub fn root_version(v: u32) -> String {
    format!("{}.{:02}/{:02}", v / 10000, (v / 100) % 100, v % 100)
}

/// A left/right-aligned column table with a header row.
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    right: Vec<bool>,
}

impl Table {
    /// A new table with these column headers (all left-aligned by default).
    pub fn new(headers: &[&str]) -> Table {
        Table {
            right: vec![false; headers.len()],
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    /// Right-align the given column indices (for numbers).
    pub fn right_align(mut self, cols: &[usize]) -> Table {
        for &c in cols {
            if let Some(slot) = self.right.get_mut(c) {
                *slot = true;
            }
        }
        self
    }

    /// Append a row (cell count should match the header count).
    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// Print the table to stdout, padding each column to its widest cell.
    pub fn print(&self) {
        let ncol = self.headers.len();
        let mut width = vec![0usize; ncol];
        for (c, h) in self.headers.iter().enumerate() {
            width[c] = h.chars().count();
        }
        for row in &self.rows {
            for (c, cell) in row.iter().enumerate().take(ncol) {
                width[c] = width[c].max(cell.chars().count());
            }
        }
        let line = |cells: &[String]| {
            let parts: Vec<String> = (0..ncol)
                .map(|c| {
                    let cell = cells.get(c).map(String::as_str).unwrap_or("");
                    pad(cell, width[c], self.right[c])
                })
                .collect();
            println!("{}", parts.join("  ").trim_end());
        };
        line(&self.headers);
        for row in &self.rows {
            line(row);
        }
    }
}

/// Pad `s` to `width` columns, on the left when `right` (right-aligned).
fn pad(s: &str, width: usize, right: bool) -> String {
    let n = s.chars().count();
    let fill = " ".repeat(width.saturating_sub(n));
    if right {
        format!("{fill}{s}")
    } else {
        format!("{s}{fill}")
    }
}
