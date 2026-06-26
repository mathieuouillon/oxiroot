//! Dev-only: extract a ROOT file's combined `TStreamerInfo` blob and write it
//! out, for baking into the crate as `src/histograms.streamerinfo.bin`.
//!
//! Regenerate the histogram streamer blob (e.g. after adding a new persistable
//! type) with:
//!
//! ```text
//! c++ $(root-config --cflags) scripts/gen_hist_streamers.cpp \
//!     $(root-config --libs) -o /tmp/gen_hist_streamers && /tmp/gen_hist_streamers
//! cargo run -p oxiroot-hist --example bake_streamers -- \
//!     /tmp/alltypes.root crates/oxiroot-hist/src/histograms.streamerinfo.bin
//! ```
use oxiroot_io_core::RFile;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, dst) = (&args[1], &args[2]);
    let f = RFile::open(src).expect("open source ROOT file");
    let blob = f
        .streamer_info_object()
        .expect("read streamer info")
        .expect("source file has streamer info");
    std::fs::write(dst, &blob).expect("write blob");
    println!("wrote {} bytes to {dst}", blob.len());
}
