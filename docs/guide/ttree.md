# TTree

The classic ROOT columnar store. `oxiroot` reads and writes `TTree`s holding
scalar, fixed- and variable-length array, string, and `std::vector<T>` branches —
across any number of baskets, compressed or not — and reads them back in ROOT,
uproot, and this crate. This page covers building branches, the write paths
(one-shot, multi-basket, and streaming), and the read API.

## Branches

A branch is a named column. Every branch is built through a typed constructor on
`Branch`, so the payload can never drift from how it is serialized. The
constructor family encodes the column shape:

| Constructor family | Shape | On-disk form |
|---|---|---|
| `Branch::i32`, `Branch::f64`, `Branch::bools`, … | scalar (one value per entry) | `TLeafI`/`TLeafD`/… |
| `Branch::strings` | one string per entry | `TLeafC` |
| `Branch::vec_i32`, `Branch::vec_f64`, … | fixed array `x[N]` (every row length `N`) | `TLeaf*` with `fLen = N` |
| `Branch::jagged_i32`, `Branch::jagged_f64`, … | variable array `x[n]` (rows differ) | `TLeaf*` + auto `n<name>` count branch |
| `Branch::vector_i32`, `Branch::vector_f64`, … | `std::vector<T>` | `TBranchElement` |
| `Branch::split_vector` | split `std::vector<MyStruct>` | parent `TBranchElement` + per-member sub-branches |

The scalar, `vec_*`, `jagged_*`, and `vector_*` families exist for each numeric
type: `bool` (as `bools`/`vec_bool`/`jagged_bool`), `i8`/`u8`, `i16`/`u16`,
`i32`/`u32`, `i64`/`u64`, `f32`/`f64`. (`vector_*` covers the integer and float
types; there is no `vector_bool`.)

```rust
use oxiroot::prelude::*;

let branches = vec![
    Branch::i32("event", (0..10).collect()),
    Branch::f64("energy", vec![10.5, 20.1, 5.0, /* … */]),
    // Variable-length rows — written with a paired `nhits` count branch.
    Branch::jagged_f64("hits", (0..10).map(|i| vec![i as f64; i % 3]).collect()),
    Branch::strings("label", (0..10).map(|i| format!("e{i}")).collect()),
];
```

