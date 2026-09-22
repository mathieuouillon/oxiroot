//! The oxiroot-rntuple RNTuple integration tests, compiled as one binary so the
//! suite links once. Each module is one topic.

mod assoc;
mod concat;
mod errors;
mod length_checks;
mod malformed;
mod mixed_file;
mod multi_ntuple;
mod optional_fields;
mod prefix_read;
mod read_anchor;
mod read_coltypes;
mod read_fields;
mod read_nested;
mod read_realprec;
mod read_stl;
mod read_streamer;
mod read_user;
mod read_values;
mod read_variant;
mod schema_extension;
mod stream_write;
mod streamer_info;
mod write_coltypes;
mod write_nested;
mod write_rntuple;
mod write_stl;
