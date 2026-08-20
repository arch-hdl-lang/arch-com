#!/usr/bin/env bash
# refactor_diff.sh — byte-identity gate for codegen-preserving refactors
# (P4 phase 0: https://github.com/arch-hdl-lang/arch-com internal plan).
#
# Usage:
#   scripts/refactor_diff.sh <base-ref> [options]
#
# Options:
#   --jobs N        Parallelism for the regression sweep (default: 4).
#   --work-dir DIR  Scratch directory (default: fresh mktemp dir).
#   --keep          Do not delete the scratch directory / base-ref worktree
#                   on exit (useful for inspecting a FAIL by hand).
#
# What it does:
#   1. Builds the `arch` compiler at <base-ref> in an isolated `git worktree`
#      checkout with its own --target-dir (never touches this checkout's
#      target/), and at HEAD via a normal `cargo build --release` in this
#      worktree — see CLAUDE.md's "Never invoke a stale compiler binary".
#   2. Runs BOTH backends' codegen — `arch build` (SystemVerilog + .archi
#      interface files) and the `arch sim --outdir` C++ model generation
#      (codegen only; no testbench is ever linked or executed, so this stays
#      fast and has no runtime nondeterminism to worry about) — with EACH
#      binary, over:
#        - every unit listed in tests/arch_regression_baseline.json, reusing
#          tools/run_arch_regression.py's own unit discovery/baseline
#          plumbing (the same mechanism scripts/nightly_equivalence.py
#          drives for the nightly sweep) — currently ~185 units;
#        - every `ck build`/`ck sim`/`ck check` fixture declared in
#          tests/backend_equiv/run.sh (parsed straight out of that script,
#          so it never drifts from the fixture list maintainers actually
#          curate there).
#   3. Byte-diffs every emitted source file (*.sv *.archi *.h *.hpp *.cpp
#      *.cc) between the base-ref run and the HEAD run. A file emitted by
#      only one side counts as a diff too (e.g. something that used to
#      build/sim now silently fails, or vice versa).
#
# PASS = zero diffs anywhere. Any diff is printed (unified, capped per file)
# and the script exits non-zero.
#
# Why the base-ref and HEAD passes share one working tree of copied test
# sources (same absolute paths both times): so that if emitted output ever
# started embedding a source path (it does not today, but nothing guarantees
# that stays true), a path *location* difference between the two checkouts
# could never masquerade as a codegen diff. Only the compiler binary varies
# between the two passes; the input files sit at identical paths both times.
#
# Determinism assumption: emitted output is assumed deterministic (no
# timestamps, no HashMap-iteration-order leakage, etc). If this script ever
# reports a diff that looks like nondeterminism — e.g. running the SAME
# binary twice over the SAME inputs disagrees with itself — STOP and report
# it as a finding; do not silence it by sorting file *contents*. Sorting is
# only used here for filesystem enumeration order (`find | sort`), never for
# the bytes inside a file.
#
# Exit status: 0 = PASS (zero diffs). 1 = diffs found. 2 = usage/setup error.
#
# Validating the harness itself: run with base-ref=HEAD first (must be a
# trivial PASS), then hand-perturb one emitter's output locally (do NOT
# commit the perturbation) and confirm the script reports a diff, then
# revert. See PR description for arch-com#refactor-diff-harness for a
# recorded example of both runs.

set -uo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "refactor_diff: not inside a git repo" >&2
  exit 2
}
cd "$ROOT"

JOBS=4
WORK_DIR=""
KEEP=0
BASE_REF=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --jobs)
      JOBS="$2"; shift 2 ;;
    --work-dir)
      WORK_DIR="$2"; shift 2 ;;
    --keep)
      KEEP=1; shift ;;
    -h|--help)
      sed -n '2,40p' "$0"; exit 0 ;;
    -*)
      echo "refactor_diff: unknown option: $1" >&2; exit 2 ;;
    *)
      if [[ -n "$BASE_REF" ]]; then
        echo "refactor_diff: unexpected extra argument: $1" >&2; exit 2
      fi
      BASE_REF="$1"; shift ;;
  esac
done

if [[ -z "$BASE_REF" ]]; then
  echo "usage: scripts/refactor_diff.sh <base-ref> [--jobs N] [--work-dir DIR] [--keep]" >&2
  exit 2
fi

