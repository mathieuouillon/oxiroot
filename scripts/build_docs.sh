#!/usr/bin/env bash
# Build the full oxiroot documentation site into ./site/.
#
# Two pieces are combined into one deployable directory:
#   1. the Zensical guide site (docs/*.md + zensical.toml)  -> site/
#   2. the rustdoc API reference (cargo doc, all features)  -> site/api/
#
# The "API reference" nav entry in zensical.toml points at api/oxiroot/index.html,
# so the two are cross-linked once both land in site/.
#
# Usage:
#   bash scripts/build_docs.sh           # build guides + API into ./site
#   bash scripts/build_docs.sh --serve   # build, then `zensical serve` (live preview)
#
# Requirements:
#   - a Rust toolchain (for cargo doc)
#   - zensical:  pip install zensical   (Python >= 3.10)

set -euo pipefail
cd "$(dirname "$0")/.."

SERVE=0
[[ "${1:-}" == "--serve" ]] && SERVE=1

echo "==> Building rustdoc API reference (all features, no deps)"
cargo doc --no-deps --all-features --workspace

echo "==> Building Zensical guide site -> ./site"
zensical build

echo "==> Copying rustdoc into ./site/api"
rm -rf site/api
cp -r target/doc site/api
# rustdoc emits the crate-list landing at /index.html; our own homepage is the
# Zensical index, so the API is entered via api/oxiroot/index.html (see the nav).

echo "==> Done. Open ./site/index.html (API reference under ./site/api/oxiroot/)."

if [[ "$SERVE" == 1 ]]; then
  echo "==> zensical serve (Ctrl-C to stop)"
  zensical serve
fi
