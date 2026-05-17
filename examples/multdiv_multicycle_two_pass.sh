#!/usr/bin/env bash
# Two-pass selective resynthesis driver for MultdivMulticycleHier
# (the hierarchy-split variant of multdiv_multicycle.arch). Implements
# Path A of the research note "Selective Resynthesis for Multicycle
# Paths in Yosys/ABC".
#
# Strategy: hierarchy-boundary freeze + selection exclusion.
#   Pass 1: synthesize ONLY the datapath child (with `hierarchy -top
#           MultdivMulticycleDatapath`), mapping at a RELAXED clock
#           period (the multicycle 3-cycle window of ~3.8 ns). Output:
#           pass1_child_only.v — a mapped Sky130 netlist for the child.
#   Pass 2: read the parent RTL + the pass-1 mapped child, synthesize
#           only the PARENT (`select -module MultdivMulticycleHier`)
#           at the REAL single-cycle clock period (5.0 ns / 200 MHz).
#           The child's mapped cells are not touched.
#
# WHY two yosys invocations and not one? `setattr -mod -set
# keep_hierarchy 1 <child>` keeps the module as a separate scope
# through `synth`, but it does NOT prevent `abc` from re-mapping the
# child when given a list of modules. Experimentally, abc visits both
# modules and produces identical output. The robust freeze is to read
# the child as already-mapped netlist and `select` only the parent for
# pass-2 abc.
#
# Requirements: yosys (any recent version), sv2v, Sky130 PDK Liberty.
#
# Outputs land under $OUT_DIR (default /tmp/multdiv-two-pass).

set -euo pipefail

OUT_DIR="${OUT_DIR:-/tmp/multdiv-two-pass}"
ARCH_COM_DIR="${ARCH_COM_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
LIB="${LIB:-$HOME/.volare/sky130A/libs.ref/sky130_fd_sc_hd/lib/sky130_fd_sc_hd__tt_025C_1v80.lib}"

# Timing targets (in ps for `abc -D`).
# RELAXED_PS = 3.8 ns × 1000 — the multicycle multiplier's required
#              cycle time when honored (divide_delay / 36 ≈ 3.79 ns).
# REAL_PS    = 5.0 ns × 1000 — the original 200 MHz single-cycle goal.
RELAXED_PS="${RELAXED_PS:-3800000}"
REAL_PS="${REAL_PS:-5000000}"

TOP="MultdivMulticycleHier"
CHILD="MultdivMulticycleDatapath"

mkdir -p "$OUT_DIR"

if [[ ! -f "$LIB" ]]; then
  echo "ERROR: Liberty $LIB not found." >&2
  echo "Set LIB=/path/to/sky130_fd_sc_hd__tt_025C_1v80.lib" >&2
  exit 1
fi

# 1. Emit the hierarchy-split design (regenerates the SV + SDC).
"$ARCH_COM_DIR/target/release/arch" build \
  "$ARCH_COM_DIR/examples/multdiv_multicycle_hier.arch" \
  -o "$OUT_DIR/multdiv_multicycle_hier.sv"

# Yosys 0.64's built-in SV parser rejects some idioms the arch-com SV
# emitter uses; sv2v converts to plain Verilog-2005.
sv2v "$OUT_DIR/multdiv_multicycle_hier.sv" > "$OUT_DIR/multdiv_multicycle_hier.v"

# ----------------------------------------------------------------------
# Pass 1: child-only synthesis at relaxed clock target.
# `hierarchy -top $CHILD` tells yosys to discard the parent and treat
# the child as the standalone top. `splitnets` after dfflibmap gives
# each Q-bit its own wire so flop cells get `<name>_reg_<bit>` names.
# ----------------------------------------------------------------------
echo "=== Pass 1: child '$CHILD' standalone @ ${RELAXED_PS} ps (~$(echo "scale=2; $RELAXED_PS/1000000" | bc) ns) ==="
cat > "$OUT_DIR/pass1.ys" <<EOF
read_verilog $OUT_DIR/multdiv_multicycle_hier.v
hierarchy -check -top $CHILD
proc; opt
fsm; opt
memory; opt
techmap; opt
dfflibmap -liberty $LIB
abc -liberty $LIB -D $RELAXED_PS
clean
splitnets
tcl $ARCH_COM_DIR/examples/multdiv_multicycle_two_pass_rename.tcl
stat -liberty $LIB
write_verilog -noattr $OUT_DIR/pass1_child_only.v
EOF
yosys -s "$OUT_DIR/pass1.ys" 2>&1 | tee "$OUT_DIR/pass1.log" | \
  grep -E "(Chip area for|cells$|Executing ABC|Executing SPLITNETS|Renaming cell.*reg)" | tail -15

# ----------------------------------------------------------------------
# Pass 2: parent resynthesis at real clock, child preserved.
#   - read parent + child RTL
#   - `delete $CHILD` to remove the RTL form of the child
#   - read pass1 mapped child (now the child is in netlist form)
#   - `setattr -mod -set keep_hierarchy 1 $CHILD` keeps the boundary
#   - `select -module $TOP` restricts abc to the parent
# ----------------------------------------------------------------------
echo "=== Pass 2: parent '$TOP' only @ ${REAL_PS} ps (~$(echo "scale=2; $REAL_PS/1000000" | bc) ns) ==="
cat > "$OUT_DIR/pass2.ys" <<EOF
read_verilog $OUT_DIR/multdiv_multicycle_hier.v
delete $CHILD
read_verilog $OUT_DIR/pass1_child_only.v
read_liberty -lib $LIB
hierarchy -check -top $TOP
setattr -mod -set keep_hierarchy 1 $CHILD
proc; opt
fsm; opt
memory; opt
techmap; opt
dfflibmap -liberty $LIB
select -module $TOP
abc -liberty $LIB -D $REAL_PS
select -clear
clean
splitnets
tcl $ARCH_COM_DIR/examples/multdiv_multicycle_two_pass_rename.tcl
stat -liberty $LIB
write_verilog -noattr $OUT_DIR/${TOP}_synth.v
EOF
yosys -s "$OUT_DIR/pass2.ys" 2>&1 | tee "$OUT_DIR/pass2.log" | \
  grep -E "(Chip area for|cells$|Executing ABC|Extracting gate|Renaming cell.*reg)" | tail -20

echo
echo "=== Two-pass summary ==="
echo "Pass 1 (child, relaxed clock):"
grep -E "Chip area for top|Chip area for module" "$OUT_DIR/pass1.log" | tail -3
echo "Pass 2 (parent, real clock) — final design:"
grep -E "Chip area for top|Chip area for module" "$OUT_DIR/pass2.log" | tail -3
echo
echo "To run OpenSTA on the final netlist:"
echo "  DESIGN=hier_with_mc OUT_DIR=$OUT_DIR \\"
echo "    sta -no_splash -exit $ARCH_COM_DIR/examples/multdiv_multicycle_sta.tcl"
