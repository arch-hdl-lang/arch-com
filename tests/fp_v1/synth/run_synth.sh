#!/usr/bin/env bash
# Combinational yosys synthesis of the arch-emitted f32 / bf16 FP operators, to
# estimate logic depth (timing proxy) and gate count (area proxy).
#
# Pipeline per operator:
#   1. write a single-operator .arch module
#   2. `arch build` it  -> SystemVerilog (the proven RTL; same IR as the SMT/Lean
#      models in tests/fp_v1/smt_proof and proofs/lean_fp_equiv)
#   3. hoist_decls.py    -> yosys-friendly Verilog (see that file for why)
#   4. yosys synth + ltp -> logic depth + cell count
#
# Requires: yosys (>=0.30), python3, and a release `arch` binary. Builds the
# binary fresh by default (the repo's no-stale-binary rule); set ARCH_BIN to
# reuse one you trust.
#
# Usage (combinational sweep, all FP operators):
#   tests/fp_v1/synth/run_synth.sh [outdir]
#
# Usage (staged/pipelined operator — proposal phase 3,
# doc/proposal_pipelined_operators.md — registry characterization flow):
#   tests/fp_v1/synth/run_synth.sh --stages N MODULE [outdir]
#
#   Emits a `port reg`/`pipe_reg<T,N>` wrapper around the registry operator
#   (currently only `F32Fma` → `fma<pipelined, N>`), builds it through `arch
#   build` (comb helper + N-deep register cascade — see `src/pipelined_ops.rs`
#   module docs for why this shape is sufficient, no separate staged-datapath
#   codegen exists), and runs it through the same yosys flow with the ABC
#   `dretime` sequential-retiming pass exercised (the `abc -fast` default
#   script always runs `strash; dretime; map`, per `yosys -h abc`) — the
#   generic-gate-mapping analogue of the registry note's
#   "Yosys abc: buffer -N 8; upsize; dnsize" recipe (that exact recipe is an
#   ABC `-liberty`/`-constr` script variant; no Liberty file or OpenSTA is
#   available in this repo's CI/dev sandboxes, so it is not reproduced here —
#   see README.md's "Staged/pipelined operators" section for what IS
#   reproducible with the checked-in toolchain).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"

stages=""
stage_module=""
if [[ "${1:-}" == "--stages" ]]; then
  stages="$2"
  stage_module="$3"
  shift 3
fi

outdir="${1:-$(mktemp -d)}"
mkdir -p "$outdir"

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch >&2 )
  ARCH_BIN="$repo/target/release/arch"
fi

# ── Staged/pipelined-operator mode (--stages N MODULE) ─────────────────────
if [[ -n "$stages" ]]; then
  case "$stage_module" in
    F32Fma)
      emit_stage() {
        cat > "$outdir/$1.arch" <<EOF
module $1
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port y: out pipe_reg<FP32, $stages>;

  seq on clk rising
    y@$stages <= fma<pipelined, $stages>(a, b, c);
  end seq
end module $1
EOF
      }
      ;;
    *)
      echo "unknown --stages MODULE '$stage_module' (only F32Fma is registered — see \`arch ops\`)" >&2
      exit 1
      ;;
  esac
  t="${stage_module}S${stages}"
  emit_stage "$t"
  "$ARCH_BIN" build "$outdir/$t.arch" -o "$outdir/$t.sv" >&2
  python3 "$here/hoist_decls.py" < "$outdir/$t.sv" > "$outdir/$t.v"
  cat > "$outdir/_$t.ys" <<EOF
