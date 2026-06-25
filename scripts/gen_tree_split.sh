#!/usr/bin/env bash
# Generate fixtures/tree_split.root: a TTree with a split (fSplitLevel=99)
# std::vector<Hit> TBranchElement, for the Rust reader's split-branch test.
# Needs ROOT 6.x (rootcling + a C++ compiler). Run from anywhere:
#
#   scripts/gen_tree_split.sh
#
# ROOT splits std::vector<Hit> (Hit = {float x; float y; int id;}) into the
# per-member sub-branches hits.x / hits.y / hits.id.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_split.root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/Hit.h" <<'H'
struct Hit { float x; float y; int id; };
H

cat > "$tmp/LinkDef.h" <<'L'
#ifdef __ROOTCLING__
#pragma link C++ class Hit+;
#pragma link C++ class std::vector<Hit>+;
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <vector>
#include <TFile.h>
#include <TTree.h>
#include "Hit.h"
int main(int argc, char **argv) {
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("T", "T");
    std::vector<Hit> v;
    t.Branch("hits", &v, 32000, 99); // split level 99
    for (int i = 0; i < 3; ++i) {
        v.clear();
        for (int j = 0; j <= i; ++j) {
            Hit h{(float)j, (float)(j + 0.5), j * 10};
            v.push_back(h);
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
    rootcling -f dict.cxx Hit.h LinkDef.h
    # shellcheck disable=SC2046
    c++ $(root-config --cflags) gen.cpp dict.cxx $(root-config --libs) -o gen
)
"$tmp/gen" "$out"
echo "wrote $out"
