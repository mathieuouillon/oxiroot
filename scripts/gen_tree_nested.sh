#!/usr/bin/env bash
# Generate fixtures/tree_nested.root: a TTree with a split (fSplitLevel=99)
# std::vector<Outer> TBranchElement, where Outer { int id; Inner inner; float w }
# nests another struct Inner { float a; int b }. ROOT flattens the nested member
# into the per-member sub-branches v.id / v.inner.a / v.inner.b / v.w, each a
# jagged array. Needs ROOT 6.x (rootcling + a C++ compiler). Run from anywhere:
#
#   scripts/gen_tree_nested.sh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_nested.root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/Types.h" <<'H'
struct Inner { float a; int b; };
struct Outer { int id; Inner inner; float w; };
H

cat > "$tmp/LinkDef.h" <<'L'
#ifdef __ROOTCLING__
#pragma link C++ class Inner+;
#pragma link C++ class Outer+;
#pragma link C++ class std::vector<Outer>+;
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <vector>
#include <TFile.h>
#include <TTree.h>
#include "Types.h"
int main(int argc, char **argv) {
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("T", "T");
    std::vector<Outer> v;
    t.Branch("v", &v, 32000, 99); // split level 99
    for (int i = 0; i < 3; ++i) {
        v.clear();
        for (int j = 0; j <= i; ++j) {
            Outer o;
            o.id = j;
            o.inner.a = j + 0.5f;
            o.inner.b = j * 2;
            o.w = j * 1.5f;
            v.push_back(o);
        }
        t.Fill();
    }
    t.Write();
    f.Close();
    return 0;
}
CPP

(
    cd "$tmp"
    rootcling -f dict.cxx Types.h LinkDef.h
    # shellcheck disable=SC2046
    c++ $(root-config --cflags) -I/opt/homebrew/include gen.cpp dict.cxx $(root-config --libs) -o gen
)
"$tmp/gen" "$out"
echo "wrote $out"
