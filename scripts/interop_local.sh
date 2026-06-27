#!/usr/bin/env bash
# Local "full interop" test harness for oxiroot — runs on this machine, not CI.
#
#   bash scripts/interop_local.sh                # full local run
#   bash scripts/interop_local.sh --no-fixtures  # skip the fixture-regen drift check (fast)
#   bash scripts/interop_local.sh --big          # also test >2 GiB (64-bit) read
#   bash scripts/interop_local.sh --keep         # keep the work dir + regenerated fixtures
#
# It exercises oxiroot's full read+write surface and cross-checks against ROOT
# C++ and uproot in both directions, then prints a PASS/FAIL/SKIP matrix and
# exits nonzero if anything failed. Missing oracles (no ROOT / no uproot venv)
# degrade to SKIP, never FAIL — `cargo test --workspace` alone is still a
# meaningful green.
#
# Mechanisms:
#   (A) canonical round-trip   — crates/oxiroot/examples/interop.rs + the
#       scripts/interop_{root.cpp,uproot.py} oracles (the lean smoke test).
#       Covers single + multi-object + subdirectory + appended histograms
#       (the RootFile builder) and subdirectory reads (read_root_in), both ways.
#   (B) manifest-driven matrix — crates/oxiroot/examples/interop_matrix.rs writes
#       ~38 cases + manifest.json; scripts/interop_matrix_{root.cpp,uproot.py}
#       consume the manifest and assert (the broad write-compat coverage).
#   (C) read-compat            — cargo test vs committed fixtures, optionally
#       regenerated from the CURRENT local oracles to catch version drift.

set -uo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
PY="$REPO/.venv/bin/python"
DO_FIXTURES=1
DO_BIG=0
KEEP=0
for arg in "$@"; do
  case "$arg" in
    --no-fixtures) DO_FIXTURES=0 ;;
    --big) DO_BIG=1 ;;
    --keep) KEEP=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

WORK="$(mktemp -d)"
cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    echo "(kept work dir: $WORK)"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

# --- result recording -------------------------------------------------------
declare -a R_LABEL R_STATUS
record() { R_STATUS+=("$1"); R_LABEL+=("$2"); }
# run <label> <cmd...> : run a command, tee output, record PASS/FAIL.
run() {
  local label="$1"; shift
  echo; echo "──── $label ────"
  if "$@"; then record "PASS" "$label"; else record "FAIL" "$label"; fi
}
skip() { echo "──── $1 ──── SKIP ($2)"; record "SKIP ($2)" "$1"; }

# ===========================================================================
# Phase 0 — preflight
# ===========================================================================
command -v cargo >/dev/null 2>&1 && HAVE_CARGO=1 || HAVE_CARGO=0
{ [ -x "$PY" ] && "$PY" -c 'import uproot,numpy,awkward' >/dev/null 2>&1; } && HAVE_PY=1 || HAVE_PY=0
command -v root-config >/dev/null 2>&1 && HAVE_ROOT=1 || HAVE_ROOT=0
command -v rootcling >/dev/null 2>&1 && HAVE_ROOTCLING=1 || HAVE_ROOTCLING=0

echo "oxiroot local interop harness"
echo "  repo: $REPO"
echo "  work: $WORK"
printf "  capabilities: cargo=%s  python+uproot=%s  root-config=%s  rootcling=%s\n" \
  "$([ $HAVE_CARGO = 1 ] && echo yes || echo NO)" \
  "$([ $HAVE_PY = 1 ] && echo yes || echo no)" \
  "$([ $HAVE_ROOT = 1 ] && echo yes || echo no)" \
  "$([ $HAVE_ROOTCLING = 1 ] && echo yes || echo no)"
if [ "$HAVE_CARGO" -eq 0 ]; then
  echo "FATAL: cargo not found — cannot run anything." >&2
  exit 2
fi

