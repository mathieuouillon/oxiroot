#!/usr/bin/env python3
"""Dev helper: dump a TH1D's members + raw streamed-object bytes from a fixture.

    .venv/bin/python scripts/dump_th1d.py [fixtures/th1d_uncompressed.root]

Used to nail the exact on-disk member layout while implementing the Rust
streamer/object reader. Not part of the build or test suite.
"""

import sys

import uproot

path = sys.argv[1] if len(sys.argv) > 1 else "fixtures/th1d_uncompressed.root"
f = uproot.open(path)
h = f["h1"]

print(f"== {path} :: h1 ({h.classname}) instance_version={h.instance_version} ==")
print("-- TH1D.all_members --")
for k, v in h.all_members.items():
    if k in ("fXaxis", "fYaxis", "fZaxis"):
        print(f"  {k}: <TAxis>")
    elif hasattr(v, "tolist"):
        lst = v.tolist()
        print(f"  {k}: <array len={len(lst)}> {lst}")
    else:
        print(f"  {k}: {v!r}")

for axname in ("fXaxis", "fYaxis", "fZaxis"):
    ax = h.member(axname)
    print(f"-- {axname}.all_members (version={ax.instance_version}) --")
    for k, v in ax.all_members.items():
        if hasattr(v, "tolist"):
            lst = v.tolist()
            print(f"    {k}: <array len={len(lst)}> {lst}")
        else:
            print(f"    {k}: {v!r}")

# Locate and dump the raw (uncompressed) streamed-object bytes.
key = f.file.root_directory.key("h1")
seek = key.fSeekKey
keylen = key.fKeylen
objlen = key.fObjlen
nbytes = key.fNbytes
print(f"-- key: fSeekKey={seek} fKeylen={keylen} fObjlen={objlen} fNbytes={nbytes} --")

raw = open(path, "rb").read()
compressed = nbytes - keylen
if compressed == objlen:  # uncompressed fixture
    obj = raw[seek + keylen : seek + keylen + objlen]
    print(f"-- raw object bytes ({len(obj)}) --")
    for off in range(0, len(obj), 16):
        chunk = obj[off : off + 16]
        hexs = " ".join(f"{b:02x}" for b in chunk)
        asc = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
        print(f"  {off:4d}: {hexs:<47}  {asc}")
else:
    print("(object is compressed; run on the uncompressed fixture for a hex dump)")
