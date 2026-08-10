#!/usr/bin/env bash
# check_tb_registered.sh — every arch-sim testbench must be in the sim manifest.
#
# Usage:
#   scripts/check_tb_registered.sh          # check the whole tests/ tree
#
# Rationale (arch-hdl-lang/arch-com#799): a `*_tb.cpp` that no harness runs is
# invisible. It keeps compiling in a reviewer's head and nowhere else, so when
# the design it targets is rewritten the testbench silently stops matching and
# nobody notices — the e203 corpus accumulated 45 such testbenches, several of
# which had not compiled for months against fixtures that had been rewired out
# from under them. The invariant that stops the rot is blunt:
#
#   a *_tb.cpp that is not in tests/arch_sim_manifest.json should not exist.
#
# This script enforces exactly that. `tools/run_arch_regression.py` runs a
# testbench only when the sim manifest maps it to a unit, so "in the manifest"
# and "actually executed by CI" are the same thing.
#
# SCOPE — `*_tb.cpp` only, deliberately. The repo has a second, much larger
# naming convention, `tb_*.cpp` (tests/axi_dma*, tests/fp_v1, tests/l1d,
# tests/thread, ...), and ~97 of those are not in the sim manifest today. Many
# are plainly not regression units (tb_verilator.cpp, tb_wave.cpp, tb_debug.cpp,
# tb_perf_*.cpp), and sorting the rest is a separate piece of work from #799,
# which is about the e203 `*_tb.cpp` corpus. Turning the check on for `tb_*.cpp`
# now would mean either 97 red entries or a 97-line allowlist, and an allowlist
# that long is indistinguishable from no check at all. Widening SCAN_GLOB below
# is a one-line change once that corpus has been triaged.
#
# The one exemption is the explicit ALLOWLIST below: testbenches driven by a
# *different* harness, or targeting a design that does not currently compile.
# Every entry carries its reason inline. Keep this list short — adding a name
# here is a decision to let a testbench go unrun, and deserves the same
# scrutiny as skipping a test.
#
# Deliberately NOT used as an exemption rule: "the file includes verilated.h".
# That looks like a clean mechanical test for "this is a Verilator cross-check,
# not an arch-sim testbench", but it is wrong in this repo — tests/e203/
# e203_itcm_tb.cpp and tests/if_wait_for_in_then_tb.cpp both include
# verilated.h and both run fine under `arch sim --tb` (they are registered and
# passing). A content-based rule would therefore silently excuse a genuine
# arch-sim testbench that happened to include the header, which is exactly the
# invisibility this check exists to prevent. An explicit list is more typing
# and less clever, and it fails loudly when someone adds a testbench.
#
# Wired into CI as the `tb registered` check (.github/workflows/tb-registered.yml).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

MANIFEST="tests/arch_sim_manifest.json"

# Which testbenches this check governs. See the SCOPE note in the header before
# widening this to include 'tb_*.cpp'.
SCAN_GLOB='*_tb.cpp'

# Testbenches that are intentionally absent from the sim manifest.
# Format: "<path>|<reason>"
ALLOWLIST=(
  "tests/backend_equiv/Vsel_arch_tb.cpp|backend-equivalence pair, driven by tests/integration_test.rs (arch-sim half of an arch-vs-Verilator co-sim; the harness compiles both halves itself)"
  "tests/backend_equiv/VselWr_arch_tb.cpp|backend-equivalence pair, driven by tests/integration_test.rs"
  "tests/backend_equiv/VselThread_arch_tb.cpp|backend-equivalence pair, driven by tests/integration_test.rs"
  "tests/thread_sim_perf/named_thread_perf_tb.cpp|performance benchmark, not a pass/fail test — takes a cycle count on argv and reports wall time (see tests/thread_sim_perf/README.md)"
  "tests/thread_sim_perf/named_batch_tb.cpp|performance benchmark, not a pass/fail test"
  "tests/thread_sim_perf/threadmm2s_perf_tb.cpp|performance benchmark, not a pass/fail test"
  "tests/thread_sim_perf/threadmm2s_batch_tb.cpp|performance benchmark, not a pass/fail test"
  "tests/buf_mgr/buf_mgr_tb.cpp|tests/buf_mgr/buf_mgr.arch does not pass 'arch check' — it drives a sub-module Reset port from a combinational expression (RDC violation), so the testbench cannot be run until the fixture is fixed"
  "tests/backend_equiv/Vsel_vl_tb.cpp|Verilator half of the backend-equivalence co-sim pair above; compiled by Verilator, not by 'arch sim --tb'"
  "tests/backend_equiv/VselWr_vl_tb.cpp|Verilator half of the backend-equivalence co-sim pair above"
  "tests/backend_equiv/VselThread_vl_tb.cpp|Verilator half of the backend-equivalence co-sim pair above"
  "tests/buf_mgr/buf_mgr_vl_tb.cpp|Verilator cross-check for tests/buf_mgr, whose design does not pass 'arch check' either (see above)"
  "tests/aes/aes_cipher_top_tb.cpp|Verilator testbench for the AES NIST FIPS-197 vector; does not run under 'arch sim --tb' (exits non-zero), so registering it would only add a red unit"
)

