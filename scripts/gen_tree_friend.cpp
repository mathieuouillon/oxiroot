// Dev-only fixture generator for fixtures/tree_friend.root: a main tree `t`
// (run:I, evt:I, x:F) and a friend tree `tf` (y:D) in the same file, with the
// friend attached via TTree::AddFriend so it is persisted in the main tree's
// fFriends list (a TList<TFriendElement>). oxiroot reads that list back as a
// Vec<Friend>; the friend is then read positionally (entry i of `t` pairs with
// entry i of `tf`). No BuildIndex — that compiles a TTreeFormula via cling,
// which segfaults under the broken local cling JIT.
//
//   c++ $(root-config --cflags) scripts/gen_tree_friend.cpp \
//       $(root-config --libs) -o /tmp/gen_tree_friend
//   /tmp/gen_tree_friend          # writes fixtures/tree_friend.root (run from repo root)
#include <TFile.h>
#include <TTree.h>

int main() {
    TFile f("fixtures/tree_friend.root", "RECREATE");
    f.SetCompressionLevel(0);

    Int_t run, evt;
    Float_t x;
    Double_t y;
    TTree t("t", "t");
    t.Branch("run", &run);
    t.Branch("evt", &evt);
    t.Branch("x", &x);
    TTree tf("tf", "tf");
    tf.Branch("y", &y);

    // (run, evt) keys deliberately out of sort order, x and y distinct per entry.
    int runs[5] = {1, 1, 2, 2, 1};
    int evts[5] = {20, 10, 5, 7, 30};
    float xs[5] = {1.5f, 2.5f, 3.5f, 4.5f, 5.5f};
    for (int i = 0; i < 5; ++i) {
        run = runs[i];
        evt = evts[i];
        x = xs[i];
        t.Fill();
        y = run * 100.0 + evt;
        tf.Fill();
    }
    t.AddFriend("tf");
    t.Write();
    tf.Write();
    f.Close();
    return 0;
}
