#!/usr/bin/env bash
# Renderer-faithfulness miters (the "Yosys-to-SMT miter" of the FP paper).
#
# The FP operators are one IR rendered three ways (SV / SMT / Lean). The SMT
# and Lean models are what the proofs check; the SV is what ships. This script
# machine-checks the SV side: Yosys — an independent implementation of
# SystemVerilog semantics — reads the emitted SV and exports it to SMT2
# (structure-preserving: read + proc + flatten only, no optimization), and z3
# checks that export equivalent to `render_smt`'s define-fun of the same IR:
#
#     emitted SV --(yosys frontend)--> SMT2  ==  render_smt(IR)
#
# `unsat` for an operator means: any semantic divergence between render_sv and
# render_smt would have to be a bug shared with Yosys's independent SV
# frontend. Because both sides come from one structure, the miters stay
# tractable even for the multiplier-bearing operators (shared substructure
# cancels under bit-blasting).
#
# Requires: yosys, z3, python3, a release `arch` binary (built fresh by
# default; set ARCH_BIN to override). Set DUMP_FP_BIN as well when the
# override does not have its sibling `examples/dump_fp`. Not run in CI (yosys/z3 not in the
# sandbox); run manually:
#
#   tests/fp_v1/smt_proof/renderer_miter.sh [outdir]
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
outdir="${1:-$(mktemp -d)}"
mkdir -p "$outdir"
timeout_s="${MITER_TIMEOUT:-3600}"
solver="${MITER_SOLVER:-}"
if [[ -z "$solver" ]]; then
  if command -v bitwuzla >/dev/null; then solver=bitwuzla; else solver=z3; fi
fi
echo "# solver: $solver, timeout ${timeout_s}s per op" >&2

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch + dump_fp (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch --example dump_fp >&2 )
  ARCH_BIN="$repo/target/release/arch"
fi
DUMP_FP_BIN="${DUMP_FP_BIN:-$(dirname "$ARCH_BIN")/examples/dump_fp}"
"$DUMP_FP_BIN" smt > "$outdir/arch_defs.smt2"

