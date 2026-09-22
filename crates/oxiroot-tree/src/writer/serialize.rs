//! The `TTree` object as ROOT streams it: the tree, its `TBranch` and
//! `TBranchElement` records, and their `TLeaf*` leaves.

use std::collections::HashMap;

use oxiroot_io_core::{
    write_tnamed, write_tobject, CountToken, Patch, TKey, WBuffer, K_BYTE_COUNT_MASK,
};

use super::baskets::BasketRec;
use super::branch::{
    class_checksum, member_type_info, vec_row_lengths, Branch, BranchKind, SplitMember,
};
use super::layout::Kind;
use super::{LeafRefs, K_MAP_OFFSET, OBJ_BITS};

/// Write a byte-counted att base (`TAttLine`/`Fill`/`Marker`).
fn write_attline(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(602);
    w.be_i16(1);
    w.be_i16(1);
    w.end_object(t);
}
fn write_attfill(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(0);
    w.be_i16(1001);
    w.end_object(t);
}
fn write_attmarker(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(1);
    w.be_i16(1);
    w.be_f32(1.0);
    w.end_object(t);
}

/// Write `ROOT::TIOFeatures` (a byte-counted object with a single `fIOBits`).
fn write_iofeatures(w: &mut WBuffer) {
    let t = w.begin_object(1);
    w.u8(0); // fIOBits
    w.end_object(t);
}

/// Begin a `ReadObjectAny` object: a byte-count placeholder then a fresh class
/// tag (`kNewClassTag` + name). Every object is written with a fresh tag (no
/// back-references), which ROOT/uproot read correctly. Returns the byte-count
/// patch.
fn begin_object_any(w: &mut WBuffer, class: &str) -> Patch {
    let bc = w.reserve(4); // byte-count placeholder
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0); // NUL terminator
    bc
}

/// Finish a `ReadObjectAny` object, back-patching its byte count (which covers
/// everything after the 4-byte count word).
fn end_object_any(w: &mut WBuffer, bc: Patch) {
    let start = w.patch_offset(bc) + 4;
    let nbytes = (w.len() - start) as u32 | K_BYTE_COUNT_MASK;
    w.patch_be_u32(bc, nbytes);
}

/// Build the `TObjArray` of branches, then the tree-level `TObjArray` of leaves.
pub(super) fn build_tree_object(
    tree_name: &str,
    branches: &[&Branch],
    baskets: &[Vec<BasketRec>],
    n_entries: i64,
    tot_bytes: i64,
    big: bool,
) -> Vec<u8> {
    // ROOT resolves object references relative to `-keylen` of the TTree key; we
    // must use the same keylen so a jagged leaf's `fLeafCount` reference lands on
    // the count leaf. The wrapping key is wider in the big (64-bit) container form,
    // so the keylen — and thus every baked reference — depends on `big`.
    let keylen = TKey::header_len("TTree", tree_name, "", big) as u32;
    let mut refs: LeafRefs = HashMap::new();

    let mut w = WBuffer::new();
    let tree = w.begin_object(20); // TTree v20
    write_tnamed(&mut w, OBJ_BITS, tree_name, "");
    write_attline(&mut w);
    write_attfill(&mut w);
    write_attmarker(&mut w);

    w.be_i64(n_entries); // fEntries
    w.be_i64(tot_bytes); // fTotBytes
    w.be_i64(tot_bytes); // fZipBytes
    w.be_i64(0); // fSavedBytes
    w.be_i64(0); // fFlushedBytes
    w.be_f64(1.0); // fWeight
    w.be_i32(0); // fTimerInterval
    w.be_i32(25); // fScanField
    w.be_i32(0); // fUpdate
    w.be_i32(1000); // fDefaultEntryOffsetLen
    w.be_i32(0); // fNClusterRange
    w.be_i64(1_000_000_000_000); // fMaxEntries
    w.be_i64(1_000_000_000_000); // fMaxEntryLoop
    w.be_i64(0); // fMaxVirtualSize
    w.be_i64(-300_000_000); // fAutoSave
    w.be_i64(-30_000_000); // fAutoFlush
    w.be_i64(1_000_000); // fEstimate
    w.u8(0); // fClusterRangeEnd (empty array marker)
    w.u8(0); // fClusterSize (empty array marker)
    write_iofeatures(&mut w);

    write_branch_array(&mut w, branches, baskets, n_entries, keylen, &mut refs);
    write_tree_leaf_array(&mut w, branches, &refs);

    w.be_u32(0); // fAliases (null TList*)
    w.be_i32(0); // fIndexValues (TArrayD, empty)
    w.be_i32(0); // fIndex (TArrayI, empty)
    w.be_u32(0); // fTreeIndex (null)
    w.be_u32(0); // fFriends (null)
    w.be_u32(0); // fUserInfo (null)
    w.be_u32(0); // fBranchRef (null)

    w.end_object(tree);
    w.into_vec()
}

