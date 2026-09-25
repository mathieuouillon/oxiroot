//! Parsing the `TTree` object: the tree, its branches, leaves, friends and
//! aliases.

use oxiroot_io_core::{
    read_named, read_object_base, Error, RBuffer, Result, StreamerElement, StreamerRegistry,
    TagReader, K_BYTE_COUNT_MASK,
};

use super::members::{member_int, member_str, walk_members, Members};
use super::types::{
    member_leaf_type, parse_nested_vector_elem, parse_vector_elem, streamer_type_to_leaf,
};
use super::{Branch, Friend, Leaf, ObjectMember, TreeIndex, TreeReader};
use crate::value::LeafType;

/// Parse a decompressed `TTree` object (`keylen` is its key's header length),
/// driving the member layout from the file's `TStreamerInfo`.
pub(super) fn read_tree(
    object: &[u8],
    keylen: usize,
    reg: &StreamerRegistry,
    class_name: &str,
) -> Result<TreeReader> {
    let mut r = RBuffer::new(object);
    let mut tags = TagReader::new(keylen);

    // A `TNtuple`/`TNtupleD` object is a `TTree` base wrapped in one extra
    // `{byte count, version}` header (plus a trailing `Int_t fNvar`). Peel that
    // outer header first; the inner `TTree` base is then read like a plain tree.
    let outer = if class_name == "TNtuple" || class_name == "TNtupleD" {
        Some(r.read_version()?)
    } else {
        None
    };
    let tree_hdr = r.read_version()?; // TTree
    let info = reg
        .get_at("TTree", i32::from(tree_hdr.version))
        .ok_or_else(|| Error::MissingStreamerInfo {
            class: "TTree".to_string(),
        })?;
    let mut out = Members::new();
    let mut branches = Vec::new();
    let mut unsupported = Vec::new();
    let mut friends = Vec::new();
    let mut aliases = Vec::new();
    let mut index = None;
    {
        let mut on_object = |name: &str, rb: &mut RBuffer| -> Result<()> {
            // The TTree streamer order (after the scalar members) is fBranches,
            // fLeaves, fAliases, fIndexValues, fIndex, fTreeIndex, fFriends, ….
            // We read the three we consume (fBranches, fAliases, fFriends) and
            // step over the rest — each with its correct on-disk framing so the
            // cursor stays aligned all the way to fFriends.
            match name {
                "fBranches" => branches = read_branch_array(rb, &mut tags, &mut unsupported, reg)?,
                "fAliases" => aliases = read_aliases(rb, &mut tags)?,
                "fFriends" => friends = read_friends(rb, &mut tags)?,
                // `TArrayD`/`TArrayI` (the legacy inline index): `[Int_t n][n
                // elements]`, with no version header.
                "fIndexValues" => {
                    let n = rb.be_i32()?.max(0) as usize;
                    rb.skip(n * 8)?;
                }
                "fIndex" => {
                    let n = rb.be_i32()?.max(0) as usize;
                    rb.skip(n * 4)?;
                }
                // `TVirtualIndex*` is an object pointer (ROOT's object protocol);
                // a null pointer is a bare 4-byte 0, which the tag header consumes.
                "fTreeIndex" => {
                    let h = tags.read_header(rb)?;
                    if h.class_name.as_deref() == Some("TTreeIndex") {
                        index = read_tree_index(rb)?;
                    }
                    if let Some(end) = h.end {
                        rb.seek(end)?;
                    }
                }
                // Inline objects carrying a version header (fLeaves `TObjArray`,
                // fIOFeatures).
                _ => skip_object(rb)?,
            }
            Ok(())
        };
        walk_members(
            &mut r,
            reg,
            &info.elements,
            &mut out,
            &mut on_object,
            "fFriends",
        )?;
    }

    // Everything after fFriends is unneeded; jump to the object's end (the outer
    // subclass wrapper's end for a TNtuple, so its trailing fNvar is skipped;
    // otherwise the TTree header's end).
    if let Some(end) = outer.and_then(|o| o.end).or(tree_hdr.end) {
        r.seek(end)?;
    }

    Ok(TreeReader {
        name: member_str(&out, "fName"),
        entries: member_int(&out, "fEntries").max(0) as u64,
        branches,
        unsupported,
        streamer_classes: Vec::new(),
        friends,
        aliases,
        index,
    })
}

