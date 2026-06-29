// Dev-only RNTuple fixture generator for an associative container ROOT can
// write: std::set<int32_t>. (std::map crashes ROOT 6.40's collection proxy on
// both write and read, so a map fixture comes from oxiroot's own writer, checked
// against uproot.)
//
//   c++ $(root-config --cflags) -I/opt/homebrew/include scripts/gen_rntuple_assoc.cpp \
//       $(root-config --libs) -o /tmp/gen_rntuple_assoc
//   /tmp/gen_rntuple_assoc        # run from the repo root
//
// (The extra include path is needed because ROOT's RVec.hxx pulls in vdt, a
// separate Homebrew formula not on ROOT's default include path.)

#include <cstdint>
#include <set>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fSet = model->MakeField<std::set<std::int32_t>>("s");

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression);
    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    *fSet = {1, 2, 3};
    writer->Fill();
    *fSet = {4, 5};
    writer->Fill();
    *fSet = {}; // an empty set
    writer->Fill();
    *fSet = {10, 20, 30, 40};
    writer->Fill();
}

int main() {
    write_rntuple("fixtures/rntuple_set_uncompressed.root", 0);
    write_rntuple("fixtures/rntuple_set_zstd.root", 505);
    return 0;
}
