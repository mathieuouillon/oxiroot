//! Writing branch data into `TBasket` records, whole or in chunks.

use std::io::{Seek, Write};

use oxiroot_io_core::{compress_if_smaller, ContainerWriter, DirId, Result, TKey, WBuffer, DATIME};

use super::branch::{vec_row_lengths, Branch, BranchKind};
use super::layout::Kind;
use super::NESTED_NOT_WRITABLE;
use crate::value::BranchValues;

/// One basket's recorded location, for the branch metadata.
#[derive(Clone, Copy)]
pub(super) struct BasketRec {
    pub(super) seek: u64,
    pub(super) nbytes: u32,
    /// Number of entries this basket holds (for the cumulative `fBasketEntry`).
    pub(super) n_entries: u32,
}

/// Write one `TBasket` of a tree in directory `dir` at the end of `file`,
/// returning its location.
pub(super) fn write_basket<W: Write + Seek>(
    file: &mut ContainerWriter<W>,
    dir: DirId,
    branch: &Branch,
    tree_name: &str,
) -> Result<BasketRec> {
    let (bytes, rec) = basket_bytes(
        branch,
        tree_name,
        file.compression_setting(),
        file.position(),
        file.dir_offset(dir)?,
    );
    file.place_blob(&bytes)?;
    Ok(rec)
}

/// The on-disk bytes of one `TBasket`, written as if it begins at absolute file
/// offset `seek` (baked into the key's `fSeekKey`), plus its [`BasketRec`].
/// `seek_pdir` is the offset of the tree's directory. A basket's key is always
/// in the big form, with the `TBasket` fields appended to the header.
fn basket_bytes(
    branch: &Branch,
    tree_name: &str,
    compression: u32,
    seek: u64,
    seek_pdir: u64,
) -> (Vec<u8>, BasketRec) {
    let (data, offsets) = branch.basket_content();
    let n_entries = branch.n_entries();
    let leaf = branch.leaf();
    // `fNevBufSize` is the per-entry buffer size: `flen * elem_size` for a
    // fixed/scalar branch; ROOT writes a default (1000) for variable baskets.
    let nev_buf_size = match branch.kind() {
        Kind::Str | Kind::Jagged | Kind::StlVector => 1000,
        _ => branch.flen() * leaf.size,
    };

    let klen = TKey::header_len("TBasket", &branch.name, tree_name, true) as u32 + 19;
    let border = data.len() as u32;

    // The uncompressed buffer is the entry data, then (for a variable branch)
    // the `fEntryOffset` array: `int32 count(=n_entries+1)` followed by
    // basket-relative offsets (the data-relative offsets plus `fKeyLen`).
    let mut buffer = data;
    if let Some(offs) = &offsets {
        buffer.extend_from_slice(&(offs.len() as i32).to_be_bytes());
        for &o in offs {
            buffer.extend_from_slice(&((o + klen) as i32).to_be_bytes());
        }
    }
    let obj_len = buffer.len() as u32;
    let payload = compress_if_smaller(&buffer, compression);
    let nbytes = klen + payload.len() as u32;
    let f_last = klen + border; // entry data ends at the border

    let mut w = WBuffer::with_capacity(nbytes as usize);
    // Big-format TKey header.
    w.be_i32(nbytes as i32);
    w.be_u16(1004); // big-format key version
    w.be_u32(obj_len);
    w.be_u32(DATIME);
    w.be_u16(klen as u16);
    w.be_u16(0); // cycle
    w.be_u64(seek);
    w.be_u64(seek_pdir); // fSeekPdir
    w.string("TBasket");
    w.string(&branch.name);
    w.string(tree_name);
    // TBasket extension (the tail of fKeyLen).
    w.be_u16(3); // basket version
    w.be_i32(32000); // fBufferSize
    w.be_i32(nev_buf_size); // fNevBufSize
    w.be_i32(n_entries as i32); // fNevBuf
    w.be_i32(f_last as i32); // fLast
    w.u8(0); // flag
    w.bytes(&payload);

    (
        w.into_vec(),
        BasketRec {
            seek,
            nbytes,
            n_entries,
        },
    )
}

