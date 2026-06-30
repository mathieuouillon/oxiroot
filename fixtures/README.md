# Test fixtures

Small, committed `.root` files plus golden JSON used by the Rust test suite.
They are **committed** so tests (and CI) run without ROOT or Python installed.

## Regenerating

Fixtures are produced by [`scripts/gen_fixtures.py`](../scripts/gen_fixtures.py)
using [uproot](https://github.com/scikit-hep/uproot5) (pure Python — no ROOT/cling
required):

```sh
python3 -m venv .venv
.venv/bin/pip install uproot numpy
.venv/bin/python scripts/gen_fixtures.py
```

Bin contents are fixed (no RNG), but ROOT/uproot embed a random UUID and a
creation timestamp, so regenerating changes the raw bytes (and the recorded
`sha256`). The committed files are the source of truth; tests assert on parsed
values, not on `sha256`. Each `<name>.root` has a sibling `golden/<name>.json`
recording parsed expectations (class names, histogram values/edges, entries, …).

Histogram types uproot cannot write (it emits doubles for everything) — `TH1F`,
`TH2F`, `TH3*`, `TProfile` — are produced by a small compiled C++ ROOT program,
[`scripts/gen_root_fixtures.cpp`](../scripts/gen_root_fixtures.cpp), then their
golden is filled in by `gen_fixtures.py`. Compiled C++ avoids the broken cling
JIT (see below). Build/run:

```sh
c++ $(root-config --cflags) scripts/gen_root_fixtures.cpp $(root-config --libs) -o /tmp/gen
/tmp/gen          # run from the repo root; writes fixtures/*.root
```

## Files

| File | Contents | Source |
|------|----------|--------|
| `th1d_uncompressed.root`, `th1d_zstd.root` | a `TH1D` `h1`, 17 bins over [-4, 4] (uncompressed + Zstd) | uproot |
| `th2d_uncompressed.root` | a `TH2D` `h2`, 3×2 bins | uproot |
| `th1f_uncompressed.root` | a `TH1F` `h1f`, 5 bins | C++ ROOT |
| `th2f_uncompressed.root` | a `TH2F` `h2f`, 3×2 bins | C++ ROOT |
| `th3d_uncompressed.root`, `th3f_uncompressed.root` | a `TH3D`/`TH3F` `h3`, 2×2×2 bins | C++ ROOT |
| `tprofile_uncompressed.root` | a `TProfile` `p`, 4 bins | C++ ROOT |
| `persist_objs.root` | a `TObjString` `label` + `TParameter<double/int/Long64_t>` `lumi`/`nevents`/`bignum` (`scripts/gen_persist_objs.cpp`) | C++ ROOT |
| `collections.root` | a `THStack` `hs` (2×`TH1F`) + `TMultiGraph` `mg` (2×`TGraph`) (`scripts/gen_collections.cpp`) | C++ ROOT |
| `linalg.root` | a `TVectorD` `v`, `TMatrixD` `m` (2×3), `TMatrixDSym` `s` (3×3) (`scripts/gen_linalg.cpp`) | C++ ROOT |
| `objlist.root` | a `TList` `mylist` (TH1F + TObjString + TParameter) + `TObjArray` `myarr` (2×TH1F) (`scripts/gen_objlist.cpp`) | C++ ROOT |

## Why uproot and not ROOT?

The official ROOT install used during development (6.38.04) has a broken
cling/PyROOT JIT on this macOS box, so it is unreliable as an oracle. uproot
sidesteps that entirely. RNTuple reference files (which uproot reads but does not
write) will be generated later via a compiled C++ ROOT program through
`root-config`, which avoids the JIT.

## Spec references

- RNTuple binary format v1.0.0.0:
  <https://github.com/root-project/root/blob/v6-34-00-patches/tree/ntuple/v7/doc/BinaryFormatSpecification.md>
