// Dev-only: regenerate fixtures/objlist.root.
//
// Writes a bare TList ("mylist") holding a TH1F, a TObjString, and a
// TParameter<double>, plus a TObjArray ("myarr") holding two TH1F — the
// collection-of-objects-as-a-key shape. oxiroot reads this fixture (members are
// stored via the object protocol, repeats using class back-references), and
// ROOT C++ and uproot read oxiroot's round-trip of it.
//
//   c++ $(root-config --cflags) scripts/gen_objlist.cpp \
//       $(root-config --libs) -o /tmp/gen_objlist && \
//   (cd <repo-root> && /tmp/gen_objlist)

#include <TFile.h>
#include <TH1F.h>
#include <TList.h>
#include <TObjArray.h>
#include <TObjString.h>
#include <TParameter.h>

int main() {
    TFile f("fixtures/objlist.root", "RECREATE");
    f.SetCompressionLevel(0); // uncompressed, so the layout is byte-inspectable

    TList* l = new TList();
    l->SetName("mylist");
    TH1F* h = new TH1F("h", "h", 4, 0, 4);
    h->SetBinContent(1, 5);
    l->Add(h);
    l->Add(new TObjString("hello"));
    l->Add(new TParameter<double>("lumi", 12.5));
    f.WriteObject(l, "mylist");

    TObjArray* a = new TObjArray();
    a->SetName("myarr");
    TH1F* h1 = new TH1F("a0", "a0", 3, 0, 3);
    h1->SetBinContent(1, 1);
    TH1F* h2 = new TH1F("a1", "a1", 3, 0, 3);
    h2->SetBinContent(2, 2);
    a->Add(h1);
    a->Add(h2);
    f.WriteObject(a, "myarr");

    f.Close();
    return 0;
}
