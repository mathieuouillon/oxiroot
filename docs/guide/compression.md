# Compression

oxiroot reads every compression codec ROOT writes (except the legacy `CS`) and
writes the three most common ones, all in pure Rust. This page covers the
`Compression` enum that every writer takes, the read/write codec coverage, and
ROOT's on-disk block framing.

## Choosing compression on write

Every write entry point — `obj.write_root(path, compression)`, the
[`RootFile`](../getting-started/quickstart.md) builder's `.write(compression)`,
`Tree::write_root`, `Ntuple::write_root` — takes a single
[`Compression`](../api/oxiroot/index.html) value. It is re-exported from the
prelude, so `use oxiroot::prelude::*;` is all you need.

```rust
use oxiroot::prelude::*;

let mut h = TH1::new(50, 0.0, 100.0).named("pt").titled("p_{T}");
h.fill(42.0);

// Zstandard at level 5 — ROOT's modern default.
h.write_root("zstd.root", Compression::Zstd(5))?;

// Or store the payload uncompressed.
h.write_root("plain.root", Compression::None)?;
```

The same value drives multi-object files and RNTuple/TTree writers:

```rust
use oxiroot::prelude::*;

let h = TH1::new(50, 0.0, 100.0).named("pt");
let prof = TProfile::new(5, 0.0, 5.0).named("prof");

RootFile::create("out.root")
    .add(&h)
    .add(&prof)
    .write(Compression::Zlib(1))?;   // older-ROOT-style zlib default
```

## The `Compression` enum

`Compression` lives in `oxiroot-io-core` (`oxiroot::Compression`) and is
re-exported through `oxiroot::prelude`. It is `Copy` and defaults to
`Compression::None`.

| Variant         | Algorithm     | Level range            | Encode | Decode |
|-----------------|---------------|------------------------|:------:|:------:|
| `None`          | stored as-is  | —                      | yes    | yes    |
| `Zstd(u32)`     | Zstandard     | 1–22 (ROOT default 5)  | yes    | yes    |
| `Zlib(u32)`     | zlib / DEFLATE| 1–9 (ROOT default 1)   | yes    | yes    |
| `Lz4(u32)`      | LZ4           | 1–9                    | yes    | yes    |

Two helpers are available on the enum:

| Method            | Returns | Meaning                                              |
|-------------------|---------|------------------------------------------------------|
| `setting()`       | `u32`   | ROOT's setting integer (`algorithm*100 + level`, `0` = none) |
| `is_enabled()`    | `bool`  | `false` only for `Compression::None`                 |

```rust
use oxiroot::prelude::*;

assert_eq!(Compression::Zstd(5).setting(), 505);
assert_eq!(Compression::Zlib(1).setting(), 101);
assert_eq!(Compression::None.setting(), 0);
assert!(!Compression::None.is_enabled());
```

!!! note "Level handling differs per backend"
    The level tunes the zlib backend. The pure-Rust Zstd and LZ4 backends are
    fast-mode only and ignore the level — the output is always valid ROOT
    framing and reads back correctly in ROOT, uproot, and oxiroot. There is no
    `Compression::Lzma` variant: LZMA is decode-only (see below).

## Read vs. write coverage

oxiroot decodes **every codec ROOT writes except the legacy `CS`**, and encodes
the three in active use.

| Codec        | Block tag | Read   | Write       |
|--------------|-----------|--------|-------------|
| Zstandard    | `ZS`      | yes    | yes         |
| zlib/DEFLATE | `ZL`      | yes    | yes         |
| LZ4          | `L4`      | yes    | yes         |
| LZMA (XZ)    | `XZ`      | yes    | no (decode-only) |
| old ROOT     | `CS`      | no     | no          |

Reading is fully automatic: a `TKey`'s payload (or an RNTuple page) carries its
own algorithm tag, so the reader picks the codec per block. Uncompressed
payloads — written with `Compression::None` or where the compressed form would
not be smaller — pass through directly.