# MODULE | arch define-fun | port list (space-sep in-ports) | in type | out type
ops=(
  "F32Add|arch_f32_add|a b|FP32|FP32|y = a + b"
  "F32Sub|arch_f32_sub|a b|FP32|FP32|y = a - b"
  "F32Mul|arch_f32_mul|a b|FP32|FP32|y = a * b"
  "F32Fma|arch_fma_f32|a b c|FP32|FP32|y = fma(a, b, c)"
  "F32Eq|arch_f32_eq|a b|FP32|Bool|y = a == b"
  "F32Ne|arch_f32_ne|a b|FP32|Bool|y = a != b"
  "F32Lt|arch_f32_lt|a b|FP32|Bool|y = a < b"
  "F32Le|arch_f32_le|a b|FP32|Bool|y = a <= b"
  "F32Gt|arch_f32_gt|a b|FP32|Bool|y = a > b"
  "F32Ge|arch_f32_ge|a b|FP32|Bool|y = a >= b"
  "Bf16Add|arch_bf16_add|a b|BF16|BF16|y = a + b"
  "Bf16Sub|arch_bf16_sub|a b|BF16|BF16|y = a - b"
  "Bf16Mul|arch_bf16_mul|a b|BF16|BF16|y = a * b"
  "Bf16Fma|arch_fma_bf16|a b c|BF16|BF16|y = fma(a, b, c)"
  "Bf16Eq|arch_bf16_eq|a b|BF16|Bool|y = a == b"
  "Bf16Ne|arch_bf16_ne|a b|BF16|Bool|y = a != b"
  "Bf16Lt|arch_bf16_lt|a b|BF16|Bool|y = a < b"
  "Bf16Le|arch_bf16_le|a b|BF16|Bool|y = a <= b"
  "Bf16Gt|arch_bf16_gt|a b|BF16|Bool|y = a > b"
  "Bf16Ge|arch_bf16_ge|a b|BF16|Bool|y = a >= b"
  "F32ToBf16|arch_f32_to_bf16|a|FP32|BF16|y = a.to_bf16()"
  "Bf16ToF32|arch_bf16_to_f32|a|BF16|FP32|y = a.to_fp32()"
  "F32ToS64|arch_f32_to_sint|a|FP32|SInt<64>|y = a.to_sint<64>()|(arch_f32_to_sint ARGa (_ bv64 32))"
  "F32ToU64|arch_f32_to_uint|a|FP32|UInt<64>|y = a.to_uint<64>()|(arch_f32_to_uint ARGa (_ bv64 32))"
  "E4m3Add|arch_e4m3_add|a b|FP8E4M3|FP8E4M3|y = a + b"
  "E4m3Sub|arch_e4m3_sub|a b|FP8E4M3|FP8E4M3|y = a - b"
  "E4m3Mul|arch_e4m3_mul|a b|FP8E4M3|FP8E4M3|y = a * b"
  "E4m3Fma|arch_fma_e4m3|a b c|FP8E4M3|FP8E4M3|y = fma(a, b, c)"
  "E4m3Eq|arch_e4m3_eq|a b|FP8E4M3|Bool|y = a == b"
  "E4m3Ne|arch_e4m3_ne|a b|FP8E4M3|Bool|y = a != b"
  "E4m3Lt|arch_e4m3_lt|a b|FP8E4M3|Bool|y = a < b"
  "E4m3Le|arch_e4m3_le|a b|FP8E4M3|Bool|y = a <= b"
  "E4m3Gt|arch_e4m3_gt|a b|FP8E4M3|Bool|y = a > b"
  "E4m3Ge|arch_e4m3_ge|a b|FP8E4M3|Bool|y = a >= b"
  "E4m3ToF32|arch_e4m3_to_f32|a|FP8E4M3|FP32|y = a.to_fp32()"
  "F32ToE4m3|arch_f32_to_e4m3|a|FP32|FP8E4M3|y = a.to_fp8e4m3()"
  "E5m2Add|arch_e5m2_add|a b|FP8E5M2|FP8E5M2|y = a + b"
  "E5m2Sub|arch_e5m2_sub|a b|FP8E5M2|FP8E5M2|y = a - b"
  "E5m2Mul|arch_e5m2_mul|a b|FP8E5M2|FP8E5M2|y = a * b"
  "E5m2Fma|arch_fma_e5m2|a b c|FP8E5M2|FP8E5M2|y = fma(a, b, c)"
  "E5m2Eq|arch_e5m2_eq|a b|FP8E5M2|Bool|y = a == b"
  "E5m2Ne|arch_e5m2_ne|a b|FP8E5M2|Bool|y = a != b"
  "E5m2Lt|arch_e5m2_lt|a b|FP8E5M2|Bool|y = a < b"
  "E5m2Le|arch_e5m2_le|a b|FP8E5M2|Bool|y = a <= b"
  "E5m2Gt|arch_e5m2_gt|a b|FP8E5M2|Bool|y = a > b"
  "E5m2Ge|arch_e5m2_ge|a b|FP8E5M2|Bool|y = a >= b"
  "E5m2ToF32|arch_e5m2_to_f32|a|FP8E5M2|FP32|y = a.to_fp32()"
  "F32ToE5m2|arch_f32_to_e5m2|a|FP32|FP8E5M2|y = a.to_fp8e5m2()"
  # OCP MX sub-8-bit storage formats: conversions only (no scalar arithmetic
  # exists on them, so there is no add/mul/cmp row to write).
  "E2m1ToF32|arch_e2m1_to_f32|a|FP4E2M1|FP32|y = a.to_fp32()"
  "F32ToE2m1|arch_f32_to_e2m1|a|FP32|FP4E2M1|y = a.to_fp4e2m1()"
  "E2m3ToF32|arch_e2m3_to_f32|a|FP6E2M3|FP32|y = a.to_fp32()"
  "F32ToE2m3|arch_f32_to_e2m3|a|FP32|FP6E2M3|y = a.to_fp6e2m3()"
  "E3m2ToF32|arch_e3m2_to_f32|a|FP6E3M2|FP32|y = a.to_fp32()"
  "F32ToE3m2|arch_f32_to_e3m2|a|FP32|FP6E3M2|y = a.to_fp6e3m2()"
  "E8m0ToF32|arch_e8m0_to_f32|a|E8M0|FP32|y = a.to_fp32()"
  "F32ToE8m0|arch_f32_to_e8m0|a|FP32|E8M0|y = a.to_e8m0()"
)

