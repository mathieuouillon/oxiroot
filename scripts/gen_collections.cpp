// Dev-only: regenerate fixtures/collections.root.
//
// Writes a THStack ("hs") of two TH1F and a TMultiGraph ("mg") of two TGraph —
// the collection objects that hold other objects in a TList. oxiroot reads this
// fixture (its members are stored via the object protocol, the second one with a
// class back-reference), and ROOT C++ and uproot read oxiroot's round-trip of it.
//
//   c++ $(root-config --cflags) scripts/gen_collections.cpp \
//       $(root-config --libs) -o /tmp/gen_collections && \
//   (cd <repo-root> && /tmp/gen_collections)

#include <TFile.h>
#include <TGraph.h>
#include <TH1F.h>
#include <THStack.h>
#include <TMultiGraph.h>

int main() {
    TFile f("fixtures/collections.root", "RECREATE");
    f.SetCompressionLevel(0); // uncompressed, so the layout is byte-inspectable

    THStack* hs = new THStack("hs", "my stack");
    TH1F* a = new TH1F("ha", "a", 4, 0, 4);
    a->SetBinContent(1, 1);
    a->SetBinContent(2, 2);
    TH1F* b = new TH1F("hb", "b", 4, 0, 4);
    b->SetBinContent(3, 3);
    b->SetBinContent(4, 4);
    hs->Add(a);
    hs->Add(b);
    hs->Write("hs");

    TMultiGraph* mg = new TMultiGraph("mg", "my multigraph");
    double x1[3] = {0, 1, 2}, y1[3] = {1, 2, 3};
    TGraph* g1 = new TGraph(3, x1, y1);
    g1->SetName("g1");
    double x2[3] = {0, 1, 2}, y2[3] = {3, 2, 1};
    TGraph* g2 = new TGraph(3, x2, y2);
    g2->SetName("g2");
    mg->Add(g1);
    mg->Add(g2);
    mg->Write("mg");

    f.Close();
    return 0;
}
