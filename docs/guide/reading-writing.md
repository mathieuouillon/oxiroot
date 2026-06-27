# Reading & writing files

Every persistable object — a histogram, a profile, a graph — shares one
persistence model: the `WriteRoot` and `ReadRoot` traits for single objects, and
the `RootFile` builder for composing several objects, subdirectories, or
appending. There is one way to do each thing, and the files produced open in
official ROOT and uproot.

`TTree` and RNTuple have their own dedicated writers (they stream rather than
hold a whole dataset in memory); see [TTree](ttree.md) and [RNTuple](rntuple.md).

## One object: `WriteRoot` / `ReadRoot`

Anything writable implements `WriteRoot`, giving it `write_root` (writes a
complete ROOT file to a path) and `to_root_bytes` (the streamed object payload,
without file/key framing). Anything readable implements `ReadRoot`, giving it
`read_root` and `read_root_in` (from a subdirectory).

```rust
use oxiroot::prelude::*;

let mut h = TH1::new(50, 0.0, 100.0).named("pt").titled("p_{T}");
h.fill(42.0);

// Write a single object as a complete ROOT file …
h.write_root("hist.root", Compression::Zstd(5))?;
// … or get just the streamed object payload (no file/key framing).
let bytes: Vec<u8> = h.to_root_bytes();

// Read it back by key name.
let same = TH1::read_root(&RFile::open("hist.root")?, "pt")?;
```

!!! note "Names belong to the file, not the object"
    A histogram is just data. It carries a name only when you persist it —
    `.named("pt")` sets the file key, `.titled(...)` the ROOT title. Construct
    with `TH1::new(nbins, lo, hi)` and any number of unnamed or same-named
    objects can coexist in memory; the name matters only at write time. See
    [Histograms](histograms.md) for the construction model.

## Several objects, subdirectories, appending: `RootFile`

For more than one object, a `TDirectory`, or appending to an existing file, use
the `RootFile` builder — the single entry point for file composition. `add`
takes any `&dyn WriteRoot`, `dir` opens a subdirectory, and `write` commits with
a chosen compression.

```rust
let prof = TProfile::new(5, 0.0, 5.0).named("prof").titled("<pt> per region");
let g = TGraph::new(vec![1.0, 2.0], vec![3.0, 4.0]).named("res");

RootFile::create("out.root")
    .add(&h)                               // any &dyn WriteRoot: hist, profile, graph…
    .add(&g)
    .dir("by_region", |d| {                // a TDirectory
        d.add(&prof)
    })
    .write(Compression::Zstd(5))?;
```

Read an object back from a subdirectory with `read_root_in`:

```rust
let f = RFile::open("out.root")?;
let p = TProfile::read_root_in(&f, "by_region", "prof")?;
```

### Appending

`RootFile::open` reopens an existing file so further objects can be appended in a
second pass (ROOT "update" mode):

```rust
RootFile::open("out.root")?
    .add(&extra)
    .write(Compression::None)?;
```

!!! warning "Append limitations"
    Append currently targets files of top-level objects. Updating into a file
    that already contains subdirectories or an RNTuple is rejected rather than
    silently corrupting it. Plain (re)writes with subdirectories via
    `RootFile::create` are fully supported.

## Choosing compression

Both `write_root` and `RootFile::write` take a `Compression` value applied to
every object's payload:

| Value | Effect |
|-------|--------|
| `Compression::None` | Store uncompressed. |
| `Compression::Zstd(level)` | Zstandard (the modern ROOT default). |
| `Compression::Zlib(level)` | zlib/deflate (older ROOT default). |
| `Compression::Lz4(level)` | LZ4 with ROOT's XXH64 block check. |

See [Compression](compression.md) for the full codec matrix (LZMA is decode-only)
and how each is verified against ROOT and uproot.

## Same-name collisions are loud

ROOT silently shadows objects written under the same key in one directory (only
the last is found on read). oxiroot rejects it instead: writing two objects with
the same name into the same directory is a `DuplicateName` error, and an empty
name is also an error. Different subdirectories are independent namespaces, so
the same name in two directories is fine.

```rust
let a = TH1::new(10, 0.0, 1.0).named("h");
let b = TH1::new(10, 0.0, 1.0).named("h");
let err = RootFile::create("dup.root").add(&a).add(&b).write(Compression::None);
assert!(err.is_err()); // Error::DuplicateName { name: "h", .. }
```

## Self-describing output

Written files embed a `TStreamerInfo` list describing every class they contain,
so they are self-describing for any ROOT reader — no external dictionary needed.
That is what lets official ROOT (C++) and uproot read oxiroot's output directly;
see [ROOT / uproot interop](interop.md).

## See also

- [Histograms](histograms.md) — the object model and construction
- [Graphs](graphs.md) — the `TGraph` family
- [Compression](compression.md) — codec choices and trade-offs
- [ROOT / uproot interop](interop.md) — cross-language round-trips