/// The `TObjArray` header (`{version} TObject name fSize fLowerBound`).
fn obj_array_header(w: &mut WBuffer, size: usize) -> CountToken {
    let tok = w.begin_object(3); // TObjArray v3
    write_tobject(w, 0);
    w.string("");
    w.be_i32(size as i32);
    w.be_i32(0); // fLowerBound
    tok
}

/// Write `fBranches`: a `TObjArray<TBranch>`.
fn write_branch_array(
    w: &mut WBuffer,
    branches: &[&Branch],
    baskets: &[Vec<BasketRec>],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tok = obj_array_header(w, branches.len());
    for (&b, group) in branches.iter().zip(baskets) {
        if b.split().is_some() {
            // The parent's object-map position: its sub-branches reference it
            // (`fBranchCount`) so ROOT can find the collection they belong to.
            let parent_ref = w.len() as u32 + keylen + K_MAP_OFFSET;
            let bc = begin_object_any(w, "TBranchElement");
            write_split_parent(w, b, group, n_entries, keylen, parent_ref, refs);
            end_object_any(w, bc);
        } else if b.stl_vector() {
            let bc = begin_object_any(w, "TBranchElement");
            write_branch_element(w, b, group, n_entries, keylen, refs);
            end_object_any(w, bc);
        } else {
            let bc = begin_object_any(w, "TBranch");
            write_branch(w, b, group, n_entries, keylen, refs);
            end_object_any(w, bc);
        }
    }
    w.end_object(tok);
}

/// Write one `TBranchElement` (v10): the `TBranch` base, then the element
/// members (`fClassName`, `fCheckSum`, …) describing the `std::vector<T>`.
fn write_branch_element(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tok = w.begin_object(10); // TBranchElement v10
    write_branch(w, branch, group, n_entries, keylen, refs); // the TBranch base
    let (class_name, checksum) = branch.stl_info();
    w.string(class_name); // fClassName, e.g. "vector<float>"
    w.string(""); // fParentName
    w.string(""); // fClonesName
    w.be_u32(checksum); // fCheckSum
    w.be_u16(6); // fClassVersion (std::vector)
    w.be_i32(-1); // fID
    w.be_i32(0); // fType
    w.be_i32(-1); // fStreamerType
    w.be_i32(0); // fMaximum
    w.be_u32(0); // fBranchCount (null)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(tok);
}

/// Write `fBasketBytes`/`fBasketEntry`/`fBasketSeek` for a branch's `group` of
/// baskets (each `int[fMaxBaskets]` / `i64[fMaxBaskets]`, preceded by a marker
/// byte). `fBasketEntry` is cumulative — the entry start of each basket, plus a
/// trailing total — with the unused tail zeroed.
fn write_basket_arrays(w: &mut WBuffer, group: &[BasketRec], max_baskets: i32) {
    let cap = max_baskets as usize;
    // cumulative[i] = entries before basket i; cumulative[group.len()] = total.
    let mut cumulative = Vec::with_capacity(group.len() + 1);
    let mut acc = 0i64;
    cumulative.push(0);
    for b in group {
        acc += i64::from(b.n_entries);
        cumulative.push(acc);
    }

    w.u8(1);
    for i in 0..cap {
        w.be_i32(group.get(i).map_or(0, |b| b.nbytes as i32));
    }
    w.u8(1);
    for i in 0..cap {
        w.be_i64(cumulative.get(i).copied().unwrap_or(0));
    }
    w.u8(1);
    for i in 0..cap {
        w.be_i64(group.get(i).map_or(0, |b| b.seek as i64));
    }
}