echo "# module            verdict   z3 time (s)"
fail=0
for spec in "${ops[@]}"; do
  IFS='|' read -r mod fn inports inty outty body smtapp <<<"$spec"
  {
    echo "module $mod"
    for p in $inports; do echo "  port $p: in $inty;"; done
    echo "  port y: out $outty;"
    echo "  comb $body; end comb"
    echo "end module $mod"
  } > "$outdir/$mod.arch"
  "$ARCH_BIN" build "$outdir/$mod.arch" -o "$outdir/$mod.sv" >/dev/null 2>&1
  python3 "$here/../synth/hoist_decls.py" < "$outdir/$mod.sv" > "$outdir/${mod}_h.v"
  {
    echo "read_verilog -sv $outdir/${mod}_h.v"
    echo "hierarchy -top $mod"
    echo "proc"
    echo "flatten"
    echo "opt_clean"
    echo "write_smt2 $outdir/$mod.yosys.smt2"
  } > "$outdir/$mod.ys"
  yosys -q "$outdir/$mod.ys"
  # Specialize away yosys's uninterpreted state sort (everything is applied to
  # one state constant), leaving pure QF_BV — required by bitwuzla, and a
  # simpler instance for every solver.
  python3 - "$mod" "$outdir" <<'PYSPEC'
