// Class for the RNTuple streamer-field fixture (gen_rntuple_streamer.cpp): a
// plain struct stored *unsplit* (one serialized blob per entry) via
// ROOT::RStreamerField.
#ifndef OXIROOT_RNTUPLE_STREAMER_TYPES_H
#define OXIROOT_RNTUPLE_STREAMER_TYPES_H

#include <cstdint>
#include <string>

struct Blob {
    std::int32_t id;
    double value;
    std::string tag;
};

#endif
