// Dev-only fixture generator for histogram types uproot cannot write
// (TH1F/TH2F/TH3*/TProfile all become TH1D/... via uproot). Requires ROOT.
//
//   c++ $(root-config --cflags) scripts/gen_root_fixtures.cpp \
//       $(root-config --libs) -o /tmp/gen_root_fixtures
//   /tmp/gen_root_fixtures          # run from the repo root
//
// Writes uncompressed .root files into fixtures/. Golden JSON is produced
// separately by scripts/gen_fixtures.py (uproot). Content is deterministic.

#include "TFile.h"
#include "TH1.h" // TH1C/TH1S/TH1I/TH1L/TH1F
#include "TH2F.h"
#include "TH3D.h"
#include "TH3F.h"
#include "TProfile.h"

template <typename F>
static void write(const char *path, const char *name, F fill) {
    TFile f(path, "RECREATE");
    f.SetCompressionLevel(0); // uncompressed: no codec needed to read it back
    fill();
    f.Close();
}

int main() {
    write("fixtures/th1f_uncompressed.root", "h1f", [] {
        TH1F h("h1f", "", 5, 0, 5);
        const double v[5] = {1.5, 3.0, 4.5, 6.0, 7.5};
        for (int i = 0; i < 5; ++i) h.SetBinContent(i + 1, v[i]);
        h.Write();
    });

    write("fixtures/th2f_uncompressed.root", "h2f", [] {
        TH2F h("h2f", "", 3, 0, 3, 2, 0, 2);
        for (int ix = 1; ix <= 3; ++ix)
            for (int iy = 1; iy <= 2; ++iy) h.SetBinContent(ix, iy, (ix - 1) * 2 + iy);
        h.Write();
    });

    write("fixtures/th3d_uncompressed.root", "h3", [] {
        TH3D h("h3", "", 2, 0, 2, 2, 0, 2, 2, 0, 2);
        int n = 1;
        for (int iz = 1; iz <= 2; ++iz)
            for (int iy = 1; iy <= 2; ++iy)
                for (int ix = 1; ix <= 2; ++ix) h.SetBinContent(ix, iy, iz, n++);
        h.Write();
    });

    write("fixtures/th3f_uncompressed.root", "h3", [] {
        TH3F h("h3", "", 2, 0, 2, 2, 0, 2, 2, 0, 2);
        int n = 1;
        for (int iz = 1; iz <= 2; ++iz)
            for (int iy = 1; iy <= 2; ++iy)
                for (int ix = 1; ix <= 2; ++ix) h.SetBinContent(ix, iy, iz, n++);
        h.Write();
    });

    write("fixtures/th1c_uncompressed.root", "h1c", [] {
        TH1C h("h1c", "", 5, 0, 5);
        for (int i = 1; i <= 5; ++i) h.SetBinContent(i, i); // 1..5 (Char_t)
        h.Write();
    });

    write("fixtures/th1s_uncompressed.root", "h1s", [] {
        TH1S h("h1s", "", 5, 0, 5);
        for (int i = 1; i <= 5; ++i) h.SetBinContent(i, i * 100); // Short_t
        h.Write();
    });

    write("fixtures/th1i_uncompressed.root", "h1i", [] {
        TH1I h("h1i", "", 5, 0, 5);
        for (int i = 1; i <= 5; ++i) h.SetBinContent(i, i * 100000); // Int_t
        h.Write();
    });

    write("fixtures/th1l_uncompressed.root", "h1l", [] {
        TH1L h("h1l", "", 5, 0, 5);
        for (int i = 1; i <= 5; ++i)
            h.SetBinContent(i, static_cast<double>(1099511627776LL * i)); // i * 2^40 (Long64_t)
        h.Write();
    });

    write("fixtures/tprofile_uncompressed.root", "p", [] {
        TProfile p("p", "", 4, 0, 4);
        // Deterministic (x, y) fills: bin means become 15, 7.5, 30, (empty).
        p.Fill(0.5, 10);
        p.Fill(0.5, 20);
        p.Fill(1.5, 5);
        p.Fill(1.5, 10);
        p.Fill(2.5, 30);
        p.Write();
    });

    return 0;
}
