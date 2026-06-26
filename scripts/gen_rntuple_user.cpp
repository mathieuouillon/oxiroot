// Dev-only RNTuple fixture generator for a user-defined class (ROOT 6.34+). A
// class with a dictionary is *split* by RNTuple into a record of named
// sub-fields, so it needs a rootcling dictionary:
//
//   cd scripts
//   rootcling -f user_dict.cxx -I. rntuple_user_types.h rntuple_user_linkdef.h
//   c++ $(root-config --cflags) -I. gen_rntuple_user.cpp user_dict.cxx \
//       $(root-config --libs) -lROOTNTuple -o /tmp/gen_rntuple_user
//   cd .. && /tmp/gen_rntuple_user        # run from the repo root

#include <cstdint>
#include <vector>

#include "rntuple_user_types.h"

#include <ROOT/RNTupleModel.hxx>
#include <ROOT/RNTupleWriter.hxx>

int main() {
    auto model = ROOT::RNTupleModel::Create();
    auto fHit = model->MakeField<Hit>("hit");
    auto fVHit = model->MakeField<std::vector<Hit>>("vhit");

    auto writer = ROOT::RNTupleWriter::Recreate(std::move(model), "ntpl",
                                                "fixtures/rntuple_user_uncompressed.root");
    for (int i = 0; i < 3; ++i) {
        *fHit = Hit{i, static_cast<double>(i) + 0.5};
        fVHit->clear();
        for (int j = 0; j <= i; ++j) {
            fVHit->push_back(Hit{j * 2, static_cast<double>(j)});
        }
        writer->Fill();
    }
    return 0;
}
