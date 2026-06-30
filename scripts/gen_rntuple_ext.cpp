// Dev-only fixture generator for fixtures/rntuple_ext.root: a schema-extended
// RNTuple. A field "y" is added mid-writing via the model updater, so it lands
// in the footer's schema-extension record (not the header), and the entries
// written before the update have no "y" data (ROOT defaults them to 0). oxiroot
// merges the extension fields/columns into the schema and back-fills the
// deferred column's leading entries. Needs ROOT 6.x with RNTuple.
//
//   c++ $(root-config --cflags) -I/opt/homebrew/include scripts/gen_rntuple_ext.cpp \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_ext
//   /tmp/gen_rntuple_ext        # writes fixtures/rntuple_ext.root (run from repo root)
#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriter.hxx>

using ROOT::RNTupleModel;
using ROOT::RNTupleWriter;

int main() {
    auto model = RNTupleModel::Create();
    auto fx = model->MakeField<int>("x");
    auto writer = RNTupleWriter::Recreate(std::move(model), "ntpl", "fixtures/rntuple_ext.root");
    *fx = 1;
    writer->Fill();
    *fx = 2;
    writer->Fill();

    // Late-extend the schema: add "y" after the first entries are written.
    auto updater = writer->CreateModelUpdater();
    updater->BeginUpdate();
    auto fy = updater->MakeField<float>("y");
    updater->CommitUpdate();

    *fx = 3;
    *fy = 3.5f;
    writer->Fill();
    *fx = 4;
    *fy = 4.5f;
    writer->Fill();
    return 0;
}
