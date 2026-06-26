// Dev-only fixture generator for a std::vector<std::string> TTree branch (ROOT
// has a built-in dictionary for it, so no rootcling is needed).
//
//   c++ $(root-config --cflags) scripts/gen_tree_vecstring.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_vecstring
//   /tmp/gen_tree_vecstring        # run from the repo root

#include <string>
#include <vector>

#include <TFile.h>
#include <TTree.h>

int main() {
    TFile f("fixtures/tree_vecstring.root", "RECREATE");
    TTree t("T", "T");
    std::vector<std::string> vs;
    t.Branch("vs", &vs);
    for (int i = 0; i < 3; ++i) {
        vs.clear();
        for (int j = 0; j <= i; ++j) {
            vs.push_back("s" + std::to_string(i) + std::to_string(j));
        }
        t.Fill();
    }
    t.Write();
    f.Close();
    return 0;
}
