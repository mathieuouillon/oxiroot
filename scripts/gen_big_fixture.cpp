// Generate a >2 GiB ROOT file to exercise oxiroot's 64-bit (big-format) READ
// path. Not committed — produced on demand by `scripts/interop_local.sh --big`.
//
//   c++ $(root-config --cflags) scripts/gen_big_fixture.cpp $(root-config --libs) \
//       -o /tmp/gen_big && /tmp/gen_big /tmp/big.root
//
// A wide "filler" TTree pushes the file past 2 GiB, then a tiny TTree "T" (i32
// branch `n` = entry index) is written *after* it — so T's key, tree object, and
// basket all sit at >2 GiB offsets. The matrix reader (`interop_matrix read-big`)
// reads T back cheaply (5 entries) while exercising the 64-bit seeks.

#include <cstdio>
#include <TFile.h>
#include <TTree.h>

int main(int argc, char** argv) {
  const char* out = argc > 1 ? argv[1] : "/tmp/big.root";
  TFile f(out, "RECREATE");
  f.SetCompressionLevel(0);  // uncompressed → predictable size growth

  {
    TTree filler("filler", "filler");
    const int W = 1024;
    double buf[W];
    filler.Branch("a", buf, "a[1024]/D");
    // 300000 * 1024 * 8 bytes ~ 2.4 GiB, comfortably past the 2 GiB threshold.
    for (long i = 0; i < 300000; ++i) {
      for (int j = 0; j < W; ++j) buf[j] = (double)(i + j);
      filler.Fill();
    }
    filler.Write();
  }
  {
    TTree t("T", "T");
    int n;
    t.Branch("n", &n, "n/I");
    for (int i = 0; i < 5; ++i) {
      n = i;
      t.Fill();
    }
    t.Write();
  }
  f.Close();
  std::printf("wrote %s (>2 GiB)\n", out);
  return 0;
}
