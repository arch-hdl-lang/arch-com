#!/usr/bin/env bash
# Value-equivalence miter for the CHECKED f32 add (`arch_f32_add_checked`, backs
# the surface `checked(a + b) : FpResult<FP32>`, added in PR #966).
#
# `arch_f32_add_checked` returns a 36-bit `{inexact[35], invalid[34],
# underflow[33], overflow[32], value[31:0]}`. Its `value[31:0]` datapath is a
# copy-paste of `arch_f32_add`'s body (and it calls `normround_flags`, whose
# `result` is a copy-paste of `normround`). That duplication is a *divergence
# trap*: a future rounding/edge-case fix to `arch_f32_add` (or `normround`) that
# is not mirrored into the checked variant would silently diverge — the value
# path is where users read the arithmetic, and nothing today pins the two copies
# together.
#
# This miter closes the trap by proving, over ALL 2^64 input pairs,
#
#     arch_f32_add_checked(a, b)[31:0]  ==  arch_f32_add(a, b)
#
# Both sides are `render_smt` define-funs (emitted by `dump_fp smt`), so — unlike
# the renderer_miter — no yosys frontend is in the loop: the miter is pure QF_BV.
# No NaN-payload disjunct is needed (mirroring renderer_miter.sh's F32Add row,
# which uses none): both sides emit the identical canonical NaN `nan32(p)` for
# every NaN-producing case, so bit-equality is the right relation everywhere.
# The add datapath carries no multiplier, so z3 discharges the whole miter
# directly in well under a second (contrast the fma alignment case-split).
#
# Modes:
#   (default)            prove equivalence -> expect `unsat`.
#   MITER_NONVACUITY=1   XOR bit 0 into the checked value first -> expect `sat`.
#                        Guards against a vacuously-unsat miter (a contradiction
#                        in the setup would make ANY assertion unsat); a genuine
#                        one-bit divergence must be detectable as `sat`.
#   MITER_UNDERFLOW_UNREACHABLE=1
#                        prove the underflow flag (bit 33) is NEVER set, for any
#                        (a, b) -> expect `unsat`. For f32 ADD a subnormal result
#                        is always exact (every finite f32 is a multiple of
#                        2^-149, and the subnormal grid is 2^-149), so the
#                        inexact-gated underflow can never fire — the
#                        Hauser/Sterbenz "subnormal add is exact" property. This
#                        documents that the underflow bit is dead for the ADD
#                        checked op (it would come alive if `normround_flags` were
#                        reused by a future checked mul/fma, a different function).
#
# Requires: arch + dump_fp (built fresh unless ARCH_BIN set), z3, python3-free.
# z3 is present in this sandbox, so unlike the other smt_proof miters this one
# also runs its full proof under `cargo test` (see
# tests/fp_add_checked_value_miter_test.rs), not just a smoke slice.
#
#   tests/fp_v1/smt_proof/add_checked_miter.sh [outdir]
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
outdir="${1:-$(mktemp -d)}"
mkdir -p "$outdir"
timeout_s="${MITER_TIMEOUT:-600}"

if [[ -z "${ARCH_BIN:-}" ]]; then
  echo "# building arch + dump_fp (release)…" >&2
  ( cd "$repo" && cargo build --release --bin arch --example dump_fp >&2 )
  ARCH_BIN="$repo/target/release/arch"
fi
DUMP_FP_BIN="${DUMP_FP_BIN:-$(dirname "$ARCH_BIN")/examples/dump_fp}"
"$DUMP_FP_BIN" smt > "$outdir/arch_defs.smt2"

if [[ -n "${MITER_UNDERFLOW_UNREACHABLE:-}" ]]; then
  # underflow flag is bit 33 of the 36-bit packing; is it ever set?
  assertion='(assert (= ((_ extract 33 33) (arch_f32_add_checked a b)) #b1))'
  label="arch_f32_add_checked underflow"
  want="unsat"
else
  # `value[31:0]` is the low 32 bits of the 36-bit checked packing.
  val='((_ extract 31 0) (arch_f32_add_checked a b))'
  if [[ -n "${MITER_NONVACUITY:-}" ]]; then
    # Flip one bit so the checked value is deliberately wrong; a correct miter
    # must now find a counterexample (sat).
    val="(bvxor $val #x00000001)"
    want="sat"
  else
    want="unsat"
  fi
  assertion="(assert (not (= $val (arch_f32_add a b))))"
  label="arch_f32_add_checked[31:0]"
fi

{
  cat "$outdir/arch_defs.smt2"
  echo '(declare-const a (_ BitVec 32))'
  echo '(declare-const b (_ BitVec 32))'
  echo "$assertion"
  echo '(check-sat)'
} > "$outdir/miter.smt2"

verdict=$(z3 -T:"$timeout_s" "$outdir/miter.smt2" 2>&1 | tail -1)
printf "%-30s %-8s (want %s)\n" "$label" "$verdict" "$want"
[[ "$verdict" == "$want" ]] && exit 0 || exit 1
