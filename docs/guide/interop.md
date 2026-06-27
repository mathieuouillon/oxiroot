# ROOT / uproot interop

oxiroot is a clean-room reimplementation of the ROOT (`TFile`) on-disk format,
so the only thing that matters is whether the bytes it writes and reads agree
with the canonical tools. This page describes the two-way interop guarantee, how
the committed test suite stays pure Rust, and how to run the full cross-language
check on your own machine.

## The two-way guarantee

Every reader and writer in oxiroot is validated in **both directions** against
both reference implementations:

| Direction | Meaning |
|-----------|---------|
| oxiroot → ROOT C++ / uproot | A file written by oxiroot opens and parses to the same values in official ROOT and in [uproot](https://uproot.readthedocs.io). |
| ROOT C++ / uproot → oxiroot | A file written by ROOT or uproot reads back to the same values in oxiroot. |

This holds across the full surface: every histogram precision and dimension,
`TProfile`/`TProfile2D`, `TEfficiency`, `THnSparse`, `TH2Poly`, the `TGraph`
family, every `TTree` branch kind (including split `std::vector<MyStruct>`), and
the RNTuple field types — uncompressed and with each writable codec.

!!! note
    uproot's RNTuple writer is experimental, so the ROOT → oxiroot RNTuple checks
    are grounded against the ROOT C++ oracle; uproot cannot write
    `std::vector<T>` `TTree` branches either. Where an oracle cannot produce a
    file, that file is simply absent and the corresponding read check is skipped
    rather than failed.

## The committed tests are pure Rust

The tests that ship in the repository and run in CI on every push need **no ROOT
and no Python**. They fall into three groups:

- **Self round-trips** — write an object, read it back, assert equality.
- **Byte-level reference agreement** — committed fixture files produced by ROOT
  C++ and uproot live under `fixtures/`; the tests read them and assert the
  decoded values, so a regression in the reader fails without any external tool.
- **Malformed-input hardening** — byte-flip and truncation fuzz tests over the
  container, RNTuple, `TTree`, and every histogram/graph reader confirm a
  crafted or truncated file yields an `Err`, never a panic.

```sh
cargo test --workspace
```

CI additionally runs the same both-ways cross-language checks described below
against official ROOT (C++) and uproot.

## Full local interop check

For a comprehensive cross-language check on your own machine (not CI), run the
harness in `scripts/interop_local.sh`. It builds oxiroot's interop drivers,
compiles the ROOT C++ oracles, exercises the full read+write surface against both
ROOT C++ and uproot in both directions, then prints a PASS/FAIL/SKIP matrix and
exits nonzero if anything failed.

```sh
bash scripts/interop_local.sh                 # full run; prints a PASS/FAIL matrix
bash scripts/interop_local.sh --no-fixtures   # skip the fixture-regen drift check (fast)
bash scripts/interop_local.sh --big           # also exercise the >2 GiB (64-bit) read path
bash scripts/interop_local.sh --keep          # keep the work dir + regenerated fixtures
```

### What it exercises

| Phase | Check |
|-------|-------|
| Canonical round-trip | The lean smoke test: a `TH1D`, an RNTuple, and a `TTree` (with every branch kind) plus the multi-object / subdirectory / append files written by the `RootFile` builder, round-tripped both ways against ROOT C++ and uproot. |
| Examples smoke | Runs the `analysis`, `tree`, and `rntuple_nested` examples (pure Rust) so a regression in the `RootFile` builder, `read_root_in`, or the `Tree`/`Ntuple` writers fails even with no oracle present. |
| Manifest-driven matrix | `interop_matrix` writes ~38 cases plus a `manifest.json`; the ROOT C++ and uproot oracles consume the manifest and assert their parse matches (Rust → oracle). |
| `cargo test --workspace` | The pure-Rust read-compat suite against the committed fixtures. |
| Fixture-regen drift check | Regenerates the committed fixtures from your *local* ROOT/uproot and re-tests, catching version drift. Skipped with `--no-fixtures`. |
| Big read | Reads back a > 2 GiB (64-bit) oracle file. Opt-in with `--big`. |

The matrix is the broad write-compat coverage. Its cases span:

- **Histograms** — every `TH1`/`TH2`/`TH3` precision (`D`/`F`/`I`/`S`/`C`/`L`)
  and dimension, `TProfile`, `Sumw2` per-bin errors, and variable bin edges.
- **File composition** — multiple objects, subdirectories, and append (the
  `RootFile` builder, plus `read_root` / `read_root_in` on the read side).
- **RNTuple** — every scalar and vector field type, across multiple clusters.
- **`TTree`** — every branch kind and scalar width, including the split
  `std::vector<Struct>` (`TBranchElement`) branch.

### Requirements

The harness degrades gracefully: a missing oracle becomes a **SKIP**, never a
**FAIL**, so a machine with neither ROOT nor uproot still gets a meaningful green
from `cargo test --workspace` alone. The only hard requirement is `cargo`.

| Capability | Provided by | If missing |
|------------|-------------|------------|
| Rust drivers + read-compat tests | `cargo` (required) | harness aborts |
| uproot side | a Python venv at `.venv` with `uproot`, `numpy`, `awkward` | uproot rows SKIP |
| ROOT C++ side | `root-config` on `PATH` | ROOT rows SKIP |
| Fixture-regen drift check | `root-config` **and** `rootcling` **and** the `.venv` | that phase SKIPs |

```sh
python -m venv .venv
.venv/bin/pip install uproot numpy awkward
```

!!! tip
    Use `--no-fixtures` for a fast iteration loop — it skips the most expensive
    phase (recompiling the C++ fixture generators and regenerating every
    committed fixture). Use `--keep` to retain the temporary work directory and
    any regenerated fixtures for inspection.

## What the round-trip looks like in code

The canonical round-trip driver lives in
[`examples/interop.rs`](../api/oxiroot/index.html) and uses only the public API.
The write side composes a histogram, an RNTuple, and a `TTree`:

```rust
use oxiroot::prelude::*;

// A histogram.
let mut h = TH1::new(4, 0.0, 4.0).named("h").titled("interop");
h.fill(0.5);
h.write_root("rust_hist.root", Compression::None)?;

// Several objects and a subdirectory via the RootFile builder.
RootFile::create("rust_multi.root")
    .add(&h)
    .dir("sub", |d| d.add(&other))
    .write(Compression::None)?;
```

The read side opens what an oracle wrote and asserts the decoded values, reading
both from the top level and from a subdirectory:

```rust
use oxiroot::prelude::*;

let f = RFile::open("oracle_dirs.root")?;
let dh = TH1::read_root(&f, "dh")?;                 // top-level key
let rh = TH1::read_root_in(&f, "region", "rh")?;    // inside subdirectory "region"
assert_eq!(dh.values(), &[2.0, 4.0]);
```

The same `read_root` / `read_root_in` (the `ReadRoot` trait) and
`write_root` / `RootFile` builder (the `WriteRoot` trait) are the only entry
points; there is one way per operation. See
[Reading & writing](reading-writing.md) for the full surface.

## See also

- [Reading & writing](reading-writing.md)
- [Compression](compression.md)
- [Quick start](../getting-started/quickstart.md)
- [API reference](../api/oxiroot/index.html)