# ===========================================================================
# Phase 1 — build Rust drivers + compile C++ oracles
# ===========================================================================
echo; echo "building Rust interop drivers…"
cargo build -q -p oxiroot --example interop --example interop_matrix \
  || { echo "FATAL: example build failed" >&2; exit 1; }

ROOT_OK=0
if [ "$HAVE_ROOT" -eq 1 ]; then
  echo "compiling ROOT C++ oracles…"
  RCFLAGS="$(root-config --cflags)"
  RLIBS="$(root-config --libs)"
  if c++ $RCFLAGS "$REPO/scripts/interop_root.cpp" $RLIBS -lROOTNTuple -o "$WORK/interop_root" 2>"$WORK/cc1.log" \
     && c++ $RCFLAGS -I "$REPO/scripts" "$REPO/scripts/interop_matrix_root.cpp" $RLIBS -lROOTNTuple -o "$WORK/interop_matrix_root" 2>"$WORK/cc2.log"; then
    ROOT_OK=1
  else
    echo "WARNING: ROOT C++ oracle compile failed; ROOT rows will SKIP. See $WORK/cc*.log"
    cat "$WORK/cc1.log" "$WORK/cc2.log" 2>/dev/null | grep -vi "modulemap\|experimental\|duplicate librar" | head
  fi
fi

# ===========================================================================
# Phase 2 — canonical round-trip (mechanism A), both directions
# ===========================================================================
canonical_root() {
  "$WORK/interop_root" write "$WORK" \
    && cargo run -q -p oxiroot --example interop -- read "$WORK" \
    && cargo run -q -p oxiroot --example interop -- write "$WORK" \
    && "$WORK/interop_root" read "$WORK"
}
canonical_uproot() {
  cargo run -q -p oxiroot --example interop -- write "$WORK" \
    && "$PY" "$REPO/scripts/interop_uproot.py" read "$WORK" \
    && "$PY" "$REPO/scripts/interop_uproot.py" write "$WORK" \
    && cargo run -q -p oxiroot --example interop -- read "$WORK"
}
[ "$ROOT_OK" -eq 1 ] && run "canonical round-trip (ROOT C++)" canonical_root \
  || { [ "$HAVE_ROOT" -eq 1 ] && skip "canonical round-trip (ROOT C++)" "compile failed" || skip "canonical round-trip (ROOT C++)" "no root-config"; }
[ "$HAVE_PY" -eq 1 ] && run "canonical round-trip (uproot)" canonical_uproot \
  || skip "canonical round-trip (uproot)" "no uproot venv"

# Smoke-run the showcase examples (pure Rust): they drive the method-based API —
# the RootFile builder (multi-object/subdirectory/append), read_root_in, and
# Tree/Ntuple::write_root — so a regression there fails even without an oracle.
run_examples() {
  cargo run -q -p oxiroot --example analysis \
    && cargo run -q -p oxiroot --example tree \
    && cargo run -q -p oxiroot --example rntuple_nested
}
run "examples smoke (RootFile builder / read_root_in / Tree+Ntuple)" run_examples

# ===========================================================================
# Phase 3 — manifest-driven matrix (mechanism B), Rust -> oracle
# ===========================================================================
MATRIX="$WORK/matrix"
if cargo run -q -p oxiroot --example interop_matrix -- write "$MATRIX"; then
  [ "$ROOT_OK" -eq 1 ] && run "matrix Rust->oracle (ROOT C++)" "$WORK/interop_matrix_root" "$MATRIX" \
    || { [ "$HAVE_ROOT" -eq 1 ] && skip "matrix Rust->oracle (ROOT C++)" "compile failed" || skip "matrix Rust->oracle (ROOT C++)" "no root-config"; }
  [ "$HAVE_PY" -eq 1 ] && run "matrix Rust->oracle (uproot)" "$PY" "$REPO/scripts/interop_matrix_uproot.py" "$MATRIX" \
    || skip "matrix Rust->oracle (uproot)" "no uproot venv"
