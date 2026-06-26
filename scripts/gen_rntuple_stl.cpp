// Dev-only RNTuple fixture generator for fixed-size STL fields that need no
// dictionary: std::array and std::bitset (ROOT 6.34+). std::map/std::set need a
// collection-proxy dictionary and are generated separately.
//
//   c++ $(root-config --cflags) scripts/gen_rntuple_stl.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_stl
//   /tmp/gen_rntuple_stl        # run from the repo root

#include <array>
#include <bitset>
#include <cstdint>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fArr = model->MakeField<std::array<std::int32_t, 3>>("arr");
    auto fBits = model->MakeField<std::bitset<8>>("bits");

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression);
    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    for (int i = 0; i < 4; ++i) {
        *fArr = {i, i * 10, i * 100};
        *fBits = std::bitset<8>(static_cast<unsigned long>(i * 5)); // 0, 5, 10, 15
        writer->Fill();
    }
}

int main() {
    write_rntuple("fixtures/rntuple_stl_uncompressed.root", 0);
    write_rntuple("fixtures/rntuple_stl_zstd.root", 505);
    return 0;
}
