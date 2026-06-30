// Dev-only fixture generator for fixtures/tree_object_old.root: old-style unsplit
// object branches. TTree::BranchOld with splitlevel 0 creates a TBranchObject
// (leaf TLeafObject) storing a whole object per entry — the pre-TBranchElement
// way of writing a class to a tree. Three classes cover string, double and int
// members: TNamed (fName/fTitle), TParameter<double> (fName/fVal) and
// TParameter<int>. oxiroot reads each as synthesized `branch.member` columns.
// All three classes have compiled dictionaries in libCore, so no cling is needed.
//
//   c++ $(root-config --cflags) scripts/gen_tree_object_old.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_object_old
//   /tmp/gen_tree_object_old      # writes fixtures/tree_object_old.root (run from repo root)
#include <TFile.h>
#include <TNamed.h>
#include <TParameter.h>
#include <TTree.h>

int main() {
    TFile f("fixtures/tree_object_old.root", "RECREATE");
    f.SetCompressionLevel(0);
    TTree t("t", "t");

    TNamed* nm = new TNamed("", "");
    TParameter<double>* pd = new TParameter<double>("pd", 0.0);
    TParameter<int>* pi = new TParameter<int>("pi", 0);
    t.BranchOld("nm", "TNamed", &nm, 32000, 0);
    t.BranchOld("pd", "TParameter<double>", &pd, 32000, 0);
    t.BranchOld("pi", "TParameter<int>", &pi, 32000, 0);

    for (int i = 0; i < 3; ++i) {
        nm->SetName(Form("name%d", i));
        nm->SetTitle(Form("ttl%d", i));
        pd->SetVal(i + 0.5);
        pi->SetVal(i * 10);
        t.Fill();
    }
    t.Write();
    f.Close();
    return 0;
}