!!! note
    A fixed-array constructor (`Branch::vec_*`) requires every inner vector to
    have the same length `N`. If the rows differ, the write fails — use the
    matching `Branch::jagged_*` constructor instead, which records per-row
    lengths in an auto-generated `n<name>` count branch (uproot's convention).

### `std::vector<T>` branches

`Branch::vector_*` writes a `std::vector<T>` as a `TBranchElement`, where each
entry carries the streamer header ROOT writes for a streamed collection:

```rust
use oxiroot::prelude::*;

let tracks = Branch::vector_f64(
    "pt",
    vec![vec![1.0, 2.0], vec![3.5], vec![]], // one inner Vec per entry
);
```

### Split `std::vector<MyStruct>` branches

A split struct collection is a parent `TBranchElement` whose per-member data
lives in sub-branches (`hits.x`, `hits.y`, …). Build it from the struct's C++
class name and one `SplitMember` per field; all members share per-entry
lengths. A `TStreamerInfo` for the element class is generated so the file is
self-describing.

```rust
use oxiroot::prelude::*;
use oxiroot::tree::SplitMember;

// Three events, each with a differing number of hits.
let xs = vec![vec![1.0, 2.0], vec![3.0], vec![]];
let ys = vec![vec![0.1, 0.2], vec![0.3], vec![]];

let hits = Branch::split_vector(
    "hits",
    "Hit", // the C++ struct's class name
    vec![
        SplitMember::f64("x", xs),
        SplitMember::f64("y", ys),
    ],
);
```

`SplitMember` has the same per-type constructors as the jagged branches:
`f64`, `f32`, `i8`/`u8` … `i64`/`u64`. Each takes the member's per-entry values
as a `Vec<Vec<T>>`.

## Writing a tree

`Tree::new` bundles a name and its branches; the write methods mirror the
histogram `write_root` ergonomics.

```rust
use oxiroot::prelude::*;

let branches = vec![
    Branch::i32("event", vec![1, 2, 3]),
    Branch::f64("energy", vec![10.5, 20.1, 5.0]),
];
Tree::new("Events", branches).write_root("tree.root", Compression::Zstd(5))?;
```

| Method | Effect |
|---|---|
| `Tree::write_root(path, compression)` | Write a single-tree file, one basket per branch. |
| `Tree::write_root_baskets(path, compression, entries_per_basket)` | Split each branch into baskets of at most `entries_per_basket` entries (`0` = one basket). |
| `Tree::to_root_bytes(file_name, compression)` | Return the complete ROOT-file bytes instead of writing to disk. |

The free functions `write_tree_file` and `write_tree_file_baskets` are the
function-style counterparts (they take the tree name and a `&[Branch]` directly)
and remain available; the `Tree` methods are thin wrappers over them.

```rust
use oxiroot::prelude::*;

// Function form — equivalent to Tree::new(...).write_root(...).
write_tree_file("tree.root", "Events", &branches, Compression::None)?;
```

!!! warning
    The single-tree writers produce the small (32-bit) ROOT container, so the
    total file must stay under 2 GiB; the writer returns an error otherwise.
    Split `std::vector<Struct>` branches are always one basket — their
    per-member alignment is not chunked.

## Streaming writes

For trees too large to hold in memory, `TTreeWriter` appends entries in
batches. Each `write_batch` call emits one basket per branch straight to the
sink, so only the current batch is resident — the way ROOT's `TTree::Fill`
flushes baskets as they fill. `finish` writes the small `TTree` metadata, the
streamer info, and the key list, then patches the file header.

```rust
use oxiroot::prelude::*;

let mut w = TTreeWriter::create("big.root", "Events", Compression::Zstd(5))?;
for batch in 0..1_000 {
    let base = batch * 10_000;
    let x: Vec<f64> = (0..10_000).map(|i| (base + i) as f64).collect();
    w.write_batch(&[Branch::f64("x", x)])?; // one basket, flushed now
}
let entries = w.num_entries();
w.finish()?; // commit metadata + header
```

`TTreeWriter::new` takes any `Write + Seek` sink if you do not want a file path.
Every batch must share the first batch's schema: branch names, element types,
the jagged / `std::vector` flags, and fixed-array widths — a mismatch is an
error. Split `std::vector<Struct>` branches are not supported here; use
`write_tree_file` for those.

## Reading a tree

Open a `TTree` by name from an `RFile`, then read branches by name.

```rust
use oxiroot::prelude::*;

let file = RFile::open("tree.root")?;
let t = TTree::open(&file, "Events")?;

println!("{} entries: {:?}", t.num_entries(), t.branch_names());

let energy = t.read_branch(&file, "energy")?;
let values: &[f64] = energy.as_f64().expect("energy is a TLeafD branch");
```

### Read methods

| Method | Returns |
|---|---|
| `read_branch(file, name)` | All entries of one branch as `BranchValues`. |
| `read_branches(file, &[name, …])` | A `Vec<BranchValues>` in the requested order (a columnar `arrays`-style read). |
| `read_branch_range(file, name, start, stop)` | Only entries `[start, stop)`, fetching just the baskets that cover the window. |
| `read_branch_flat(file, name)` | A `Jagged` view — cumulative `offsets` over one flat scalar `BranchValues`, no `Vec<Vec<_>>` allocation (numeric branches only). |

`read_branch_range` clamps `stop` to the entry count and `start` to `stop`, so an
out-of-range window yields fewer (or no) entries rather than an error.

```rust
use oxiroot::prelude::*;
// Just the baskets covering entries 3..7.
let window = t.read_branch_range(&file, "event", 3, 7)?;
let evts: &[i32] = window.as_i32().unwrap();

// Several columns at once.
let cols = t.read_branches(&file, &["event", "energy"])?;
```

### `BranchValues`

`BranchValues` is the decoded column. Scalar branches yield a flat vector;
fixed (`x[N]`) and variable (`x[n]`) branches yield a nested `Vec<Vec<T>>` (one
inner vector per entry); `TLeafC` yields `Vec<String>`, and
`std::vector<std::string>` yields `Vec<Vec<String>>`. Match on the variant, or
use the typed `as_*` accessors that return `Option<&[T]>`:

```rust
use oxiroot::prelude::*;
let hits = t.read_branch(&file, "hits")?;
match hits {
    BranchValues::VecF64(rows) => {
        for (i, row) in rows.iter().enumerate() {
            println!("entry {i}: {} hits", row.len());
        }
    }
    other => println!("unexpected variant: {:?}", other.leaf_type()),
}
```

The scalar accessors are `as_bool`, `as_i8`/`as_u8`, `as_i16`/`as_u16`,
`as_i32`/`as_u32`, `as_i64`/`as_u64`, `as_f32`/`as_f64`, and `as_str`. Each
returns `Some(&[T])` only for the matching scalar variant. `BranchValues` also
offers `len()`, `is_empty()`, and `leaf_type()`.

The flat view returns `Jagged` with an `offsets` vector of `num_entries + 1`
cumulative boundaries (`offsets[0] == 0`) over a single scalar `values`; entry
`i`'s elements are `values[offsets[i] .. offsets[i+1]]`:

```rust
use oxiroot::prelude::*;
let jagged = t.read_branch_flat(&file, "hits")?;
let flat: &[f64] = jagged.values.as_f64().unwrap();
for i in 0..jagged.len() {
    let (a, b) = (jagged.offsets[i] as usize, jagged.offsets[i + 1] as usize);
    let row = &flat[a..b];
    // …
}
```

## Introspection

A `TTree` reports each branch's type and shape without reading its data:

| Method | Returns |
|---|---|
| `branch_names()` | The names of the readable branches, in tree order. |
| `branch_type(name)` | The element `LeafType` (`I32`, `F64`, `Str`, …), or `None`. |
| `branch_len(name)` | `fLen` — the per-entry element count of a fixed array (`1` for a scalar). |
| `branch_shape(name)` | The fixed-array shape: `[N]` for `x[N]`, `[N, M]` for `x[N][M]`, `[]` for scalar/variable. |
| `branch_title(name)` | `fTitle` — the leaf-list / shape string (e.g. `x[3]`, `n`). |
| `unsupported_branches()` | `(name, reason)` pairs for branches present in the file but not read. |
| `streamer_classes()` | The `(class, version)` pairs declared in the file's `TStreamerInfo`. |

```rust
use oxiroot::prelude::*;
for name in t.branch_names() {
    println!(
        "{name}: type={:?} shape={:?}",
        t.branch_type(name).unwrap(),
        t.branch_shape(name).unwrap(),
    );
}
// Anything the reader skipped, and why.
for (name, why) in t.unsupported_branches() {
    eprintln!("skipped {name}: {why}");
}
```

!!! note
    The reader is streamer-info-driven: it parses `TTree`/`TBranch`/
    `TBranchElement` by walking the member list in the file's own
    `TStreamerInfo` rather than at fixed offsets, so a schema change is absorbed
    instead of misread, and an unknown member type is reported (via
    `unsupported_branches`) rather than parsed at a guessed offset.
    `streamer_classes()` exposes the schema the file was written against.

## Spanning files with `TChain`

`TChain` reads a branch across several same-schema trees as one concatenated
column, the way ROOT's `TChain` spans a dataset split over many files. The files
are borrowed, so keep them alive for the chain's lifetime.

```rust
use oxiroot::prelude::*;

let f1 = RFile::open("part1.root")?;
let f2 = RFile::open("part2.root")?;

let mut chain = TChain::new();
chain.add(&f1, "Events")?;
chain.add(&f2, "Events")?;

println!("{} entries over {} files", chain.num_entries(), chain.num_trees());
let pt = chain.read_branch("pt")?; // values from both files, in add order
```

`read_branch` fails if a tree lacks the branch or yields a different element type
than the others.

!!! tip
    With the `rayon` feature enabled, per-basket decompression runs in parallel
    inside `read_branch`/`read_branch_range`, with no API change. See
    [Compression](compression.md) for the supported codecs.

## See also

- [Quickstart](../getting-started/quickstart.md) — the end-to-end write/read tour.
- [RNTuple](rntuple.md) — ROOT's modern columnar format.
- [Compression](compression.md) — codecs shared by every writer.
- [API reference](../api/oxiroot/index.html) — type-level docs for `oxiroot::tree`.
