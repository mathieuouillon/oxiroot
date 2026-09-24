// Writes fixtures/tree_index.root: a main tree and a friend tree holding the
// same (run, event) keys in a different order, with TTree::BuildIndex on the
// friend — ROOT's index-based join, which pairs entries by key rather than by
// entry number.
//
// The friend's `weight` is `run * 1000 + event`, so a join that pairs the wrong
// entries is obvious. crates/oxiroot-tree/tests/it/tree_index.rs asserts the
// pairing ROOT itself reports for this file (printed at the end of this
// program). Build and run from the repository root:
//
//   c++ $(root-config --cflags) scripts/gen_tree_index.cpp $(root-config --libs) -o /tmp/gen_tree_index
//   /tmp/gen_tree_index
#include <TFile.h>
#include <TTree.h>
#include <cstdio>

int main() {
   TFile f("fixtures/tree_index.root", "RECREATE", "", 0);

   Int_t run, event;
   Double_t value;

   // Main tree: (run, event) in order.
   TTree main("main", "main");
   main.Branch("run", &run, "run/I");
   main.Branch("event", &event, "event/I");
   main.Branch("value", &value, "value/D");
   const Int_t runs[6] = {1, 1, 1, 2, 2, 2};
   const Int_t events[6] = {10, 11, 12, 10, 11, 12};
   for (int i = 0; i < 6; ++i) {
      run = runs[i];
      event = events[i];
      value = 100 + i;
      main.Fill();
   }
   main.Write();

   // Friend: the same keys, shuffled, with a value that names its key.
   TTree fr("fr", "friend");
   Double_t weight;
   fr.Branch("run", &run, "run/I");
   fr.Branch("event", &event, "event/I");
   fr.Branch("weight", &weight, "weight/D");
   const Int_t order[6] = {5, 3, 0, 4, 2, 1}; // entry i of the friend holds key order[i]
   for (int i = 0; i < 6; ++i) {
      run = runs[order[i]];
      event = events[order[i]];
      weight = run * 1000 + event; // 1010, 1011, … so a wrong join is obvious
      fr.Fill();
   }
   fr.BuildIndex("run", "event");
   fr.Write();

   f.Close();

   // What ROOT's own join gives, for the test to match.
   TFile g("fixtures/tree_index.root");
   TTree *m = (TTree *)g.Get("main");
   TTree *x = (TTree *)g.Get("fr");
   m->AddFriend(x);
   Double_t w;
   m->SetBranchAddress("value", &value);
   x->SetBranchAddress("weight", &w);
   for (Long64_t i = 0; i < m->GetEntries(); ++i) {
      m->GetEntry(i);
      Long64_t fe = x->GetEntryNumberWithIndex(runs[i], events[i]);
      x->GetEntry(fe);
      printf("main %lld value %g -> friend entry %lld weight %g\n", i, value, fe, w);
   }
   return 0;
}
