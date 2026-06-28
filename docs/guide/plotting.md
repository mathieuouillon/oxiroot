# Plotting

Render histograms and graphs to **SVG and PNG** with a matplotlib-like API and an
mplhep histogram style — pure Rust, no matplotlib, no system fonts. Plotting is
behind the **`plot` feature** and exposed as `oxiroot::plot`.

<figure markdown="span">
  ![A filled MC histogram with data points overlaid, and a 2-D viridis heatmap](../images/plot-mass.png){ width="49%" }
  ![viridis heatmap with colorbar](../images/plot-heatmap.png){ width="49%" }
  <figcaption>Both produced by the bundled <code>plot</code> example, as PNG and SVG.</figcaption>
</figure>

```toml
[dependencies]
oxiroot = { git = "https://github.com/mathieuouillon/oxiroot", features = ["plot"] }
```

!!! note "How it works"
    One backend-independent draw IR fans out to a [`tiny-skia`](https://crates.io/crates/tiny-skia)
    raster (PNG) and a hand-written SVG, so the two outputs share identical
    geometry. DejaVu Sans (matplotlib's own default font) is bundled and
    text is reduced to glyph outlines, so the SVG is self-contained. `$…$`
    labels are typeset as real LaTeX math by the pure-Rust
    [ReX](https://github.com/KenyC/ReX) TeX engine into the same IR. The `plot`
    feature pulls a pinned git dependency on ReX (it is not on crates.io).

## A first plot

`Axes` mirrors matplotlib's `Axes`. Build it, add artists, label the axes, and
`save` — the format is chosen by the file extension (`.png` or `.svg`).

```rust
use oxiroot::plot::Axes;
use oxiroot::prelude::*;

let mut h = TH1::new(50, 0.0, 100.0).named("pt");
h.fill(42.0);

let mut ax = Axes::new();
ax.hist(&h);                       // mplhep step staircase
ax.set_xlabel("$p_T$ [GeV]");      // LaTeX math via ReX
ax.set_ylabel("Events");
ax.save("pt.png")?;                // or "pt.svg"
# Ok::<(), oxiroot::plot::Error>(())
```

## Histograms (mplhep style)

`hist` draws a `TH1` as an mplhep staircase. `histplot` takes a `HistOpts`
builder for control over the type, error bars, color, fill, and legend label.

| `HistType` | Look |
|-----------|------|
| `Step` | Staircase outline closed to the baseline (default) |
| `Fill` | Filled staircase down to the baseline |
| `Errorbar` | Markers at bin centers with error bars (data-point look) |
| `Band` | Shaded `y ± yerr` uncertainty band |

```rust
use oxiroot::plot::{Axes, HistType, HistOpts, Color};

let mut ax = Axes::new();
ax.histplot(
    &mc,
    HistOpts::new()
        .histtype(HistType::Fill)
        .fill_color(Color::hex("#1f77b4").with_alpha(0.4))
        .label("MC"),
);
ax.histplot(&data_hist, HistOpts::new().yerr(true).label("data")); // √N / Sumw2 bars
```

Error bars come from `√N` (or the Sumw2 per-bin error when the histogram tracks
it). `hist`/`histplot` snap the x-axis to the bin edges and start the y-axis at
zero, the mplhep convention.

## Graphs and profiles

`errorbar` draws a `TGraph` (plain, symmetric, or asymmetric errors) as data
points; `profile` draws a `TProfile` at bin centers. `ErrorbarOpts` controls the
marker, color, caps, and an optional connecting line.

```rust
use oxiroot::plot::{Axes, ErrorbarOpts, Color};

let mut ax = Axes::new();
ax.errorbar_opts(&graph, ErrorbarOpts::new().color(Color::BLACK).label("data"));
ax.profile(&prof);
ax.legend();
```

## 2-D histograms

`hist2d`/`hist2dplot` render a `TH2` as a `pcolormesh`-style color grid with a
colorbar, using the real matplotlib `viridis`/`plasma` colormaps.

```rust
use oxiroot::plot::{Axes, Hist2dOpts, Colormap};

let mut ax = Axes::new();
ax.hist2dplot(&th2, Hist2dOpts::new().cmap(Colormap::Viridis).label("entries"));
ax.set_xlabel("$x$");
ax.set_ylabel("$y$");
ax.save("heatmap.svg")?;
# Ok::<(), oxiroot::plot::Error>(())
```

`Colormap` covers `Viridis`, `Plasma`, `Gray`, and `GrayReversed`. The value
range autoscale can be overridden with `Hist2dOpts::vrange(vmin, vmax)`.

## Style

The default look reproduces a plain matplotlib figure: 640×480 px, DejaVu Sans,
the `tab10` color cycle, a black rectangular frame, out-pointing major ticks on
the bottom and left, and 5 % data margins. `Style::mplhep()` switches to
in-pointing ticks on all four sides with minor ticks and a frameless legend.

```rust
use oxiroot::plot::{Axes, Style};

let mut ax = Axes::with_style(Style::mplhep());
```

A `Style` exposes the figure size, dpi, fonts, colors, tick geometry, and margins
if you need to customize further.

## Math labels

Any axis label, title, or colorbar label may contain `$…$` math, typeset by ReX:
fractions, radicals, big operators with limits, sub/superscripts, and Greek all
render, e.g. `"$\\frac{1}{\\sigma}\\frac{d\\sigma}{dp_T}$"` or `"$\\sqrt{s} = 13\\,\\mathrm{TeV}$"`.
A malformed math run falls back to plain text rather than failing.

## Figures and saving

For a single panel, `Axes::save` is the convenient path. For composing or for the
matplotlib-familiar entry point, use `Figure`/`subplots`:

```rust
use oxiroot::plot::subplots;

let (fig, mut ax) = subplots();
ax.hist(&h);
fig.with(ax).savefig("pt.png")?;
# Ok::<(), oxiroot::plot::Error>(())
```

## Worked example

```sh
cargo run -p oxiroot --example plot --features plot
```

It renders a Z → μμ overlay (filled MC + data points + legend + LaTeX labels) and
a 2-D heatmap, each to both PNG and SVG.

## See also

- [Histograms](histograms.md) — the objects being plotted
- [Graphs](graphs.md) — the `TGraph` family
- [Fitting](fitting.md) — fit a model to the same data
- [Reading & writing files](reading-writing.md) — persist the histograms first
