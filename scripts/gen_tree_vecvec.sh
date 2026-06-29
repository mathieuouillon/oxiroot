#!/usr/bin/env bash
# Generate fixtures/tree_vecvec.root: a TTree with unsplit
# std::vector<std::vector<int>> and std::vector<std::vector<double>>
# TBranchElement branches (a doubly-nested collection per entry). Needs ROOT 6.x
# (rootcling + a C++ compiler). Run from anywhere:
#
#   scripts/gen_tree_vecvec.sh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_vecvec.root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/LinkDef.h" <<'L'
#ifdef __ROOTCLING__
#pragma link C++ class std::vector<std::vector<int>>+;
#pragma link C++ class std::vector<std::vector<double>>+;
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <vector>
#include <TFile.h>
#include <TTree.h>
int main(int argc, char **argv) {
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("T", "T");
    std::vector<std::vector<int>> vi;
    std::vector<std::vector<double>> vd;
    t.Branch("vi", &vi);
    t.Branch("vd", &vd);
    for (int i = 0; i < 3; ++i) {
        vi.clear();
        vd.clear();
        for (int j = 0; j <= i; ++j) {
            std::vector<int> inni;
            std::vector<double> innd;
            for (int k = 0; k <= j; ++k) {
                inni.push_back(i * 100 + j * 10 + k);
                innd.push_back(i + j * 0.1 + k * 0.01);
            }
            vi.push_back(inni);
            vd.push_back(innd);
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
    rootcling -f dict.cxx LinkDef.h
    # shellcheck disable=SC2046
    c++ $(root-config --cflags) -I/opt/homebrew/include gen.cpp dict.cxx $(root-config --libs) -o gen
)
"$tmp/gen" "$out"
echo "wrote $out"