else
  record "FAIL" "matrix writer"
fi

# ===========================================================================
# Phase 4 — cargo test (read-compat vs committed fixtures, pure Rust)
# ===========================================================================
run "cargo test --workspace" cargo test -q --workspace

# ===========================================================================
# Phase 5 — fixture-regen drift check (regenerate from CURRENT local oracles)
# ===========================================================================
regen_fixtures() {
  local rc=0
  ( cd "$REPO"
    set -e
    RCFLAGS="$(root-config --cflags)"; RLIBS="$(root-config --libs)"
    c++ $RCFLAGS scripts/gen_root_fixtures.cpp    $RLIBS               -o "$WORK/gen_root"
    c++ $RCFLAGS scripts/gen_rntuple_fixtures.cpp $RLIBS -lROOTNTuple  -o "$WORK/gen_rntuple"
    c++ $RCFLAGS scripts/gen_tree_vector.cpp      $RLIBS               -o "$WORK/gen_tree_vector"
    c++ $RCFLAGS scripts/gen_compressed_fixtures.cpp $RLIBS            -o "$WORK/gen_compressed"
    "$WORK/gen_root"
    "$WORK/gen_rntuple"
    "$WORK/gen_tree_vector" fixtures/tree_vector.root
    "$WORK/gen_compressed"
    bash scripts/gen_tree_split.sh
    "$PY" scripts/gen_tree_fixtures.py
    "$PY" scripts/gen_fixtures.py
  ) || rc=1
  cargo test -q --workspace || rc=1
  if [ "$KEEP" -eq 0 ]; then git -C "$REPO" checkout -- fixtures/ 2>/dev/null; fi
  return $rc
}
if [ "$DO_FIXTURES" -eq 1 ]; then
  if [ "$HAVE_PY" -eq 1 ] && [ "$HAVE_ROOT" -eq 1 ] && [ "$HAVE_ROOTCLING" -eq 1 ]; then
    run "fixture regen + retest (oracle->Rust)" regen_fixtures
  else
    skip "fixture regen + retest (oracle->Rust)" "needs root+rootcling+uproot"
  fi
else
  skip "fixture regen + retest (oracle->Rust)" "--no-fixtures"
fi

# ===========================================================================
# Phase 6 — >2 GiB read (opt-in)
# ===========================================================================
big_read() {
  local rcflags rlibs
  rcflags="$(root-config --cflags)"; rlibs="$(root-config --libs)"
  c++ $rcflags "$REPO/scripts/gen_big_fixture.cpp" $rlibs -o "$WORK/gen_big" \
    && "$WORK/gen_big" "$WORK/big.root" \
    && cargo run -q -p oxiroot --example interop_matrix -- read-big "$WORK/big.root"
}
if [ "$DO_BIG" -eq 1 ]; then
  [ "$HAVE_ROOT" -eq 1 ] && run "big >2GiB read (oracle->Rust)" big_read \
    || skip "big >2GiB read (oracle->Rust)" "no root-config"
else
  skip "big >2GiB read (oracle->Rust)" "--big not set"
fi

# ===========================================================================
# Phase 7 — matrix report
# ===========================================================================
echo
echo "═══════════════════════ INTEROP MATRIX ═══════════════════════"
fails=0; skips=0; passes=0
for i in "${!R_LABEL[@]}"; do
  st="${R_STATUS[$i]}"
  printf "  %-44s %s\n" "${R_LABEL[$i]}" "$st"
  case "$st" in
    PASS) passes=$((passes+1)) ;;
    FAIL) fails=$((fails+1)) ;;
    *) skips=$((skips+1)) ;;
  esac
done
echo "──────────────────────────────────────────────────────────────"
if [ "$fails" -gt 0 ]; then
  echo "RESULT: FAIL ($fails failed, $passes passed, $skips skipped)"
  exit 1
else
  echo "RESULT: PASS ($passes passed, $skips skipped)"
  exit 0
fi
