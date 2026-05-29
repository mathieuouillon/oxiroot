#!/usr/bin/env python3
"""Generate committed ROOT fixture files and their golden JSON.

Dev-only tool: uses `uproot` (pure Python — no ROOT/cling needed) to write
small, deterministic `.root` files plus a sibling `golden/<name>.json`
capturing parsed expectations the Rust tests assert against.

    .venv/bin/python scripts/gen_fixtures.py

Re-running reproduces the fixtures byte-for-byte (no randomness, no timestamps
that uproot would vary).
"""

from __future__ import annotations

import hashlib
import json
import os

import numpy as np
import uproot

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT_DIR, "fixtures")
GOLDEN = os.path.join(FIXTURES, "golden")


def _sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        h.update(fh.read())
    return h.hexdigest()


def _write_golden(name: str, payload: dict) -> None:
    with open(os.path.join(GOLDEN, name + ".json"), "w") as fh:
        json.dump(payload, fh, indent=2, sort_keys=True)
        fh.write("\n")


def th1d_fixture(compression) -> None:
    """A deterministic 1-D double histogram (TH1D)."""
    suffix = "uncompressed" if compression is None else "zstd"
    fname = f"th1d_{suffix}.root"
    path = os.path.join(FIXTURES, fname)

    # Fixed bin contents and edges — no RNG, fully reproducible.
    counts = np.array(
        [2, 5, 11, 18, 30, 44, 60, 70, 72, 68, 55, 40, 28, 16, 9, 4, 1],
        dtype=np.float64,
    )
    edges = np.linspace(-4.0, 4.0, len(counts) + 1)

    with uproot.recreate(path, compression=compression) as f:
        f["h1"] = (counts, edges)

    with uproot.open(path) as f:
        keys = f.keys(cycle=False)
        classnames = {k: f[k].classname for k in keys}
        h = f["h1"]
        values = h.values(flow=False).tolist()
        read_edges = h.axis().edges(flow=False).tolist()

    _write_golden(
        f"th1d_{suffix}",
        {
            "file": fname,
            "compression": "none" if compression is None else "zstd",
            "sha256": _sha256(path),
            "keys": keys,
            "classnames": {k: classnames[k] for k in keys},
            "h1": {
                "name": "h1",
                "values": values,
                "edges": read_edges,
                "n_bins": len(counts),
                "entries": float(counts.sum()),
            },
        },
    )
    print(f"  wrote {fname}: keys={keys} classes={list(classnames.values())}")


def th2d_fixture(compression) -> None:
    """A deterministic 2-D double histogram (TH2D), 3x2 bins."""
    suffix = "uncompressed" if compression is None else "zstd"
    fname = f"th2d_{suffix}.root"
    path = os.path.join(FIXTURES, fname)

    # values[ix][iy], 3 x bins, 2 y bins.
    values = np.array([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]], dtype=np.float64)
    xedges = np.linspace(0.0, 3.0, 4)
    yedges = np.linspace(0.0, 2.0, 3)

    with uproot.recreate(path, compression=compression) as f:
        f["h2"] = (values, xedges, yedges)

    with uproot.open(path) as f:
        h = f["h2"]
        read_values = h.values(flow=False).tolist()
        xe = h.axis(0).edges(flow=False).tolist()
        ye = h.axis(1).edges(flow=False).tolist()

    _write_golden(
        f"th2d_{suffix}",
        {
            "file": fname,
            "compression": "none" if compression is None else "zstd",
            "sha256": _sha256(path),
            "h2": {
                "name": "h2",
                "values": read_values,
                "xedges": xe,
                "yedges": ye,
                "nx": len(xedges) - 1,
                "ny": len(yedges) - 1,
                "entries": float(values.sum()),
            },
        },
    )
    print(f"  wrote {fname}: x={len(xedges) - 1} y={len(yedges) - 1} bins")


def golden_from_file(fixture: str, key: str, ndim: int) -> None:
    """Read an existing fixture (e.g. one written by the C++ ROOT generator)
    and emit its golden JSON. Used for TH1F/TH2F/TH3*/TProfile."""
    path = os.path.join(FIXTURES, fixture)
    with uproot.open(path) as f:
        h = f[key]
        payload = {
            "file": fixture,
            "sha256": _sha256(path),
            "classname": h.classname,
            "name": key,
            "values": h.values(flow=False).tolist(),
            "edges": [h.axis(i).edges(flow=False).tolist() for i in range(ndim)],
            "entries": float(h.member("fEntries")),
        }
    name = fixture[:-5] if fixture.endswith(".root") else fixture
    _write_golden(name, payload)
    print(f"  golden for {fixture}: {h.classname}")


def main() -> None:
    os.makedirs(GOLDEN, exist_ok=True)
    print("Generating fixtures with uproot", uproot.__version__)
    th1d_fixture(compression=None)
    th1d_fixture(compression=uproot.ZSTD(5))
    th2d_fixture(compression=None)

    # Golden for fixtures written by scripts/gen_root_fixtures.cpp (needs ROOT).
    for fixture, key, ndim in [
        ("th1f_uncompressed.root", "h1f", 1),
        ("th2f_uncompressed.root", "h2f", 2),
        ("th3d_uncompressed.root", "h3", 3),
        ("th3f_uncompressed.root", "h3", 3),
        ("th1c_uncompressed.root", "h1c", 1),
        ("th1s_uncompressed.root", "h1s", 1),
        ("th1i_uncompressed.root", "h1i", 1),
        ("th1l_uncompressed.root", "h1l", 1),
        ("tprofile_uncompressed.root", "p", 1),
    ]:
        if os.path.exists(os.path.join(FIXTURES, fixture)):
            golden_from_file(fixture, key, ndim)
    print("done.")


if __name__ == "__main__":
    main()
