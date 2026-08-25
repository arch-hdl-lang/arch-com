#!/usr/bin/env bash
# Structural renderer-faithfulness miter for `scaled_dot` (Theorem A of
# proofs/lean_fp_equiv/SCALED_DOT_ACCUMULATION_SCOPE.md).
#
# The scalar FP miters in `renderer_miter.sh` each check one *atomic* operator:
# emitted SV --(yosys)--> SMT2  ==  the operator's own `render_smt` define-fun.
# `scaled_dot` has no atomic model — it is glue that widens, multiplies, sums a
# pairwise tree, and applies the two block scales. So this miter's spec side is
# a HAND-COMPOSED define-fun tree, built from the already-miter-checked atomic
# nodes (`arch_e<elem>_to_f32`, `arch_e8m0_to_f32`, `arch_f32_mul`,
# `arch_f32_add`) wired exactly as `src/fp_block.rs::dot_schedule` /`sv_dot`
# specify. `unsat` therefore means:
#
#   the emitted SV implements precisely the defined balanced-pairwise schedule
#   and the one-at-a-time scale application — no wiring divergence, modulo a
#   bug shared with yosys's independent SV frontend.
#
# This is the composition-wiring half of the accumulation proof. The *value*
# half (that the pairwise sum meets the O(log N) error bound) is Theorem B, a
# Lean development — see ScaledDot.lean / the scope doc.
#
# Element order (`sv_dot`): element i at a[i*EW +: EW], scale at a[BW-1 : BW-SW].
# Scales applied ONE AT A TIME: ((S * Xa) * Xb), never (Xa*Xb)*S — pre-forming
# the scale product can overflow to Inf even when the result is representable.
#
# Requires: yosys, z3 (or bitwuzla), python3, a release `arch` + `dump_fp`
# (built fresh by default; set ARCH_BIN / DUMP_FP_BIN to override). Not run in
# CI (yosys/z3 absent in the sandbox); run manually:
#
#   tests/fp_v1/smt_proof/scaled_dot_miter.sh [outdir]
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
echo "# solver: $solver, timeout ${timeout_s}s per shape" >&2

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch + dump_fp (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch --example dump_fp >&2 )
  ARCH_BIN="$repo/target/release/arch"
fi
DUMP_FP_BIN="${DUMP_FP_BIN:-$(dirname "$ARCH_BIN")/examples/dump_fp}"
"$DUMP_FP_BIN" smt > "$outdir/arch_defs.smt2"

# MOD | elem-format tag | arch widen fn | N | element width | scale width
#
# Default set = the shapes that clear monolithically. `ScaledDotE4m3N4` is
# omitted: its wide-significand products in a variable-alignment reduction tree
# are SAT-hard, and — unlike the fma miter — a case-split does NOT rescue it.
# Investigated 2026-08-24:
#   * monolithic: bitwuzla + z3 both exhaust a 1200 s budget (2401 s total);
#   * even the pure 4-way add-tree over free FP32 (no multipliers) times out at
#     1200 s, so the adder barrel shifters alone are the wall;
#   * the fma alignment case-split works there because fma has ONE alignment gap
#     (product vs addend). A reduction tree has one gap PER add, and they are
#     coupled: splitting on just the top add's gap still times out (900 s/case),
#     and splitting on all gaps is ~35^depth cases — infeasible.
# There is no single splitting variable for a tree, so the monolithic-SMT route
# is a dead end for wide/large shapes. Those are instead covered compositionally:
# node faithfulness by `renderer_miter.sh` (F32Mul/F32Add/E*ToF32/E8m0ToF32 all
# unsat) + wiring faithfulness by `tests/scaled_dot_wiring_test.rs` (checks the
# emitted function IS dot_schedule's composition, for all N and all formats, in
# CI). This miter's small shapes remain the bit-exact end-to-end cross-check.
# Run the SAT-hard shape standalone anyway (expect timeout) with:
#   shapes_override="ScaledDotE4m3N4|e4m3|arch_e4m3_to_f32|4|8|8" bash scaled_dot_miter.sh
shapes=(
  "ScaledDotE4m3N2|e4m3|arch_e4m3_to_f32|2|8|8"
  "ScaledDotE2m1N4|e2m1|arch_e2m1_to_f32|4|4|8"
)
# Allow the caller to override the set (e.g. to run the SAT-hard shape alone).
if [[ -n "${shapes_override:-}" ]]; then read -r -a shapes <<<"$shapes_override"; fi