/// A sub-range `[start, start+len)` of a non-split branch's entries, as a fresh
/// `Branch` — used to split a branch into multiple baskets.
fn chunk_branch(branch: &Branch, start: usize, len: usize) -> Branch {
    Branch {
        name: branch.name.clone(),
        values: chunk_values(&branch.values, start, len),
        kind: branch.chunk_kind(),
    }
}

/// Slice a [`BranchValues`] to `[start, start+len)` (clamped), preserving variant.
pub(super) fn chunk_values(bv: &BranchValues, start: usize, len: usize) -> BranchValues {
    use BranchValues::*;
    macro_rules! sl {
        ($variant:ident, $v:expr) => {{
            let end = (start + len).min($v.len());
            let s = start.min(end);
            $variant($v[s..end].to_vec())
        }};
    }
    match bv {
        Bool(v) => sl!(Bool, v),
        I8(v) => sl!(I8, v),
        U8(v) => sl!(U8, v),
        I16(v) => sl!(I16, v),
        U16(v) => sl!(U16, v),
        I32(v) => sl!(I32, v),
        U32(v) => sl!(U32, v),
        I64(v) => sl!(I64, v),
        U64(v) => sl!(U64, v),
        F32(v) => sl!(F32, v),
        F64(v) => sl!(F64, v),
        VecBool(v) => sl!(VecBool, v),
        VecI8(v) => sl!(VecI8, v),
        VecU8(v) => sl!(VecU8, v),
        VecI16(v) => sl!(VecI16, v),
        VecU16(v) => sl!(VecU16, v),
        VecI32(v) => sl!(VecI32, v),
        VecU32(v) => sl!(VecU32, v),
        VecI64(v) => sl!(VecI64, v),
        VecU64(v) => sl!(VecU64, v),
        VecF32(v) => sl!(VecF32, v),
        VecF64(v) => sl!(VecF64, v),
        Str(v) => sl!(Str, v),
        VecStr(v) => sl!(VecStr, v),
        Nested { .. } => unreachable!("{NESTED_NOT_WRITABLE}"),
    }
}

/// Write the basket(s) backing one branch. A leaf branch has exactly one. A
/// split `std::vector<MyStruct>` branch has a *count* basket (the parent's
/// per-entry element counts, as a variable `i32`) followed by one jagged basket
/// per member sub-branch; `basket_groups[i][0]` is the count basket and `[1..]`
/// the members, matching the order `serialize::write_split_parent` reads them back.
pub(super) fn write_branch_baskets<W: Write + Seek>(
    file: &mut ContainerWriter<W>,
    dir: DirId,
    branch: &Branch,
    tree_name: &str,
    entries_per_basket: usize,
) -> Result<Vec<BasketRec>> {
    let Some(spec) = branch.split() else {
        // A single-leaf branch is split into baskets of `entries_per_basket`
        // entries (0 = one basket). An empty branch still gets one empty basket.
        let n = branch.n_entries() as usize;
        let epb = if entries_per_basket == 0 {
            n.max(1)
        } else {
            entries_per_basket
        };
        let mut recs = Vec::new();
        let mut start = 0;
        while start < n {
            let len = epb.min(n - start);
            recs.push(write_basket(
                file,
                dir,
                &chunk_branch(branch, start, len),
                tree_name,
            )?);
            start += len;
        }
        if recs.is_empty() {
            recs.push(write_basket(file, dir, branch, tree_name)?);
        }
        return Ok(recs);
    };
    // Count basket: per-entry element counts as single-element jagged `i32`
    // rows, so it carries the same `fEntryOffset` ROOT writes for the parent.
    let counts = vec_row_lengths(&spec.members[0].values);
    let count_branch = Branch {
        name: branch.name.clone(),
        values: BranchValues::VecI32(counts.into_iter().map(|n| vec![n]).collect()),
        kind: BranchKind::Jagged,
    };
    let mut recs = vec![write_basket(file, dir, &count_branch, tree_name)?];
    for m in &spec.members {
        let sub = Branch {
            name: format!("{}.{}", branch.name, m.name),
            values: m.values.clone(),
            kind: BranchKind::Jagged,
        };
        recs.push(write_basket(file, dir, &sub, tree_name)?);
    }
    Ok(recs)
}
