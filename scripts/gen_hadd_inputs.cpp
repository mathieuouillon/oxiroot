// Dev-only: regenerate fixtures/hadd_a.root, fixtures/hadd_b.root and
// fixtures/hadd_merged.root.
//
// Writes two files holding one object of every class oxiroot's `hadd` handles,
// with different contents, so ROOT's own `hadd` shows what it merges and what it
// leaves alone. `fixtures/hadd_merged.root` is ROOT's answer, which
// `crates/oxiroot/tests/it/hadd_root_parity.rs` checks oxiroot reproduces.
//
// TGraphMultiErrors is left out: ROOT 6.40's hadd segfaults merging one
// (TGraphMultiErrors::CopyPoints), so there is no answer to compare against.
//
//   c++ $(root-config --cflags) scripts/gen_hadd_inputs.cpp \
//       $(root-config --libs) -o /tmp/gen_hadd_inputs
//   /tmp/gen_hadd_inputs fixtures
//   hadd -f fixtures/hadd_merged.root fixtures/hadd_a.root fixtures/hadd_b.root
#include <TEfficiency.h>
#include <TF1.h>
#include <TFile.h>
#include <TGraph.h>
#include <TGraph2D.h>
#include <TGraphAsymmErrors.h>
#include <TGraphErrors.h>
#include <TH1.h>
#include <TH2Poly.h>
#include <THStack.h>
#include <THnSparse.h>
#include <TMultiGraph.h>
#include <TObjString.h>
#include <TParameter.h>
#include <TProfile.h>

#include <string>

// `shift` separates the two files' contents; `npoints` their graphs' lengths.
static void fill(const std::string &path, double shift, int npoints) {
  TFile f(path.c_str(), "RECREATE");
  f.SetCompressionLevel(0);

  TH1D h("h", "h", 4, 0, 4);
  h.Fill(1.5, 1 + shift);
  h.Write();

  TProfile p("p", "p", 4, 0, 4);
  p.Fill(1.5, 2 + shift);
  p.Write();

  TEfficiency eff("eff", "eff", 4, 0, 4);
  eff.Fill(true, 0.5);
  eff.Fill(false, 1.5);
  if (shift > 0) eff.Fill(true, 2.5);
  eff.Write();

  TH2Poly poly;
  poly.SetName("poly");
  poly.AddBin(0, 0, 1, 1);
  poly.AddBin(1, 1, 2, 2);
  poly.Fill(0.5, 0.5, 1 + shift);
  poly.Fill(1.5, 1.5, 2);
  poly.Write();

  Int_t bins[2] = {4, 2};
  Double_t lo[2] = {0, 0}, hi[2] = {4, 2};
  THnSparseD sp("sp", "sp", 2, bins, lo, hi);
  Double_t x1[2] = {1.5, 0.5}, x2[2] = {2.5, 1.5};
  sp.Fill(x1, 1 + shift);
  sp.Fill(x2, 3);
  sp.Write();

  TGraph g(npoints);
  for (int i = 0; i < npoints; ++i) g.SetPoint(i, i + shift, 10 * (i + shift));
  g.SetName("g");
  g.Write();

  TGraphErrors ge(npoints);
  for (int i = 0; i < npoints; ++i) {
    ge.SetPoint(i, i + shift, 10 * (i + shift));
    ge.SetPointError(i, 0.1, 0.5);
  }
  ge.SetName("ge");
  ge.Write();

  TGraphAsymmErrors ga(npoints);
  for (int i = 0; i < npoints; ++i) {
    ga.SetPoint(i, i + shift, 10 * (i + shift));
    ga.SetPointError(i, 0.1, 0.2, 0.3, 0.4);
  }
  ga.SetName("ga");
  ga.Write();

  TGraph2D g2(npoints);
  for (int i = 0; i < npoints; ++i) g2.SetPoint(i, i + shift, i, 10 * (i + shift));
  g2.SetName("g2");
  g2.Write();

  THStack st("st", "st");
  TH1D *sh = new TH1D("sh", "sh", 4, 0, 4);
  sh->Fill(2.5, 1 + shift);
  st.Add(sh);
  st.Write();

  TMultiGraph mg("mg", "mg");
  TGraph *mgg = new TGraph(npoints);
  for (int i = 0; i < npoints; ++i) mgg->SetPoint(i, i + shift, i);
  mgg->SetName("mgg");
  mg.Add(mgg);
  mg.Write();

  TF1 fn("fn", "[0]*x", 0, 1);
  fn.SetParameter(0, 1 + shift);
  fn.Write();

  TObjString s(shift > 0 ? "second" : "first");
  s.Write("s");

  TParameter<double> par("par", 1 + shift);
  par.Write();

  f.Close();
}

int main(int argc, char **argv) {
  const std::string dir = argc > 1 ? argv[1] : "fixtures";
  fill(dir + "/hadd_a.root", 0, 2);
  fill(dir + "/hadd_b.root", 1, 3);
  return 0;
}
