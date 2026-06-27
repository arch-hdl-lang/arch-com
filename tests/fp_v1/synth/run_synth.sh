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
# Usage:   tests/fp_v1/synth/run_synth.sh [outdir]
# Output:  a results table on stdout; per-op logs under <outdir> (default: a
#          temp dir). Nothing is written back into the repo tree.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
outdir="${1:-$(mktemp -d)}"
mkdir -p "$outdir"

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch >&2 )
  ARCH_BIN="$repo/target/release/arch"
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
  ( cd "$outdir" && yosys -q "_$t.ys" > "log_$t.txt" 2>&1 )
done

printf '\n%-12s %12s %12s\n' "operator" "cells" "depth"
printf '%-12s %12s %12s\n' "--------" "-----" "-----"
for t in "${ops[@]}"; do
  cells=$(grep -m1 "Number of cells:" "$outdir/log_$t.txt" | grep -oE "[0-9]+" || echo "?")
  depth=$(grep -m1 "Longest topological path" "$outdir/log_$t.txt" | grep -oE "length=[0-9]+" | grep -oE "[0-9]+" || echo "?")
  printf '%-12s %12s %12s\n' "$t" "$cells" "$depth"
done
echo
echo "# logs + netlists in: $outdir"
