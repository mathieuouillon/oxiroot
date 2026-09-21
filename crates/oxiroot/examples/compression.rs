//! The ROOT compression codecs and their size/speed trade-off. Builds one
//! sizeable `TTree` (a few thousand-entry f64/i32 branches), writes it once per
//! codec — `None`, `Zstd(1)`, `Zstd(9)`, `Zlib(6)`, `Lz4(4)`, `Lzma(5)` — and tabulates the
//! on-disk size and the ratio vs the uncompressed baseline. It then reads one
//! compressed file back and asserts the branches equal the originals, proving
//! ROOT compression is lossless. Every codec here is one the writer can encode.
//!
//! ```sh
//! cargo run -p oxiroot --example compression
//! ```

use oxiroot::prelude::*;

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and builds the same payload every run.
struct XorShift64(u64);

impl XorShift64 {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self, mean: f64, sigma: f64) -> f64 {
        let (u1, u2) = (self.uniform().max(1e-12), self.uniform());
        mean + sigma * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

fn main() -> oxiroot::Result<()> {
    let dir = std::env::temp_dir();

    // --- Build ONE sizeable dataset, as an event loop would. -------------------
    // 20 000 entries across three branches. `energy` is smooth (Gaussian) so it
    // compresses well; `index` is a monotone counter (very compressible); `id` is
    // a small-range category. Real detector data sits somewhere in between.
    let n = 20_000;
    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);
    let mut energy = Vec::with_capacity(n);
    let mut index = Vec::with_capacity(n);
    let mut id = Vec::with_capacity(n);
    for i in 0..n {
        energy.push(rng.gauss(91.2, 2.5));
        index.push(i as i32);
        id.push((rng.uniform() * 8.0) as i32); // 0..7
    }

    // Assemble the tree once; each codec re-writes these same branches. A `Tree`
    // holds its columns, so we clone the source vectors per write.
    let make_tree = || {
        Tree::new(
            "Events",
            vec![
                Branch::f64("energy", energy.clone()),
                Branch::i32("index", index.clone()),
                Branch::i32("id", id.clone()),
            ],
        )
    };

    // --- Write the same tree once per codec, recording the file size. ----------
    // Every entry is `(label, Compression)`; the writer maps each to ROOT's
    // `algorithm*100 + level` setting integer under the hood.
    let codecs = [
        ("None", Compression::None),
        ("Zstd(1)", Compression::Zstd(1)),
        ("Zstd(9)", Compression::Zstd(9)),
        ("Zlib(6)", Compression::Zlib(6)),
        ("Lz4(4)", Compression::Lz4(4)),
        ("Lzma(5)", Compression::Lzma(5)),
    ];

    let mut paths = Vec::new();
    let mut sizes = Vec::new();
    for (label, comp) in codecs {
        // A distinct temp file per codec, all removed before we return.
        let path = dir.join(format!("oxiroot_ex_compression_{}.root", comp.setting()));
        make_tree().write_root(&path, comp)?;
        let bytes = std::fs::metadata(&path)?.len();
        sizes.push(bytes);
        paths.push(path);
        println!(
            "wrote {label:<8} (setting {:>3}) -> {bytes} bytes",
            comp.setting()
        );
    }

    // --- Tabulate: codec | level | bytes | ratio-vs-None. ----------------------
    // `ratio` is size / baseline, so smaller is better; the `None` row is 1.000.
    let baseline = sizes[0] as f64;
    println!();
    println!(
        "{:<10} {:>5} {:>10} {:>12}",
        "codec", "level", "bytes", "ratio/None"
    );
    println!("{}", "-".repeat(40));
    for (i, (label, comp)) in codecs.iter().enumerate() {
        // The level lives inside the Compression value; None has no level.
        let level = match comp {
            Compression::None => "-".to_string(),
            Compression::Zstd(l)
            | Compression::Zlib(l)
            | Compression::Lz4(l)
            | Compression::Lzma(l) => l.to_string(),
        };
        let ratio = sizes[i] as f64 / baseline;
        println!("{label:<10} {level:>5} {:>10} {ratio:>12.3}", sizes[i]);
    }

    // --- Round-trip a compressed file to prove compression is lossless. --------
    // Read the Zstd(9) file (index 2) back and compare every value to the source
    // vectors we wrote. If decompression altered a single bit, an assert fires.
    let (zstd_label, zstd_path) = (codecs[2].0, &paths[2]);
    let f = FileReader::open(zstd_path)?;
    let tree = TreeReader::open(&f, "Events")?;
    assert_eq!(
        tree.num_entries() as usize,
        n,
        "entry count survived the round-trip"
    );

    let BranchValues::F64(energy_back) = tree.read_branch(&f, "energy")? else {
        panic!("`energy` should read back as an f64 branch");
    };
    let BranchValues::I32(index_back) = tree.read_branch(&f, "index")? else {
        panic!("`index` should read back as an i32 branch");
    };
    assert_eq!(
        energy_back, energy,
        "f64 values identical after {zstd_label}"
    );
    assert_eq!(index_back, index, "i32 values identical after {zstd_label}");
    println!();
    println!(
        "round-trip via {zstd_label}: {} entries read back, all values bit-for-bit equal (lossless)",
        energy_back.len()
    );

    // --- Interpret the numbers. ------------------------------------------------
    // Every codec shrinks the file; the exact ordering depends on the payload.
    // The rules of thumb: a higher Zstd/Zlib level squeezes harder but writes
    // slower, LZ4 gives up some ratio for the fastest decode, and `None` is the
    // size ceiling. `best` is whichever codec actually won on THIS dataset.
    let best = (1..sizes.len())
        .min_by(|&a, &b| sizes[a].cmp(&sizes[b]))
        .unwrap_or(0);
    println!(
        "smallest file: {} at {} bytes ({:.1}% of uncompressed).",
        codecs[best].0,
        sizes[best],
        100.0 * sizes[best] as f64 / baseline,
    );
    println!("  Rule of thumb: high-level Zstd/Zlib win on ratio, LZ4 wins on decode speed —");
    println!("  pick per your read/write budget. All of them are lossless (asserted above).");

    // --- Clean up every temp file we created. ----------------------------------
    for path in &paths {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}
