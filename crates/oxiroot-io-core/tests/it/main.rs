//! The oxiroot-io-core container and object integration tests, compiled as one binary so the
//! suite links once. Each module is one topic.

mod collection_streamers;
mod container;
mod cycles;
mod decompress_error;
mod errors;
mod generic_read;
mod malformed;
mod mmap;
mod proptest_buffer;
mod ranged;
mod read_fixture;
mod read_streamers;
