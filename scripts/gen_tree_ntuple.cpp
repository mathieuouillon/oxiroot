// Dev-only fixture generator for fixtures/tree_ntuple.root: a TNtuple (all
// float, "x:y:z") and a TNtupleD (all double, "a:b"). Both are TTree subclasses
// whose key class is "TNtuple"/"TNtupleD" rather than "TTree"; the streamed
// object is a TTree base wrapped in one extra {byte count, version} header plus
// a trailing Int_t fNvar, and the branch/leaf substructure is an ordinary flat
// tree of TLeafF/TLeafD scalars.
//
//   c++ $(root-config --cflags) scripts/gen_tree_ntuple.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_ntuple
//   /tmp/gen_tree_ntuple        # writes fixtures/tree_ntuple.root (run from repo root)
#include <TFile.h>
#include <TNtuple.h>
#include <TNtupleD.h>

int main() {
    TFile f("fixtures/tree_ntuple.root", "RECREATE");
    f.SetCompressionLevel(0);

    TNtuple nt("nt", "nt", "x:y:z");
    for (int i = 0; i < 4; ++i) {
        nt.Fill(i * 10 + 1.5f, i * 10 + 2.5f, i * 10 + 3.5f);
    }
    nt.Write();

    TNtupleD ntd("ntd", "ntd", "a:b");
    for (int i = 0; i < 3; ++i) {
        ntd.Fill(100.25 + i * 200, 200.25 + i * 200);
    }
    ntd.Write();

    f.Close();
    return 0;
}
