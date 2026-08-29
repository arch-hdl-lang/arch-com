#!/usr/bin/env bash
# Latency-equivalence miter for the RETIMED STAGED FP operators (`op<pipelined,
# N>`, emitted by `arch build --staged-ops`).
#
# The staged emitter cuts a combinational FP operator's dataflow graph into
# pipeline stages so the design runs at a higher clock (fma<pipelined,6> ~ 260
# MHz vs ~180 MHz single-cycle). The *proof obligation* — stated but deferred by
# `arch formal`, see src/formal.rs — is that the staged datapath is LOGICALLY
# equivalent to the single-cycle operator: sampled at its latency L, it must
# produce exactly the correctly-rounded result the comb operator produces. Today
# that equivalence rests on a prose "by construction" argument plus randomized
# lockstep simulation (tests/pipelined_fma_lockstep_test.rs) — samples, not a
# proof over all inputs. Simulation already missed one skew bug of this class
# (the scaled_dot scale-byte off-by-one, arch#955 follow-up), which is invisible
# under stable/low-entropy stimulus.
#
# This script discharges the obligation for all inputs by decomposing it into
# two independently-checkable lemmas:
#
#   Lemma B — TIMING (structural, no solver).  The staged module is a BALANCED
#     feed-forward pipeline: every path from a data input to the output crosses
#     the same number L of registers (tests/fp_v1/synth/pipeline_balance.py).
#     This is exactly the property the skew bug class violates.
#
#   Lemma A — ARITHMETIC (SMT).  Short every pipeline register to a wire (Q:=D);
#     the resulting purely-combinational transfer function is miter-checked equal
#     to the operator's `render_smt` model (`arch_fma_f32`), reusing the same
#     alignment case-split renderer_miter.sh uses for the single-cycle fma.
#
# Balanced-at-L (B) + register-shorted-function-equals-op (A) ⟹ for a
# feed-forward pipeline, output[t+L] == op(input[t]) for ALL inputs. The
# "balanced feed-forward ⟹ transfer = L-delayed comb function" step is the
# standard retiming fact; the Lean development (Route B, ArchFpEquiv) formalizes
# it once and for all so A+B become a complete machine-checked proof for every
# shape. Here it is the harness contract.
#
# Requires: arch + dump_fp (built fresh unless ARCH_BIN set), yosys, z3, python3.
# Not run in CI (yosys/z3 absent in the sandbox). Run manually:
#
#   tests/fp_v1/smt_proof/staged_ops_miter.sh [outdir]
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
outdir="${1:-$(mktemp -d)}"
mkdir -p "$outdir"
timeout_s="${MITER_TIMEOUT:-600}"
split_jobs="${MITER_SPLIT_JOBS:-8}"

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch + dump_fp (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch --example dump_fp >&2 )
  ARCH_BIN="$repo/target/release/arch"
fi
DUMP_FP_BIN="${DUMP_FP_BIN:-$(dirname "$ARCH_BIN")/examples/dump_fp}"
"$DUMP_FP_BIN" smt > "$outdir/arch_defs.smt2"

# ── operator table ─────────────────────────────────────────────────────────
# NAME | staged module | latency L (input->out of the staged submodule) | \
#   render_smt fn | mode
#
# mode selects how the arithmetic half (Lemma A) is discharged:
#   fma          — register-shorted miter vs `arch_fma_f32`, alignment split
#   balance-only — Lemma B only; Lemma A deferred (see below)
#
# The block operators (`scaled_dot`, `scaled_quantize`, arch#955) run
# `balance-only`. Their arithmetic Lemma A — the register-shorted transfer
# function equals the combinational block operator — is SAT-hard: mitering the
# two independently bit-blasted `f32_add` reduction trees has no single
# collapsing split (unlike fma's one alignment gap), and times out even at N=2.
# For the block ops the equivalence rests instead on the composition of:
#   * Lemma B here (the staged pipeline is balanced — the check that catches the
#     non-power-of-two skew of arch#960, and skew generally);
#   * the *combinational* block schedule miter (`scaled_dot_miter.sh`, Theorem A
#     — emitted comb SV == the balanced-pairwise + one-at-a-time-scale schedule);
#   * the by-construction identity (the staged emitter cuts that same comb IR
#     into stages without changing operations) formalized generally by the Lean
#     retiming lemma (proofs/lean_fp_equiv/ArchFpEquiv/StagedPipeline.lean).
# `scaled_dot` is emittable only for power-of-two block sizes (the type checker
# rejects the rest — arch#960); `scaled_quantize`'s per-element multiplies are
# parallel at uniform depth, so any N balances.
#
# `L` is the staged *submodule* latency (the user-visible pipe_reg adds one more
# cycle via the wrapper's reset/valid register, an ordinary pipe_reg cascade).
ops=(
  "fma6|ArchF32FmaStaged6|5|arch_fma_f32|fma"
  "dot8|arch_scaled_dot_e2m1_8_e8m0_staged6|5|-|balance-only"
  "quant8|arch_scaled_quantize_e2m1_8_e8m0_floor_rne_staged5|4|-|balance-only"
)