/// Write a `TLeafElement` (v1): the `TLeaf` base (`fLen`/`fLenType`/…/`fLeafCount`)
/// then the element extras `fID`/`fType`. `f_leaf_count` is written verbatim — a
/// null (`0`) or an object back-reference to the counter leaf.
fn write_leaf_element(
    w: &mut WBuffer,
    name: &str,
    title: &str,
    len_type: i32,
    f_id: i32,
    f_type: i32,
    f_leaf_count: u32,
) {
    let outer = w.begin_object(1); // TLeafElement v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, name, title);
    w.be_i32(1); // fLen
    w.be_i32(len_type); // fLenType
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(0); // fIsUnsigned
    w.be_u32(f_leaf_count); // fLeafCount
    w.end_object(base);
    w.be_i32(f_id); // fID
    w.be_i32(f_type); // fType
    w.end_object(outer);
}

/// Write the parent `TBranchElement` (`fType=4`) of a split
/// `std::vector<MyStruct>`: the `TBranch` base (count basket + the `name_`
/// counter leaf), the member sub-branches in `fBranches`, then the element
/// members (`fClassName="vector<MyStruct>"`, `fType=4`, `fMaximum=max count`).
///
/// The counter leaf (`name_`) is written *inline* the first time it is needed —
/// inside the first sub-branch's leaf `fLeafCount` — and back-referenced here and
/// by the other sub-branches, so all four references resolve to one object (ROOT
/// relies on this when wiring `leaf->GetBranch()`/`GetLeafCount()`).
fn write_split_parent(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    parent_ref: u32,
    refs: &mut LeafRefs,
) {
    let spec = branch.split().expect("split spec");
    let counter = format!("{}_", branch.name);
    let count_basket = &group[0];
    let checksum = class_checksum(&spec.class_name, &spec.members);
    let max_count = vec_row_lengths(&spec.members[0].values)
        .into_iter()
        .max()
        .unwrap_or(0);
    let max_baskets = 10i32;

    let te = w.begin_object(10); // TBranchElement v10
    let tb = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &counter);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(1000); // fEntryOffsetLen
    w.be_i32(1); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(99); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(count_basket.nbytes as i64); // fTotBytes
    w.be_i64(count_basket.nbytes as i64); // fZipBytes

    // fBranches: the member sub-branches. The first writes `counter` inline.
    let sub_tok = obj_array_header(w, spec.members.len());
    for (i, m) in spec.members.iter().enumerate() {
        let bc = begin_object_any(w, "TBranchElement");
        write_split_sub(
            w,
            &branch.name,
            &counter,
            &spec.class_name,
            checksum,
            m,
            i as i32,
            &group[i + 1],
            n_entries,
            keylen,
            parent_ref,
            refs,
            i == 0,
        );
        end_object_any(w, bc);
    }
    w.end_object(sub_tok);

    // fLeaves: one entry, an object back-reference to the inline `counter` leaf.
    let leaf_tok = obj_array_header(w, 1);
    w.be_u32(refs.get(&counter).copied().unwrap_or(0));
    w.end_object(leaf_tok);

    let baskets = obj_array_header(w, 0); // fBaskets (empty)
    w.end_object(baskets);
    write_basket_arrays(w, std::slice::from_ref(count_basket), max_baskets);
    w.string(""); // fFileName
    w.end_object(tb);

    // TBranchElement members for the collection itself.
    w.string(&format!("vector<{}>", spec.class_name)); // fClassName
    w.string(""); // fParentName
    w.string(&spec.class_name); // fClonesName
    w.be_u32(0); // fCheckSum (ROOT does not validate the STL parent's checksum)
    w.be_u16(6); // fClassVersion (std::vector)
    w.be_i32(-1); // fID
    w.be_i32(4); // fType (split STL collection)
    w.be_i32(-1); // fStreamerType
    w.be_i32(max_count); // fMaximum (largest per-entry element count)
    w.be_u32(0); // fBranchCount (null)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(te);
}

