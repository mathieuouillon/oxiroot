// Dev-only fixture generator for a multi-leaf (leaflist) TTree branch.
//
//   c++ $(root-config --cflags) scripts/gen_tree_leaflist.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_leaflist
//   /tmp/gen_tree_leaflist        # run from the repo root
//
// One branch "s" with three leaves a/F, b/I, c/D packed at their fOffsets.

#include <TFile.h>
#include <TTree.h>

struct S {
    float a;
    int b;
    double c;
};

int main() {
    TFile f("fixtures/tree_leaflist.root", "RECREATE");
    TTree t("Events", "Events");
    S s;
    t.Branch("s", &s, "a/F:b/I:c/D");
    for (int i = 0; i < 4; ++i) {
        s.a = i + 0.5f;
        s.b = i * 10;
        s.c = i * 1.25;
        t.Fill();
    }
    t.Write();
    f.Close();
    return 0;
}
