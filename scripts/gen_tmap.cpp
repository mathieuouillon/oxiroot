// Dev-only: regenerate fixtures/tmap.root.
//
// Writes a TMap ("meta") of string keys -> mixed objects (a TObjString, a
// TParameter<double>, a TH1F), the way ROOT keeps string-keyed metadata.
// oxiroot reads this fixture (pairs stored via the object protocol, repeats
// using class back-references) and ROOT C++ reads oxiroot's round-trip of it.
// Note: uproot has no TMap model, so neither this fixture nor oxiroot's output
// is readable there — a uproot limitation.
//
//   c++ $(root-config --cflags) scripts/gen_tmap.cpp \
//       $(root-config --libs) -o /tmp/gen_tmap && \
//   (cd <repo-root> && /tmp/gen_tmap)

#include <TFile.h>
#include <TH1F.h>
#include <TMap.h>
#include <TObjString.h>
#include <TParameter.h>

int main() {
    TFile f("fixtures/tmap.root", "RECREATE");
    f.SetCompressionLevel(0); // uncompressed, so the layout is byte-inspectable

    TMap* m = new TMap();
    m->SetName("meta");
    m->Add(new TObjString("version"), new TObjString("2.1"));
    m->Add(new TObjString("lumi"), new TParameter<double>("lumi", 137.5));
    TH1F* h = new TH1F("h", "h", 4, 0, 4);
    h->SetBinContent(1, 5);
    m->Add(new TObjString("hist"), h);
    f.WriteObject(m, "meta");

    f.Close();
    return 0;
}
