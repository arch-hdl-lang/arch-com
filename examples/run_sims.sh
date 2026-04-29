#!/usr/bin/env bash
# Run every example that has a *_tb.cpp testbench through `arch sim`.
# Reports PASS / FAIL per pairing and a final tally.
#
# Usage:
#   examples/run_sims.sh            # run them all sequentially
#   examples/run_sims.sh -j8        # 8-way parallel (use $(nproc) on CI)
#   examples/run_sims.sh --verbose  # also show stdout snippets per FAIL
#   examples/run_sims.sh --keep     # leave the temp work dirs for inspection
#   examples/run_sims.sh foo        # only the testbench whose base matches "foo"
#
# Notes:
#   - Most tbs map 1:1: examples/foo_tb.cpp pairs with examples/foo.arch.
#   - 11 tbs include a header for a module whose name doesn't match their tb's
#     base; those use the explicit TB_MAP below.
#   - PASS/FAIL is heuristic: arch sim usually exits 0 even when the tb prints
#     "FAIL", so we also grep stdout for the standard patterns.
#   - With -jN > 1, per-test runs are independent (each gets its own work dir);
#     output is gathered after all workers finish, so the order of the per-test
#     PASS/FAIL lines reflects the original test list, not completion order.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH_BIN="${ARCH_BIN:-${REPO_ROOT}/target/release/arch}"
EX_DIR="${REPO_ROOT}/examples"

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

# ── worker mode ─────────────────────────────────────────────────────────────
# Invoked as: ARCH_BIN=… EX_DIR=… ROOT_WORK=… examples/run_sims.sh --worker <tb_base>
# Stages the test, runs arch sim, and writes a status line to
# "$ROOT_WORK/$tb_base/.status" of the form "VERDICT|tb_base|detail".
# VERDICT is one of PASS, FAIL, SKIP. The main process reads these.
if [[ "${1:-}" == "--worker" ]]; then
  tb_base="$2"
  resolved="$(resolve_arch "$tb_base")"
  primary="${resolved%%+*}"
  deps_str="${resolved#*+}"
  arch_src="$EX_DIR/$primary.arch"
  work="$ROOT_WORK/$tb_base"
  mkdir -p "$work"

  if [[ ! -f "$arch_src" ]]; then
    echo "SKIP|$tb_base|no .arch found for $primary" > "$work/.status"
    exit 0
  fi

  cp "$arch_src" "$work/"
  cp "$EX_DIR/${tb_base}_tb.cpp" "$work/"
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

  arch_files="$primary.arch"
  if [[ "$deps_str" != "$resolved" ]]; then
    for d in "${deps[@]}"; do
      [[ -f "$work/$d.arch" ]] && arch_files="$arch_files $d.arch"
    done
  fi
  ( cd "$work" && "$ARCH_BIN" sim $arch_files --tb "${tb_base}_tb.cpp" ) >"$log" 2>&1
  rc=$?

  if [[ $rc -ne 0 ]]; then
    echo "FAIL|$tb_base|arch sim exit=$rc" > "$work/.status"
  elif grep -qiE '\b(FAIL|FAILED|ERROR|TESTS FAILED)\b' "$log" \
       && ! grep -qiE 'no errors|0 errors|errors: 0' "$log"; then
    echo "FAIL|$tb_base|testbench reported failure" > "$work/.status"
  else
    echo "PASS|$tb_base|" > "$work/.status"
  fi
  exit 0
fi

# ── main mode ───────────────────────────────────────────────────────────────
VERBOSE=0
KEEP=0
JOBS=1
FILTER=""
for arg in "$@"; do
  case "$arg" in
    --verbose|-v) VERBOSE=1 ;;
    --keep)       KEEP=1 ;;
    -j)           JOBS=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4) ;;
    -j*)          JOBS="${arg#-j}" ;;
    -h|--help)    sed -n '2,/^$/p' "$0"; exit 0 ;;
    -*)           echo "unknown flag: $arg" >&2; exit 2 ;;
    *)            FILTER="$arg" ;;
  esac
done

if [[ ! -x "$ARCH_BIN" ]]; then
  echo "error: $ARCH_BIN not found. Run: cargo build --release" >&2
  exit 1
fi

# Explicit /tmp path so the GH workflow's `actions/upload-artifact` step can
# find work dirs at a stable location across Linux/macOS (macOS mktemp
# defaults to /var/folders/... if -t is used without an explicit prefix).
ROOT_WORK="$(mktemp -d /tmp/arch_sim_runs.XXXXXX)"
if [[ $KEEP -eq 0 ]]; then
  trap 'rm -rf "$ROOT_WORK"' EXIT
else
  echo "keeping work dir: $ROOT_WORK"
fi

# Build the list of testbench bases (preserves on-disk order so output is stable).
tb_list=()
for tb in "$EX_DIR"/*_tb.cpp; do
  base=$(basename "$tb" _tb.cpp)
  if [[ -n "$FILTER" && "$base" != *"$FILTER"* ]]; then
    continue
  fi
  tb_list+=("$base")
done

# Fan out workers via xargs -P. Each worker writes a per-test status file.
# We export the env vars the worker needs.
export ARCH_BIN EX_DIR ROOT_WORK
printf '%s\n' "${tb_list[@]}" | xargs -P "$JOBS" -I{} bash "$0" --worker {}

# Aggregate in tb_list order so output is deterministic regardless of -j.
PASS=0
FAIL=0
SKIP=0
FAIL_LIST=()
for tb_base in "${tb_list[@]}"; do
  status_file="$ROOT_WORK/$tb_base/.status"
  if [[ ! -f "$status_file" ]]; then
    FAIL=$((FAIL+1))
    FAIL_LIST+=("$tb_base")
    printf "FAIL    %-40s (worker produced no status file)\n" "$tb_base"
    continue
  fi
  IFS='|' read -r verdict tb detail < "$status_file"
  case "$verdict" in
    PASS)
      PASS=$((PASS+1))
      printf "PASS    %s\n" "$tb_base"
      ;;
    FAIL)
      FAIL=$((FAIL+1))
      FAIL_LIST+=("$tb_base")
      printf "FAIL    %-40s (%s)\n" "$tb_base" "$detail"
      [[ $VERBOSE -eq 1 ]] && tail -10 "$ROOT_WORK/$tb_base/run.log" 2>/dev/null | sed 's/^/          /'
      ;;
    SKIP)
      SKIP=$((SKIP+1))
      printf "SKIP    %-40s (%s)\n" "$tb_base" "$detail"
      ;;
  esac
done

echo
echo "----- summary -----"
TOTAL=$((PASS+FAIL+SKIP))
printf "  PASS:    %3d\n" "$PASS"
printf "  FAIL:    %3d\n" "$FAIL"
printf "  SKIP:    %3d  (no .arch found)\n" "$SKIP"
printf "  total:   %3d\n" "$TOTAL"
[[ $JOBS -gt 1 ]] && printf "  jobs:    %3d  (parallel workers)\n" "$JOBS"

if [[ ${#FAIL_LIST[@]} -gt 0 ]]; then
  echo
  echo "FAIL files (re-run with --verbose to see snippets, --keep to inspect logs):"
  printf "  %s\n" "${FAIL_LIST[@]}"
fi

# Exit non-zero if anything actually failed.
[[ $FAIL -eq 0 ]]
