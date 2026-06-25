#!/usr/bin/env python3
"""uproot side of the manifest-driven interop matrix.

  python scripts/interop_matrix_uproot.py <dir>

Reads <dir>/manifest.json (written by crates/oxiroot/examples/interop_matrix.rs),
opens each Rust-written .root file, and asserts uproot's parse matches. Cases
flagged ``uproot_skip`` (RNTuple, unsplit ``std::vector``) are skipped — ROOT C++
is the authoritative oracle for those. Prints ``PASS``/``MISMATCH``/``SKIP`` per
case; exits nonzero if any non-skipped case failed.
"""

from __future__ import annotations

import json
import os
import sys

import uproot

FAIL = 0
PASS = 0
SKIP = 0


def close(a: float, b: float) -> bool:
    return abs(float(a) - float(b)) <= 1e-6 * max(1.0, abs(float(b)))


def close_nested(got, want) -> bool:
    """Recursively compare nested lists/scalars with float tolerance."""
    if isinstance(want, list):
        got = list(got)
        if len(got) != len(want):
            return False
        return all(close_nested(g, w) for g, w in zip(got, want))
    if isinstance(want, bool):
        return bool(got) == want
    return close(got, want)


def to_lists(arr):
    """Turn an (awkward/np) array into plain nested Python lists."""
    out = []
    for row in arr:
        if hasattr(row, "__len__") and not isinstance(row, (bytes, str)):
            out.append([float(x) for x in row])
        else:
            out.append(row)
    return out


def check_hist(obj, dim, c, where=""):
    fails = []
    vals = c["values"]
    edges = c["edges"]
    got = obj.values(flow=False)
    if not close_nested(got.tolist(), vals):
        fails.append(f"{where}values got={got.tolist()} want={vals}")
    axes = [obj.axis(i) if dim > 1 else obj.axis() for i in range(dim)]
    for i, ax in enumerate(axes):
        ge = ax.edges(flow=False).tolist()
        if not close_nested(ge, edges[i]):
            fails.append(f"{where}edges[{i}] got={ge} want={edges[i]}")
    if "sumw2_error" in c:
        ge = obj.errors(flow=False)
        if not close_nested(ge.tolist(), c["sumw2_error"]):
            fails.append(f"{where}errors got={ge.tolist()} want={c['sumw2_error']}")
    return fails


def check_tree_branches(tree, branches):
    fails = []
    ta = tree.arrays(library="np")
    for b in branches:
        name = b["name"]
        leaf = b["leaf"]
        want = b["values"]
        arr = ta[name]
        if leaf == "scalar":
            ty = b["type"]
            if ty == "bool":
                got = [bool(x) for x in arr]
            elif "float" in ty:
                got = [float(x) for x in arr]
            else:
                got = [int(x) for x in arr]
            if got != want:
                fails.append(f"{name} got={got} want={want}")
        elif leaf == "fixed":
            got = [[float(x) for x in row] for row in arr]
            if not close_nested(got, want):
                fails.append(f"{name} got={got} want={want}")
        elif leaf == "jagged":
            got = [[float(x) for x in row] for row in arr]
            if not close_nested(got, want):
                fails.append(f"{name} got={got} want={want}")
        elif leaf == "string":
            got = [s.decode() if isinstance(s, bytes) else str(s) for s in arr]
            if got != want:
                fails.append(f"{name} got={got} want={want}")
        elif leaf == "stl_vector":
            got = [[float(x) for x in row] for row in arr]
            if not close_nested(got, want):
                fails.append(f"{name} got={got} want={want}")
        else:
            fails.append(f"{name}: unknown leaf {leaf}")
    return fails


def check_tree_split(tree, split):
    fails = []
    branch = split["branch"]
    ta = tree.arrays(library="np")
    for m in split["members"]:
        field = f"{branch}.{m['name']}"
        want = m["values"]
        got = [[float(x) for x in row] for row in ta[field]]
        if not close_nested(got, want):
            fails.append(f"{field} got={got} want={want}")
    return fails


def run_case(d, c):
    global FAIL, PASS, SKIP
    cid = c["id"]
    if c.get("uproot_skip"):
        print(f"SKIP {cid} (uproot_skip)")
        SKIP += 1
        return
    kind = c["kind"]
    path = os.path.join(d, c["file"]) if "file" in c else None
    fails = []
    try:
        if kind == "hist":
            f = uproot.open(path)
            fails = check_hist(f[c["name"]], int(c["dim"]), c)
        elif kind == "profile":
            f = uproot.open(path)
            obj = f[c["name"]]
            got = obj.values(flow=False).tolist()
            if not close_nested(got, c["values"]):
                fails.append(f"profile values got={got} want={c['values']}")
        elif kind == "hist_multi":
            f = uproot.open(path)
            for o in c["objects"]:
                fails += check_hist(f[o["name"]], int(o["dim"]), o, where=o["name"] + " ")
        elif kind == "hist_dirs":
            f = uproot.open(path)
            for o in c["root_objects"]:
                fails += check_hist(f[o["name"]], int(o["dim"]), o, where=o["name"] + " ")
            for dd in c["dirs"]:
                for o in dd["objects"]:
                    key = f"{dd['dir']}/{o['name']}"
                    fails += check_hist(f[key], int(o["dim"]), o, where=key + " ")
        elif kind == "tree":
            tree = uproot.open(path)[c["name"]]
            if "split" in c:
                fails = check_tree_split(tree, c["split"])
            else:
                fails = check_tree_branches(tree, c["branches"])
        else:
            fails = [f"unknown kind {kind}"]
    except Exception as e:  # noqa: BLE001 — report any reader error as a mismatch
        fails = [f"exception: {e}"]

    if fails:
        print(f"MISMATCH {cid}: {fails[0]}")
        FAIL += 1
    else:
        print(f"PASS {cid}")
        PASS += 1


def main():
    if len(sys.argv) != 2:
        print("usage: interop_matrix_uproot.py <dir>", file=sys.stderr)
        sys.exit(2)
    d = sys.argv[1]
    with open(os.path.join(d, "manifest.json")) as fh:
        manifest = json.load(fh)
    for c in manifest["cases"]:
        run_case(d, c)
    print(f"uproot matrix: {PASS} passed, {FAIL} failed, {SKIP} skipped")
    sys.exit(1 if FAIL else 0)


if __name__ == "__main__":
    main()
