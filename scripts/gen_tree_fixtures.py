#!/usr/bin/env python3
"""Generate committed TTree fixtures (uproot — pure Python, no ROOT needed).

    .venv/bin/python scripts/gen_tree_fixtures.py

Writes deterministic flat trees the Rust tests read back:
  - tree_flat.root        : one uncompressed basket per branch
  - tree_zstd.root        : Zstd-compressed
  - tree_multibasket.root : two baskets per branch (two extend() flushes)
"""

from __future__ import annotations

import os

import numpy as np
import uproot

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT_DIR, "fixtures")

# Canonical flat dataset (5 entries) — must match the Rust test expectations.
DATA = {
    "i4": np.array([0, 1, 2, 3, 4], dtype=np.int32),
    "i8": np.array([10, 11, 12, 13, 14], dtype=np.int64),
    "f4": np.array([0.5, 1.5, 2.5, 3.5, 4.5], dtype=np.float32),
    "f8": np.array([0.25, 1.25, 2.25, 3.25, 4.25], dtype=np.float64),
    "b1": np.array([True, False, True, False, True], dtype=bool),
    "u4": np.array([100, 200, 300, 400, 4000000000], dtype=np.uint32),
}
TYPES = {k: v.dtype for k, v in DATA.items()}


def write(name: str, compression, splits) -> None:
    path = os.path.join(FIXTURES, name)
    with uproot.recreate(path, compression=compression) as f:
        f.mktree("Events", TYPES)
        for lo, hi in splits:
            f["Events"].extend({k: v[lo:hi] for k, v in DATA.items()})
    print("wrote", path)


def main() -> None:
    write("tree_flat.root", None, [(0, 5)])
    write("tree_zstd.root", uproot.ZSTD(5), [(0, 5)])
    write("tree_multibasket.root", None, [(0, 3), (3, 5)])


if __name__ == "__main__":
    main()