/// Write one member sub-branch (`fType=41`) of a split collection: a jagged
/// array of the member type, counted by the parent's `counter` leaf. When
/// `write_counter_inline` is set (the first member), the `counter` leaf is
/// emitted in full as this leaf's `fLeafCount` and its position recorded in
/// `refs`; otherwise `fLeafCount` is a back-reference to that recorded object.
#[allow(clippy::too_many_arguments)]
fn write_split_sub(
    w: &mut WBuffer,
    parent: &str,
    counter: &str,
    class_name: &str,
    checksum: u32,
    member: &SplitMember,
    index: i32,
    basket: &BasketRec,
    n_entries: i64,
    keylen: u32,
    parent_ref: u32,
    refs: &mut LeafRefs,
    write_counter_inline: bool,
) {
    let (type_code, _typename, size) = member_type_info(&member.values);
    let name = format!("{parent}.{}", member.name);
    let title = format!("{}[{counter}]", member.name);
    let max_baskets = 10i32;

    let te = w.begin_object(10); // TBranchElement v10
    let tb = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(1000); // fEntryOffsetLen
    w.be_i32(1); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(0); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(basket.nbytes as i64); // fTotBytes
    w.be_i64(basket.nbytes as i64); // fZipBytes

    let sub = obj_array_header(w, 0); // fBranches (empty)
    w.end_object(sub);

    // fLeaves: this member's TLeafElement. Its fLeafCount references `counter`.
    let leaf_tok = obj_array_header(w, 1);
    let leaf_pos = w.len() as u32;
    let lbc = begin_object_any(w, "TLeafElement");
    let outer = w.begin_object(1); // TLeafElement v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &name, &title);
    w.be_i32(1); // fLen
    w.be_i32(size); // fLenType (element width in bytes)
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(0); // fIsUnsigned
    if write_counter_inline {
        // First occurrence of the counter leaf: write it in full, record it.
        let cpos = w.len() as u32;
        let cbc = begin_object_any(w, "TLeafElement");
        write_leaf_element(w, counter, counter, 0, -1, -1, 0);
        end_object_any(w, cbc);
        refs.entry(counter.to_string())
            .or_insert(cpos + keylen + K_MAP_OFFSET);
    } else {
        w.be_u32(refs.get(counter).copied().unwrap_or(0)); // fLeafCount back-ref
    }
    w.end_object(base);
    w.be_i32(index); // fID
    w.be_i32(type_code); // fType (basic-type code)
    w.end_object(outer);
    end_object_any(w, lbc);
    refs.entry(name.clone())
        .or_insert(leaf_pos + keylen + K_MAP_OFFSET);
    w.end_object(leaf_tok);

    let baskets = obj_array_header(w, 0); // fBaskets (empty)
    w.end_object(baskets);
    write_basket_arrays(w, std::slice::from_ref(basket), max_baskets);
    w.string(""); // fFileName
    w.end_object(tb);

    // TBranchElement members for the member element.
    w.string(class_name); // fClassName (the struct, e.g. "Hit")
    w.string(class_name); // fParentName
    w.string(""); // fClonesName
    w.be_u32(checksum); // fCheckSum (the struct's class checksum)
    w.be_u16(1); // fClassVersion
    w.be_i32(index); // fID (member index within the struct)
    w.be_i32(41); // fType (split STL member)
    w.be_i32(type_code); // fStreamerType
    w.be_i32(0); // fMaximum
    w.be_u32(parent_ref); // fBranchCount (object ref to the parent collection)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(te);
}