# combinationalize: within a staged submodule's always_ff blocks (which are pure
# `r <= s;` copies), rewrite each to a continuous `assign r = s;` and drop the
# always_ff wrapper, yielding the register-shorted transfer function.
combinationalize () { # $1 = infile  $2 = module  $3 = outfile
  awk -v M="module $2" '
    $0 ~ "^"M {inmod=1}
    inmod && /always_ff @\(posedge clk\) begin/ {inff=1; next}
    inff && /^[[:space:]]*end[[:space:]]*$/ {inff=0; next}
    inff { line=$0; sub(/<=/,"=",line); sub(/^[[:space:]]*/,"  assign ",line); print line; next }
    {print}
    /^endmodule/ && inmod {inmod=0}
  ' "$1" > "$3"
}

echo "# module                                        lemmaB(balance)  lemmaA(arith)    L"
fail=0
for spec in "${ops[@]}"; do
  IFS='|' read -r name mod L fn split <<<"$spec"
  d="$outdir/$name"; mkdir -p "$d"

  # emit the staged SV for this operator
  case "$name" in
    fma6)
      cat > "$d/src.arch" <<ARCH
module ${name}_top
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in FP32;
  port b: in FP32;
  port c: in FP32;
  port y: out pipe_reg<FP32, 6> reset rst => 0.0;
  seq on clk rising
    y@6 <= fma<pipelined, 6>(a, b, c);
  end seq
end module ${name}_top
ARCH
      ;;
    dot8)
      cat > "$d/src.arch" <<'ARCH'
package DF
  type B8 = ScaledVec<FP4E2M1, 8, E8M0>;
end package DF
module dot8_top
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port a: in B8;
  port b: in B8;
  port o: out pipe_reg<FP32, 6> reset rst => 0.0;
  seq on clk rising
    o@6 <= scaled_dot<pipelined, 6>(a, b);
  end seq
end module dot8_top
ARCH
      ;;
    quant8)
      cat > "$d/src.arch" <<'ARCH'
package QF
  type B4 = ScaledVec<FP4E2M1, 8, E8M0>;
end package QF
module quant8_top
  port clk: in Clock<Sys>;
  port rst: in Reset<Sync, High>;
  port v: in Vec<FP32, 8>;
  port y: out pipe_reg<B4, 5> reset rst => 0;
  seq on clk rising
    y@5 <= scaled_quantize<B4, pipelined, 5>(v);
  end seq
end module quant8_top
ARCH
      ;;
  esac
  "$ARCH_BIN" build --staged-ops "$d/src.arch" -o "$d/staged.sv" >/dev/null 2>&1

  # ── Lemma B: structural balance / uniform latency ────────────────────────
  yosys -q -p "read_verilog -sv $d/staged.sv; hierarchy -top $mod; proc; flatten; opt_clean; write_json $d/staged.json" 2>/dev/null
  if python3 "$here/../synth/pipeline_balance.py" "$d/staged.json" "$mod" --expect "$L" >"$d/balance.txt" 2>&1; then
    vB="balanced@$L"
  else
    vB="UNBALANCED"; fail=1
  fi

  # ── Lemma A: register-shorted arithmetic miter vs render_smt ──────────────
  if [[ "$split" == "balance-only" ]]; then
    # Block operators: Lemma A is SAT-hard (see the operator table). The balance
    # check above is the structural guard; arithmetic equivalence rests on the
    # comb schedule miter + the by-construction/Lean retiming composition.
    vA="deferred(block)"
    printf "%-46s %-16s %-16s %s\n" "$mod" "$vB" "$vA" "$L"
    continue
  fi
  combinationalize "$d/staged.sv" "$mod" "$d/comb.v"
  yosys -q -p "read_verilog -sv $d/comb.v; hierarchy -top $mod; proc; flatten; opt_clean; check -assert; write_smt2 $d/comb.smt2" 2>/dev/null
  # specialize yosys's state sort away -> pure QF_BV (combinational: one state)
  python3 - "$mod" "$d" <<'PYSPEC'
