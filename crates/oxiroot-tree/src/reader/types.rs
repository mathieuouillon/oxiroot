//! Mapping ROOT type names and streamer element types to [`LeafType`].

use oxiroot_io_core::StreamerElement;

use crate::value::LeafType;

/// Map a class member's streamer element to its [`LeafType`] — basic scalars (via
/// [`streamer_type_to_leaf`]) and `TString` (`fType` 65). `None` for member types
/// we don't surface from a `TBranchObject` (objects, arrays, pointers).
pub(super) fn member_leaf_type(el: &StreamerElement) -> Option<LeafType> {
    if el.el_type == 65 {
        return Some(LeafType::Str);
    }
    streamer_type_to_leaf(el.el_type)
}

/// Map a `TStreamerInfo` basic-type code (`fStreamerType`, ROOT's `EDataType`)
/// to its [`LeafType`], for split member sub-branches. `None` for unsupported.
pub(super) fn streamer_type_to_leaf(st: i32) -> Option<LeafType> {
    Some(match st {
        1 => LeafType::I8,        // kChar
        2 => LeafType::I16,       // kShort
        3 => LeafType::I32,       // kInt
        4 | 16 => LeafType::I64,  // kLong / kLong64
        5 => LeafType::F32,       // kFloat
        8 => LeafType::F64,       // kDouble
        11 => LeafType::U8,       // kUChar
        12 => LeafType::U16,      // kUShort
        13 => LeafType::U32,      // kUInt
        14 | 17 => LeafType::U64, // kULong / kULong64
        18 => LeafType::Bool,     // kBool
        _ => return None,
    })
}

/// Strip a `vector<...>` / `std::vector<...>` wrapper, returning the (trimmed)
/// element type spelling.
fn strip_vector(class_name: &str) -> Option<&str> {
    Some(
        class_name
            .strip_prefix("vector<")
            .or_else(|| class_name.strip_prefix("std::vector<"))?
            .strip_suffix('>')?
            .trim(),
    )
}

/// Map a C++ fundamental type name to its [`LeafType`], or `None` if unsupported.
fn basic_leaf_type(name: &str) -> Option<LeafType> {
    Some(match name {
        "float" => LeafType::F32,
        "double" => LeafType::F64,
        "int" | "Int_t" => LeafType::I32,
        "unsigned int" | "UInt_t" => LeafType::U32,
        "short" | "Short_t" => LeafType::I16,
        "unsigned short" | "UShort_t" => LeafType::U16,
        "char" | "Char_t" | "int8_t" => LeafType::I8,
        "unsigned char" | "UChar_t" | "uint8_t" => LeafType::U8,
        "bool" | "Bool_t" => LeafType::Bool,
        "long" | "long long" | "Long64_t" | "Long_t" => LeafType::I64,
        "unsigned long" | "unsigned long long" | "ULong64_t" | "ULong_t" => LeafType::U64,
        // std::vector<std::string>: the element is a string (decoded specially).
        "string" | "std::string" => LeafType::Str,
        _ => return None,
    })
}

/// Strip a single-element STL container wrapper — `vector` / `set` / `multiset`
/// (bare or `std::`) — returning the trimmed element type. `std::set` and
/// `std::vector` share an object-wise on-disk layout (a 10-byte streamer header
/// then the contiguous elements), so the reader treats them the same.
fn strip_collection(class_name: &str) -> Option<&str> {
    for prefix in [
        "vector<",
        "std::vector<",
        "set<",
        "std::set<",
        "multiset<",
        "std::multiset<",
    ] {
        if let Some(inner) = class_name.strip_prefix(prefix) {
            return inner.strip_suffix('>').map(str::trim);
        }
    }
    None
}

/// Map an unsplit single-element STL container (`std::vector<T>`/`std::set<T>`/…)
/// class name to its element [`LeafType`], or `None` for an unsupported element.
pub(super) fn parse_vector_elem(class_name: &str) -> Option<LeafType> {
    basic_leaf_type(strip_collection(class_name)?)
}

/// Map a `std::vector<std::vector<T>>` class name to `T`'s element [`LeafType`],
/// or `None` if it is not a doubly-nested vector of a supported basic type.
/// `std::vector<std::vector<std::string>>` is excluded (the inner string
/// decoding does not compose with the nested reader).
pub(super) fn parse_nested_vector_elem(class_name: &str) -> Option<LeafType> {
    let inner = strip_vector(class_name)?; // e.g. "vector<int>"
    let elem = basic_leaf_type(strip_vector(inner)?)?;
    if elem == LeafType::Str {
        return None;
    }
    Some(elem)
}