/// Write one `TBranch` (v13).
fn write_branch(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tot_bytes: i64 = group.iter().map(|b| i64::from(b.nbytes)).sum();
    let leaf = branch.leaf();
    // Branch title encodes the layout: `name/CODE`, `name[N]/CODE` (fixed),
    // `name[count]/CODE` (jagged), or `name/C` (string).
    let title = match branch.kind() {
        Kind::Scalar => format!("{}/{}", branch.name, leaf.code),
        Kind::FixedArray(n) => format!("{}[{}]/{}", branch.name, n, leaf.code),
        Kind::Jagged => format!("{}[{}]/{}", branch.name, branch.count_name(), leaf.code),
        // A std::vector branch's title is just its name (the type lives in
        // the TBranchElement's fClassName).
        Kind::StlVector => branch.name.clone(),
        Kind::Str => format!("{}/C", branch.name),
    };
    // Variable (string/jagged/vector) branches carry an `fEntryOffset` array,
    // flagged by a non-zero `fEntryOffsetLen`; fixed/scalar branches set it to 0.
    let entry_offset_len = match branch.kind() {
        Kind::Str | Kind::Jagged | Kind::StlVector => 1000,
        _ => 0,
    };
    // ROOT writes fSplitLevel = 99 for a (top-level, unsplit) std::vector
    // TBranchElement; that is the value its reader/cache expects.
    let split_level = if matches!(branch.kind(), Kind::StlVector) {
        99
    } else {
        0
    };
    // `fMaxBaskets` is the allocated length of the basket arrays; it must be at
    // least `fWriteBasket` (= group.len()). ROOT's default is 10, which we keep
    // for the common case so small files stay byte-identical, but a streamed
    // tree can hold more than 10 baskets per branch, so grow it to fit.
    let max_baskets = (group.len() as i32).max(10);

    let tok = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(entry_offset_len); // fEntryOffsetLen
    w.be_i32(group.len() as i32); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(split_level); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(tot_bytes); // fTotBytes
    w.be_i64(tot_bytes); // fZipBytes

    // fBranches (empty), fLeaves (one leaf), fBaskets (empty TObjArrays).
    let e = obj_array_header(w, 0);
    w.end_object(e);
    write_leaf_array(w, &[branch], keylen, refs);
    let e = obj_array_header(w, 0);
    w.end_object(e);

    write_basket_arrays(w, group, max_baskets);
    w.string(""); // fFileName
    w.end_object(tok);
}

/// Write a `TObjArray<TLeaf>` for `branches` (one leaf each), recording each
/// leaf's object-reference position (first occurrence) so a later jagged leaf's
/// `fLeafCount` can point back to its count leaf.
fn write_leaf_array(w: &mut WBuffer, branches: &[&Branch], keylen: u32, refs: &mut LeafRefs) {
    let tok = obj_array_header(w, branches.len());
    for &b in branches {
        let bc_pos = w.len() as u32; // the byte-count word position (object-relative)
        let bc = begin_object_any(w, b.leaf().class);
        write_leaf(w, b, refs);
        end_object_any(w, bc);
        refs.entry(b.name.clone())
            .or_insert(bc_pos + keylen + K_MAP_OFFSET);
    }
    w.end_object(tok);
}

/// The tree-level `fLeaves` references each branch's already-written leaf via an
/// object back-reference, rather than re-emitting it. ROOT relies on these being
/// the *same* leaf objects (so `leaf->GetBranch()` is set when it reconstructs
/// the tree); duplicating them leaves the tree-level copies with a null branch
/// and crashes ROOT's `TTreeCache` on the first read.
fn write_tree_leaf_array(w: &mut WBuffer, branches: &[&Branch], refs: &LeafRefs) {
    let names: Vec<String> = branches
        .iter()
        .flat_map(|&b| branch_leaf_names(b))
        .collect();
    let tok = obj_array_header(w, names.len());
    for name in &names {
        let objref = refs.get(name).copied().unwrap_or(0);
        w.be_u32(objref); // object reference to the branch-level leaf
    }
    w.end_object(tok);
}

