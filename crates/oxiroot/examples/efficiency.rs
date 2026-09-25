//! `Efficiency` — a trigger turn-on curve. For many simulated events we draw a
//! transverse momentum `pT`, decide pass/fail against a logistic acceptance
//! `p(pT) = 1 / (1 + exp(-(pT - x50)/w))`, and let `Efficiency` accumulate the
//! per-bin passed/total ratio. We print the recovered efficiency, put a
//! Clopper–Pearson confidence interval on one bin with `oxiroot::stat`, then
//! write the object to a ROOT file and read it straight back.
//!
//! ```sh
//! cargo run -p oxiroot --example efficiency
//! ```

use oxiroot::prelude::*;

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run. Copied verbatim from the
/// `fit` example.
struct XorShift64(u64);

impl XorShift64 {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() -> oxiroot::Result<()> {
    let mut rng = XorShift64(0x0DD_F00D_CAFE_BEEF);

    // The true turn-on: 50% efficiency at x50 GeV, rising over a width w.
    let (x50, w) = (30.0_f64, 5.0_f64);
    let logistic = |pt: f64| 1.0 / (1.0 + (-(pt - x50) / w).exp());

    // 20 uniform bins over the pT range the trigger turns on in.
    let (nbins, pt_lo, pt_hi) = (20, 0.0, 60.0);
    let mut eff = Efficiency::new(nbins, pt_lo, pt_hi)
        .named("trig_turnon")
        .titled("trigger turn-on;p_{T} [GeV];#epsilon");

    // Event loop: draw a flat pT, draw a uniform u, the event "passes" the
    // trigger when u < p(pT). `fill` bumps `total` always, `passed` when true.
    let n_events = 200_000;
    for _ in 0..n_events {
        let pt = pt_lo + (pt_hi - pt_lo) * rng.uniform();
        let u = rng.uniform();
        eff.fill(u < logistic(pt), pt);
    }
    println!(
        "Filled {n_events} events into {nbins} bins of [{pt_lo}, {pt_hi}] GeV \
         (true x50 = {x50} GeV, width = {w} GeV)."
    );

    // --- Per-bin efficiency: the recovered turn-on curve. ----------------------
    // Bins are 1-based (bin 0 is underflow); the low edge of bin i is a stride in.
    let stride = (pt_hi - pt_lo) / nbins as f64;
    println!("\n  bin   pT-center   passed/total     eff     truth");
    for bin in 1..=nbins as usize {
        let center = pt_lo + (bin as f64 - 0.5) * stride;
        // `passed`/`total` are the two embedded TH1D histograms; their `contents`
        // are 1-based too, so we can index by `bin` directly.
        let passed = eff.passed.contents[bin];
        let total = eff.total.contents[bin];
        // Print a coarse subset so the output stays readable.
        if bin % 2 == 1 {
            println!(
                "  {bin:>3}   {center:>7.1}    {passed:>6.0}/{total:<6.0}   {:>6.3}   {:>6.3}",
                eff.efficiency(bin),
                logistic(center),
            );
        }
    }

    // --- Clopper–Pearson interval on the bin straddling the 50% point. ---------
    // `Efficiency` stores ROOT's default 68.27% Clopper–Pearson recipe; here we
    // reproduce that interval directly from the bin's passed/total counts with
    // the stats library, and report the efficiency with asymmetric errors.
    let mid_bin = ((x50 - pt_lo) / stride).floor() as usize + 1;
    let k = eff.passed.contents[mid_bin];
    let n = eff.total.contents[mid_bin];
    let center = pt_lo + (mid_bin as f64 - 0.5) * stride;
    let (lo, hi) = oxiroot::stat::clopper_pearson(k, n, eff.conf_level);
    let e = eff.efficiency(mid_bin);
    println!("\nBin {mid_bin} (pT ~ {center:.1} GeV, straddles x50): {k:.0}/{n:.0} passed",);
    println!(
        "  efficiency = {e:.4}  +{:.4} -{:.4}   (Clopper-Pearson {:.1}% CL: [{lo:.4}, {hi:.4}])",
        hi - e,
        e - lo,
        eff.conf_level * 100.0,
    );

    // --- Write the Efficiency to a ROOT file, then read it back. ---------------
    // Temp file, named for this example, removed before we return — no litter.
    // (uproot cannot read Efficiency, but ROOT C++ and oxiroot itself can.)
    let path = std::env::temp_dir().join("oxiroot_ex_efficiency.root");
    FileWriter::create(&path)
        .add(&eff)
        .write(Compression::Zstd(5))?;
    println!("\nwrote TEfficiency -> {}", path.display());

    let file = FileReader::open(&path)?;
    let back = Efficiency::read_root(&file, "trig_turnon")?;
    println!(
        "read back `{}`: {} total trials, mid-bin eff = {:.4} (matches: {})",
        back.name,
        back.total.contents.iter().sum::<f64>(),
        back.efficiency(mid_bin),
        (back.efficiency(mid_bin) - e).abs() < 1e-12,
    );

    let _ = std::fs::remove_file(&path);
    Ok(())
}
