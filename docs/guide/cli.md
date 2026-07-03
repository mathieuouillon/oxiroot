# Command-line inspector (`oxroot`)

`oxroot` is a command-line tool for looking into a ROOT file — its objects, the
structure of a `TTree` or RNTuple, and the actual data — without ROOT or Python.
It ships in the `oxiroot-cli` crate and is built on the same readers as the
library.

```sh
cargo install --path crates/oxiroot-cli   # installs the `oxroot` binary
# or, from a checkout:
cargo run -p oxiroot-cli -- <command> ...
```

Objects are addressed as `file.root:name` (or `file.root:subdir/name`), the same
convention uproot uses.

## `ls` — list objects

```console
$ oxroot ls data.root -l
name    class          title                 cycle   entries
Events  TTree          reconstructed events      1     10000
pt      TH1D           transverse momentum       1         -
events  ROOT::RNTuple  columnar events           1      5000
```

`-l` adds the cycle and (for `TTree`/RNTuple) the entry count; `-r` recurses one
level into `TDirectory` subdirectories, prefixing names with `subdir/`.

## `show` — structure of a TTree or RNTuple

```console
$ oxroot show data.root:Events
TTree "Events"  (10000 entries, 4 branches)
branch  type
i       int32_t
x       double
hits    double[]
tag     char*
```

Each branch's type is shown as a scalar (`double`), a fixed array (`double[3]`),
a variable/vector (`double[]`), or a string (`char*`); branches oxiroot cannot
read are listed with a leading `!` and the reason. An RNTuple shows its
top-level fields with their C++ type names (`std::vector<float>`, `std::string`,
…).

## `dump` — print data

```console
$ oxroot dump data.root:Events -n 3 -b i,x,hits
TTree "Events"  (10000 entries; showing 3)
#  i   x      hits
0  0   1.5    [1, 2, 3]
1  1   2.5    []
2  2   3.5    [4, 5]
```

`dump` adapts to the object:

- **`TTree` / RNTuple** — the first `-n` entries as a column table; restrict the
  columns with `-b name1,name2`.
- **`TH1`** — bin edges, contents, and errors, with `mean` / `std` / `integral`
  in the header. `TH2`/`TH3` print a shape-and-stats summary rather than the full
  grid.
- **`TProfile`** — per-bin mean-y.
- **`TGraph`** — the first `-n` points.
- **`TF1` / `TF2` / `TF3`** — the formula, parameters, and (for `TF1`) range.
- **`TObjString` / `TParameter`** — the stored value.

Any other class the library can read but has no dedicated view (e.g.
`THnSparse`, `TEfficiency`) is reported by name and class rather than erroring —
use `oxroot show`/`ls` for those.

## `stat` — file summary

```console
$ oxroot stat data.root
file         data.root
size         2.4 MiB (2516481 B)
ROOT version 6.30/04
compression  zstd (level 5)
objects      3
streamers    24 classes
    TTree (v20)
    ...
```

## Subdirectories

Objects nested in `TDirectory`s are addressed with a `/`-path:
`oxroot show data.root:cal/run2/Events` descends two levels and shows the tree
there — the same for histograms, RNTuples, and `dump`. `ls -r` recurses into
**every** subdirectory, listing each object by its full `dir/sub/name` path (and,
with `-l`, resolving entry counts for trees/RNTuples at any depth).

## JSON output

The global `--json` flag makes every command emit JSON instead of a table —
handy for piping into `jq` or another tool:

```console
$ oxroot show data.root:Events --json
{"name":"Events","class":"TTree","entries":10000,"branches":[{"name":"i","type":"int32_t"},{"name":"hits","type":"double[]"}],"unreadable":[]}

$ oxroot dump data.root:Events -n 2 -b i,hits --json | jq
{
  "name": "Events",
  "class": "TTree",
  "entries": 10000,
  "showing": 2,
  "columns": ["i", "hits"],
  "rows": [[0, [1, 2, 3]], [1, []]]
}
```

Dump rows are typed — scalars become JSON numbers/booleans/strings and vector
branches become JSON arrays; non-finite floats render as `null`. The writer is
dependency-free, so `--json` adds no crates.

## Scope

`oxroot` reads what the library reads: classes it cannot decode are reported
rather than guessed. `dump` reads an RNTuple field in full before showing the
first `-n` entries.
