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
//   - TTree "Tree": ti = int32 [1..5], tf = double [1.5..5.5],
//     tv = double[3] fixed array, ts = string, tj = jagged double,
//     tw = std::vector<double> (TBranchElement), th = split std::vector<Hit>
//     (Hit = {float x; float y; int id;}) read as th.x/th.y/th.id.

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include <TFile.h>
#include <TH1D.h>
#include <TTree.h>
#include <TTreeReader.h>
#include <TTreeReaderArray.h>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleReader.hxx>
#include <ROOT/RNTupleWriter.hxx>

static const double HIST_BINS[4] = {1, 2, 3, 4};
static const std::int32_t NTPL_X[5] = {1, 2, 3, 4, 5};
static const double NTPL_Y[5] = {1.5, 2.5, 3.5, 4.5, 5.5};
static const double TREE_TV[5][3] = {
    {1, 2, 3}, {4, 5, 6}, {7, 8, 9}, {10, 11, 12}, {13, 14, 15}};
static const char *TREE_TS[5] = {"a", "bb", "ccc", "dddd", "eeeee"};
// Jagged column tj = [[1],[2,3],[],[4,5,6],[7]] as per-entry lengths + flattened.
static const int TREE_TJ_LEN[5] = {1, 2, 0, 3, 1};
static const double TREE_TJ_FLAT[7] = {1, 2, 3, 4, 5, 6, 7};
// std::vector column tw = [[10,20],[],[30],[40,50],[60,70,80]].
static const std::vector<std::vector<double>> TREE_TW = {
    {10, 20}, {}, {30}, {40, 50}, {60, 70, 80}};
// Split std::vector<Hit> column th: Hit = {float x; float y; int id;}, read back
// as the per-member sub-branches th.x/th.y/th.id (counts [1,0,2,1,3]).
static const std::vector<std::vector<float>> TREE_TH_X = {
    {1}, {}, {2, 3}, {4}, {5, 6, 7}};
static const std::vector<std::vector<float>> TREE_TH_Y = {
    {1.5}, {}, {2.5, 3.5}, {4.5}, {5.5, 6.5, 7.5}};
