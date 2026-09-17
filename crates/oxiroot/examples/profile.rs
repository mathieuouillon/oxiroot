//! `TProfile` (and a short `TProfile2D`): collapse a 2-D scatter into a 1-D
//! trend. For each event we draw a true energy `x` and a *measured* response
//! `y = response(x) + Gaussian scatter`, then `fill(x, y)`. A profile keeps the
//! mean response per x-bin with the error on that mean — the physicist's way to
//! read a detector-response curve out of a noisy cloud of points. We print the
//! per-bin trend, write it to a ROOT file, read it back, then do the same in 2-D.
//!
//! ```sh
//! cargo run -p oxiroot --example profile
//! ```

use oxiroot::prelude::*;

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
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

/// The "true" detector response we are trying to measure: a gentle saturating
/// curve. The profile should recover this trend from the scattered fills.
fn response(x: f64) -> f64 {
    8.0 * (1.0 - (-x / 4.0).exp())
}

fn main() -> oxiroot::Result<()> {
    let dir = std::env::temp_dir();
    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);

    // --- Fill a 1-D profile from a stream of (x, y) events. --------------------
    // A profile has the same shape as a TH1 (bins in x), but each bin stores the
    // running mean of y and the count needed for its error on the mean.
    let mut prof = Hist::reg(10, 0.0, 10.0)
        .profile()
        .named("resp")
        .titled("detector response vs true energy");

    // 20 000 events: draw a true energy uniformly in [0, 10), measure a noisy
    // response, and profile it. The scatter (sigma = 1.5) is deliberately large
    // so that any single event is a poor estimate — the profile averages it out.
    for _ in 0..20_000 {
        let x = 10.0 * rng.uniform();
        let y = response(x) + rng.gauss(0.0, 1.5);
        prof.fill(x, y);
    }
    println!(
        "TProfile `{}`: {} bins, {} entries",
        prof.name,
        prof.values().len(),
        prof.entries,
    );

    // Per-bin trend: bin center, measured mean response ± error on the mean, and
    // the true value for comparison. `values()[i]` is the mean of bin i+1;
    // `bin_error` takes the flow-inclusive bin index, so bin i+1.
    println!("  x-center   <response>     error     truth");
    let means = prof.values();
    for (i, &mean) in means.iter().enumerate() {
        let bin = i + 1; // ROOT bins are 1-based; 0 is underflow
        let xc = prof.xaxis.bin_center(bin);
        println!(
            "  {:7.2}   {:9.3}   ±{:7.4}   {:7.3}",
            xc,
            mean,
            prof.bin_error(bin),
            response(xc),
        );
    }

    // --- Write it to a ROOT file and read it straight back. --------------------
    // `RootFile::create(...).add(...)` is the one way to compose a file; a single
    // profile could also use `prof.write_root(path, comp)`.
    let path = dir.join("oxiroot_ex_profile.root");
    RootFile::create(&path)
        .add(&prof)
        .write(Compression::Zstd(5))?;
    println!("wrote profile -> {}", path.display());

    let f = RFile::open(&path)?;
    let back = TProfile::read_root(&f, "resp")?;
    // TProfile derives PartialEq, so a full round-trip is a single comparison.
    println!(
        "read back `{}`: identical to what we wrote? {}",
        back.name,
        back == prof,
    );
    println!(
        "  first bin mean: written {:.3}, read {:.3}",
        prof.values()[0],
        back.values()[0],
    );

    // --- A 2-D profile: mean z over an (x, y) grid. ----------------------------
    // Same idea, one dimension up. Each (x, y) cell holds the mean of a third
    // quantity z; here z is a smooth surface plus scatter. `.reg(...).reg(...)`
    // gives two axes, then `.profile()` makes it a TProfile2D.
    let mut prof2d = Hist::reg(4, 0.0, 4.0)
        .reg(3, 0.0, 3.0)
        .profile()
        .named("surface")
        .titled("mean z over (x, y)");

    for _ in 0..30_000 {
        let x = 4.0 * rng.uniform();
        let y = 3.0 * rng.uniform();
        let z = (x + 2.0 * y) + rng.gauss(0.0, 0.5); // true surface z = x + 2y
        prof2d.fill(x, y, z);
    }

    // `values()[ix][iy]` is the mean z in each in-range (x, y) cell. Print one
    // slice: the row at the first y-bin, across all x-bins.
    let grid = prof2d.values();
    println!(
        "TProfile2D `{}`: {} x-bins x {} y-bins, {} entries",
        prof2d.name,
        prof2d.nx(),
        prof2d.ny(),
        prof2d.entries,
    );
    println!("  slice at y-bin 1 (mean z per x-bin, truth in parens):");
    let yc = prof2d.yaxis.bin_center(1);
    for (ix, row) in grid.iter().enumerate() {
        let xc = prof2d.xaxis.bin_center(ix + 1);
        println!(
            "    x = {:4.2}, y = {:4.2}:  z = {:6.3}   ({:.3})",
            xc,
            yc,
            row[0],
            xc + 2.0 * yc,
        );
    }

    // --- Clean up: never leave files behind. -----------------------------------
    let _ = std::fs::remove_file(&path);

    Ok(())
}
