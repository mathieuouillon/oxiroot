// Dev-only fixture generator for a multidimensional fixed-array TTree branch.
//
//   c++ $(root-config --cflags) scripts/gen_tree_multidim.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_multidim
//   /tmp/gen_tree_multidim        # run from the repo root
//
// One branch "m" of type float[2][3]; data is stored row-major.

#include <TFile.h>
#include <TTree.h>

int main() {
    TFile f("fixtures/tree_multidim.root", "RECREATE");
    TTree t("Events", "Events");
    float m[2][3];
    t.Branch("m", m, "m[2][3]/F");
    for (int e = 0; e < 3; ++e) {
        for (int i = 0; i < 2; ++i)
            for (int j = 0; j < 3; ++j)
                m[i][j] = e * 10 + i * 3 + j;
        t.Fill();
    }
    t.Write();
    f.Close();
    return 0;
}
