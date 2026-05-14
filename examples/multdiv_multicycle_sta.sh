#!/usr/bin/env bash
# Run OpenSTA over the three configurations of the multdiv comparison
# and report WNS/TNS for each at a reasonable target clock. Plus a
# sweep to bracket each design's Fmax.
#
# Prerequisites:
#   - bash examples/multdiv_multicycle_synth.sh  (produces the
#     post-synth gate-level netlists under /tmp/multdiv-synth/)
#   - OpenSTA installed (sta binary on PATH, or STA env var pointing
#     at it)
#
# Output goes to stdout and to /tmp/multdiv-synth/*.sta.log.

set -euo pipefail

OUT_DIR="${OUT_DIR:-/tmp/multdiv-synth}"
STA="${STA:-$HOME/OpenSTA/build/sta}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TCL="$SCRIPT_DIR/multdiv_multicycle_sta.tcl"

if [[ ! -x "$STA" ]]; then
  echo "ERROR: OpenSTA at $STA not found (set STA env var)" >&2
  exit 1
fi

run_one() {
  local design=$1
  local clk=$2
  local out="$OUT_DIR/sta_${design}_p${clk}.log"
  echo "=== DESIGN=$design CLOCK_NS=$clk ==="
  DESIGN=$design CLOCK_NS=$clk OUT_DIR=$OUT_DIR \
    "$STA" -no_splash -exit "$TCL" 2>&1 | tee "$out" \
      | grep -E "wns max|tns max" || true
}

# Headline runs.
run_one fsm           12.0
run_one mul_nosdc     200.0
run_one mul_with_mc   4.0

# Fmax bracket — find the period at which WNS just turns negative.
echo "=== Fmax sweep: ibex_multdiv_fast FSM ==="
for P in 13.0 12.0 11.5 11.4 11.3; do
  echo -n "  P=${P}ns "
  DESIGN=fsm CLOCK_NS=$P OUT_DIR=$OUT_DIR \
    "$STA" -no_splash -exit "$TCL" 2>&1 | grep "wns max" | head -1
done

echo "=== Fmax sweep: MultdivMulticycle (no SDC, worst case) ==="
for P in 150.0 140.0 137.0 136.0; do
  echo -n "  P=${P}ns "
  DESIGN=mul_nosdc CLOCK_NS=$P OUT_DIR=$OUT_DIR \
    "$STA" -no_splash -exit "$TCL" 2>&1 | grep "wns max" | head -1
done

echo "=== Fmax sweep: MultdivMulticycle (multicycle paths applied) ==="
for P in 4.0 3.85 3.80 3.79 3.78 3.50; do
  echo -n "  P=${P}ns "
  DESIGN=mul_with_mc CLOCK_NS=$P OUT_DIR=$OUT_DIR \
    "$STA" -no_splash -exit "$TCL" 2>&1 | grep "wns max" | head -1
done
