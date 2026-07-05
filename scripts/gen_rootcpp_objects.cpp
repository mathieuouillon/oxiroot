// Regenerate the ROOT-C++-written interop fixture for the generic object reader.
// A foreign file (written by official ROOT, not oxiroot) proving the generic
// reader decodes arbitrary ROOT files. Build + run with a local ROOT install:
//   c++ $(root-config --cflags) scripts/gen_rootcpp_objects.cpp $(root-config --libs) -o /tmp/gen \
//     && cd fixtures && /tmp/gen   # writes rootcpp_objects.root
// (writes to /tmp/rootcpp_gen.root; copy to fixtures/rootcpp_objects.root)

#include "TFile.h"
#include "TRandom.h"
#include "TH1D.h"
#include "TObjString.h"
#include "TNamed.h"
#include "TList.h"
#include "TParameter.h"
int main() {
    TFile f("/tmp/rootcpp_gen.root", "RECREATE");
    TH1D h("hpx", "px distribution", 10, -3, 3);
    for (int i=0;i<1000;i++) h.Fill(gRandom->Gaus());
    h.Write();
    TObjString s("written by ROOT 6.40");
    s.Write("note");
    TParameter<double> p("thr", 2.5); p.Write();
    TList lst; lst.SetName("mylist");
    lst.Add(new TNamed("a","alpha")); lst.Add(new TObjString("beta"));
    lst.Write("mylist", TObject::kSingleKey);
    f.Close();
    return 0;
}