static const std::vector<std::vector<int>> TREE_TH_ID = {
    {1}, {}, {2, 3}, {4}, {5, 6, 7}};

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
    // TTree "otree": scalar oi, jagged oj, string os, std::vector<double> ov.
    {
        TFile f(join(dir, "oracle_tree.root").c_str(), "RECREATE");
        TTree t("otree", "otree");
        std::int32_t oi = 0;
        int noj = 0;
        double oj[16] = {0};
        char os[64] = {0};
        std::vector<double> ov;
        t.Branch("oi", &oi, "oi/I");
        t.Branch("noj", &noj, "noj/I");
        t.Branch("oj", oj, "oj[noj]/D");
        t.Branch("os", os, "os/C");
        t.Branch("ov", &ov);
        const int OI[3] = {10, 11, 12};
        const int NOJ[3] = {2, 0, 1};
        const double OJ[3][2] = {{1, 2}, {}, {3}};
        const char *OS[3] = {"x", "yy", "zzz"};
        const std::vector<std::vector<double>> OV = {{1}, {2, 3}, {}};
        for (int i = 0; i < 3; ++i) {
            oi = OI[i];
            noj = NOJ[i];
            for (int j = 0; j < noj; ++j)
                oj[j] = OJ[i][j];
            std::strcpy(os, OS[i]);
            ov = OV[i];
            t.Fill();
        }
        t.Write();
        f.Close();
    }
    std::printf("ROOT C++ wrote oracle_hist.root + oracle_ntuple.root + "
                "oracle_tree.root\n");
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
    // TTree written by Rust: scalar (ti/tf), fixed array (tv[3]), string (ts).
    {
        TFile *f = TFile::Open(join(dir, "rust_tree.root").c_str());
        if (!f || f->IsZombie())
            fail("cannot open rust_tree.root");
        TTree *tree = dynamic_cast<TTree *>(f->Get("Tree"));
        if (!tree)
            fail("rust_tree.root has no TTree 'Tree'");
        std::int32_t ti = 0;
        double tf = 0.0;
        double tv[3] = {0, 0, 0};
        char ts[64] = {0};
        std::int32_t ntj = 0; // jagged count (read before tj each entry)
        double tj[16] = {0};
        std::vector<double> *tw = nullptr; // std::vector branch (TBranchElement)
        tree->SetBranchAddress("ti", &ti);
        tree->SetBranchAddress("tf", &tf);
        tree->SetBranchAddress("tv", tv);
        tree->SetBranchAddress("ts", ts);
        tree->SetBranchAddress("ntj", &ntj);
        tree->SetBranchAddress("tj", tj);
        tree->SetBranchAddress("tw", &tw);
        if (tree->GetEntries() != 5)
            fail("rust tree entry count");
        int off = 0; // running offset into the flattened jagged data
        for (Long64_t i = 0; i < 5; ++i) {
            tree->GetEntry(i);
            if (ti != NTPL_X[i])
                fail("rust tree ti at " + std::to_string(i));
            if (std::fabs(tf - NTPL_Y[i]) > 1e-9)
                fail("rust tree tf at " + std::to_string(i));
            for (int j = 0; j < 3; ++j)
                if (std::fabs(tv[j] - TREE_TV[i][j]) > 1e-9)
                    fail("rust tree tv at " + std::to_string(i));
            if (std::string(ts) != TREE_TS[i])
                fail("rust tree ts at " + std::to_string(i));
            // tj is variable-length: ROOT sizes it from the tj leaf's fLeafCount
            // (the ntj leaf), exercising the object reference the writer emits.
            if (ntj != TREE_TJ_LEN[i])
                fail("rust tree ntj at " + std::to_string(i));
            for (int j = 0; j < ntj; ++j)
                if (std::fabs(tj[j] - TREE_TJ_FLAT[off + j]) > 1e-9)
                    fail("rust tree tj at " + std::to_string(i));
            off += ntj;
            // tw is a std::vector<double> TBranchElement.
            if (tw->size() != TREE_TW[i].size())
                fail("rust tree tw size at " + std::to_string(i));
            for (size_t j = 0; j < tw->size(); ++j)
                if (std::fabs((*tw)[j] - TREE_TW[i][j]) > 1e-9)
                    fail("rust tree tw at " + std::to_string(i));
        }
        f->Close();
    }
    // Split std::vector<Hit> branch `th`: read the per-member sub-branches via
    // TTreeReaderArray. No Hit dictionary is linked — the Rust-written file is
    // self-describing (it embeds Hit's TStreamerInfo), so ROOT builds the class
    // and the leaf-count linkage on its own. This is the strict check that the
    // generated streamer + fBranchCount/fLeafCount references are correct.
    {
        TFile *f = TFile::Open(join(dir, "rust_tree.root").c_str());
        if (!f || f->IsZombie())
            fail("cannot open rust_tree.root (th)");
        TTreeReader reader("Tree", f);
        TTreeReaderArray<float> hx(reader, "th.x");
        TTreeReaderArray<float> hy(reader, "th.y");
        TTreeReaderArray<int> hid(reader, "th.id");
        Long64_t i = 0;
        while (reader.Next()) {
            if (i >= 5)
                fail("rust tree th too many entries");
            if (hx.GetSize() != TREE_TH_X[i].size())
                fail("rust tree th size at " + std::to_string(i));
            for (size_t j = 0; j < hx.GetSize(); ++j) {
                if (std::fabs(hx[j] - TREE_TH_X[i][j]) > 1e-6)
                    fail("rust tree th.x at " + std::to_string(i));
                if (std::fabs(hy[j] - TREE_TH_Y[i][j]) > 1e-6)
                    fail("rust tree th.y at " + std::to_string(i));
                if (hid[j] != TREE_TH_ID[i][j])
                    fail("rust tree th.id at " + std::to_string(i));
            }
            ++i;
        }
        if (i != 5)
            fail("rust tree th entry count");
        f->Close();
    }
    std::printf("ROOT C++ read Rust hist + RNTuple + TTree (incl. split "
                "std::vector<Hit>) — values match\n");
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