import re, sys
mod, outdir = sys.argv[1], sys.argv[2]
src = open(f'{outdir}/{mod}.yosys.smt2').read()
src = re.sub(r'\(declare-sort \|%s_s\| 0\)\n' % mod, '', src)
src = re.sub(r'\(declare-fun \|%s_is\| \(\|%s_s\|\) Bool\)\n' % (mod, mod), '', src)
src = re.sub(r'\(declare-fun (\|[^|]+\|) \(\|%s_s\|\) ' % mod, r'(declare-const \1 ', src)
src = src.replace(f'((state |{mod}_s|))', '()')
src = re.sub(r'\((\|[^|]+\|) state\)', r'\1', src)
src = re.sub(r'\(define-fun \|%s_t\|[^\n]*\n' % mod, '', src)
open(f'{outdir}/{mod}.yosys.qfbv.smt2', 'w').write(src)
PYSPEC
  {
    cat "$outdir/arch_defs.smt2" "$outdir/$mod.yosys.qfbv.smt2"
    args=""
    for p in $inports; do args+=" |${mod}_n $p|"; done
    # yosys exports 1-bit wires as Bool; the arch define-funs use (_ BitVec 1)
    if [[ -n "${smtapp:-}" ]]; then
      app="${smtapp//ARGa/|${mod}_n a|}"
      echo "(assert (not (= |${mod}_n y| $app)))"
    elif [[ "$outty" == "Bool" ]]; then
      echo "(assert (not (= |${mod}_n y| (= ($fn$args) #b1))))"
    else
      echo "(assert (not (= |${mod}_n y| ($fn$args))))"
    fi
    echo "(check-sat)"
  } > "$outdir/$mod.miter.smt2"
  t0=$(date +%s)
  if [[ "$mod" == *Fma ]]; then
    # The monolithic fma miter is SAT-hard: the two renderers implement the
    # variable alignment/normalize shifts as different circuits, and the
    # 48-bit product sits in every shifted bit's cone. Sound fix: case-split
    # on the alignment gap diff = |eunb(a)+eunb(b) - eunb(c)|. For each
    # constant diff both barrel shifters collapse and each sub-miter is
    # near-structural; diff ranges over [0,508] (eab in [-298,210], ec in
    # [-149,105]), plus a catchall (diff > 508, unsat by range). All
    # sub-miters unsat => the full miter is unsat.
    conv=""; convw=16
    [[ "$mod" == Bf16Fma ]] && conv="arch_bf16_to_f32"
    [[ "$mod" == E4m3Fma ]] && { conv="arch_e4m3_to_f32"; convw=8; }
    [[ "$mod" == E5m2Fma ]] && { conv="arch_e5m2_to_f32"; convw=8; }
    splitdir="$outdir/$mod.split"
    mkdir -p "$splitdir"
    {
      sed '$d' "$outdir/$mod.miter.smt2"   # strip (check-sat)
      if [[ -n "$conv" ]]; then
        echo "(define-fun spl_in ((x (_ BitVec $convw))) (_ BitVec 32) ($conv x))"
        w=$convw
      else
        echo "(define-fun spl_in ((x (_ BitVec 32))) (_ BitVec 32) x)"
        w=32
      fi
      cat <<SPL
(define-fun spl_eunb ((x (_ BitVec $w))) (_ BitVec 16)
  (ite (= ((_ extract 30 23) (spl_in x)) (_ bv0 8)) (_ bv65387 16)
       (bvsub ((_ zero_extend 8) ((_ extract 30 23) (spl_in x))) (_ bv150 16))))
(define-fun spl_eab () (_ BitVec 16)
  (bvadd (spl_eunb |${mod}_n a|) (spl_eunb |${mod}_n b|)))
(define-fun spl_ec () (_ BitVec 16) (spl_eunb |${mod}_n c|))
(define-fun spl_dif () (_ BitVec 16)
  (ite (bvsle spl_ec spl_eab) (bvsub spl_eab spl_ec) (bvsub spl_ec spl_eab)))
SPL
    } > "$splitdir/body.smt2"
    for k in $(seq 0 508); do
      { cat "$splitdir/body.smt2"
        echo "(assert (= spl_dif (_ bv${k} 16)))"
        echo "(check-sat)"
      } > "$splitdir/case_$k.smt2"
    done
    { cat "$splitdir/body.smt2"
      echo "(assert (bvugt spl_dif (_ bv508 16)))"
      echo "(check-sat)"
    } > "$splitdir/case_over.smt2"
    ls "$splitdir"/case_*.smt2 \
      | MITER_TO="$timeout_s" xargs -P "${MITER_SPLIT_JOBS:-8}" -I{} bash -c \
          'echo "{} $(z3 -T:"$MITER_TO" {} 2>&1 | tail -1)"' \
      > "$splitdir/results.txt"
    nbad=$(grep -cv "unsat$" "$splitdir/results.txt" || true)
    ncase=$(wc -l < "$splitdir/results.txt" | tr -d ' ')
    if [[ "$nbad" == "0" && "$ncase" == "510" ]]; then
      verdict="unsat"
    else
      verdict="split-fail($nbad/$ncase)"
    fi
  elif [[ "$solver" == "bitwuzla" ]]; then
    # bitwuzla's -t is in milliseconds
    verdict=$(bitwuzla -t $((timeout_s * 1000)) "$outdir/$mod.miter.smt2" 2>&1 | tail -1)
    if [[ "$verdict" != "unsat" ]]; then
      # solver variance is real (bitwuzla stalls on the mul miter that z3
      # clears in ~30s, and vice versa for others) — retry with z3
      verdict=$(z3 -T:"$timeout_s" "$outdir/$mod.miter.smt2" 2>&1 | tail -1)
    fi
  else
    verdict=$(z3 -T:"$timeout_s" "$outdir/$mod.miter.smt2" 2>&1 | tail -1)
  fi
  t1=$(date +%s)
  printf "%-18s %-9s %6d\n" "$mod" "$verdict" $((t1 - t0))
  [[ "$verdict" == "unsat" ]] || fail=1
done
exit $fail
