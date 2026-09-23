# Reading & writing files

Every persistable object — a histogram, a profile, a graph — shares one
persistence model: the `WriteRoot` and `ReadRoot` traits for single objects, and
`FileWriter` for composing several objects, subdirectories, or
appending. There is one way to do each thing, and the files produced open in
official ROOT and uproot.

`TTree` and RNTuple have their own dedicated writers (they stream rather than
hold a whole dataset in memory); see [TTree](ttree.md) and [RNTuple](rntuple.md).

Types that read a file end in `Reader` (`FileReader`, `TreeReader`,
`NtupleReader`, `ChainReader`) and types that write one end in `Writer`
(`FileWriter`, `TreeWriter`, `NtupleWriter`).

## One object: `WriteRoot` / `ReadRoot`

Anything writable implements `WriteRoot`, giving it `write_root` (writes a
complete ROOT file to a path) and `to_root_bytes` (the streamed object payload,
without file/key framing). Anything readable implements `ReadRoot`, giving it
`read_root` and `read_root_in` (from a subdirectory).

```rust
use oxiroot::prelude::*;

let mut h = Hist::reg(50, 0.0, 100.0).double().named("pt").titled("p_{T}");
h.fill(42.0);

// Write a single object as a complete ROOT file …
h.write_root("hist.root", Compression::Zstd(5))?;
// … or get just the streamed object payload (no file/key framing).
let bytes: Vec<u8> = h.to_root_bytes();

// Read it back by key name.
let same = TH1::read_root(&FileReader::open("hist.root")?, "pt")?;
```

!!! note "Names belong to the file, not the object"
    A histogram is just data. It carries a name only when you persist it —
    `.named("pt")` sets the file key, `.titled(...)` the ROOT title. Construct
    with `Hist::reg(nbins, lo, hi).double()` and any number of unnamed or same-named
    objects can coexist in memory; the name matters only at write time. See
    [Histograms](histograms.md) for the construction model.

## Any class: the generic reader

The typed readers (`TH1::read_root`, …) need a Rust model for the class. When you
just want to *inspect* an object — including a class oxiroot has no model for —
`FileReader::get_value` decodes it generically, driven entirely by the file's
`TStreamerInfo`, into a dynamic [`Value`](../api/oxiroot/enum.Value.html) tree:

```rust
use oxiroot::{FileReader, Value};

let f = FileReader::open("hist.root")?;
let h = f.get_value("pt")?; // a TH1D, decoded from streamer info alone

assert_eq!(h.class(), Some("TH1D"));
assert_eq!(h.get("fTitle").and_then(Value::as_str), Some("transverse momentum"));
// Members nest: fXaxis is a TAxis object, fArray is the bin-content array.
let nbins = h.get("fXaxis").and_then(|a| a.get("fNbins")).and_then(Value::as_i64);
let bins = h.get("fArray").and_then(Value::as_array);

println!("{h}"); // pretty rootprint-style tree
# Ok::<(), oxiroot::Error>(())
```

`Value` is a tree of primitives, `Str`, `Array`, and `Object { class, members }`
(members keep their on-disk order); accessors are `class()`, `get(name)`,
`as_f64()`/`as_i64()`/`as_str()`/`as_array()`, and `Display` renders the tree.
This is the engine behind `oxroot dump` for classes without a dedicated view.
STL members decode too — `vector`, `set`, `list`, `map` and `pair`, of numbers,
strings, `TArray`s, nested containers, objects or pointers, written objectwise
or memberwise — and a slot pointing at an object written elsewhere in the same
object is `Value::Ref`, naming the class it points at. A member the reader
cannot decode (a class with no streamer info) becomes `Value::Unsupported`
rather than failing the whole object, and `get_value_in(dir, name)` reads from a
subdirectory.

## Remote and lazy reads

`FileReader::open` reads the whole file into memory. When you only need a few objects
from a large file — or the file lives on a web server — you can instead read only
the byte ranges each object touches, the way ROOT and uproot do:

```rust
use oxiroot::FileReader;

// Local, positioned reads — never slurps the whole file:
let f = FileReader::open_ranged("big.root")?;

// Remote, over HTTP(S) byte-range requests (the `http` feature):
let f = FileReader::open_url("https://example.org/data/big.root")?;

// Remote, over CERN's XRootD protocol (the `xrootd` feature):
let f = FileReader::open_url("root://eospublic.cern.ch//eos/root-eos/hsimple.root")?;

let h = oxiroot::hist::TH1::read_root(&f, "hpx")?; // fetches only that key's bytes
# Ok::<(), oxiroot::Error>(())
```

All return an ordinary [`FileReader`]; every reader (histograms, graphs, `TTree`
branches, RNTuple fields, `get_value`) works unchanged and pulls only what it
reads — a single `TTree` branch fetches just its baskets, an RNTuple field just
its pages. Opening parses only the header, directory, key list, and streamer
info (a few small ranges).

`open_url` dispatches on the URL scheme:

- `http://` / `https://` — the `http` feature (adds the pure-Rust `ureq`/rustls
  client). The server must honor `Range` requests (`Accept-Ranges: bytes`).
