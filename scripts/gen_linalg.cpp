// Dev-only: regenerate fixtures/linalg.root.
//
// Writes the linear-algebra objects a fit result carries: a TVectorD "v", a
// dense TMatrixD "m", and a symmetric TMatrixDSym "s" (a covariance shape).
// oxiroot reads this fixture (TMatrixDSym stores only the upper triangle on
// disk), and ROOT C++ and uproot read oxiroot's round-trip of it.
//
//   c++ $(root-config --cflags) scripts/gen_linalg.cpp \
//       $(root-config --libs) -o /tmp/gen_linalg && \
//   (cd <repo-root> && /tmp/gen_linalg)

#include <TFile.h>
#include <TMatrixD.h>
#include <TMatrixDSym.h>
#include <TVectorD.h>

int main() {
    TFile f("fixtures/linalg.root", "RECREATE");
    f.SetCompressionLevel(0); // uncompressed, so the layout is byte-inspectable

    TVectorD v(3);
    v[0] = 1.5;
    v[1] = 2.5;
    v[2] = 3.5;
    v.Write("v");

    TMatrixD m(2, 3);
    m(0, 0) = 1;
    m(0, 1) = 2;
    m(0, 2) = 3;
    m(1, 0) = 4;
    m(1, 1) = 5;
    m(1, 2) = 6;
    m.Write("m");

    TMatrixDSym s(3);
    s(0, 0) = 1;
    s(1, 1) = 2;
    s(2, 2) = 3;
    s(0, 1) = 0.5;
    s(1, 0) = 0.5;
    s.Write("s");

    f.Close();
    return 0;
}