allowlisted() {
  local path="$1" entry
  for entry in "${ALLOWLIST[@]}"; do
    [[ "${entry%%|*}" == "$path" ]] && return 0
  done
  return 1
}

if [[ ! -f "$MANIFEST" ]]; then
  echo "check_tb_registered: $MANIFEST not found" >&2
  exit 2
fi

# tb_files entries from the manifest, one per line.
registered="$(python3 -c '
import json, sys
data = json.load(open(sys.argv[1]))
for entry in data.get("entries", []):
    for tb in entry.get("tb_files", []):
        print(tb)
' "$MANIFEST" | sort -u)"

unregistered=()
while IFS= read -r tb; do
  [[ -z "$tb" ]] && continue
  grep -qxF "$tb" <<<"$registered" && continue
  allowlisted "$tb" && continue
  unregistered+=("$tb")
done < <(find tests -name "$SCAN_GLOB" -type f | sort)

# A manifest entry pointing at a file that no longer exists is the same rot in
# the other direction, and makes the regression run fail confusingly. This half
# is NOT scope-limited: every tb_files path in the manifest must exist,
# whichever naming convention it uses.
missing=()
while IFS= read -r tb; do
  [[ -z "$tb" ]] && continue
  [[ -f "$tb" ]] || missing+=("$tb")
done <<<"$registered"

status=0

if (( ${#unregistered[@]} > 0 )); then
  status=1
  echo "ERROR: ${#unregistered[@]} testbench(es) are not registered in $MANIFEST:" >&2
  printf '  %s\n' "${unregistered[@]}" >&2
  cat >&2 <<'HOWTO'

A *_tb.cpp that is not in the sim manifest is never run by
tools/run_arch_regression.py, so it rots silently (arch-hdl-lang/arch-com#799).

Fix by adding an entry to tests/arch_sim_manifest.json:

  {
    "name": "<dir>__<module>",
    "arch_files": ["tests/<dir>/<module>.arch", "<...every transitive dep...>"],
    "tb_files":   ["tests/<dir>/<module>_tb.cpp"]
  }

`name` must match the unit name tools/run_arch_regression.py discovers (the
.arch path with separators replaced by "__" and the suffix dropped, or the
directory name when the directory's group check passes). Verify with:

  ./target/release/arch sim <arch_files> --tb <tb_file> --outdir /tmp/tbcheck
  python3 tools/run_arch_regression.py --release --pattern '<dir>/*'

An entry may also carry "args" (extra `arch sim` flags) and "env" (environment
for that step only, e.g. {"ARCH_OPT": "-O0"} to skip link-time optimization on
a very large hierarchy).

If the testbench genuinely should not run under `arch sim` (it is a Verilator
cross-check, or belongs to another harness), add it to ALLOWLIST in
scripts/check_tb_registered.sh with a reason. Otherwise delete it — an unrun
testbench is worse than no testbench.
HOWTO
fi

if (( ${#missing[@]} > 0 )); then
  status=1
  echo "ERROR: ${#missing[@]} manifest tb_files entry/entries point at files that do not exist:" >&2
  printf '  %s\n' "${missing[@]}" >&2
  echo "Remove the stale entry from $MANIFEST, or restore the file." >&2
fi

if (( status == 0 )); then
  total="$(find tests -name "$SCAN_GLOB" -type f | wc -l | tr -d ' ')"
  echo "check_tb_registered: OK — all $total '$SCAN_GLOB' testbench(es) accounted for" \
       "($((total - ${#ALLOWLIST[@]})) registered in $MANIFEST, ${#ALLOWLIST[@]} allowlisted)."
fi

exit "$status"