- `root://` — the `xrootd` feature (pure `std::net`, no dependencies). It uses
  the credential-free `unix` security protocol, so it reads world-readable /
  open data from servers that offer it (e.g. `root://eospublic.cern.ch`);
  GSI/Kerberos/token security is not implemented.

Both features are off by default. The `oxroot` CLI accepts a URL anywhere it
takes a path when built with the matching feature:
`oxroot dump root://eospublic.cern.ch//eos/root-eos/hsimple.root:ntuple -n 5`.

[`FileReader`]: ../api/oxiroot/struct.FileReader.html

## Several objects, subdirectories, appending: `FileWriter`

For more than one object, a `TDirectory`, or appending to an existing file, use
`FileWriter` — the single entry point for file composition. `add`
takes any `&dyn WriteRoot`, `dir` opens a subdirectory, and `write` commits with
a chosen compression.

```rust
let prof = Hist::reg(5, 0.0, 5.0).profile().named("prof").titled("<pt> per region");
let g = TGraph::new(vec![1.0, 2.0], vec![3.0, 4.0])?.named("res");

FileWriter::create("out.root")
    .add(&h)                               // any &dyn WriteRoot: hist, profile, graph…
    .add(&g)
    .dir("by_region", |d| {                // a TDirectory
        d.add(&prof)
    })
    .write(Compression::Zstd(5))?;
```

Read an object back from a subdirectory with `read_root_in`:

```rust
let f = FileReader::open("out.root")?;
let p = TProfile::read_root_in(&f, "by_region", "prof")?;
```

### Appending

`FileWriter::open` reopens an existing file so further objects can be appended in a
second pass (ROOT "update" mode):

```rust
FileWriter::open("out.root")?
    .add(&extra)
    .write(Compression::None)?;
```

!!! warning "Append limitations"
    Append currently targets files of top-level objects. Updating into a file
    that already contains subdirectories or an RNTuple is rejected rather than
    silently corrupting it. Plain (re)writes with subdirectories via
    `FileWriter::create` are fully supported.

## Choosing compression

Both `write_root` and `FileWriter::write` take a `Compression` value applied to
every object's payload:

| Value | Effect |
|-------|--------|
| `Compression::None` | Store uncompressed. |
| `Compression::Zstd(level)` | Zstandard (the modern ROOT default). |
| `Compression::Zlib(level)` | zlib/deflate (older ROOT default). |
| `Compression::Lz4(level)` | LZ4 with ROOT's XXH64 block check. |
| `Compression::Lzma(level)` | LZMA (XZ stream), ROOT's high-ratio codec. |

See [Compression](compression.md) for the full codec matrix and how each is
verified against ROOT and uproot.

## Same-name collisions are loud

ROOT silently shadows objects written under the same key in one directory (only
the last is found on read). oxiroot rejects it instead: writing two objects with
the same name into the same directory is a `DuplicateName` error, and an empty
name is also an error. Different subdirectories are independent namespaces, so
the same name in two directories is fine.

```rust
let a = Hist::reg(10, 0.0, 1.0).double().named("h");
let b = Hist::reg(10, 0.0, 1.0).double().named("h");
let err = FileWriter::create("dup.root").add(&a).add(&b).write(Compression::None);
assert!(err.is_err()); // Error::DuplicateName { name: "h", .. }
```

## When a read or write fails

Everything returns `oxiroot::Error`, whose variants say what went wrong, so a
caller can handle a missing object differently from a corrupt file:

| Variant | Means |
| --- | --- |
| `NotFound { what, name }` | No key, subdirectory, branch or field of that name |
| `WrongClass { name, found, expected }` | The key holds another class: a `TH2F` read as a `TH1`, say |
| `UnsupportedVersion { class, version }` | A class version oxiroot cannot decode, such as one written by a ROOT release older than streamer info |
| `MissingStreamerInfo { class }` | The file has no `TStreamerInfo` for a class it needs to decode |
| `ChecksumMismatch { what, .. }` | An RNTuple checksum does not match its data: the data is corrupt |
| `Format(message)` | The bytes break the ROOT format: a truncated or corrupt file |
| `Unsupported(message)` | Valid ROOT that oxiroot does not read or write yet |
| `InvalidInput(message)` | An argument cannot be used, such as an unnamed object; nothing was written |
| `LengthMismatch { what, expected, found }` | Inputs that must have the same length do not |
| `Io { kind, message }` | An I/O failure, local or remote, with its `std::io::ErrorKind` |

`Error` is `#[non_exhaustive]`, so match the variants you handle and keep a
wildcard arm:

```rust
use oxiroot::prelude::*;
use oxiroot::Error;

let file = FileReader::open("hist.root")?;
match TH1::read_root(&file, "h") {
    Ok(h) => println!("{} entries", h.entries()),
    Err(Error::NotFound { .. }) => println!("no object named h"),
    Err(Error::WrongClass { found, .. }) => println!("h is a {found}, not a 1-D histogram"),
    Err(e) => return Err(e),
}
# Ok::<(), oxiroot::Error>(())
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
