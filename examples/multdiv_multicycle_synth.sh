#!/usr/bin/env bash
# Synthesis comparison driver for multdiv_multicycle (this PR) vs
# ibex_multdiv_fast (the FSM design). Used to produce the numbers in
# doc/multdiv_multicycle_vs_fsm.md. Re-run anytime to regenerate.
#
# Requirements: yosys (any recent version), and EITHER the Sky130 PDK
# (~/.volare/sky130A/.../sky130_fd_sc_hd__tt_025C_1v80.lib) for
# Liberty-mapped numbers, OR fall back to generic-cell synth for
# cell-count-only.
#
# Outputs are written under $OUT_DIR (default /tmp/multdiv-synth).
#
# This script does not consume the multicycle SDC — Yosys + abc do not
# read SDC natively. The unretimed cell count + critical path it
# reports for the multicycle design is therefore a worst-case lower
# bound; commercial tools (DC, Genus) given the SDC could retime
# across the multicycle window.

set -euo pipefail

OUT_DIR="${OUT_DIR:-/tmp/multdiv-synth}"
ARCH_COM_DIR="${ARCH_COM_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ARCH_IBEX_DIR="${ARCH_IBEX_DIR:-$HOME/github/arch-ibex}"
LIB="${LIB:-$HOME/.volare/sky130A/libs.ref/sky130_fd_sc_hd/lib/sky130_fd_sc_hd__tt_025C_1v80.lib}"

mkdir -p "$OUT_DIR"

# 1. Emit the multicycle design (regenerates the SV + SDC).
"$ARCH_COM_DIR/target/release/arch" build \
  "$ARCH_COM_DIR/examples/multdiv_multicycle.arch" \
  -o "$OUT_DIR/multdiv_multicycle.sv"

# 2. Copy the FSM design from arch-ibex's build tree.
if [[ -f "$ARCH_IBEX_DIR/build/ibex_multdiv_fast.sv" ]]; then
  cp "$ARCH_IBEX_DIR/build/ibex_multdiv_fast.sv" "$OUT_DIR/"
else
  echo "WARN: $ARCH_IBEX_DIR/build/ibex_multdiv_fast.sv missing; build arch-ibex first." >&2
  exit 1
fi

# Yosys 0.64's built-in SV parser rejects some SV idioms (cast syntax,
# packed-array slicing) used by the arch-com SV emitter. Run sv2v on
# both inputs to get plain Verilog-2005 that yosys reliably accepts.
sv2v "$OUT_DIR/multdiv_multicycle.sv" > "$OUT_DIR/multdiv_multicycle.v"
sv2v "$OUT_DIR/ibex_multdiv_fast.sv"  > "$OUT_DIR/ibex_multdiv_fast.v"

# Yosys 0.64 also rejects function-call-followed-by-bit-select
# (`fn(...)[15:0]`) even in V2005 mode; the FSM thread emitter uses
# that pattern when consuming `MacRes_b_16_b_16_34`. Lift each call
# into a module-scope wire so the bit-select applies to a name, not a
# function call. Purely a tooling workaround; the design is unchanged.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$SCRIPT_DIR/multdiv_multicycle_lift_funcidx.py" \
  "$OUT_DIR/ibex_multdiv_fast.v" > "$OUT_DIR/ibex_multdiv_fast_fixed.v"

# Pick liberty flow if PDK present, else generic.
if [[ -f "$LIB" ]]; then
  LIBERTY_AVAILABLE=1
  echo "Using Sky130 lib: $LIB"
else
  LIBERTY_AVAILABLE=0
  echo "WARN: Liberty $LIB not found; running generic-cell synth (cell counts only, no ns timing)."
fi

# Synth one design. $1 = top, $2 = .sv file, $3 = stat-output file,
# $4 = optional `rename` to apply the wire->reg cell-rename TCL helper
# (so arch-com's emitted SDC `Module/<wire>_reg*` glob resolves
# against the post-synth netlist).
synth_one() {
  local top=$1
  local src=$2
  local stat_out=$3
  local rename=${4:-}
  echo "=== Synthesizing $top from $src ==="
  local rename_step=""
  if [[ "$rename" == "rename" ]]; then
    # splitnets converts bus-indexed Q nets (`mul_result[3]`) into
    # per-bit wires so the TCL pass can give each DFF cell a name
    # derived from its full Q net.
    rename_step="splitnets; tcl $SCRIPT_DIR/multdiv_multicycle_yosys_rename.tcl;"
  fi
  if [[ "$LIBERTY_AVAILABLE" == "1" ]]; then
    yosys -p "
      read_verilog $src
      hierarchy -check -top $top
      proc; opt
      fsm; opt
      memory; opt
      techmap; opt
      dfflibmap -liberty $LIB
      abc -liberty $LIB
      clean
      $rename_step
      stat -liberty $LIB
      ltp -noff
      write_verilog $OUT_DIR/${top}_synth.v
    " 2>&1 | tee "$stat_out"
  else
    yosys -p "
      read_verilog $src
      hierarchy -check -top $top
      proc; opt
      fsm; opt
      memory; opt
      techmap; opt
      abc -g cmos
      clean
      stat
    " 2>&1 | tee "$stat_out"
  fi
}

synth_one MultdivMulticycle "$OUT_DIR/multdiv_multicycle.v" \
  "$OUT_DIR/multdiv_multicycle.stat.log" rename
synth_one ibex_multdiv_fast "$OUT_DIR/ibex_multdiv_fast_fixed.v" \
  "$OUT_DIR/ibex_multdiv_fast.stat.log"

echo
echo "Reports written to: $OUT_DIR/{multdiv_multicycle,ibex_multdiv_fast}.stat.log"
