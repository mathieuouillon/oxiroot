// Dev-only RNTuple fixture generator for *nested* collection fields (ROOT 6.34+).
// uproot can read RNTuple but not write it, so reference files come from ROOT.
//
//   c++ $(root-config --cflags) scripts/gen_rntuple_nested.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_nested
//   /tmp/gen_rntuple_nested        # run from the repo root
//
// Three fields exercise the three nesting shapes, written twice (uncompressed =
// non-split index/leaf columns, Zstd = split index/leaf columns):
//   vs  : std::vector<std::string>                  (collection of strings)
//   vvi : std::vector<std::vector<std::int32_t>>    (collection of collections)
//   vp  : std::vector<std::pair<std::int32_t,double>> (collection of records)
// std::pair serializes as an anonymous record with sub-fields "_0"/"_1", so it
// needs no dictionary.

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fVs = model->MakeField<std::vector<std::string>>("vs");
    auto fVvi = model->MakeField<std::vector<std::vector<std::int32_t>>>("vvi");
    auto fVp = model->MakeField<std::vector<std::pair<std::int32_t, double>>>("vp");

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression); // 0 = none, 505 = Zstd level 5

    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    for (int i = 0; i < 5; ++i) {
        fVs->clear();
        fVvi->clear();
        fVp->clear();
        for (int j = 0; j < i; ++j) {
            fVs->push_back("r" + std::to_string(i) + "_" + std::to_string(j));
            fVvi->push_back(std::vector<std::int32_t>(static_cast<std::size_t>(j + 1), i * 10 + j));
            fVp->emplace_back(i * 100 + j, static_cast<double>(i) + 0.5 * j);
        }
        writer->Fill();
    }
}

int main() {
    write_rntuple("fixtures/rntuple_nested_uncompressed.root", 0);
    write_rntuple("fixtures/rntuple_nested_zstd.root", 505);
    return 0; // each writer's destructor commits its dataset
}
