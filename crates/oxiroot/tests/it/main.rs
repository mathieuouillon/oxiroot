//! The oxiroot end-to-end integration tests, compiled as one binary so the
//! suite links once. Each module is one topic.

mod append_rntuple;
mod end_to_end;
mod hadd;
mod hadd_histograms;
mod mixed_file;
mod plot;
mod remote_http;
mod remote_xrootd;
