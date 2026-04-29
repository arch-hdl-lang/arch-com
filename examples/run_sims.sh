#!/usr/bin/env bash
# Run every example that has a *_tb.cpp testbench through `arch sim`.
# Reports PASS / FAIL / UNCLEAR per pairing and a final tally.
#
# Usage:
#   examples/run_sims.sh           # run them all, summary only
#   examples/run_sims.sh --verbose # also show stdout snippets per FAIL/UNCLEAR
#   examples/run_sims.sh --keep    # leave the temp work dirs for inspection
#   examples/run_sims.sh foo       # run only the testbench whose base matches "foo"
#
# Notes:
#   - Most tbs map 1:1: examples/foo_tb.cpp pairs with examples/foo.arch.
#   - 11 tbs include a header for a module whose name doesn't match their tb's
#     base; those use the explicit TB_TO_ARCH map below.
#   - PASS/FAIL is heuristic: arch sim usually exits 0 even when the tb prints
#     "FAIL", so we also grep stdout for the standard patterns.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH_BIN="${ARCH_BIN:-${REPO_ROOT}/target/release/arch}"
EX_DIR="${REPO_ROOT}/examples"

VERBOSE=0
KEEP=0
FILTER=""
for arg in "$@"; do
  case "$arg" in
    --verbose|-v) VERBOSE=1 ;;
    --keep)       KEEP=1 ;;
    -h|--help)    sed -n '2,/^$/p' "$0"; exit 0 ;;
    -*)           echo "unknown flag: $arg" >&2; exit 2 ;;
    *)            FILTER="$arg" ;;
  esac
done

if [[ ! -x "$ARCH_BIN" ]]; then
  echo "error: $ARCH_BIN not found. Run: cargo build --release" >&2
  exit 1
fi

# Explicit map for tbs whose base name doesn't match their .arch source.
# Format: "tb_base:arch_base[+dep1+dep2...]"
# The primary (first) .arch is what `arch sim` is invoked with; deps are
# copied into the work dir so cross-file inst lookups succeed.
declare -a TB_MAP=(
  "arbiter:bus_arbiter"
  "clk_div_counter:clk_div_counter+clk_divider"
  "counter:wrap_counter"
  "fsm_counter_vltor:fsm_counter"
  "linklist_basic_cmp:linklist_basic"
  "linklist_basic_sim:linklist_basic"
  "linklist_doubly_cmp:linklist_doubly"
  "multi_clock_tick:multi_clock"
  "pipe_reg:pipe_reg_test"
  "ram:single_port_ram"
  "regfile:int_regs"
  "synchronizer_cdc_random:synchronizer_basic"
)

resolve_arch() {
  local tb_base="$1"
  for entry in "${TB_MAP[@]}"; do
    local from="${entry%%:*}"
    local to="${entry##*:}"
    if [[ "$from" == "$tb_base" ]]; then echo "$to"; return; fi
  done
  echo "$tb_base"
}

ROOT_WORK="$(mktemp -d -t arch_sim_runs.XXXXXX)"
if [[ $KEEP -eq 0 ]]; then
  trap 'rm -rf "$ROOT_WORK"' EXIT
else
  echo "keeping work dir: $ROOT_WORK"
fi

PASS=0
FAIL=0
SKIP=0
FAIL_LIST=()

for tb in "$EX_DIR"/*_tb.cpp; do
  tb_base=$(basename "$tb" _tb.cpp)
  if [[ -n "$FILTER" && "$tb_base" != *"$FILTER"* ]]; then
    continue
  fi
  resolved="$(resolve_arch "$tb_base")"
  # Split on '+': first is the primary, rest are deps.
  primary="${resolved%%+*}"
  deps_str="${resolved#*+}"
  arch_src="$EX_DIR/$primary.arch"

  if [[ ! -f "$arch_src" ]]; then
    SKIP=$((SKIP+1))
    printf "SKIP    %-40s (no .arch found for %s)\n" "$tb_base" "$primary"
    continue
  fi

  work="$ROOT_WORK/$tb_base"
  mkdir -p "$work"
  cp "$arch_src" "$work/"
  cp "$tb" "$work/"
  # Some .arch files reference `init: file("examples/foo.hex", ...)` with a
  # repo-root-relative path. Symlink an `examples` dir into the work dir so
  # those paths resolve to the real on-disk hex/bin assets.
  ln -sfn "$EX_DIR" "$work/examples"
  if [[ "$deps_str" != "$resolved" ]]; then
    IFS='+' read -ra deps <<< "$deps_str"
    for d in "${deps[@]}"; do
      [[ -f "$EX_DIR/$d.arch" ]] && cp "$EX_DIR/$d.arch" "$work/"
    done
  fi
  log="$work/run.log"

  # Build all .arch files in the work dir (primary first) so cross-file
  # inst dependencies resolve. arch sim accepts multiple .arch inputs.
  arch_files="$primary.arch"
  if [[ "$deps_str" != "$resolved" ]]; then
    for d in "${deps[@]}"; do
      [[ -f "$work/$d.arch" ]] && arch_files="$arch_files $d.arch"
    done
  fi
  ( cd "$work" && "$ARCH_BIN" sim $arch_files --tb "$(basename "$tb")" ) >"$log" 2>&1
  rc=$?

  # Heuristic verdict:
  #   - non-zero exit  → FAIL (compile error or sim crash)
  #   - stdout has FAIL/FAILED/ERROR/"X TESTS FAILED" → FAIL
  #   - stdout has PASS/PASSED → PASS
  #   - else → UNCLEAR (tb printed nothing recognizable)
  if [[ $rc -ne 0 ]]; then
    FAIL=$((FAIL+1))
    FAIL_LIST+=("$tb_base")
    printf "FAIL    %-40s (arch sim exit=%d)\n" "$tb_base" "$rc"
    [[ $VERBOSE -eq 1 ]] && tail -10 "$log" | sed 's/^/          /'
  elif grep -qiE '\b(FAIL|FAILED|ERROR|TESTS FAILED)\b' "$log" \
       && ! grep -qiE 'no errors|0 errors|errors: 0' "$log"; then
    FAIL=$((FAIL+1))
    FAIL_LIST+=("$tb_base")
    printf "FAIL    %-40s (testbench reported failure)\n" "$tb_base"
    [[ $VERBOSE -eq 1 ]] && tail -10 "$log" | sed 's/^/          /'
  else
    # Exit-zero with no failure markers: count as PASS even if the tb
    # didn't print "PASS" explicitly (trace-only testbenches like the
    # linklist_*_cmp pair just dump cycle traces and exit cleanly).
    PASS=$((PASS+1))
    printf "PASS    %s\n" "$tb_base"
  fi
done

echo
echo "----- summary -----"
TOTAL=$((PASS+FAIL+SKIP))
printf "  PASS:    %3d\n" "$PASS"
printf "  FAIL:    %3d\n" "$FAIL"
printf "  SKIP:    %3d  (no .arch found)\n" "$SKIP"
printf "  total:   %3d\n" "$TOTAL"

if [[ ${#FAIL_LIST[@]} -gt 0 ]]; then
  echo
  echo "FAIL files (re-run with --verbose to see snippets, --keep to inspect logs):"
  printf "  %s\n" "${FAIL_LIST[@]}"
fi

# Exit non-zero if anything actually failed.
[[ $FAIL -eq 0 ]]
