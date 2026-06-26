// Generate fixtures/analysis.root — inputs for the histogram-analysis tests
// (labelled axes, interpolate, quantiles, Chi2Test, KolmogorovTest), pinned to
// real ROOT output.
//
//   c++ $(root-config --cflags) scripts/gen_analysis_fixtures.cpp $(root-config --libs) \
//       -o /tmp/gen_analysis && (cd <repo> && /tmp/gen_analysis)

#include <TFile.h>
#include <TH1D.h>

int main() {
  TFile f("fixtures/analysis.root", "RECREATE");
  f.SetCompressionLevel(0);

  // Alphanumeric (labelled) axis.
  TH1D hl("hl", "labelled", 3, 0, 3);
  hl.Fill("apple", 5);
  hl.Fill("banana", 2);
  hl.Fill("cherry", 8);
  hl.Write();

  // A smooth parabola for interpolate / quantiles.
  TH1D h("h", "data", 20, 0, 20);
  for (int i = 1; i <= 20; ++i) h.SetBinContent(i, (double)(i * (21 - i)));
  h.SetEntries(2000);
  h.Write();

  // A perturbed copy for Chi2Test / KolmogorovTest against `h`.
  TH1D g("g", "data2", 20, 0, 20);
  for (int i = 1; i <= 20; ++i) g.SetBinContent(i, (double)(i * (21 - i)) + (i % 3 ? 3 : -2));
  g.SetEntries(2000);
  g.Write();

  f.Close();
  return 0;
}
