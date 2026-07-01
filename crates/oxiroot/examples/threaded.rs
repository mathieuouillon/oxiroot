//! Multithreaded histogram fill — oxiroot's `ThreadedHist`, the analog of ROOT's
//! `TThreadedObject<TH1D>`. Run with:
//!
//! ```sh
//! cargo run -p oxiroot --example threaded
//! ```
//!
//! Each thread just calls `hist.fill(x)` and transparently gets its own private
//! copy of the histogram (created on first use); `merge()` combines them at the
//! end into a result identical to a single-threaded fill. The ROOT C++ this
//! mirrors:
//!
//! ```cpp
//! ROOT::TThreadedObject<TH1D> hist("h", "", 100, 0., 100.);
//! // in each task:           hist->Fill(x);
//! auto merged = hist.Merge();
//! ```

use oxiroot::prelude::*;

fn main() {
    // A binning prototype (normally empty); each thread copies it on first fill.
    let hist = ThreadedHist::new(
        Hist::reg(100, 0.0, 100.0)
            .double()
            .named("mass")
            .titled("toy mass [GeV]"),
    );

    // Some data to fill, split across the available worker threads.
    let data: Vec<f64> = (0..1_000_000)
        .map(|i| (i as f64 * 0.61803) % 100.0)
        .collect();
    let n_threads = std::thread::available_parallelism().map_or(4, |n| n.get());

    std::thread::scope(|s| {
        for chunk in data.chunks(data.len().div_ceil(n_threads)) {
            let hist = &hist;
            s.spawn(move || {
                // No per-thread setup, no locks in your code — just fill.
                for &x in chunk {
                    hist.fill(x);
                }
            });
        }
    });

    println!(
        "filled {} values across {} thread-local copies",
        data.len(),
        hist.num_slots()
    );

    // Combine every thread's copy into one — exact (contents + entries + moments).
    let merged = hist.merge().expect("identical binning");
    println!(
        "merged: {} entries, mean {:.4}, std {:.4}, peak bin {} = {}",
        merged.entries,
        merged.mean(),
        merged.std_dev(),
        merged.maximum_bin(),
        merged.maximum(),
    );

    // The result is identical to a plain single-threaded fill (bins are exact;
    // moment sums agree up to floating-point summation order).
    let mut serial = Hist::reg(100, 0.0, 100.0)
        .double()
        .named("mass")
        .titled("toy mass [GeV]");
    for &x in &data {
        serial.fill(x);
    }
    assert_eq!(merged.values(), serial.values(), "bins match serial fill");
    assert_eq!(merged.entries, serial.entries, "entry count matches");
    println!("\u{2713} identical to a single-threaded fill");
}
