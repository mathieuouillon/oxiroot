#!/usr/bin/env bash
# Generate fixtures/tree_object.root: a TTree with a split (fSplitLevel=99)
# single struct branch Outer { int id; Inner inner; float w }, where Inner {
# float a; int b } is a nested struct. ROOT splits the single object into scalar
# member sub-branches id / inner.a / inner.b / w (one value per entry, unlike the
# jagged sub-branches of a split std::vector). Needs ROOT 6.x. Run from anywhere:
#
#   scripts/gen_tree_object.sh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_object.root"
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
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <TFile.h>
#include <TTree.h>
#include "Types.h"
int main(int argc, char **argv) {
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("T", "T");
    Outer o;
    t.Branch("o", &o, 32000, 99); // split a single struct-of-struct
    for (int i = 0; i < 3; ++i) {
        o.id = i;
        o.inner.a = i + 0.5f;
        o.inner.b = i * 2;
        o.w = i * 1.5f;
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