read_verilog -sv $t.v
hierarchy -top $t
proc
synth -top $t -flatten
abc -fast -g AND,OR,XOR,NAND,NOR,XNOR,ANDNOT,ORNOT,MUX
opt_clean
stat
ltp -noff
EOF
  ( cd "$outdir" && yosys "_$t.ys" > "log_$t.txt" 2>&1 )

  cells=$(grep -m1 -E "^[[:space:]]*[0-9]+ cells$" "$outdir/log_$t.txt" | grep -oE "[0-9]+" || echo "?")
  dffs=$(grep -oE '[0-9]+[[:space:]]+\$_S?DFF[A-Z_]*_' "$outdir/log_$t.txt" | grep -oE '^[0-9]+' | awk '{s+=$1} END{print s+0}' || echo 0)
  depth=$(grep -m1 "Longest topological path" "$outdir/log_$t.txt" | grep -oE "length=[0-9]+" | grep -oE "[0-9]+" || echo "?")
  printf '\n%-16s %12s %12s %12s\n' "module" "cells" "dff-bits" "ltp -noff"
  printf '%-16s %12s %12s %12s\n' "------" "-----" "--------" "---------"
  printf '%-16s %12s %12s %12s\n' "$t" "$cells" "$dffs" "$depth"
  echo
  echo "# stages=$stages; dff-bits is the total flip-flop bit count post-synth"
  echo "# (cascade regs + reset mux logic; not simply (stages-1)*32 — abc's mapping"
  echo "# folds the reset-select mux into extra \$_SDFF*_ cells)."
  echo "# 'ltp -noff' is the longest FF-excluded topological path in the *whole*"
  echo "# flattened netlist, i.e. still the un-rebalanced input->first-register comb"
  echo "# depth (open-source abc's default 'dretime' pass did not redistribute"
  echo "# registers across the comb cone with generic 2-input gates / no -liberty"
  echo "# target in this environment — see README.md 'Staged/pipelined operators'"
  echo "# section for the honest reproducibility statement). This is a logic-depth"
  echo "# proxy, NOT an fmax number."
  echo "# logs + netlists in: $outdir"
  exit 0
fi

emit() { printf '%s\n' "$2" > "$outdir/$1.arch"; }

emit F32ToBf16 'module F32ToBf16
  port a: in FP32; port y: out BF16;
  comb y = a.to_bf16(); end comb
end module F32ToBf16'
emit Bf16ToF32 'module Bf16ToF32
  port a: in BF16; port y: out FP32;
  comb y = a.to_fp32(); end comb
end module Bf16ToF32'
emit F32Mul 'module F32Mul
  port a: in FP32; port b: in FP32; port y: out FP32;
  comb y = a * b; end comb
end module F32Mul'
emit F32Add 'module F32Add
  port a: in FP32; port b: in FP32; port y: out FP32;
  comb y = a + b; end comb
end module F32Add'
emit F32Sub 'module F32Sub
  port a: in FP32; port b: in FP32; port y: out FP32;
  comb y = a - b; end comb
end module F32Sub'
emit F32Fma 'module F32Fma
  port a: in FP32; port b: in FP32; port c: in FP32; port y: out FP32;
  comb y = fma(a, b, c); end comb
end module F32Fma'
emit Bf16Mul 'module Bf16Mul
  port a: in BF16; port b: in BF16; port y: out BF16;
  comb y = a * b; end comb
end module Bf16Mul'
emit Bf16Add 'module Bf16Add
  port a: in BF16; port b: in BF16; port y: out BF16;
  comb y = a + b; end comb
end module Bf16Add'
emit Bf16Sub 'module Bf16Sub
  port a: in BF16; port b: in BF16; port y: out BF16;
  comb y = a - b; end comb
end module Bf16Sub'
emit Bf16Fma 'module Bf16Fma
  port a: in BF16; port b: in BF16; port c: in BF16; port y: out BF16;
  comb y = fma(a, b, c); end comb
end module Bf16Fma'

ops=(Bf16ToF32 F32ToBf16 Bf16Mul F32Mul F32Add F32Sub Bf16Add Bf16Sub Bf16Fma F32Fma)

for t in "${ops[@]}"; do
  "$ARCH_BIN" build "$outdir/$t.arch" -o "$outdir/$t.sv" >&2
  python3 "$here/hoist_decls.py" < "$outdir/$t.sv" > "$outdir/$t.v"
  sed "s/TOP/$t/g" "$here/flow.ys.tmpl" > "$outdir/_$t.ys"
  ( cd "$outdir" && yosys "_$t.ys" > "log_$t.txt" 2>&1 )
done

printf '\n%-12s %12s %12s\n' "operator" "cells" "depth"
printf '%-12s %12s %12s\n' "--------" "-----" "-----"
for t in "${ops[@]}"; do
  cells=$(grep -m1 -E "^[[:space:]]*[0-9]+ cells$" "$outdir/log_$t.txt" | grep -oE "[0-9]+" || echo "?")
  depth=$(grep -m1 "Longest topological path" "$outdir/log_$t.txt" | grep -oE "length=[0-9]+" | grep -oE "[0-9]+" || echo "?")
  printf '%-12s %12s %12s\n' "$t" "$cells" "$depth"
done
echo
echo "# logs + netlists in: $outdir"