import re, sys
mod, d = sys.argv[1], sys.argv[2]
src = open(f'{d}/comb.smt2').read()
src = re.sub(r'\(declare-sort \|%s_s\| 0\)\n' % mod, '', src)
src = re.sub(r'\(declare-fun \|%s_is\| \(\|%s_s\|\) Bool\)\n' % (mod, mod), '', src)
src = re.sub(r'\(declare-fun (\|[^|]+\|) \(\|%s_s\|\) ' % mod, r'(declare-const \1 ', src)
src = src.replace(f'((state |{mod}_s|))', '()')
src = re.sub(r'\((\|[^|]+\|) state\)', r'\1', src)
src = re.sub(r'\(define-fun \|%s_t\|[^\n]*\n' % mod, '', src)
open(f'{d}/comb.qfbv.smt2', 'w').write(src)
PYSPEC

  {
    cat "$outdir/arch_defs.smt2" "$d/comb.qfbv.smt2"
    cat <<SPL
(define-fun aa () (_ BitVec 32) |${mod}_n a|)
(define-fun bb () (_ BitVec 32) |${mod}_n b|)
(define-fun cc () (_ BitVec 32) |${mod}_n c|)
(assert (not (= |${mod}_n y| ($fn aa bb cc))))
SPL
  } > "$d/miter_body.smt2"

  if [[ "$split" == "fma" ]]; then
    # The monolithic fma miter is SAT-hard (the 24x24 product sits in every
    # aligned bit's cone). Split on the alignment gap diff = |eunb(a)+eunb(b) -
    # eunb(c)|; within each constant diff both barrel shifters collapse and the
    # sub-miter is near-structural. diff in [0,508] plus a catch-all (unsat by
    # range). All 510 unsat => the miter is unsat.  (Same split as
    # renderer_miter.sh's single-cycle fma; shares the exact spl_* formulas.)
    sed '$d' "$d/miter_body.smt2" > "$d/split_body.smt2"
    cat >> "$d/split_body.smt2" <<'SPL'
(define-fun spl_eunb ((x (_ BitVec 32))) (_ BitVec 16)
  (ite (= ((_ extract 30 23) x) (_ bv0 8)) (_ bv65387 16)
       (bvsub ((_ zero_extend 8) ((_ extract 30 23) x)) (_ bv150 16))))
SPL
    cat >> "$d/split_body.smt2" <<SPL
(define-fun spl_eab () (_ BitVec 16) (bvadd (spl_eunb aa) (spl_eunb bb)))
(define-fun spl_ec  () (_ BitVec 16) (spl_eunb cc))
(define-fun spl_dif () (_ BitVec 16)
  (ite (bvsle spl_ec spl_eab) (bvsub spl_eab spl_ec) (bvsub spl_ec spl_eab)))
(assert (not (= |${mod}_n y| ($fn aa bb cc))))
SPL
    sp="$d/split"; mkdir -p "$sp"
    # MITER_SMOKE=N runs only diffs 0..N + the catch-all (fast self-test of the
    # harness mechanics, NOT a proof — the verdict is annotated "(smoke)").
    hi=508; [[ -n "${MITER_SMOKE:-}" ]] && hi="$MITER_SMOKE"
    for k in $(seq 0 "$hi"); do
      { cat "$d/split_body.smt2"; echo "(assert (= spl_dif (_ bv${k} 16)))"; echo "(check-sat)"; } > "$sp/case_$k.smt2"
    done
    { cat "$d/split_body.smt2"; echo "(assert (bvugt spl_dif (_ bv508 16)))"; echo "(check-sat)"; } > "$sp/case_over.smt2"
    want=$((hi + 2))
    ls "$sp"/case_*.smt2 \
      | MITER_TO="$timeout_s" xargs -P "$split_jobs" -I{} bash -c \
          'echo "{} $(z3 -T:"$MITER_TO" {} 2>&1 | tail -1)"' > "$sp/results.txt"
    nbad=$(grep -cv 'unsat$' "$sp/results.txt" || true)
    ncase=$(wc -l < "$sp/results.txt" | tr -d ' ')
    if [[ "$nbad" == "0" && "$ncase" == "$want" ]]; then
      vA="unsat"; [[ -n "${MITER_SMOKE:-}" ]] && vA="unsat(smoke)"
    else vA="split-fail($nbad/$ncase)"; fail=1; fi
  else
    echo "(check-sat)" >> "$d/miter_body.smt2"
    vA=$(z3 -T:"$timeout_s" "$d/miter_body.smt2" 2>&1 | tail -1)
    [[ "$vA" == "unsat" ]] || fail=1
  fi

  printf "%-46s %-16s %-16s %s\n" "$mod" "$vB" "$vA" "$L"
done
exit $fail
