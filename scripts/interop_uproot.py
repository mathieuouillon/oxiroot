#!/usr/bin/env python3
"""uproot side of the round-trip interop check.

  python scripts/interop_uproot.py read  <dir>   # read Rust-written files
  python scripts/interop_uproot.py write <dir>   # write oracle files for Rust

Canonical dataset (must match crates/oxiroot/examples/interop.rs):
  - TH1D "h": 4 bins over [0, 4), in-range bin contents [1, 2, 3, 4].
  - RNTuple "ntpl": x = int32 [1..5], y = double [1.5..5.5].

uproot reliably reads/writes classic histograms and reads RNTuple. Its RNTuple
*writer* is experimental, so the "oracle writes RNTuple -> Rust reads" direction
is left to the ROOT C++ job; here we only read the Rust-written RNTuple.
"""

from __future__ import annotations

import os
import sys

import numpy as np
import uproot

HIST_BINS = [1.0, 2.0, 3.0, 4.0]
HIST_EDGES = [0.0, 1.0, 2.0, 3.0, 4.0]
NTPL_X = [1, 2, 3, 4, 5]
NTPL_Y = [1.5, 2.5, 3.5, 4.5, 5.5]
TREE_TV = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0], [10.0, 11.0, 12.0], [13.0, 14.0, 15.0]]
TREE_TS = ["a", "bb", "ccc", "dddd", "eeeee"]
TREE_TJ = [[1.0], [2.0, 3.0], [], [4.0, 5.0, 6.0], [7.0]]
TREE_TW = [[10.0, 20.0], [], [30.0], [40.0, 50.0], [60.0, 70.0, 80.0]]
# Oracle-written TTree "otree" (uproot → Rust); uproot cannot write std::vector,
# so it omits the `ov` branch that the ROOT C++ oracle adds.
OTREE_OI = [10, 11, 12]
OTREE_OJ = [[1.0, 2.0], [], [3.0]]
OTREE_OS = ["x", "yy", "zzz"]


def _fail(msg: str) -> None:
    print(f"interop MISMATCH: {msg}", file=sys.stderr)
    sys.exit(1)


def read(d: str) -> None:
    # Histogram written by Rust.
    h = uproot.open(os.path.join(d, "rust_hist.root"))["h"]
    counts = list(h.values())
    if counts != HIST_BINS:
        _fail(f"rust hist contents: got {counts}, want {HIST_BINS}")

    # RNTuple written by Rust (uproot reads RNTuple natively).
    ntpl = uproot.open(os.path.join(d, "rust_ntuple.root"))["ntpl"]
    arr = ntpl.arrays()
    x = [int(v) for v in arr["x"]]
    y = [float(v) for v in arr["y"]]
    if x != NTPL_X:
        _fail(f"rust ntuple x: got {x}, want {NTPL_X}")
    if y != NTPL_Y:
        _fail(f"rust ntuple y: got {y}, want {NTPL_Y}")

    # TTree written by Rust.
    tree = uproot.open(os.path.join(d, "rust_tree.root"))["Tree"]
    ta = tree.arrays(library="np")
    ti = [int(v) for v in ta["ti"]]
    tf = [float(v) for v in ta["tf"]]
    tv = [[float(x) for x in row] for row in ta["tv"]]
    ts = [s.decode() if isinstance(s, bytes) else s for s in ta["ts"]]
    tj = [[float(x) for x in row] for row in ta["tj"]]
    tw = [[float(x) for x in row] for row in ta["tw"]]
    if ti != NTPL_X:
        _fail(f"rust tree ti: got {ti}, want {NTPL_X}")
    if tf != NTPL_Y:
        _fail(f"rust tree tf: got {tf}, want {NTPL_Y}")
    if tv != TREE_TV:
        _fail(f"rust tree tv: got {tv}, want {TREE_TV}")
    if ts != TREE_TS:
        _fail(f"rust tree ts: got {ts}, want {TREE_TS}")
    if tj != TREE_TJ:
        _fail(f"rust tree tj: got {tj}, want {TREE_TJ}")
    if tw != TREE_TW:
        _fail(f"rust tree tw: got {tw}, want {TREE_TW}")

    print("uproot read Rust hist + RNTuple + TTree — values match")


def write(d: str) -> None:
    # A TH1D with the canonical in-range bin contents.
    with uproot.recreate(os.path.join(d, "oracle_hist.root")) as f:
        f["h"] = (np.array(HIST_BINS, dtype=np.float64), np.array(HIST_EDGES, dtype=np.float64))

    # A TTree "otree": scalar oi, jagged oj, string os. (uproot's TTree writer
    # cannot emit std::vector/TBranchElement, so the ROOT C++ oracle adds `ov`.)
    import awkward as ak

    with uproot.recreate(os.path.join(d, "oracle_tree.root")) as f:
        f.mktree("otree", {"oi": np.int32, "oj": "var * float64", "os": str})
        f["otree"].extend(
            {
                "oi": np.array(OTREE_OI, dtype=np.int32),
                "oj": ak.Array(OTREE_OJ),
                "os": OTREE_OS,
            }
        )
    print("uproot wrote oracle_hist.root + oracle_tree.root")


def main() -> None:
    if len(sys.argv) != 3 or sys.argv[1] not in ("read", "write"):
        print("usage: interop_uproot.py <read|write> <dir>", file=sys.stderr)
        sys.exit(2)
    (read if sys.argv[1] == "read" else write)(sys.argv[2])


if __name__ == "__main__":
    main()
