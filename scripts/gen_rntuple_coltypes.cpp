// Dev-only RNTuple fixture generator for the remaining physical column types:
// the small/medium integers and reduced-precision reals (ROOT 6.34+).
//
//   c++ $(root-config --cflags) scripts/gen_rntuple_coltypes.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_coltypes
//   /tmp/gen_rntuple_coltypes        # run from the repo root
//
// Written twice: uncompressed (non-split column types) and Zstd (split types).

#include <cstdint>
#include <vector>

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fI8 = model->MakeField<std::int8_t>("i8");
    auto fU8 = model->MakeField<std::uint8_t>("u8");
    auto fI16 = model->MakeField<std::int16_t>("i16");
    auto fU16 = model->MakeField<std::uint16_t>("u16");
    auto fF16 = model->MakeField<Float16_t>("f16");   // reduced-precision float
    auto fD32 = model->MakeField<Double32_t>("d32");  // reduced-precision double
    auto fVI16 = model->MakeField<std::vector<std::int16_t>>("vi16");

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression);

    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    for (int i = 0; i < 5; ++i) {
        *fI8 = static_cast<std::int8_t>(i - 2);          // -2..2
        *fU8 = static_cast<std::uint8_t>(i + 250);       // 250..254 (> int8 range)
        *fI16 = static_cast<std::int16_t>(i * 1000 - 2000);
        *fU16 = static_cast<std::uint16_t>(i * 10000 + 5);  // up to 40005 (> int16)
        *fF16 = static_cast<float>(i) + 0.25f;
        *fD32 = static_cast<double>(i) * 1.5;
        fVI16->assign(static_cast<std::size_t>(i), static_cast<std::int16_t>(100 + i));
        writer->Fill();
    }
}

int main() {
    write_rntuple("fixtures/rntuple_coltypes_uncompressed.root", 0);
    write_rntuple("fixtures/rntuple_coltypes_zstd.root", 505);
    return 0;
}
