# oxiroot-rex

A trimmed, vendored copy of [ReX](https://github.com/KenyC/ReX), the TeX math
layout engine that `oxiroot-plot` uses to typeset `$…$` spans in labels.

- **Upstream:** KenyC/ReX at `aeccdba38f3fa54195c469319b65c423e17a77ae`
  (2026-02-24), package version 0.1.2. ReX's `deps/unicode-math` path
  dependency is folded in as the private module `unicode_math`.
- **Status:** an internal part of `oxiroot-plot`. The API is ReX's and has no
  semver promise; nothing in oxiroot re-exports it.
- **Why vendored:** ReX is not on crates.io, and a git dependency would keep
  `oxiroot-plot` (and so the `oxiroot` facade and the CLI) from being published.
- **Licences:** see [LICENSE-3rdparty](LICENSE-3rdparty), and
  [resources/FONTS.md](resources/FONTS.md) for the test fonts.

## What was removed

- The renderers (`src/render/{cairo,femtovg,pathfinder,raqote,tinyskia}.rs`);
  oxiroot-plot draws through its own `Backend`.
- The `font` crate backend (`src/font/backend/font.rs`); only the
  `ttf-parser` backend is kept, and it is always on.
- The `serde` derives and the `log` dependency (both unused).
- unicode-math's build script and its `nom`/`regex` build dependencies: its
  output is committed as `src/unicode_math/symbols.rs` and
  `src/unicode_math/reserved_replacements.rs`.
- `OPERATOR_LIMITS` and the per-symbol `description` field of unicode-math,
  which nothing reads.
- Upstream examples, benchmarks, samples, the integration tests that need
  cairo/raqote/LaTeX, and their images.

## Local patch list

Everything else under `src/` is byte-identical to upstream (with
`deps/unicode-math/src/{lib,common}.rs` at `src/unicode_math/{mod,common}.rs`).

- `src/lib.rs`: the upstream crate doc (cairo examples) is replaced by a short
  header; `extern crate serde_derive` and `extern crate log` are removed;
  `mod unicode_math;` is added.
- `src/dimensions.rs`, `src/layout/mod.rs`, `src/parser/color.rs`: the
  `#[derive(Serialize, Deserialize)]` lines are removed.
- `src/dimensions.rs`: the doctest imports `oxiroot_rex` instead of `rex`.
- `src/font/backend.rs`: only `pub mod ttf_parser;` remains, without its
  feature gate.
- `src/font/common.rs`: the `font`-crate impls and the feature gates on the
  `ttf-parser` impls are removed, as is an unused `std::convert` import.
- `src/render/mod.rs`: the five feature-gated renderer modules are removed.
- `src/render/bbox.rs`: the two `ttfparser-fontparser` feature gates are
  removed.
- `unicode_math::` paths become `crate::unicode_math::` in
  `src/font/{mod,style}.rs`, `src/layout/engine.rs` and
  `src/parser/{mod,environments,control_sequence,symbols}.rs`.
- Warnings fixed: an unused `VBox` import and an unused `LayoutError` import in
  `src/layout/engine.rs`, and an unneeded `mut` in `src/parser/mod.rs`.
- Array column counts (bug fixes; upstream dropped cells, and debug builds
  panicked on a `debug_assert_eq!` in the array layout):
  - `src/parser/environments.rs`: a `matrix`/`pmatrix`/…/`aligned` environment
    takes its column count from its widest row, not its last row; an `array`
    row with more cells than its column format declares is the new
    `ParseError::TooManyCellsInArrayRow` (LaTeX's "Extra alignment tab").
  - `src/parser/error.rs`: that variant and its message.
  - `src/layout/engine.rs`: the array layout takes its column count from the
    column format, so a format wider than its rows lays out empty cells.
- `src/unicode_math/mod.rs`: a module header; the tables are included from the
  committed files; the `OPERATOR_LIMITS` re-export is removed.
- `src/unicode_math/common.rs`: the `serde` `cfg_attr`, `OPERATOR_LIMITS` and
  the `Symbol::description` field are removed.
- `src/unicode_math/symbols.rs`: the `description: "…",` entries are removed.
- Parser snapshots are renamed from `rex__parser__tests__*` to
  `oxiroot_rex__parser__tests__*` (insta names them after the module path);
  their contents are unchanged. Four orphaned `snapshot_style*` files are not
  copied.

`src/` is excluded from rustfmt (`src/rustfmt.toml`) and from most clippy
lints, so the code stays diffable against upstream. Keep local patches small
and list them here.

## Tests

- The upstream unit tests in `src/`, including 150 parser snapshots, run
  unchanged. They load the fonts in `resources/`.
- `tests/history.rs` replays upstream's backend-independent render history
  (`tests/data/history_regression_render.yaml`, converted to
  `tests/history.txt`; 154 snippets): every snippet must lay out to the same
  size and draw the same glyphs and rules.

## Regenerating the tables

```sh
git clone https://github.com/KenyC/ReX && cd ReX
git checkout aeccdba38f3fa54195c469319b65c423e17a77ae
cargo build
cp target/debug/build/unicode-math-*/out/symbols.rs <oxiroot>/crates/oxiroot-rex/src/unicode_math/symbols.rs
cp target/debug/build/unicode-math-*/out/math_alphanumeric_table_reserved_replacements.rs \
   <oxiroot>/crates/oxiroot-rex/src/unicode_math/reserved_replacements.rs
perl -pi -e 's/ description: "(?:[^"\\]|\\.)*",//g' <oxiroot>/crates/oxiroot-rex/src/unicode_math/symbols.rs
```

The build output should have these SHA-1 sums: `symbols.rs`
`4edfc0287967ce82b3b79c374fabc2b1f2d9bf96` (before the description removal;
`40cf6eb322d56f8dd38638ce0725c9d1446fb74b` after), and
`reserved_replacements.rs` `eba407f45e1d427f7bb4d4b95251c7a9bac9b5fa`.

To regenerate `tests/history.txt` from an upstream checkout (needs PyYAML):

```sh
python3 scripts/gen_rex_history.py <ReX checkout> crates/oxiroot-rex/tests/history.txt
```

## Porting an upstream fix

1. Diff upstream `src/` against this crate's `src/`, mapping
   `deps/unicode-math/src/{lib,common}.rs` to `src/unicode_math/{mod,common}.rs`.
   Also diff upstream `deps/unicode-math/src/{build.rs,parser.rs,parser/}` and
   `deps/unicode-math/resources/`: if they changed, regenerate the tables as
   above and update the SHA-1 sums.
2. Apply the change, keeping the local patches above.
3. Rename any new snapshot from `rex__…` to `oxiroot_rex__…`.
4. If upstream re-recorded its render history, rerun
   `scripts/gen_rex_history.py`.
5. Update the rev (and the version, if it changed) in `Cargo.toml`
   (`[package.metadata.vendored]` `rev` and `upstream-version`), in this README
   (the header and the `git checkout` above), in the `src/lib.rs` crate header,
   and in `LICENSE-3rdparty`, checking upstream's authors and licence text
   there.
