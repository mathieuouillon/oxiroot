// Dev-only RNTuple fixture generator for a *streamer* field (the kStreamer
// structural role): a class stored unsplit as one serialized blob per entry,
// rather than split into a record of member columns. Forced with
// ROOT::RStreamerField. The class TStreamerInfo is written to the file so the
// blob is self-describing.
//
//   rootcling -f streamer_dict.cxx -I scripts \
//       scripts/rntuple_streamer_types.h scripts/rntuple_streamer_linkdef.h
//   c++ $(root-config --cflags) -I/opt/homebrew/include -Iscripts \
//       scripts/gen_rntuple_streamer.cpp streamer_dict.cxx $(root-config --libs) \
//       -o /tmp/gen_rntuple_streamer
//   /tmp/gen_rntuple_streamer        # run from the repo root
//
// (streamer_dict.cxx and its .pcm are build byproducts — not committed.)

#include <memory>

#include <ROOT/RField.hxx>
#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriter.hxx>

#include "rntuple_streamer_types.h"

int main() {
    auto model = ROOT::RNTupleModel::Create();
    model->AddField(std::make_unique<ROOT::RStreamerField>("blob", "Blob"));
    auto writer =
        ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl", "fixtures/rntuple_streamer.root");

    auto entry = writer->GetModel().CreateBareEntry();
    Blob b;
    auto token = writer->GetModel().GetToken("blob");
    entry->BindRawPtr(token, &b);

    b.id = 7;
    b.value = 3.25;
    b.tag = "hello";
    writer->Fill(*entry);
    b.id = 42;
    b.value = -1.5;
    b.tag = "world!!";
    writer->Fill(*entry);
    b.id = -1;
    b.value = 0.0;
    b.tag = "";
    writer->Fill(*entry);
    return 0;
}
