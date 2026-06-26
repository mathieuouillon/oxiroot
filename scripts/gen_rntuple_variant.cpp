// Dev-only RNTuple fixture generator for a std::variant field (the Switch
// column / Variant field role). ROOT 6.34+.
//
//   c++ $(root-config --cflags) scripts/gen_rntuple_variant.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_variant
//   /tmp/gen_rntuple_variant        # run from the repo root

#include <cstdint>
#include <variant>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fV = model->MakeField<std::variant<std::int32_t, float>>("v");

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression);
    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    // Alternate the active alternative: int (entries 0,2,4) / float (1,3).
    for (int i = 0; i < 5; ++i) {
        if (i % 2 == 0)
            *fV = static_cast<std::int32_t>(i * 10);
        else
            *fV = static_cast<float>(i) + 0.5f;
        writer->Fill();
    }
}

int main() {
    write_rntuple("fixtures/rntuple_variant_uncompressed.root", 0);
    return 0;
}