echo "# shape              verdict   solver time (s)"
fail=0
for spec in "${shapes[@]}"; do
  IFS='|' read -r mod tag widen n ew sw <<<"$spec"
  bw=$((sw + n * ew))
  elem_ty=$(case "$tag" in
    e4m3) echo "FP8E4M3" ;; e5m2) echo "FP8E5M2" ;;
    e2m1) echo "FP4E2M1" ;; e2m3) echo "FP6E2M3" ;; e3m2) echo "FP6E3M2" ;;
  esac)

  # 1. Emit the ARCH module and build → SV → yosys → QF_BV (same pipeline as
  #    renderer_miter.sh).
  {
    echo "package P_$mod"
    echo "  type B = ScaledVec<$elem_ty, $n, E8M0>;"
    echo "end package P_$mod"
    echo "module $mod"
    echo "  port a: in B;"
    echo "  port b: in B;"
    echo "  port y: out FP32;"
    echo "  comb y = scaled_dot(a, b); end comb"
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

  # 2. Build the composed spec define-fun. `dot_schedule` is reimplemented in
  #    Python (identical balanced-pairwise loop as src/fp_block.rs) so the spec
  #    encodes the SAME accumulation order, generated rather than transcribed.
  #    Element i occupies bits [i*ew +: ew]; scale occupies [bw-1 : bw-sw].
  python3 - "$outdir/$mod.spec.smt2" "$n" "$ew" "$bw" "$sw" "$widen" <<'PYSPEC2'
import sys
out, n, ew, bw, sw, widen = sys.argv[1:7]
n, ew, bw, sw = int(n), int(ew), int(bw), int(sw)

# dot_schedule(n): balanced pairwise, lone trailing element passes through.
# Temps 0..n-1 = element-pair products; temp n+k = add k.
adds, cur, nxt_id = [], list(range(n)), n
while len(cur) > 1:
    nxt, i = [], 0
    while i + 1 < len(cur):
        adds.append((cur[i], cur[i + 1])); nxt.append(nxt_id); nxt_id += 1; i += 2
    if i < len(cur):
        nxt.append(cur[i])
    cur = nxt
last = cur[0]

def ext(hi, lo, v):
    return f'((_ extract {hi} {lo}) {v})'

lines = [f'(define-fun scaled_dot_spec ((a (_ BitVec {bw})) (b (_ BitVec {bw}))) (_ BitVec 32)']
# Products: exact FP32 pair products of widened elements.
for i in range(n):
    hi, lo = i * ew + ew - 1, i * ew
    lines.append(f'  (let ((t{i} (arch_f32_mul ({widen} {ext(hi, lo, "a")}) ({widen} {ext(hi, lo, "b")}))))')
# Pairwise adds.
for k, (l, r) in enumerate(adds):
    lines.append(f'  (let ((t{n + k} (arch_f32_add t{l} t{r})))')
# Body: ((S * Xa) * Xb) — scales applied ONE AT A TIME (sv_dot), never
# (Xa*Xb)*S: pre-forming the scale product can overflow to Inf/flush to 0.
shi, slo = bw - 1, bw - sw
lines.append(f'  (arch_f32_mul (arch_f32_mul t{last}')
lines.append(f'    (arch_e8m0_to_f32 {ext(shi, slo, "a")}))')
lines.append(f'    (arch_e8m0_to_f32 {ext(shi, slo, "b")}))')
# Close: one ) per let (n products + len(adds) adds) + the define-fun paren.
lines.append(')' * (n + len(adds)) + ')')
open(out, 'w').write('\n'.join(lines) + '\n')
PYSPEC2

  # 3. Assemble and solve the miter.
  {
    cat "$outdir/arch_defs.smt2" "$outdir/$mod.yosys.qfbv.smt2" "$outdir/$mod.spec.smt2"
    echo "(assert (not (= |${mod}_n y| (scaled_dot_spec |${mod}_n a| |${mod}_n b|))))"
    echo "(check-sat)"
  } > "$outdir/$mod.miter.smt2"

  t0=$(date +%s)
  if [[ "$solver" == "bitwuzla" ]]; then
    verdict=$(bitwuzla -t $((timeout_s * 1000)) "$outdir/$mod.miter.smt2" 2>&1 | tail -1)
    if [[ "$verdict" != "unsat" ]]; then
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
