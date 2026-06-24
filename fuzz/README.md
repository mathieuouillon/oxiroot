# Fuzz targets

Coverage-guided fuzzing of the byte parsers, via
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) (nightly toolchain):

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run rfile        # TFile container parser
cargo +nightly fuzz run rntuple      # RNTuple anchor/envelope/page decode
cargo +nightly fuzz run decompress   # ROOT compression block decoder
```

Seed the corpus from the committed fixtures for faster coverage:

```sh
mkdir -p corpus/rfile && cp ../fixtures/*.root corpus/rfile/
```

This is a standalone workspace, so it is excluded from the main
`cargo build/test --workspace`. The same inputs are also checked, non-randomly,
by the `malformed.rs` integration tests in each crate.
