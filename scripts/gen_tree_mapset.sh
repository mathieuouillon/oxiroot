#!/usr/bin/env bash
# Generate fixtures/tree_mapset.root: a TTree with a std::set<int> branch and a
# std::map<int,double> branch. These need a collection-proxy dictionary; the
# proxy only registers when the dictionary is loaded as a SHARED LIBRARY at
# runtime (linking dict.cxx straight into the exe leaves cling unable to register
# it and TTree::Branch silently returns null). Needs ROOT 6.x. Run from anywhere:
#
#   scripts/gen_tree_mapset.sh
#
# ROOT writes set<int> as an unsplit object-wise collection (fType=0) and
# map<int,double> split into m.first / m.second sub-branches (fType=4/41).
#
# NOTE: registering the std::map collection proxy is cling-dependent and does not
# always succeed in this environment (same flaky cling JIT noted in the toolchain
# memo); when it fails TTree::Branch("m",...) returns null and the script aborts
# loudly rather than writing a set-only file. The committed fixture was produced
# on a run where the proxy registered; the on-disk bytes are stable regardless.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/fixtures/tree_mapset.root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/LinkDef.h" <<'L'
#ifdef __ROOTCLING__
#pragma link C++ class std::set<int>+;
#pragma link C++ class std::pair<int,double>+;
#pragma link C++ class std::pair<const int,double>+;
#pragma link C++ class std::map<int,double>+;
#endif
L

cat > "$tmp/gen.cpp" <<'CPP'
#include <map>
#include <set>
#include <TFile.h>
#include <TTree.h>
#include <TSystem.h>
int main(int argc, char **argv) {
    gSystem->Load(argv[2]); // the collection-proxy dictionary shared library
    TFile f(argv[1], "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("t", "t");
    std::set<int> s;
    std::map<int, double> m;
    if (!t.Branch("s", &s) || !t.Branch("m", &m)) {
        fprintf(stderr, "ERROR: a collection branch was not created (dictionary not loaded)\n");
        return 1;
    }
    int rows[3][3] = {{11, 22, -1}, {100, -1, -1}, {7, 8, 9}};
    for (int i = 0; i < 3; ++i) {
        s.clear();
        m.clear();
        for (int j = 0; j < 3 && rows[i][j] != -1; ++j) {
            s.insert(rows[i][j]);
            m[rows[i][j]] = rows[i][j] + 0.5;
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
    c++ -shared -fPIC $(root-config --cflags) -I/opt/homebrew/include -I. dict.cxx \
        $(root-config --libs) -o libDict.so
    # shellcheck disable=SC2046
    c++ $(root-config --cflags) -I/opt/homebrew/include gen.cpp \
        $(root-config --libs) -o gen
    # Run from the dict directory so ROOT discovers dict_rdict.pcm in the cwd
    # (needed to register the std::map collection proxy at load time).
    ./gen "$out" "$PWD/libDict.so"
)
echo "wrote $out"