/// The leaf names a branch contributes to the tree-level `fLeaves`, in order. A
/// leaf branch contributes one (its own name); a split branch contributes the
/// parent counter leaf (`name_`) followed by each member leaf (`name.member`).
fn branch_leaf_names(b: &Branch) -> Vec<String> {
    match b.split() {
        Some(spec) => std::iter::once(format!("{}_", b.name))
            .chain(
                spec.members
                    .iter()
                    .map(|m| format!("{}.{}", b.name, m.name)),
            )
            .collect(),
        None => vec![b.name.clone()],
    }
}

/// Write one `TLeaf*` (v1): the `TLeaf` base then the subclass min/max. A
/// `std::vector` branch instead writes a `TLeafElement` (the `TLeaf` base then
/// `fID`/`fType`).
fn write_leaf(w: &mut WBuffer, branch: &Branch, refs: &LeafRefs) {
    let leaf = branch.leaf();
    if branch.stl_vector() {
        let outer = w.begin_object(1); // TLeafElement v1
        let base = w.begin_object(2); // TLeaf v2
        write_tnamed(w, OBJ_BITS, &branch.name, &branch.name);
        w.be_i32(1); // fLen
        w.be_i32(0); // fLenType
        w.be_i32(0); // fOffset
        w.u8(0); // fIsRange
        w.u8(0); // fIsUnsigned
        w.be_u32(0); // fLeafCount (null)
        w.end_object(base);
        w.be_i32(-1); // fID
        w.be_i32(-1); // fType
        w.end_object(outer);
        return;
    }
    // The leaf title carries `[N]` (fixed) or `[count]` (jagged), else the name.
    let title = match branch.kind() {
        Kind::FixedArray(n) => format!("{}[{}]", branch.name, n),
        Kind::Jagged => format!("{}[{}]", branch.name, branch.count_name()),
        _ => branch.name.clone(),
    };
    // A jagged leaf's `fLeafCount` is an object reference to its count leaf
    // (already written and recorded); everything else has a null `fLeafCount`.
    let f_leaf_count = match branch.kind() {
        Kind::Jagged => refs.get(&branch.count_name()).copied().unwrap_or(0),
        _ => 0,
    };
    // A TLeafC carries fLen = longest-string + 1 and fLenType = 1 (one char);
    // every other leaf uses its element count/width.
    let is_str = leaf.code == 'C';
    let f_len = if is_str {
        branch.str_len()
    } else {
        branch.flen()
    };
    let f_len_type = if is_str { 1 } else { leaf.len_type };
    // As ROOT fills them: a count leaf is a range, and it and a TLeafC track
    // their maximum (the largest count, the longest string + 1); any other
    // leaf keeps fMaximum at 0.
    let is_count = matches!(branch.kind, BranchKind::Count);
    let f_maximum = if is_count || is_str {
        branch.leaf_max()
    } else {
        0
    };
    let outer = w.begin_object(1); // TLeafX v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    w.be_i32(f_len); // fLen
    w.be_i32(f_len_type); // fLenType
    w.be_i32(0); // fOffset
    w.u8(u8::from(is_count)); // fIsRange
    w.u8(leaf.unsigned as u8); // fIsUnsigned
    w.be_u32(f_leaf_count); // fLeafCount (object ref to the count leaf, or null)
    w.end_object(base);
    // fMinimum (0) and fMaximum. TLeafC stores them as 4-byte ints (string
    // lengths); every other leaf uses its element width.
    let minmax_size = if is_str { 4 } else { leaf.size };
    write_leaf_minmax(w, minmax_size, f_maximum);
    w.end_object(outer);
}

/// Write a leaf's `fMinimum` (0) and `fMaximum` (`max`) in the element width.
fn write_leaf_minmax(w: &mut WBuffer, size: i32, max: i64) {
    match size {
        1 => {
            w.u8(0);
            w.u8(max as u8);
        }
        2 => {
            w.be_i16(0);
            w.be_i16(max as i16);
        }
        8 => {
            w.be_i64(0);
            w.be_i64(max);
        }
        _ => {
            w.be_i32(0);
            w.be_i32(max as i32);
        }
    }
}