/// Read a `TTreeIndex` body, the cursor just past its object-pointer header.
///
/// Version 2 (what ROOT 6 writes) is `TVirtualIndex` — a `Named` — then
/// `fMajorName`, `fMinorName`, `fN`, and three `Long64_t[fN]` arrays written by
/// its own streamer, so they carry no per-array framing: the major keys, the
/// minor keys, and the entry each key names. An older version packed the two
/// keys into one array; it is left unread rather than guessed at.
fn read_tree_index(r: &mut RBuffer) -> Result<Option<TreeIndex>> {
    let header = r.read_version()?;
    if header.version < 2 {
        return Ok(None);
    }
    skip_object(r)?; // the TVirtualIndex (Named) base
    let major_name = r.string()?;
    let minor_name = r.string()?;
    let n = r.be_i64()?;
    let n = usize::try_from(n).map_err(|_| {
        Error::Format(format!(
            "tree index declares {n} entries, which cannot be read"
        ))
    })?;
    // Three arrays of n: refuse a count the record cannot hold rather than
    // allocating for it.
    if n.saturating_mul(24) > r.remaining() {
        return Err(Error::Format(format!(
            "tree index declares {n} entries, more than its record holds"
        )));
    }
    let read_column =
        |r: &mut RBuffer| -> Result<Vec<i64>> { (0..n).map(|_| r.be_i64()).collect() };
    let major = read_column(r)?;
    let minor = read_column(r)?;
    let entries = read_column(r)?;
    let keys: Vec<(i64, i64)> = major.into_iter().zip(minor).collect();
    let entries: Vec<u64> = entries.into_iter().map(|e| e.max(0) as u64).collect();
    Ok(Some(TreeIndex::new(major_name, minor_name, keys, entries)))
}

/// Position the cursor at a `TList`-valued member's body and return its object
/// end offset (when known) and element count. ROOT frames a `TList*` member two
/// ways: *inline* — a bare version header (`fAliases`) — or via the *object
/// protocol* — a byte count then a class tag, then the list's own version header
/// (`fFriends`). A null pointer (`0`) reads as an empty list. After this returns,
/// the next read is the first element's object header.
fn open_tlist(r: &mut RBuffer, tags: &mut TagReader) -> Result<(Option<usize>, i32)> {
    const K_NEW_CLASS_TAG: u32 = 0xFFFF_FFFF;
    const K_CLASS_MASK: u32 = 0x8000_0000;

    let start = r.pos();
    let word = r.be_u32()?;
    if word == 0 {
        return Ok((None, 0)); // null pointer: no list
    }
    r.seek(start)?;
    let end = if word & K_BYTE_COUNT_MASK != 0 {
        // Distinguish the two framings by the word after the byte count: a class
        // tag (new-class marker or a high-bit class reference) means the object
        // protocol; anything else is a `{version, …}` header read inline.
        let after = r.pos() + 4;
        r.seek(after)?;
        let tag = r.be_u32()?;
        r.seek(start)?;
        if tag == K_NEW_CLASS_TAG || tag & K_CLASS_MASK != 0 {
            let header = tags.read_header(r)?;
            r.read_version()?; // the list's own (inner) version header
            header.end
        } else {
            r.read_version()?.end
        }
    } else {
        r.read_version()?.end
    };
    read_object_base(r)?;
    r.string()?; // the list's fName
    let n = r.be_i32()?.max(0);
    Ok((end, n))
}

