// Writes fixtures/stl_members.root: ROOT objects whose members are STL
// containers, one of every shape ROOT streams them in.
//
//   TF1                 fParams, a map<TString,int> streamed objectwise, and an
//                       empty vector<TObject*>
//   TEfficiency         fBeta_bin_params, a vector<pair<double,double>> streamed
//                       memberwise (a column of firsts, then of seconds)
//   TGraphMultiErrors   fEyL/fEyH, vectors of TArrayD streamed objectwise, and
//                       fAttFill/fAttLine, vectors of a class streamed memberwise
//   TH2Poly             fCells, a TStreamerLoop of TLists holding the bins, which
//                       fBins then points back at
//
// The values are the ones crates/oxiroot-io-core/tests/it/generic_read.rs asserts
// on. Build and run from the repository root:
//
//   c++ $(root-config --cflags) scripts/gen_stl_members.cpp $(root-config --libs) -o /tmp/gen_stl
//   /tmp/gen_stl

#include <TEfficiency.h>
#include <TF1.h>
#include <TFile.h>
#include <TGraphMultiErrors.h>
#include <TH2Poly.h>

int main() {
   // Uncompressed, so the members can be read straight out of the file.
   TFile f("fixtures/stl_members.root", "RECREATE", "", 0);

   TF1 fn("fn", "[0]+[1]*x*x", 0, 10);
   fn.SetParameters(1.5, 2.5);
   fn.Write();

   TEfficiency eff("eff", "eff;x;#epsilon", 3, 0, 3);
   eff.Fill(true, 0.5);
   eff.Fill(false, 1.5);
   eff.SetBetaBinParameters(1, 2.0, 3.0);
   eff.SetBetaBinParameters(2, 4.0, 5.0);
   eff.Write();

   Double_t x[3] = {1, 2, 3}, y[3] = {4, 5, 6};
   Double_t exl[3] = {0.1, 0.1, 0.1}, exh[3] = {0.2, 0.2, 0.2};
   Double_t eyl1[3] = {0.3, 0.3, 0.3}, eyh1[3] = {0.4, 0.4, 0.4};
   Double_t eyl2[3] = {0.5, 0.5, 0.5}, eyh2[3] = {0.6, 0.6, 0.6};
   TGraphMultiErrors gme("gme", "gme", 3, x, y, exl, exh, eyl1, eyh1);
   gme.AddYError(3, eyl2, eyh2);
   gme.SetFillColor(1, kRed);  // the second error bar's attributes
   gme.SetLineColor(1, kBlue);
   gme.SetLineWidth(1, 3);
   gme.Write();

   TH2Poly poly("poly", "poly", 0, 2, 0, 2);
   poly.AddBin(0, 0, 1, 1);
   poly.AddBin(1, 1, 2, 2);
   poly.Fill(0.5, 0.5);
   poly.Fill(1.5, 1.5, 2.0);
   poly.Write();

   f.Close();
   return 0;
}
