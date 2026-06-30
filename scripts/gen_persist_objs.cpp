// Dev-only: regenerate fixtures/persist_objs.root.
//
// Writes the two small persistable objects ROOT constantly stores alongside
// histograms: a TObjString ("label") and three TParameter<T> instantiations
// (double/int/Long64_t). oxiroot reads this fixture (its byte layout must match
// ROOT's), and ROOT C++ reads oxiroot's round-trip of these objects.
//
//   c++ $(root-config --cflags) scripts/gen_persist_objs.cpp \
//       $(root-config --libs) -o /tmp/gen_persist_objs && \
//   (cd <repo-root> && /tmp/gen_persist_objs)

#include <TFile.h>
#include <TObjString.h>
#include <TParameter.h>

int main() {
    TFile f("fixtures/persist_objs.root", "RECREATE");
    f.SetCompressionLevel(0); // uncompressed, so the layout is byte-inspectable

    TObjString s("hello world");
    s.Write("label");

    TParameter<double> pd("lumi", 137.5);
    pd.Write("lumi");

    TParameter<int> pi("nevents", 42);
    pi.Write("nevents");

    TParameter<long long> pl("bignum", 9000000000LL);
    pl.Write("bignum");

    f.Close();
    return 0;
}
