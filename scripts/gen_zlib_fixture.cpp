// Generate fixtures/th1d_zlib.root — a zlib-compressed (ZL) TH1D, to exercise
// oxiroot's zlib *decode* path (oxiroot writes only zstd/none; every other
// committed fixture is uncompressed or zstd, so nothing else tests zlib read).
//
//   c++ $(root-config --cflags) scripts/gen_zlib_fixture.cpp $(root-config --libs) \
//       -o /tmp/gen_zlib && (cd <repo> && /tmp/gen_zlib)
//
// The histogram is deliberately large (500 bins) so the payload exceeds ROOT's
// compression threshold and is actually zlib-compressed, not stored raw.

#include <cstdio>
#include <Compression.h>
#include <TFile.h>
#include <TH1D.h>
#include <TKey.h>

int main(int argc, char** argv) {
  const char* out = argc > 1 ? argv[1] : "fixtures/th1d_zlib.root";
  TFile f(out, "RECREATE");
  f.SetCompressionAlgorithm(ROOT::RCompressionSetting::EAlgorithm::kZLIB);
  f.SetCompressionLevel(5);

  TH1D h("h", "zlib", 500, 0.0, 500.0);
  double entries = 0;
  for (int i = 1; i <= 500; ++i) {
    double c = (i % 7) + 0.5 * (i % 3);  // varied but deterministic
    h.SetBinContent(i, c);
    entries += c;
  }
  h.SetEntries(entries);
  h.Write();
  f.Close();

  // Report whether the key was actually compressed (obj_len > on-disk payload).
  TFile* g = TFile::Open(out);
  TKey* k = g->GetKey("h");
  std::printf("wrote %s: fObjlen=%d fNbytes=%d compressed=%s\n", out, k->GetObjlen(),
              k->GetNbytes(), (k->GetObjlen() > k->GetNbytes() ? "yes" : "NO"));
  g->Close();
  return 0;
}
