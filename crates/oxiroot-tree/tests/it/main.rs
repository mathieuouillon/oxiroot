//! The oxiroot-tree TTree integration tests, compiled as one binary so the
//! suite links once. Each module is one topic.

mod chain;
mod concat;
mod introspect;
mod malformed;
mod read_alias;
mod read_arrays;
mod read_clones;
mod read_flat;
mod read_friend;
mod read_jagged_flat;
mod read_leaflist;
mod read_mapset;
mod read_multidim;
mod read_nested;
mod read_ntuple;
mod read_object;
mod read_object_old;
mod read_par;
mod read_range;
mod read_schema;
mod read_split;
mod read_subdir;
mod read_vecstring;
mod read_vector;
mod read_vecvec;
mod write_multibasket;
mod write_roundtrip;
mod write_split;
mod write_streaming;
