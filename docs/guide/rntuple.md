# RNTuple

RNTuple is ROOT's columnar event-data format (the successor to `TTree`).
oxiroot reads and writes it in pure Rust — anchor, header/footer envelopes,
page lists, and per-column page decoding — with a typed field API that covers
scalars, strings, nested collections, records, variants, fixed-size arrays, and
user classes. This page covers building and writing RNTuples (one-shot and
streaming) and reading them back.

## Writing fields

An RNTuple is a set of named `Field`s. The `Field::*` constructors build the
common column types directly from `Vec`s; the type name and physical column
encoding are derived for you.

```rust
use oxiroot::prelude::*;

let fields = vec![
    Field::f64("mass", vec![91.2, 125.0]),
    Field::i32("charge", vec![0, -1]),
];

Ntuple::new("events", fields).write_root("data.root", Compression::None)?;
```

[`Ntuple::new(name, fields)`](#one-shot-writing) builds a writable RNTuple;
`write_root` writes a complete one-RNTuple ROOT file.

!!! note
    Every field must have the same number of entries (the first field defines
    the entry count). The entry count is a 32-bit field on disk, so a single
    write is capped at `u32::MAX` entries — use [`RNTupleWriter`](#streaming-writes)
    to split a larger dataset across clusters.

### Scalar field constructors

| Constructor | Element type | On-disk type |
| --- | --- | --- |
| `Field::bools` | `bool` | `bool` |
| `Field::i8` / `Field::u8` | `i8` / `u8` | `std::int8_t` / `std::uint8_t` |
| `Field::i16` / `Field::u16` | `i16` / `u16` | `std::int16_t` / `std::uint16_t` |
| `Field::i32` / `Field::u32` | `i32` / `u32` | `std::int32_t` / `std::uint32_t` |
| `Field::i64` / `Field::u64` | `i64` / `u64` | `std::int64_t` / `std::uint64_t` |
| `Field::f32` / `Field::f64` | `f32` / `f64` | `float` / `double` |
| `Field::strings` | `String` | `std::string` |

```rust
use oxiroot::prelude::*;

let fields = vec![
    Field::bools("trigger", vec![true, false, true]),
    Field::u16("nhits", vec![3, 0, 12]),
    Field::strings("label", vec!["mu".into(), "e".into(), "jet".into()]),
];
```

!!! warning
    The boolean constructor is `Field::bools` (plural), not `Field::bool`.

### Reduced-precision reals

A `float` field can be stored in less space by trading precision. Each maps to a
distinct RNTuple real column.

| Constructor | Column | Notes |
| --- | --- | --- |
| `Field::half(name, values)` | `Real16` | half precision (binary16), ~3 decimal digits |
| `Field::truncated(name, values, bits)` | `Real32Trunc` | mantissa truncated to `bits` total bits (`10..=31`) |
| `Field::quantized(name, values, min, max, bits)` | `Real32Quant` | linear quantization into `bits`-wide ints over `[min, max]` |

```rust
use oxiroot::prelude::*;

let pts = vec![5.5_f32, 12.25, 88.0];
let fields = vec![
    Field::half("pt_half", pts.clone()),
    Field::truncated("pt_trunc", pts.clone(), 16),
    Field::quantized("pt_quant", pts, 0.0, 100.0, 12),
];
```

!!! warning
    `quantized` assumes every value lies within `[min, max]`; values are clamped
    into the range before being encoded.

### Collections

`std::vector<T>` fields take one inner `Vec<T>` per entry:

| Constructor | C++ type |
| --- | --- |
| `Field::vec_bool` | `std::vector<bool>` |
| `Field::vec_i8` / `vec_u8` / `vec_i16` / `vec_u16` | `std::vector<intN_t>` |
| `Field::vec_i32` / `vec_i64` | `std::vector<int32_t>` / `<int64_t>` |
| `Field::vec_f32` / `vec_f64` | `std::vector<float>` / `<double>` |
| `Field::vec_str` | `std::vector<std::string>` |

Doubly-nested vectors take `Vec<Vec<Vec<T>>>` (outer = entries):

| Constructor | C++ type |
| --- | --- |
| `Field::vec_vec_bool` | `std::vector<std::vector<bool>>` |
| `Field::vec_vec_i32` / `vec_vec_i64` | `std::vector<std::vector<int32_t>>` / `<int64_t>` |
| `Field::vec_vec_f32` / `vec_vec_f64` | `std::vector<std::vector<float>>` / `<double>` |
| `Field::vec_vec_str` | `std::vector<std::vector<std::string>>` |

```rust
use oxiroot::prelude::*;

// Per-entry lists of tags, and per-entry hit patterns.
let tags = Field::vec_str("tags", vec![
    vec![],
    vec!["mu".into(), "iso".into()],
    vec!["jet".into()],
]);
let hits = Field::vec_vec_i32("hits", vec![
    vec![],
    vec![vec![1, 2]],
    vec![vec![3], vec![4, 5]],
]);
```

### Records, nested collections, and variants

For shapes the `Field::*` helpers do not cover, build a `Column` directly and
wrap it with `Field::new(name, column)`.

`Column::Record` is a struct-of-arrays: named sub-fields, each with one value per
record instance. A two-field record serializes as `std::pair`, more as
`std::tuple`. `Column::Nested` is a collection whose element is itself a
collection or a record — its cumulative `offsets` (one per entry) partition the
flattened child column. Together they express `std::vector<MyStruct>`:

```rust
use oxiroot::prelude::*;

// std::vector<std::pair<int32_t, double>>: a vector of records.
let clusters = Field::new(
    "clusters",
    Column::Nested {
        offsets: vec![0, 1, 3], // entry 0: none, entry 1: 1, entry 2: 2
        items: Box::new(Column::Record(vec![
            ("_0".into(), Column::I32(vec![10, 20, 21])),
            ("_1".into(), Column::F64(vec![1.5, 2.5, 3.5])),
        ])),
    },
);
```

A `std::variant` field is built with `Field::variant(name, alternatives, tags)`.
The `alternatives` hold each alternative's densely-packed active values; `tags`
selects the 1-based active alternative per entry (`0` means valueless). Each
alternative must hold exactly as many values as there are entries selecting it.

```rust
use oxiroot::prelude::*;

// std::variant<int32_t, double>: entry 0 -> int, entry 1 -> double, entry 2 -> int.
let v = Field::variant(
    "payload",
    vec![Column::I32(vec![7, 9]), Column::F64(vec![2.5])],
    vec![1, 2, 1],
);
```

!!! tip
    The full set of `Column` variants — `Bool`, all integer widths, `F32`/`F64`,
    `Str`, the `Vec*` collections, `HalfF32` / `TruncF32` / `QuantF32`, `Record`,
    `Nested`, and `Variant` — is enumerated in the
    [API reference](../api/oxiroot/index.html).

## One-shot writing

`Ntuple` is the method-based entry point. Build one from a name and its
fields, then write it or get its bytes:

```rust
use oxiroot::prelude::*;

let ntpl = Ntuple::new("events", vec![
    Field::f64("mass", vec![91.2, 125.0]),
    Field::i32("charge", vec![0, -1]),
]);

// Write a one-RNTuple ROOT file...
ntpl.write_root("data.root", Compression::Zstd(5))?;

// ...or get the complete file bytes (file_name is the TFile name in the header).
let bytes: Vec<u8> = ntpl.to_root_bytes("data.root", Compression::None)?;
```

The free function `write_rntuple_file` is the equivalent procedural form, and
`rntuple_file_bytes` returns the bytes without writing a file:

```rust
use oxiroot::prelude::*;

let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
write_rntuple_file("data.root", "events", &fields, Compression::None)?;
```

Both forms automatically switch to ROOT's 64-bit ("big") container layout once
the file would exceed 2 GiB.

## Streaming writes

`RNTupleWriter` writes one *cluster* per call, so a large dataset is never held
in memory all at once. Each `write_batch` flushes a cluster; the first batch
fixes the schema (and writes the header); `finish` writes the page list, footer,
and anchor.

```rust
use oxiroot::prelude::*;

let mut w = RNTupleWriter::create("big.root", "events", Compression::Zstd(5))?;

for chunk in 0..3 {
    let base = chunk * 1000;
    let mass: Vec<f64> = (base..base + 1000).map(|i| i as f64).collect();
    w.write_batch(&[Field::f64("mass", mass)])?;
}

w.finish()?;
```

| Constructor | Container | Use when |
| --- | --- | --- |
| `RNTupleWriter::create(path, name, compression)` | 32-bit | total file `<= 2 GiB` |
| `RNTupleWriter::create_large(path, name, compression)` | 64-bit | file may exceed 2 GiB |
| `RNTupleWriter::new(sink, file_name, name, compression)` | 32-bit | any `Write + Seek` sink |
| `RNTupleWriter::new_large(sink, file_name, name, compression)` | 64-bit | large file into a custom sink |

!!! warning
    Every batch must share the same field schema (field names, types, and
    column layout). A differing batch is rejected with an error rather than
    silently mis-described. A 32-bit writer whose stream passes 2 GiB errors at
    `finish` — use `create_large` / `new_large` for those.

## Reading

[`RNTuple::open`](#reading) reads the anchor, parses the schema, and indexes the
clusters; column data is decoded on demand. Read a top-level field by name with
`read_field`, which returns a `FieldValues`.

```rust
use oxiroot::prelude::*;

let file = RFile::open("data.root")?;
let ntpl = RNTuple::open(&file, "events")?;

println!("{} entries", ntpl.num_entries());
println!("fields: {:?}", ntpl.field_names());

match ntpl.read_field(&file, "mass")? {
    FieldValues::F64(values) => {
        for m in values {
            println!("mass = {m}");
        }
    }
    other => println!("unexpected shape: {other:?}"),
}
```

### What `read_field` reconstructs

`read_field` walks the field tree and reconstructs per-entry values for every
supported shape:

| Shape | `FieldValues` variant |
| --- | --- |
| scalar leaf | `Bool`, `I8`/`U8`/…/`I64`/`U64`, `F32`, `F64` |
| `std::string` | `Str(Vec<String>)` |
| `std::vector<T>` | `VecBool`, `VecI32`, `VecF64`, `VecStr`, … |
| `std::vector<std::vector<T>>` and deeper | `Nested { offsets, items }` |
| record / struct (incl. user classes) | `Record(Vec<(String, FieldValues)>)` |
| `std::vector<MyStruct>` | `Nested` wrapping a `Record` |
| `std::variant` | `Variant { alternatives, tags, indices }` |
| `std::array<T, N>` / `std::bitset<N>` | grouped `N` per entry (a `Vec*` variant) |

`FieldValues::len` gives the element count at a level (entries, for a top-level
field), and `is_empty` reports an empty level.

!!! note
    `Record` is a struct-of-arrays: each named sub-field holds one value per
    record instance. Inside a `Nested` collection the sub-fields hold the
    *flattened* record instances, partitioned by the collection's `offsets`.

### Reading nested values

For a `std::vector<MyStruct>` (a `Nested` wrapping a `Record`), the `offsets`
slice the flattened sub-field arrays into per-entry groups:

```rust
use oxiroot::prelude::*;

let file = RFile::open("data.root")?;
let ntpl = RNTuple::open(&file, "events")?;

if let FieldValues::Nested { offsets, items } = ntpl.read_field(&file, "clusters")? {
    if let FieldValues::Record(fields) = *items {
        // `fields` is [("_0", I32([...])), ("_1", F64([...]))] — flattened.
        // entry k spans offsets[k-1]..offsets[k] (offsets[-1] = 0).
        println!("{} entries, {} record instances", offsets.len(), fields[0].1.len());
    }
}
```

### Lower-level access

For schema introspection and raw decoding, the reader also exposes:

| Method | Returns |
| --- | --- |
| `ntpl.header()` | parsed schema (fields and columns) |
| `ntpl.footer()` | cluster groups |
| `ntpl.anchor()` | the verified anchor |
| `ntpl.read_column(&file, index)` | a single physical column as `ColumnValues` |

## Compression

The `compression` argument on every write path is a `Compression`: `None`, or
`Zstd`, `Zlib`, or `Lz4` at a given level (e.g. `Compression::Zstd(5)`). Pages
are compressed only when the result is actually smaller, exactly as ROOT does.
On read, oxiroot decodes Zstd, zlib, LZ4, and LZMA. See
[Compression](compression.md) for the full codec matrix.

## Interoperability

Files written by oxiroot are read by official ROOT and uproot, and oxiroot reads
RNTuples those tools write — including split, zigzag, and delta column encodings,
all integer widths, reduced-precision reals, the `Switch` (variant) column, and
user classes split into a record of their members. For a runnable end-to-end
example of the nested shapes, see `crates/oxiroot/examples/rntuple_nested.rs`:

```sh
cargo run -p oxiroot --example rntuple_nested
```

## See also

- [Getting started](../getting-started/quickstart.md) — install and first read/write.
- [TTree](ttree.md) — the classic row-wise event format.
- [Compression](compression.md) — codecs available on read and write.
- [API reference](../api/oxiroot/index.html) — full type-level docs.
