// Dev-only RNTuple fixture generator for reduced-precision real columns:
// kReal16 (half), kReal32Trunc (truncated mantissa), kReal32Quant (quantized).
//
//   c++ $(root-config --cflags) scripts/gen_rntuple_realprec.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_realprec
//   /tmp/gen_rntuple_realprec        # run from the repo root

#include <vector>

#include <ROOT/RField.hxx>
#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriteOptions.hxx>
#include <ROOT/RNTupleWriter.hxx>

static void write_rntuple(const char *path, int compression) {
    auto model = ROOT::RNTupleModel::Create();
    auto fHalf = model->MakeField<float>("half");
    auto fTrunc = model->MakeField<float>("trunc");
    auto fQuant = model->MakeField<float>("quant");
    auto fQuant12 = model->MakeField<float>("quant12");

    // half: 16-bit IEEE half precision.
    dynamic_cast<ROOT::RField<float> &>(model->GetMutableField("half")).SetHalfPrecision();
    // trunc: keep sign + 8 exponent + 7 mantissa bits = 16 bits total.
    dynamic_cast<ROOT::RField<float> &>(model->GetMutableField("trunc")).SetTruncated(16);
    // quant: linear quantization of [0, 100] into 16-bit integers (byte-aligned).
    dynamic_cast<ROOT::RField<float> &>(model->GetMutableField("quant")).SetQuantized(0.0f, 100.0f, 16);
    // quant12: 12-bit quantization (sub-byte, exercises the bit packing).
    dynamic_cast<ROOT::RField<float> &>(model->GetMutableField("quant12")).SetQuantized(0.0f, 100.0f, 12);

    ROOT::RNTupleWriteOptions options;
    options.SetCompression(compression);
    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", path, options);

    // Powers of two are exact under half + 16-bit truncation; 0/100 are exact
    // quant endpoints (mid values carry a small quantization error).
    const float half_v[] = {0.5f, 1.0f, 2.0f, 4.0f, 8.0f};
    const float quant_v[] = {0.0f, 25.0f, 50.0f, 75.0f, 100.0f};
    for (int i = 0; i < 5; ++i) {
        *fHalf = half_v[i];
        *fTrunc = half_v[i];
        *fQuant = quant_v[i];
        *fQuant12 = quant_v[i];
        writer->Fill();
    }
}

int main() {
    write_rntuple("fixtures/rntuple_realprec_uncompressed.root", 0);
    write_rntuple("fixtures/rntuple_realprec_zstd.root", 505);
    return 0;
}
