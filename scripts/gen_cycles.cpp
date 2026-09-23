// Writes fixtures/cycles.root: one name written three times, so the file holds
// three cycles of it (`h;1`, `h;2`, `h;3`), plus a deleted key and a two-cycle
// name inside a subdirectory. ROOT keeps every cycle and reads the highest
// unless a name asks for one: `Get("h")` is `h;3`, `Get("h;1")` is the first.
//
// Each cycle's single bin holds its own cycle number, so a reader that picks the
// wrong one is obvious. Build and run from the repository root:
//
//   c++ $(root-config --cflags) scripts/gen_cycles.cpp $(root-config --libs) -o /tmp/gen_cycles
//   /tmp/gen_cycles

#include <TFile.h>
#include <TH1D.h>

int main() {
   TFile f("fixtures/cycles.root", "RECREATE", "", 0);

   // Three cycles of "h": bin 1 holds the cycle number.
   for (int cycle = 1; cycle <= 3; ++cycle) {
      TH1D h("h", "cycles", 1, 0, 1);
      h.SetBinContent(1, cycle);
      h.Write();
   }

   // A key written then deleted: ROOT takes it out of the directory, so no read
   // finds it — by name or by cycle.
   TH1D gone("gone", "deleted", 1, 0, 1);
   gone.SetBinContent(1, 99);
   gone.Write();
   f.Delete("gone;1");

   // Two cycles inside a subdirectory, to check the same rule one level down.
   TDirectory *sub = f.mkdir("sub");
   sub->cd();
   for (int cycle = 1; cycle <= 2; ++cycle) {
      TH1D h("d", "cycles in a subdirectory", 1, 0, 1);
      h.SetBinContent(1, 10 * cycle);
      h.Write();
   }
   f.cd();

   f.Close();
   return 0;
}