!!! warning "LZMA is decode-only"
    Files written by ROOT with LZMA read back fine, but oxiroot cannot *produce*
    LZMA. There is no enum variant for it, and the underlying encoder rejects the
    LZMA algorithm code with an error. Use `Zstd`, `Zlib`, or `Lz4` for writing.

## On-disk block framing

ROOT stores a compressed payload as a sequence of independently compressed
blocks, each prefixed by a fixed **9-byte header**. A single block carries at
most ~16 MiB (`0xFFFFFF`) of uncompressed data; larger payloads are split across
consecutive blocks and stitched back on read.

The header layout (offsets within the 9 bytes):

| Bytes   | Field                | Notes                                  |
|---------|----------------------|----------------------------------------|
| `[0..2]`| algorithm tag        | two ASCII chars, e.g. `ZS`, `ZL`, `L4` |
| `[2]`   | method / version     | algorithm-specific                     |
| `[3..6]`| compressed size      | 24-bit little-endian                   |
| `[6..9]`| uncompressed size    | 24-bit little-endian                   |

These framing primitives live in the `oxiroot-compress` crate (re-exported as
`oxiroot::compress`): `BlockHeader`, `HDR_SIZE` (`9`), `MAX_CHUNK_SIZE`
(`0xFF_FFFF`), and the `Algorithm` tag enum. Most code never touches them
directly — they are the building blocks `decompress` / `compress` use — but they
are public for low-level inspection.

!!! tip "LZ4 integrity check"
    ROOT prefixes each LZ4 block payload with an 8-byte big-endian **XXH64**
    checksum of the compressed bytes. oxiroot writes that checksum on encode and
    **verifies it on read** — a corrupted LZ4 block is rejected with an error
    rather than silently mis-decoded.

## Pure-Rust backends

All codecs are pure Rust, so the no-libROOT promise holds with no C toolchain:

| Codec        | Crate                                                |
|--------------|------------------------------------------------------|
| Zstandard    | [`ruzstd`](https://crates.io/crates/ruzstd)          |
| zlib/DEFLATE | [`miniz_oxide`](https://crates.io/crates/miniz_oxide)|
| LZ4          | [`lz4_flex`](https://crates.io/crates/lz4_flex)      |
| LZMA (XZ)    | [`lzma-rs`](https://crates.io/crates/lzma-rs) (decode)|

The LZ4 XXH64 integrity check uses
[`xxhash-rust`](https://crates.io/crates/xxhash-rust), shared with RNTuple's
page hashing.

## Low-level (de)compression

The `oxiroot::compress` module exposes the raw round-trip functions for callers
that hold ROOT block bytes directly. They operate on the same setting integer
the `Compression` enum produces via `setting()`.

```rust
use oxiroot::compress::{compress, decompress, compression_settings};

// Settings integer: algorithm * 100 + level (Zstd = 5).
let settings = compression_settings(5, 5);          // 505
let payload = b"the quick brown fox ".repeat(64);

let blocks = compress(&payload, settings)?;          // ROOT block framing
let back = decompress(&blocks, payload.len())?;      // needs the original length
assert_eq!(back, payload);
```

| Function                              | Purpose                                            |
|---------------------------------------|----------------------------------------------------|
| `compress(src, settings)`             | Encode into ROOT blocks; `settings == 0` stores as-is |
| `decompress(src, uncompressed_len)`   | Decode; passthrough when `src.len() == uncompressed_len` |
| `compression_settings(algo, level)`   | Build `algo*100 + level`                           |
| `split_settings(settings)`            | Split back into `(algo, level)`                    |

`decompress` needs the expected uncompressed length (ROOT stores it in the
enclosing `TKey` or RNTuple anchor) and validates the produced size against it,
returning a `CompressError` on any truncation or size mismatch. For normal
file IO you never call these directly — `write_root` / `RFile::open` apply them
for you from the `Compression` value.

## See also

- [Quick start](../getting-started/quickstart.md) — writing files with a chosen compression
- [Histograms](histograms.md) — `write_root` and the `RootFile` builder
- [RNTuple](rntuple.md) — per-page compression in the columnar format
- [API reference](../api/oxiroot/index.html) — `Compression` and the `compress` module
