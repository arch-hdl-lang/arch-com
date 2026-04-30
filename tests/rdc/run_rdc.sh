#!/usr/bin/env bash
# Run `arch check` against every scenario in this directory and verify
# the outcome matches the filename suffix:
#
#   *_ok.arch    → expected to type-check cleanly
#   *_fail.arch  → expected to be rejected with an RDC error
#
# Phase 1 of the RDC checker (currently shipped) handles the cross-clock
# async-reset case. Phase 2a (data-path reach analysis) is not yet
# implemented; the *_fail.arch scenarios that depend on phase 2a are
# listed in PHASE_2A_PENDING below and reported as XFAIL until that
# implementation lands.
#
# Usage:
#   tests/rdc/run_rdc.sh           # run all
#   tests/rdc/run_rdc.sh foo       # filter by substring

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && cd .. && pwd)"
ARCH_BIN="${ARCH_BIN:-${REPO_ROOT}/target/release/arch}"
THIS_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ ! -x "$ARCH_BIN" ]]; then
  echo "error: $ARCH_BIN not found. Run: cargo build --release" >&2
  exit 1
fi

# Scenarios whose violation depends on phase-2a data-path analysis.
# Until that lands, `arch check` won't reject them; treat as XFAIL so
# the script reports useful status without going red.
PHASE_2A_PENDING=(
  rdc_a2_diff_async_direct_fail
  rdc_b1_async_none_async_diff_fail
  rdc_b3_async_sync_async_diff_fail
  rdc_c1_two_async_converge_at_none_fail
  rdc_d2_diff_async_diff_clocks_with_path_fail
  rdc_e2_mutual_feedback_diff_domains_fail
)

is_phase2a_pending() {
  local name="$1"
  for p in "${PHASE_2A_PENDING[@]}"; do
    [[ "$p" == "$name" ]] && return 0
  done
  return 1
}

FILTER="${1:-}"
PASS=0
FAIL=0
XFAIL=0
FAIL_LIST=()

for f in "$THIS_DIR"/*.arch; do
  name="$(basename "$f" .arch)"
  if [[ -n "$FILTER" && "$name" != *"$FILTER"* ]]; then
    continue
  fi

  expected_outcome="${name##*_}"   # last token: "ok" or "fail"

  # Run `arch check`. Capture exit code only; stdout/stderr discarded.
  if "$ARCH_BIN" check "$f" >/dev/null 2>&1; then
    rc=0
  else
    rc=1
  fi

  case "$expected_outcome:$rc" in
    ok:0)
      PASS=$((PASS+1))
      printf "PASS    %s\n" "$name"
      ;;
    ok:1)
      FAIL=$((FAIL+1))
      FAIL_LIST+=("$name")
      printf "FAIL    %-60s (expected pass, got error)\n" "$name"
      ;;
    fail:0)
      if is_phase2a_pending "$name"; then
        XFAIL=$((XFAIL+1))
        printf "XFAIL   %-60s (phase 2a impl pending)\n" "$name"
      else
        FAIL=$((FAIL+1))
        FAIL_LIST+=("$name")
        printf "FAIL    %-60s (expected violation, got pass)\n" "$name"
      fi
      ;;
    fail:1)
      PASS=$((PASS+1))
      printf "PASS    %s\n" "$name"
      ;;
  esac
done

echo
echo "----- summary -----"
TOTAL=$((PASS+FAIL+XFAIL))
printf "  PASS:   %3d\n" "$PASS"
printf "  XFAIL:  %3d  (phase-2a pending)\n" "$XFAIL"
printf "  FAIL:   %3d  (unexpected)\n" "$FAIL"
printf "  total:  %3d\n" "$TOTAL"

if [[ ${#FAIL_LIST[@]} -gt 0 ]]; then
  echo
  echo "FAIL files:"
  printf "  %s\n" "${FAIL_LIST[@]}"
fi

[[ $FAIL -eq 0 ]]
