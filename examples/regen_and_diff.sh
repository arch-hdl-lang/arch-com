#!/usr/bin/env bash
# Regenerate every examples/*.arch into SystemVerilog and diff against the
# committed examples/*.sv. Useful for spotting regressions in arch build.
#
# Usage:
#   examples/regen_and_diff.sh            # summary only
#   examples/regen_and_diff.sh --verbose  # also show the first 40 diff lines per mismatch
#   examples/regen_and_diff.sh --keep     # leave the temp work dir for inspection
#
# Notes:
#   - Builds run inside a temp dir, so examples/ is not polluted with .archi
#     side-effect files.
#   - Some diffs are expected, not refactor regressions: committed .sv files
#     predate the doc-comment harvester (//!, ///) and FIFO auto-assertions.
#     Inspect the diff to tell drift apart from regression.
#   - cdc_inst_violation.arch is intentionally a build-failure example (no .sv);
#     it's tracked as XFAIL.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH_BIN="${REPO_ROOT}/target/release/arch"
EX_DIR="${REPO_ROOT}/examples"

VERBOSE=0
KEEP=0
for arg in "$@"; do
  case "$arg" in
    --verbose|-v) VERBOSE=1 ;;
    --keep)       KEEP=1 ;;
    -h|--help)    sed -n '2,/^$/p' "$0"; exit 0 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

if [[ ! -x "$ARCH_BIN" ]]; then
  echo "error: $ARCH_BIN not found. Run: cargo build --release" >&2
  exit 1
fi

WORK="$(mktemp -d -t arch_regen.XXXXXX)"
if [[ $KEEP -eq 0 ]]; then
  trap 'rm -rf "$WORK"' EXIT
else
  echo "keeping work dir: $WORK"
fi

# Copy every .arch from examples/ (top-level only, not subdirs).
# We do NOT copy committed .archi files — many are stale or conflict with
# inline definitions in the .arch files (e.g. examples/DmaRegs.archi vs
# regfile DmaRegs inside dma_engine.arch). Files that depend on sibling
# .arch modules will be reported as BUILD_FAIL with "undefined module";
# that's a known limitation of single-file building, not a refactor regression.
cp "$EX_DIR"/*.arch "$WORK"/ 2>/dev/null

# Known pre-existing data issues (not caused by refactors):
#   clk_div_counter         needs cross-file inst from clk_divider.arch
#   dma_engine              examples/DmaRegs.archi duplicates inline regfile
#   pkt_queue               examples/TaskQueue.archi uses removed `track_tail;` syntax
#   synchronizer_handshake  uses `kind handshake;` but `handshake` is reserved
# These show up as BUILD_FAIL; treat them as expected until the source is fixed.
KNOWN_BAD=( clk_div_counter dma_engine pkt_queue synchronizer_handshake )
is_known_bad() {
  local b="$1"
  for k in "${KNOWN_BAD[@]}"; do [[ "$k" == "$b" ]] && return 0; done
  return 1
}

PASS=0
DIFF=0
NO_SV=0
XFAIL=0
KNOWN_FAIL=0
BUILD_FAIL=0
DIFF_LIST=()
FAIL_LIST=()

for arch_src in "$EX_DIR"/*.arch; do
  base="$(basename "$arch_src" .arch)"
  ref_sv="$EX_DIR/$base.sv"
  gen_sv="$WORK/${base}.gen.sv"

  build_log="$WORK/${base}.log"
  ( cd "$WORK" && "$ARCH_BIN" build -o "$gen_sv" "${base}.arch" ) >"$build_log" 2>&1
  rc=$?

  if [[ $rc -ne 0 ]]; then
    if [[ ! -f "$ref_sv" ]]; then
      XFAIL=$((XFAIL+1))
      printf "XFAIL  %-40s (no .sv, build fails as expected)\n" "$base"
    elif is_known_bad "$base"; then
      KNOWN_FAIL=$((KNOWN_FAIL+1))
      printf "KNOWN  %-40s (known stale-source issue — see comment block in this script)\n" "$base"
    else
      BUILD_FAIL=$((BUILD_FAIL+1))
      FAIL_LIST+=("$base")
      printf "FAIL   %-40s (build error — see %s)\n" "$base" "$build_log"
      [[ $VERBOSE -eq 1 ]] && sed -n '1,20p' "$build_log" | sed 's/^/         /'
    fi
    continue
  fi

  if [[ ! -f "$ref_sv" ]]; then
    NO_SV=$((NO_SV+1))
    printf "NEW    %-40s (built OK, no committed .sv to diff)\n" "$base"
    continue
  fi

  if diff -q "$ref_sv" "$gen_sv" >/dev/null 2>&1; then
    PASS=$((PASS+1))
    [[ $VERBOSE -eq 1 ]] && printf "PASS   %s\n" "$base"
  else
    DIFF=$((DIFF+1))
    DIFF_LIST+=("$base")
    added=$(diff "$ref_sv" "$gen_sv" | grep -c '^>')
    removed=$(diff "$ref_sv" "$gen_sv" | grep -c '^<')
    printf "DIFF   %-40s (+%d -%d lines)\n" "$base" "$added" "$removed"
    if [[ $VERBOSE -eq 1 ]]; then
      diff -u "$ref_sv" "$gen_sv" | sed -n '1,40p' | sed 's/^/         /'
    fi
  fi
done

echo
echo "----- summary -----"
TOTAL=$((PASS+DIFF+NO_SV+XFAIL+KNOWN_FAIL+BUILD_FAIL))
printf "  PASS:        %3d  (byte-identical to committed .sv)\n" "$PASS"
printf "  DIFF:        %3d  (built OK, differs from committed — usually expected drift)\n" "$DIFF"
printf "  NEW (no .sv):%3d  (built OK, no committed .sv yet)\n" "$NO_SV"
printf "  XFAIL:       %3d  (no .sv, build-fail expected)\n" "$XFAIL"
printf "  KNOWN_FAIL:  %3d  (known stale-source issue, NOT a refactor regression)\n" "$KNOWN_FAIL"
printf "  BUILD_FAIL:  %3d  (unexpected — investigate)\n" "$BUILD_FAIL"
printf "  total:       %3d\n" "$TOTAL"

if [[ ${#DIFF_LIST[@]} -gt 0 ]]; then
  echo
  echo "DIFF files (re-run with --verbose to see snippets):"
  printf "  %s\n" "${DIFF_LIST[@]}"
fi
if [[ ${#FAIL_LIST[@]} -gt 0 ]]; then
  echo
  echo "BUILD_FAIL files (unexpected — check the .log files):"
  printf "  %s\n" "${FAIL_LIST[@]}"
fi

# Exit non-zero only on unexpected build failures (DIFF is expected drift).
[[ $BUILD_FAIL -eq 0 ]]
