#!/usr/bin/env bash
# Generate fixtures/tree_clones.root: a TTree with a split (fSplitLevel=99)
# TClonesArray of a TObject-derived Particle { float px, py; int pid }. ROOT
# splits it into the per-member jagged sub-branches parts.px / parts.py /
# parts.pid (plus the TObject housekeeping parts.fUniqueID / parts.fBits).
# Needs ROOT 6.x (rootcling + a C++ compiler). Run from anywhere:
#
#   scripts/gen_tree_clones.sh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_clones.root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/Particle.h" <<'H'
#include <TObject.h>
class Particle : public TObject {
public:
    float px = 0, py = 0;
    int pid = 0;
    ClassDef(Particle, 1)
};
H

cat > "$tmp/LinkDef.h" <<'L'
#ifdef __ROOTCLING__
#pragma link C++ class Particle+;
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <TFile.h>
#include <TTree.h>
#include <TClonesArray.h>
#include "Particle.h"
int main(int argc, char **argv) {
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("T", "T");
    TClonesArray *arr = new TClonesArray("Particle");
    t.Branch("parts", &arr, 32000, 99); // split level 99
    for (int i = 0; i < 3; ++i) {
        arr->Clear();
        for (int j = 0; j <= i; ++j) {
            Particle *p = (Particle *)arr->ConstructedAt(j);
            p->px = j;
            p->py = j + 0.5f;
            p->pid = j * 10;
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
    rootcling -f dict.cxx Particle.h LinkDef.h
    # shellcheck disable=SC2046
    c++ $(root-config --cflags) -I/opt/homebrew/include gen.cpp dict.cxx $(root-config --libs) -o gen
)
"$tmp/gen" "$out"
echo "wrote $out"
