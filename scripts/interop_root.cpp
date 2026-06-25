// ROOT C++ side of the round-trip interop check (requires ROOT 6.34+).
//
//   c++ $(root-config --cflags) scripts/interop_root.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/interop_root
//   /tmp/interop_root write <dir>   # write oracle files for Rust to read
//   /tmp/interop_root read  <dir>   # read Rust-written files, assert
//
// Canonical dataset (must match crates/oxiroot/examples/interop.rs):
//   - TH1D "h": 4 bins over [0, 4), in-range bin contents [1, 2, 3, 4].
//   - RNTuple "ntpl": x = int32 [1..5], y = double [1.5..5.5].

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

#include <TFile.h>
#include <TH1D.h>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleReader.hxx>
#include <ROOT/RNTupleWriter.hxx>

static const double HIST_BINS[4] = {1, 2, 3, 4};
static const std::int32_t NTPL_X[5] = {1, 2, 3, 4, 5};
static const double NTPL_Y[5] = {1.5, 2.5, 3.5, 4.5, 5.5};

static void fail(const std::string &msg) {
    std::fprintf(stderr, "interop MISMATCH: %s\n", msg.c_str());
    std::exit(1);
}

static std::string join(const char *dir, const char *file) {
    return std::string(dir) + "/" + file;
}

static void write_oracle(const char *dir) {
    // TH1D with the canonical in-range bin contents.
    {
        TFile f(join(dir, "oracle_hist.root").c_str(), "RECREATE");
        TH1D h("h", "interop", 4, 0.0, 4.0);
        for (int bin = 1; bin <= 4; ++bin)
            h.SetBinContent(bin, HIST_BINS[bin - 1]);
        h.SetEntries(10);
        h.Write();
        f.Close();
    }
    // RNTuple "ntpl" with x:int32, y:double.
    {
        auto model = ROOT::RNTupleModel::Create();
        auto x = model->MakeField<std::int32_t>("x");
        auto y = model->MakeField<double>("y");
        auto writer = ROOT::RNTupleWriter::Recreate(
            std::move(model), "ntpl", join(dir, "oracle_ntuple.root").c_str());
        for (int i = 0; i < 5; ++i) {
            *x = NTPL_X[i];
            *y = NTPL_Y[i];
            writer->Fill();
        }
    }
    std::printf("ROOT C++ wrote oracle_hist.root + oracle_ntuple.root\n");
}

static void read_rust(const char *dir) {
    // Histogram written by Rust.
    {
        TFile *f = TFile::Open(join(dir, "rust_hist.root").c_str());
        if (!f || f->IsZombie())
            fail("cannot open rust_hist.root");
        TH1D *h = dynamic_cast<TH1D *>(f->Get("h"));
        if (!h)
            fail("rust_hist.root has no TH1D 'h'");
        for (int bin = 1; bin <= 4; ++bin) {
            double got = h->GetBinContent(bin);
            if (std::fabs(got - HIST_BINS[bin - 1]) > 1e-9)
                fail("rust hist bin " + std::to_string(bin));
        }
        f->Close();
    }
    // RNTuple written by Rust.
    {
        auto reader =
            ROOT::RNTupleReader::Open("ntpl", join(dir, "rust_ntuple.root").c_str());
        if (reader->GetNEntries() != 5)
            fail("rust ntuple entry count");
        auto vx = reader->GetView<std::int32_t>("x");
        auto vy = reader->GetView<double>("y");
        for (std::uint64_t i = 0; i < 5; ++i) {
            if (vx(i) != NTPL_X[i])
                fail("rust ntuple x at " + std::to_string(i));
            if (std::fabs(vy(i) - NTPL_Y[i]) > 1e-9)
                fail("rust ntuple y at " + std::to_string(i));
        }
    }
    std::printf("ROOT C++ read Rust hist + RNTuple — values match\n");
}

int main(int argc, char **argv) {
    if (argc != 3) {
        std::fprintf(stderr, "usage: interop_root <write|read> <dir>\n");
        return 2;
    }
    std::string mode = argv[1];
    if (mode == "write")
        write_oracle(argv[2]);
    else if (mode == "read")
        read_rust(argv[2]);
    else {
        std::fprintf(stderr, "unknown mode: %s\n", mode.c_str());
        return 2;
    }
    return 0;
}