BASE_SHA="$(git rev-parse --verify "${BASE_REF}^{commit}" 2>/dev/null)" || {
  echo "refactor_diff: bad base-ref '$BASE_REF' (not a valid git commit-ish)" >&2
  exit 2
}
HEAD_SHA="$(git rev-parse --verify HEAD)"

if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/refactor_diff.XXXXXX")"
fi
mkdir -p "$WORK_DIR"
BASE_WORKTREE="$WORK_DIR/base_worktree"
OUT_BASE="$WORK_DIR/out_base"
OUT_HEAD="$WORK_DIR/out_head"
SCRATCH_BE="$WORK_DIR/backend_equiv_src"
BASELINE="$ROOT/tests/arch_regression_baseline.json"
REGRESSION_WORK="$WORK_DIR/regression"

worktree_added=0
cleanup() {
  if [[ "$KEEP" -eq 1 ]]; then
    echo "refactor_diff: --keep set, leaving scratch dir: $WORK_DIR" >&2
    return
  fi
  if [[ "$worktree_added" -eq 1 ]]; then
    git worktree remove --force "$BASE_WORKTREE" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

echo "refactor_diff: base-ref=$BASE_REF ($BASE_SHA)"
echo "refactor_diff: head=$HEAD_SHA"
echo "refactor_diff: work-dir=$WORK_DIR"

# ---------------------------------------------------------------------------
# 1. Build both binaries.
# ---------------------------------------------------------------------------

echo "refactor_diff: [1/3] building base-ref binary in an isolated worktree..."
git worktree add --detach --quiet "$BASE_WORKTREE" "$BASE_SHA"
worktree_added=1
if ! cargo build --release \
      --manifest-path "$BASE_WORKTREE/Cargo.toml" \
      --target-dir "$BASE_WORKTREE/target" \
      >"$WORK_DIR/build_base.log" 2>&1; then
  echo "refactor_diff: base-ref build FAILED — see $WORK_DIR/build_base.log" >&2
  tail -n 40 "$WORK_DIR/build_base.log" >&2
  exit 2
fi
BASE_BIN="$BASE_WORKTREE/target/release/arch"

echo "refactor_diff: [1/3] building HEAD binary (fresh, in this worktree)..."
if ! cargo build --release >"$WORK_DIR/build_head.log" 2>&1; then
  echo "refactor_diff: HEAD build FAILED — see $WORK_DIR/build_head.log" >&2
  tail -n 40 "$WORK_DIR/build_head.log" >&2
  exit 2
fi
HEAD_BIN="$ROOT/target/release/arch"

[[ -x "$BASE_BIN" ]] || { echo "refactor_diff: base binary missing at $BASE_BIN" >&2; exit 2; }
[[ -x "$HEAD_BIN" ]] || { echo "refactor_diff: head binary missing at $HEAD_BIN" >&2; exit 2; }

# ---------------------------------------------------------------------------
# 2. Generate codegen for both binaries.
#
# gen_all() drives one pass (one binary) over both corpora. The baseline
# corpus reuses tools/run_arch_regression.py's own unit discovery against a
# fixed --work-dir ($REGRESSION_WORK); running both passes against that same
# dir means the copied .arch sources (and hence any embedded path, if that
# ever becomes a thing) sit at identical absolute paths across both passes.
# --sim-manifest points at a path that does not exist, so every unit takes
# the plain (non-TB) `arch sim --outdir` codegen path — no testbench is ever
# compiled or executed here, only the generated C++ model files are compared.
# ---------------------------------------------------------------------------

gen_all() {
  local arch_bin="$1" out_dir="$2"
  mkdir -p "$out_dir/units" "$out_dir/backend_equiv"

  echo "refactor_diff:   baseline corpus via run_arch_regression.py ($arch_bin)"
  python3 "$ROOT/tools/run_arch_regression.py" \
    --arch-bin "$arch_bin" \
    --work-dir "$REGRESSION_WORK" \
    --baseline "$BASELINE" \
    --sim-manifest "$WORK_DIR/__no_such_manifest.json" \
    --skip-verilator \
    --skip-sim-compile \
    --jobs "$JOBS" \
    --allow-failures \
    >"$out_dir/regression_run.log" 2>&1
  cp "$REGRESSION_WORK/summary.json" "$out_dir/regression_summary.json" 2>/dev/null || true
  cp -R "$REGRESSION_WORK/units/." "$out_dir/units/" 2>/dev/null || true

  echo "refactor_diff:   backend_equiv fixtures ($arch_bin)"
  rm -rf "$SCRATCH_BE"
  cp -R "$ROOT/tests/backend_equiv" "$SCRATCH_BE"

  local idx=0 line kind label top_stem
  while IFS= read -r line; do
    line="${line%%#*}"
    [[ "$line" =~ ^ck[[:space:]] ]] || continue
    # shellcheck disable=SC2206
    local toks=($line)
    kind="${toks[1]:-}"
    local files=("${toks[@]:2}")
    (( ${#files[@]} == 0 )) && continue
    idx=$((idx + 1))
    top_stem="$(basename "${files[-1]}" .arch)"
    label="$(printf '%02d_%s_%s' "$idx" "$kind" "$top_stem")"
    local abs_files=()
    for f in "${files[@]}"; do abs_files+=("$SCRATCH_BE/$f"); done
    local unit_dir="$out_dir/backend_equiv/$label"
    mkdir -p "$unit_dir"
    "$arch_bin" build "${abs_files[@]}" -o "$unit_dir/$label.sv" \
      >"$unit_dir/build.stdout" 2>"$unit_dir/build.stderr"
    "$arch_bin" sim "${abs_files[@]}" --outdir "$unit_dir/sim" \
      >"$unit_dir/sim.stdout" 2>"$unit_dir/sim.stderr"
  done <"$ROOT/tests/backend_equiv/run.sh"
}

echo "refactor_diff: [2/3] generating codegen — base-ref pass"
gen_all "$BASE_BIN" "$OUT_BASE"

echo "refactor_diff: [2/3] generating codegen — HEAD pass"
gen_all "$HEAD_BIN" "$OUT_HEAD"

# ---------------------------------------------------------------------------
# 3. Byte-diff every emitted source file.
# ---------------------------------------------------------------------------

echo "refactor_diff: [3/3] byte-diffing emitted sources..."

CODEGEN_EXTS=(-name '*.sv' -o -name '*.archi' -o -name '*.h' -o -name '*.hpp' -o -name '*.cpp' -o -name '*.cc')

(cd "$OUT_BASE" && find . -type f \( "${CODEGEN_EXTS[@]}" \)) | sort >"$WORK_DIR/files_base.txt"
(cd "$OUT_HEAD" && find . -type f \( "${CODEGEN_EXTS[@]}" \)) | sort >"$WORK_DIR/files_head.txt"

only_base="$(comm -23 "$WORK_DIR/files_base.txt" "$WORK_DIR/files_head.txt")"
only_head="$(comm -13 "$WORK_DIR/files_base.txt" "$WORK_DIR/files_head.txt")"
common="$(comm -12 "$WORK_DIR/files_base.txt" "$WORK_DIR/files_head.txt")"

mismatches=0

if [[ -n "$only_base" ]]; then
  mismatches=1
  echo
  echo "## Emitted at base-ref ($BASE_REF) but MISSING at HEAD:"
  echo "$only_base" | sed 's/^/  - /'
fi

if [[ -n "$only_head" ]]; then
  mismatches=1
  echo
  echo "## Emitted at HEAD but MISSING at base-ref ($BASE_REF):"
  echo "$only_head" | sed 's/^/  - /'
fi

diff_count=0
common_count=0
if [[ -n "$common" ]]; then
  while IFS= read -r rel; do
    [[ -z "$rel" ]] && continue
    common_count=$((common_count + 1))
    if ! cmp -s "$OUT_BASE/$rel" "$OUT_HEAD/$rel"; then
      diff_count=$((diff_count + 1))
      mismatches=1
      echo
      echo "## DIFF: $rel"
      diff -u "$OUT_BASE/$rel" "$OUT_HEAD/$rel" | head -n 60
    fi
  done <<<"$common"
fi

echo
echo "refactor_diff: compared $common_count common file(s); $diff_count byte-differ; $(echo "$only_base" | grep -c .) base-only; $(echo "$only_head" | grep -c .) head-only."

if [[ "$mismatches" -eq 0 ]]; then
  echo "refactor_diff: PASS — zero diffs between $BASE_REF and HEAD ($HEAD_SHA)"
  exit 0
else
  echo "refactor_diff: FAIL — codegen differs between $BASE_REF and HEAD ($HEAD_SHA)" >&2
  exit 1
fi
