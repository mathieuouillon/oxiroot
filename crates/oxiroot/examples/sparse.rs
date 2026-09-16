//! `THnSparse` — a memory-efficient N-dimensional histogram. Only the cells that
//! actually get filled are stored, so a high-dimensional cut-optimization space
//! (here 4-D: pt, eta, isolation, mass) costs memory proportional to the *events*
//! seen, not to the astronomically large dense grid. We fill a few thousand
//! correlated events, report the occupied-vs-dense cell counts to show the win,
//! then write the object to a ROOT file and read it back.
//!
//! ```sh
//! cargo run -p oxiroot --example sparse
//! ```

use oxiroot::prelude::*;

/// A tiny deterministic RNG (xorshift64) + Box–Muller, so the example needs no
/// dependency and prints the same numbers every run.
struct Rng(u64);

impl Rng {
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
    let mut rng = Rng(0x5A17_C0FFEE_D15EA5);

    // --- A 4-D cut-optimization space: (pt, eta, isolation, mass). -------------
    // Each axis is (nbins, lo, hi). The dense grid would be the *product* of the
    // per-axis bin counts — here 40 × 20 × 25 × 60 = 1_200_000 cells — but a
    // THnSparse only materializes the cells an event actually lands in.
    let mut hs = THnSparse::new(&[
        (40, 0.0, 200.0),  // pt   [GeV]
        (20, -2.5, 2.5),   // eta
        (25, 0.0, 1.0),    // isolation (relative)
        (60, 60.0, 120.0), // mass [GeV]
    ])
    .named("cutspace")
    .titled("4-D cut optimization space");

    // --- Draw a few thousand *correlated* events and fill(&[pt, eta, iso, mass]).
    // Correlations (high-pt tracks are more central and better isolated; the mass
    // peaks near the Z) mean events cluster — so the sparse fill touches only a
    // small fraction of the 1.2 M dense cells.
    let n_events = 4_000;
    for _ in 0..n_events {
        let pt = rng.gauss(50.0, 25.0).clamp(0.0, 199.9);
        // More central (small |eta|) as pt grows.
        let eta = rng.gauss(0.0, 1.2 - 0.5 * (pt / 200.0)).clamp(-2.49, 2.49);
        // Better isolated (small iso) as pt grows.
        let iso = (rng.uniform() * (0.4 - 0.3 * (pt / 200.0))).clamp(0.0, 0.99);
        let mass = rng.gauss(91.2, 4.0).clamp(60.0, 119.9);
        hs.fill(&[pt, eta, iso, mass]);
    }

    // --- The memory win: occupied cells vs the dense grid. ---------------------
    let dense_cells: i64 = hs.axes.iter().map(|ax| ax.nbins as i64).product();
    let filled = hs.bins.len();
    let occupancy = 100.0 * filled as f64 / dense_cells as f64;
    println!("THnSparse `{}`: \"{}\"", hs.name, hs.title);
    println!("  dimensions       : {}", hs.ndim());
    for ax in &hs.axes {
        println!(
            "    {:<4} {:>3} bins over [{:>6.1}, {:>6.1}]",
            ax.name, ax.nbins, ax.xmin, ax.xmax
        );
    }
    println!("  events filled    : {}", n_events);
    println!("  entries          : {}", hs.entries);
    println!("  filled cells     : {filled}");
    println!("  dense grid cells : {dense_cells}  (product of per-axis bins)");
    println!(
        "  occupancy        : {occupancy:.3}%  — only {filled} of {dense_cells} cells are stored"
    );
    // f64 per dense cell vs (coords + content) per sparse cell; the sparse store
    // is roughly `filled / dense_cells` of the dense footprint here.
    println!(
        "  memory ratio     : ~{:.0}x smaller than a dense THn of the same axes",
        dense_cells as f64 / filled.max(1) as f64
    );

    // Peek at the single most-populated cell (max content over the sparse bins).
    if let Some(peak) = hs
        .bins
        .iter()
        .max_by(|a, b| a.content.total_cmp(&b.content))
    {
        // `coords` are per-axis, flow-inclusive bin indices (0 = underflow).
        println!(
            "  busiest cell     : bins {:?} holds {} event(s)",
            peak.coords, peak.content
        );
    }

    // --- Write the THnSparse to a ROOT file and read it back. ------------------
    // Under std::env::temp_dir(), named for THIS example, and removed before we
    // return — never litter the repo or cwd.
    let path = std::env::temp_dir().join("oxiroot_ex_sparse.root");
    hs.write_root(&path, Compression::Zstd(5))?; // WriteRoot trait
    println!("\nwrote THnSparse -> {}", path.display());

    let f = RFile::open(&path)?;
    let back = THnSparse::read_root(&f, "cutspace")?; // ReadRoot trait
    println!(
        "read back `cutspace`: {} dims, {} entries, {} filled cells",
        back.ndim(),
        back.entries,
        back.bins.len(),
    );

    // The round-trip must preserve the entry count and the occupied-cell set.
    assert_eq!(back.ndim(), hs.ndim(), "ndim survived the round-trip");
    assert_eq!(back.entries, hs.entries, "entries survived the round-trip");
    assert_eq!(
        back.bins.len(),
        filled,
        "the filled-cell count survived the round-trip"
    );
    println!("round-trip OK: entries and filled-cell count match");

    // --- Clean up the temp file. ----------------------------------------------
    let _ = std::fs::remove_file(&path);

    Ok(())
}