/// Read a `TTree`'s `fFriends` (`TList<TFriendElement>` — the friends added with
/// `TTree::AddFriend`). The cursor is positioned at the `fFriends` member; a null
/// pointer (no friends) reads as an empty list. Each `TFriendElement` carries the
/// friend's tree name (`fTreeName`), the alias (`Named::fName`), and the file it
/// lives in (`Named::fTitle`, empty for a same-file friend).
fn read_friends(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<Friend>> {
    let (end, n) = open_tlist(r, tags)?;
    let mut friends = Vec::with_capacity(n.max(0) as usize);
    for _ in 0..n {
        let elem = tags.read_header(r)?;
        if elem.class_name.as_deref() == Some("TFriendElement") {
            let vh = r.read_version()?;
            let named = read_named(r)?; // fName = alias, fTitle = file name
            let tree_name = r.string()?; // fTreeName
            friends.push(Friend {
                tree_name,
                file_name: named.title,
                alias: named.name,
            });
            if let Some(end) = vh.end {
                r.seek(end)?;
            }
        }
        if let Some(end) = elem.end {
            r.seek(end)?;
        }
        r.string()?; // the per-object option string TList writes after each entry
    }
    if let Some(end) = end {
        r.seek(end)?;
    }
    Ok(friends)
}

/// Read a `TTree`'s `fAliases` (`TList<Named>` — the `(name, expression)` pairs
/// set with `TTree::SetAlias`). The cursor is positioned at the `fAliases` member;
/// a null pointer (no aliases) reads as an empty list. Each entry's `fName` is the
/// alias and `fTitle` is the expression it stands for.
fn read_aliases(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<(String, String)>> {
    let (end, n) = open_tlist(r, tags)?;
    let mut aliases = Vec::with_capacity(n.max(0) as usize);
    for _ in 0..n {
        let elem = tags.read_header(r)?;
        if elem.class_name.as_deref() == Some("TNamed") {
            let named = read_named(r)?;
            aliases.push((named.name, named.title));
        }
        if let Some(end) = elem.end {
            r.seek(end)?;
        }
        r.string()?; // the per-object option string
    }
    if let Some(end) = end {
        r.seek(end)?;
    }
    Ok(aliases)
}

/// Read a `TObjArray` of `TBranch`es. Branch classes we don't yet handle are
/// skipped via the object byte count.
fn read_branch_array(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut branches = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        match header.class_name.as_deref() {
            Some("TBranch") => {
                branches.extend(read_branch(r, tags, diag, reg)?);
            }
            Some("TBranchElement") => {
                branches.extend(read_branch_element(r, tags, diag, reg)?);
            }
            Some("TBranchObject") => {
                branches.extend(read_branch_object(r, tags, diag, reg)?);
            }
            Some(other) => diag.push((other.to_string(), "unsupported branch class".to_string())),
            None => {}
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(branches)
}

/// Read a `TBranch`'s scalar members (`fName`, `fWriteBasket`, `fBasketSeek`, …)
/// by walking `reg`'s `TBranch` streamer elements, dispatching the object
/// members (`fBranches`/`fLeaves`/`fBaskets`) to the readers that consume them.
/// Shared by [`read_branch`] and (as the `TBranch` base) [`read_branch_element`].
/// Returns the captured members, the sub-branches, and the leaves.
fn read_tbranch_base(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
    elements: &[StreamerElement],
    stop_after: &str,
) -> Result<(Members, Vec<Branch>, Vec<Leaf>)> {
    let mut out = Members::new();
    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    {
        let mut on_object = |name: &str, rb: &mut RBuffer| -> Result<()> {
            match name {
                "fBranches" => sub = read_branch_array(rb, tags, diag, reg)?,
                "fLeaves" => leaves = read_leaf_array(rb, tags)?,
                "fBaskets" => read_skip_array(rb, tags)?,
                _ => skip_object(rb)?, // fIOFeatures, and any other object member
            }
            Ok(())
        };
        walk_members(r, reg, elements, &mut out, &mut on_object, stop_after)?;
    }
    Ok((out, sub, leaves))
}

/// Assemble `(write_basket, basket_entry, basket_seek, basket_bytes)` from a
/// branch's captured members: `fBasketEntry` is truncated to `fWriteBasket` (the
/// live baskets), `fBasketSeek`/`fBasketBytes` are clamped to non-negative and
/// taken up to the live basket count.
fn basket_locators(out: &Members) -> (usize, Vec<i64>, Vec<u64>, Vec<u64>) {
    let write_basket = member_int(out, "fWriteBasket").max(0) as usize;
    let basket_entry = out
        .get("fBasketEntry")
        .map(|m| m.ints().iter().copied().take(write_basket).collect())
        .unwrap_or_default();
    let basket_seek = out
        .get("fBasketSeek")
        .map(|m| m.ints().iter().map(|&s| s.max(0) as u64).collect())
        .unwrap_or_default();
    let basket_bytes = out
        .get("fBasketBytes")
        .map(|m| {
            m.ints()
                .iter()
                .take(write_basket)
                .map(|&b| b.max(0) as u64)
                .collect()
        })
        .unwrap_or_default();
    (write_basket, basket_entry, basket_seek, basket_bytes)
}

/// Read one `TBranch` body (after its object header) by walking the file's
/// `TBranch` streamer elements. Yields one [`Branch`] for a single-leaf branch,
/// several for a leaflist branch, or none (recorded in `diag`) when the branch
/// has sub-branches or an unsupported leaf type.
pub(super) fn read_branch(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    let info = reg
        .get("TBranch")
        .ok_or_else(|| Error::MissingStreamerInfo {
            class: "TBranch".to_string(),
        })?;
    let _vh = r.read_version()?; // TBranch
    let (out, sub, leaves) = read_tbranch_base(r, tags, diag, reg, &info.elements, "")?;

    let name = member_str(&out, "fName");
    let title = member_str(&out, "fTitle");
    let (write_basket, basket_entry, basket_seek, basket_bytes) = basket_locators(&out);

    // A branch with its own sub-branches (other than the split-element path) is
    // not handled here.
    if !sub.is_empty() {
        diag.push((
            name,
            "branch with sub-branches is not supported".to_string(),
        ));
        return Ok(Vec::new());
    }
    if leaves.is_empty() {
        diag.push((name, "no supported leaf type".to_string()));
        return Ok(Vec::new());
    }
    if leaves.len() > 1 && leaves.iter().any(|l| l.leaf_type == LeafType::Str) {
        diag.push((
            name,
            "leaflist containing a string leaf is not supported".to_string(),
        ));
        return Ok(Vec::new());
    }

    // Single-leaf branch: the branch *is* the leaf.
    if leaves.len() == 1 {
        let leaf = &leaves[0];
        return Ok(vec![Branch {
            name,
            title,
            leaf_type: leaf.leaf_type,
            leaf_len: leaf.len,
            n_baskets: write_basket,
            basket_seek,
            basket_bytes,
            basket_entry,
            elem_header: 0,
            leaflist: None,
            dims: parse_dims(&leaf.title),
            nested_elem: None,
            object_member: None,
        }]);
    }

    // Leaflist branch: each entry packs the fixed-size leaves at their offsets;
    // expose each as a `branch.leaf` sub-branch sliced from the per-entry stride.
    let stride = leaves
        .iter()
        .map(|l| l.offset + l.len.max(1) as usize * l.leaf_type.size())
        .max()
        .unwrap_or(0);
    let out = leaves
        .iter()
        .map(|leaf| Branch {
            name: format!("{}.{}", name, leaf.name),
            title: title.clone(),
            leaf_type: leaf.leaf_type,
            leaf_len: leaf.len,
            n_baskets: write_basket,
            basket_seek: basket_seek.clone(),
            basket_bytes: basket_bytes.clone(),
            basket_entry: basket_entry.clone(),
            elem_header: 0,
            leaflist: Some((leaf.offset, stride)),
            dims: parse_dims(&leaf.title),
            nested_elem: None,
            object_member: None,
        })
        .collect();
    Ok(out)
}

/// Read one `TBranchElement` (v10) body, after its object header. Returns the
/// readable branches it contributes:
/// - an unsplit `std::vector<T>` (`fType` 0) → one branch (element type from
///   `fClassName`, each entry prefixed by a 10-byte streamer header);
/// - a split STL/clones collection (`fType` 3/4) → its member sub-branches (the
///   parent holds no data of its own);
/// - a split member sub-branch (`fType` 41/31) → one jagged branch (element type
///   from `fStreamerType`, no per-entry header).
///
/// Unsupported element types contribute nothing.
fn read_branch_element(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    let info = reg
        .get("TBranchElement")
        .ok_or_else(|| Error::MissingStreamerInfo {
            class: "TBranchElement".to_string(),
        })?;
    let _vh = r.read_version()?; // TBranchElement — the object's own version
                                 // Walk the TBranchElement elements: the first is the `TBranch` base (read
                                 // in place via its own streamer info, capturing the basket locators and the
                                 // sub-branches), then fClassName/fType/fStreamerType. We stop after
                                 // fStreamerType — fMaximum/fBranchCount* are not needed and would mean
                                 // streaming object pointers.
    let (out, sub, _leaves) =
        read_tbranch_base(r, tags, diag, reg, &info.elements, "fStreamerType")?;

    let name = member_str(&out, "fName");
    let class_name = member_str(&out, "fClassName");
    let f_type = member_int(&out, "fType") as i32;
    let f_streamer_type = member_int(&out, "fStreamerType") as i32;
    let (write_basket, basket_entry, basket_seek, basket_bytes) = basket_locators(&out);

    // Any branch with sub-branches is a split parent — an STL/clones collection
    // (`fType` 3/4), or a split single object or its sub-object member (`fType`
    // 0/2). It holds no data itself; its members do, and they were just parsed
    // into `sub`.
    if !sub.is_empty() {
        return Ok(sub);
    }

    // An unsplit `std::vector<std::vector<T>>` (`0`) reads as a doubly-nested
    // collection: the inner element type drives a [`BranchValues::Nested`].
    let nested_elem = if f_type == 0 {
        parse_nested_vector_elem(&class_name)
    } else {
        None
    };

    // Pick the element type and per-entry header for a data-bearing leaf branch:
    // - a member sub-branch (STL `41`, TClonesArray `31`) — a jagged array typed
    //   by `fStreamerType`, no header;
    // - an unsplit `std::vector<T>` (`0`, class `vector<...>`) — typed by the
    //   class, each entry prefixed by the 10-byte streamer header;
    // - a scalar member of a split single object (`0`, a plain class) — one value
    //   per entry, typed by `fStreamerType`, no header.
    let member = f_type == 41 || f_type == 31;
    let (leaf_type, elem_header) = if let Some(elem) = nested_elem {
        (Some(elem), 10) // the 10-byte header carries the outer count
    } else if member {
        (streamer_type_to_leaf(f_streamer_type), 0)
    } else if let Some(elem) = parse_vector_elem(&class_name) {
        (Some(elem), 10)
    } else if f_type == 0 {
        (streamer_type_to_leaf(f_streamer_type), 0)
    } else {
        (None, 0)
    };
    let Some(leaf_type) = leaf_type else {
        diag.push((
            name,
            format!("unsupported TBranchElement (fType={f_type}, class {class_name:?})"),
        ));
        return Ok(Vec::new());
    };
    Ok(vec![Branch {
        name,
        title: member_str(&out, "fTitle"),
        leaf_type,
        leaf_len: 1,
        n_baskets: write_basket,
        basket_seek,
        basket_bytes,
        basket_entry,
        elem_header,
        leaflist: None,
        dims: Vec::new(),
        nested_elem,
        object_member: None,
    }])
}

/// Read one old-style unsplit `TBranchObject` (leaf `TLeafObject`): a whole
/// object of class `fClassName` is stored per entry, with no sub-branches. We
/// synthesize one column per (basic or string) member of the object class —
/// named `branch.member`, mirroring the split-object path — each decoding that
/// member out of every entry's object. Base classes and members of types we
/// can't decode are skipped; if none remain, the branch is recorded in `diag`.
fn read_branch_object(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    let info = reg
        .get("TBranchObject")
        .ok_or_else(|| Error::MissingStreamerInfo {
            class: "TBranchObject".to_string(),
        })?;
    let _vh = r.read_version()?; // TBranchObject
    let (out, sub, _leaves) = read_tbranch_base(r, tags, diag, reg, &info.elements, "")?;

    let name = member_str(&out, "fName");
    let title = member_str(&out, "fTitle");
    let class_name = member_str(&out, "fClassName");
    let (write_basket, basket_entry, basket_seek, basket_bytes) = basket_locators(&out);

    if !sub.is_empty() {
        diag.push((
            name,
            "TBranchObject with sub-branches is not supported".to_string(),
        ));
        return Ok(Vec::new());
    }
    let Some(class_info) = reg.get(&class_name) else {
        diag.push((
            name,
            format!("TBranchObject class {class_name:?} has no streamer info"),
        ));
        return Ok(Vec::new());
    };

    // One synthesized column per decodable (basic / string) member of the object.
    let mut branches = Vec::new();
    for el in &class_info.elements {
        if el.element_class == "TStreamerBase" {
            continue; // base-class members (e.g. TObject's) are not surfaced
        }
        let Some(leaf_type) = member_leaf_type(el) else {
            continue;
        };
        branches.push(Branch {
            name: format!("{name}.{}", el.name),
            title: title.clone(),
            leaf_type,
            leaf_len: 1,
            n_baskets: write_basket,
            basket_seek: basket_seek.clone(),
            basket_bytes: basket_bytes.clone(),
            basket_entry: basket_entry.clone(),
            elem_header: 0,
            leaflist: None,
            dims: Vec::new(),
            nested_elem: None,
            object_member: Some(ObjectMember {
                member: el.name.clone(),
                class_elements: class_info.elements.clone(),
            }),
        });
    }
    if branches.is_empty() {
        diag.push((
            name,
            format!("TBranchObject class {class_name:?} has no readable members"),
        ));
    }
    Ok(branches)
}

/// Read a `TObjArray` of `TLeaf`s, returning `(type, fLen)` for each supported
/// leaf (unsupported leaves are skipped).
fn read_leaf_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<Leaf>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut leaves = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if let Some(class) = header.class_name.clone() {
            if let Some(leaf) = read_leaf(r, &class)? {
                leaves.push(leaf);
            }
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(leaves)
}

/// Read one `TLeaf*` (v1) body enough to recover its name, element type, `fLen`,
/// and `fOffset` (its byte position within an entry, for leaflist branches).
fn read_leaf(r: &mut RBuffer, class: &str) -> Result<Option<Leaf>> {
    r.read_version()?; // TLeafX (v1) — the leaf subclass wrapper
    r.read_version()?; // TLeaf base (v2)
    let named = read_named(r)?; // fName, fTitle
    let len = r.be_i32()?; // fLen
    r.be_i32()?; // fLenType
    let offset = r.be_i32()?; // fOffset
    r.u8()?; // fIsRange
    let unsigned = r.u8()? != 0; // fIsUnsigned
                                 // fLeafCount, fMinimum, fMaximum follow; we skip to the leaf's end via the
                                 // caller's byte count.
    Ok(LeafType::from_leaf(class, unsigned).map(|leaf_type| Leaf {
        name: named.name,
        title: named.title,
        leaf_type,
        len,
        offset: offset.max(0) as usize,
    }))
}

/// Parse the per-entry array shape from a leaf title: `x[2][3]` → `[2, 3]`,
/// `x[5]` → `[5]`, a scalar (no brackets) → `[]`.
fn parse_dims(title: &str) -> Vec<usize> {
    let mut dims = Vec::new();
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        if let Ok(n) = after[..close].parse::<usize>() {
            dims.push(n);
        }
        rest = &after[close + 1..];
    }
    dims
}

/// Read a `TObjArray` and discard it (used for `fBaskets`, always empty here).
fn read_skip_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<()> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(())
}

/// Read the `{version}` + `TObject` + name prefix common to `TObjArray`/`TList`.
fn read_version_tobject_header(r: &mut RBuffer) -> Result<()> {
    r.read_version()?;
    read_object_base(r)?;
    r.string()?; // fName
    Ok(())
}

/// Skip an inline object member, using its version-header byte count when
/// present, else assuming a single trailing byte (e.g. `TIOFeatures`).
pub(super) fn skip_object(r: &mut RBuffer) -> Result<()> {
    let vh = r.read_version()?;
    match vh.end {
        Some(end) => r.seek(end)?,
        None => {
            r.u8()?;
        }
    }
    Ok(())
}
