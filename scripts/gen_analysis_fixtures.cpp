// Generate fixtures/analysis.root — inputs for the histogram-analysis tests
// (labelled axes, interpolate, quantiles, Chi2Test, KolmogorovTest), pinned to
// real ROOT output.
//
//   c++ $(root-config --cflags) scripts/gen_analysis_fixtures.cpp $(root-config --libs) \
//       -o /tmp/gen_analysis && (cd <repo> && /tmp/gen_analysis)

#include <TFile.h>
#include <TH1D.h>
#include <cmath>

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

  // A deterministic gaussian-shaped histogram for fitting (const=1000, mean=0.5,
  // sigma=1.3, rounded to integer bin contents).
  TH1D hg("hg", "gauss", 50, -5, 5);
  for (int i = 1; i <= 50; ++i) {
    double x = hg.GetBinCenter(i);
    double v = 1000.0 * exp(-0.5 * ((x - 0.5) / 1.3) * ((x - 0.5) / 1.3));
    hg.SetBinContent(i, (double)((long)(v + 0.5)));
  }
  hg.Write();

  f.Close();
  return 0;
}
