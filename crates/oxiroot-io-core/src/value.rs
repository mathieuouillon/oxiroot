//! A dynamic value tree — the output of the generic, streamer-info-driven object
//! reader ([`read_object`](crate::read_object::read_object)).
//!
//! Where the typed models (`TH1`, `TGraph`, …) decode a *known* class into a Rust
//! struct, [`Value`] represents *any* class: a tree of named members whose shape
//! comes entirely from the file's `TStreamerInfo`. It is what powers rootls /
//! rootprint-style inspection of arbitrary ROOT files.

use core::fmt;

/// A dynamically-typed value decoded from a ROOT object.
///
/// Where the typed models (`TH1`, `TGraph`, …) decode a *known* class into a Rust
/// struct, a `Value` represents *any* class: a tree of named members whose shape
/// comes entirely from the file's `TStreamerInfo` (see
/// [`read_object`](fn@crate::read_object)).
///
/// Numbers keep their exact on-disk width; strings, arrays, and nested objects
/// nest recursively. Members preserve their stream order (a `Vec`, not a map), so
/// a printed object matches ROOT's layout. A member the reader cannot decode
/// becomes [`Value::Unsupported`] rather than aborting the whole object.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A null object pointer.
    Null,
    /// A boolean (`Bool_t`).
    Bool(bool),
    /// A signed 8-bit integer (`Char_t`).
    I8(i8),
    /// A signed 16-bit integer (`Short_t`).
    I16(i16),
    /// A signed 32-bit integer (`Int_t`).
    I32(i32),
    /// A signed 64-bit integer (`Long64_t`).
    I64(i64),
    /// An unsigned 8-bit integer (`UChar_t`).
    U8(u8),
    /// An unsigned 16-bit integer (`UShort_t`).
    U16(u16),
    /// An unsigned 32-bit integer (`UInt_t`).
    U32(u32),
    /// An unsigned 64-bit integer (`ULong64_t`).
    U64(u64),
    /// A 32-bit float (`Float_t`).
    F32(f32),
    /// A 64-bit float (`Double_t`).
    F64(f64),
    /// A string (`TString`, `char*`, or `std::string`).
    Str(String),
    /// An array or STL container of values (a C array, `T[n]`, or `std::vector`).
    Array(Vec<Value>),
    /// A nested object: its class name and its members in stream order.
    Object {
        /// The object's class name.
        class: String,
        /// The members, `(name, value)`, in the order they appear on disk.
        members: Vec<(String, Value)>,
    },
    /// A slot holding an object that is written elsewhere in the same object:
    /// ROOT streams a shared object once and points at it from every other
    /// place it appears (a `TH2Poly`'s `fBins` points at the bins its `fCells`
    /// grid holds in full). The value carries the class it points at; the object
    /// itself is in the tree where it was written.
    Ref {
        /// The class of the object this slot points at.
        class: String,
    },
    /// A member (or object) the reader could not decode — its class/type and why.
    /// The enclosing object's byte count lets decoding continue past it.
    Unsupported {
        /// The class or C++ type name that was not decoded.
        class: String,
        /// Why it was not decoded (e.g. an unhandled `fType`, or a class the
        /// file does not describe).
        reason: String,
    },
}

impl Value {
    /// The class name, for an [`Object`](Value::Object), a [`Ref`](Value::Ref)
    /// or an [`Unsupported`](Value::Unsupported).
    #[must_use]
    pub fn class(&self) -> Option<&str> {
        match self {
            Value::Object { class, .. }
            | Value::Ref { class }
            | Value::Unsupported { class, .. } => Some(class),
            _ => None,
        }
    }

    /// The members of an [`Object`](Value::Object).
    #[must_use]
    pub fn members(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object { members, .. } => Some(members),
            _ => None,
        }
    }

    /// Look up an object member by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.members()?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    /// The elements of an [`Array`](Value::Array).
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }

    /// This value as an `f64`, coercing any numeric variant.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        Some(match *self {
            Value::Bool(b) => f64::from(b),
            Value::I8(v) => f64::from(v),
            Value::I16(v) => f64::from(v),
            Value::I32(v) => f64::from(v),
            Value::I64(v) => v as f64,
            Value::U8(v) => f64::from(v),
            Value::U16(v) => f64::from(v),
            Value::U32(v) => f64::from(v),
            Value::U64(v) => v as f64,
            Value::F32(v) => f64::from(v),
            Value::F64(v) => v,
            _ => return None,
        })
    }

    /// This value as an `i64`, coercing any integer variant.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        Some(match *self {
            Value::Bool(b) => i64::from(b),
            Value::I8(v) => i64::from(v),
            Value::I16(v) => i64::from(v),
            Value::I32(v) => i64::from(v),
            Value::I64(v) => v,
            Value::U8(v) => i64::from(v),
            Value::U16(v) => i64::from(v),
            Value::U32(v) => i64::from(v),
            Value::U64(v) => v as i64,
            _ => return None,
        })
    }

    /// This value as a `bool`.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// This value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Whether this is a null object pointer.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Whether every element of an array is a scalar (for compact one-line
    /// printing).
    fn is_scalar(&self) -> bool {
        !matches!(
            self,
            Value::Array(_) | Value::Object { .. } | Value::Ref { .. } | Value::Unsupported { .. }
        )
    }

    /// A short one-line rendering of a scalar/leaf value.
    fn render_scalar(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::I8(v) => v.to_string(),
            Value::I16(v) => v.to_string(),
            Value::I32(v) => v.to_string(),
            Value::I64(v) => v.to_string(),
            Value::U8(v) => v.to_string(),
            Value::U16(v) => v.to_string(),
            Value::U32(v) => v.to_string(),
            Value::U64(v) => v.to_string(),
            Value::F32(v) => v.to_string(),
            Value::F64(v) => v.to_string(),
            Value::Str(s) => format!("{s:?}"),
            _ => String::new(),
        }
    }

    /// Render this value as an indented tree (rootprint-style). `indent` is the
    /// current depth; `label` is the member name prefix (`None` at the root).
    fn render_into(&self, out: &mut String, indent: usize, label: Option<&str>) {
        const PAD: &str = "  ";
        let pad = PAD.repeat(indent);
        let name = label.map_or(String::new(), |l| format!("{l}: "));
        match self {
            Value::Object { class, members } => {
                out.push_str(&format!("{pad}{name}{class}\n"));
                for (member, value) in members {
                    value.render_into(out, indent + 1, Some(member));
                }
            }
            Value::Array(items) => {
                if items.iter().all(Value::is_scalar) {
                    // A short scalar array prints inline; a long one is truncated.
                    let shown: Vec<String> =
                        items.iter().take(20).map(Value::render_scalar).collect();
                    let more = if items.len() > 20 { ", …" } else { "" };
                    out.push_str(&format!(
                        "{pad}{name}[{}] {}{}\n",
                        items.len(),
                        shown.join(", "),
                        more
                    ));
                } else {
                    out.push_str(&format!("{pad}{name}[{}]\n", items.len()));
                    for item in items {
                        item.render_into(out, indent + 1, None);
                    }
                }
            }
            Value::Ref { class } => {
                out.push_str(&format!("{pad}{name}<{class} written above>\n"));
            }
            Value::Unsupported { class, reason } => {
                out.push_str(&format!("{pad}{name}<unsupported {class}: {reason}>\n"));
            }
            scalar => out.push_str(&format!("{pad}{name}{}\n", scalar.render_scalar())),
        }
    }
}

impl fmt::Display for Value {
    /// Render as an indented tree.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.render_into(&mut out, 0, None);
        f.write_str(out.trim_end())
    }
}
