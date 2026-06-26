// Generate the compressed TH1D fixtures that exercise oxiroot's *decode* paths
// against real ROOT output: fixtures/th1d_{zlib,lz4,lzma}.root. oxiroot can now
// encode zlib/lz4/zstd, but LZMA is decode-only, and these committed fixtures
// pin every decoder to genuine ROOT bytes.
//
//   c++ $(root-config --cflags) scripts/gen_compressed_fixtures.cpp $(root-config --libs) \
//       -o /tmp/gen_compressed && (cd <repo> && /tmp/gen_compressed)
//
// Each histogram is 500 bins so the payload exceeds ROOT's compression threshold
// and is actually compressed (not stored raw). All three share the same content:
//   bin i (1-based) = (i % 7) + 0.5 * (i % 3).

#include <cstdio>
#include <Compression.h>
#include <TFile.h>
#include <TH1D.h>
#include <TKey.h>

using EAlgorithm = ROOT::RCompressionSetting::EAlgorithm;

static void gen(const char* out, int algo, int level) {
  TFile f(out, "RECREATE");
  f.SetCompressionAlgorithm(algo);
  f.SetCompressionLevel(level);

  TH1D h("h", "compressed", 500, 0.0, 500.0);
  double entries = 0;
  for (int i = 1; i <= 500; ++i) {
    double c = (i % 7) + 0.5 * (i % 3);
    h.SetBinContent(i, c);
    entries += c;
  }
  h.SetEntries(entries);
  h.Write();
  f.Close();

  TFile* g = TFile::Open(out);
  TKey* k = g->GetKey("h");
  std::printf("wrote %s: fObjlen=%d fNbytes=%d compressed=%s\n", out, k->GetObjlen(),
              k->GetNbytes(), (k->GetObjlen() > k->GetNbytes() ? "yes" : "NO"));
  g->Close();
}

int main() {
  gen("fixtures/th1d_zlib.root", static_cast<int>(EAlgorithm::kZLIB), 5);
  gen("fixtures/th1d_lz4.root", static_cast<int>(EAlgorithm::kLZ4), 4);
  gen("fixtures/th1d_lzma.root", static_cast<int>(EAlgorithm::kLZMA), 5);
  return 0;
}
