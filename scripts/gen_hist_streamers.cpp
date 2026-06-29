// Dev-only: regenerate crates/oxiroot-hist/src/histograms.streamerinfo.bin.
//
// Writes one object of every histogram class oxiroot can write into a single
// ROOT file, so ROOT accumulates a TStreamerInfo for each (plus all their
// bases). The combined TList<TStreamerInfo> is then extracted by oxiroot
// (RFile::streamer_info_object) and baked into the crate. Adding a new
// persistable type means adding one object here and re-running.
//
//   c++ $(root-config --cflags) scripts/gen_hist_streamers.cpp \
//       $(root-config --libs) -o /tmp/gen_hist_streamers
//   /tmp/gen_hist_streamers           # writes /tmp/alltypes.root
//   # then: cargo run --example bake_streamers (see oxiroot-hist)
#include <TFile.h>
#include <TH1.h>
#include <TH2.h>
#include <TH3.h>
#include <TProfile.h>
#include <TProfile2D.h>
#include <TProfile3D.h>
#include <TEfficiency.h>
#include <THnSparse.h>
#include <TH2Poly.h>
#include <TGraph.h>
#include <TGraphErrors.h>
#include <TGraphAsymmErrors.h>
#include <TGraph2D.h>
#include <TGraphMultiErrors.h>

int main() {
  TFile f("/tmp/alltypes.root", "RECREATE");
  f.SetCompressionLevel(0);

  TH1C h1c("h1c","",2,0,2); h1c.Write();
  TH1S h1s("h1s","",2,0,2); h1s.Write();
  TH1I h1i("h1i","",2,0,2); h1i.Write();
  TH1L h1l("h1l","",2,0,2); h1l.Write();
  TH1F h1f("h1f","",2,0,2); h1f.Write();
  TH1D h1d("h1d","",2,0,2); h1d.Write();
  TH2C h2c("h2c","",2,0,2,2,0,2); h2c.Write();
  TH2S h2s("h2s","",2,0,2,2,0,2); h2s.Write();
  TH2I h2i("h2i","",2,0,2,2,0,2); h2i.Write();
  TH2L h2l("h2l","",2,0,2,2,0,2); h2l.Write();
  TH2F h2f("h2f","",2,0,2,2,0,2); h2f.Write();
  TH2D h2d("h2d","",2,0,2,2,0,2); h2d.Write();
  TH3C h3c("h3c","",2,0,2,2,0,2,2,0,2); h3c.Write();
  TH3S h3s("h3s","",2,0,2,2,0,2,2,0,2); h3s.Write();
  TH3I h3i("h3i","",2,0,2,2,0,2,2,0,2); h3i.Write();
  TH3L h3l("h3l","",2,0,2,2,0,2,2,0,2); h3l.Write();
  TH3F h3f("h3f","",2,0,2,2,0,2,2,0,2); h3f.Write();
  TH3D h3d("h3d","",2,0,2,2,0,2,2,0,2); h3d.Write();
  TProfile p("p","",2,0,2); p.Write();
  TProfile2D p2("p2","",2,0,2,2,0,2); p2.Write();
  TProfile3D p3("p3","",2,0,2,2,0,2,2,0,2); p3.Write();
  TEfficiency e("e","",2,0,2); e.Write();
  Int_t nb[2] = {2,2}; Double_t lo[2] = {0,0}, hi[2] = {2,2};
  THnSparseD hs("hs","",2,nb,lo,hi); hs.Write();
  TH2Poly hp("hp","",0,2,0,2); hp.AddBin(0,0,1,1); hp.AddBin(1,1,2,2); hp.Write();
  Double_t gx[2] = {0,1}, gy[2] = {0,1}, ge[2] = {0,0};
  TGraph gr(2,gx,gy); gr.SetName("gr"); gr.Write();
  TGraphErrors gre(2,gx,gy,ge,ge); gre.SetName("gre"); gre.Write();
  TGraphAsymmErrors grae(2,gx,gy,ge,ge,ge,ge); grae.SetName("grae"); grae.Write();
  Double_t gz[2] = {0,1};
  TGraph2D gr2(2,gx,gy,gz); gr2.SetName("gr2"); gr2.SetTitle(""); gr2.Write();
  TGraphMultiErrors gme("gme","",2,gx,gy,ge,ge,ge,ge);
  gme.AddYError(2,ge,ge); // a second y-error layer
  gme.Write();

  f.Close();
  return 0;
}
