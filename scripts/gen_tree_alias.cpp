// Dev-only fixture generator for fixtures/tree_alias.root: a tree `t` (x:I, y:F)
// with two SetAlias definitions (persisted in fAliases as a TList<TNamed> of
// (name, expression) pairs) plus a standalone TEntryList `elist` selecting
// entries 0, 2, 4 of `t`. oxiroot reads the aliases via TTree::aliases() and the
// entry list via TEntryList::open. No cling needed (SetAlias stores the
// expression string without compiling it; TEntryList is a plain bit array).
//
//   c++ $(root-config --cflags) scripts/gen_tree_alias.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_alias
//   /tmp/gen_tree_alias        # writes fixtures/tree_alias.root (run from repo root)
#include <TEntryList.h>
#include <TFile.h>
#include <TTree.h>

int main() {
    TFile f("fixtures/tree_alias.root", "RECREATE");
    f.SetCompressionLevel(0);

    Int_t x;
    Float_t y;
    TTree t("t", "t");
    t.Branch("x", &x);
    t.Branch("y", &y);
    for (int i = 0; i < 5; ++i) {
        x = i;
        y = i + 0.5f;
        t.Fill();
    }
    t.SetAlias("z", "x+y");
    t.SetAlias("twice", "2*x");
    t.Write();

    TEntryList el("elist", "selected entries");
    el.SetTree("t", "");
    el.Enter(0);
    el.Enter(2);
    el.Enter(4);
    el.Write();

    f.Close();
    return 0;
}
