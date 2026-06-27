//! SMT-LIB2 equivalence proofs for the FP helpers, generated from the SAME
//! shared IR as the synthesizable SystemVerilog (`crate::fp_ops`).
//!
//! `equiv_proof(op, profile)` returns a complete SMT-LIB2 query: the helper
//! `define-fun`s rendered from the IR, followed by a miter asserting the negation
//! of equivalence to the IEEE-754 `FloatingPoint` theory. `unsat` from a solver
//! ⇒ the emitted RTL operator equals IEEE-754 over its entire input space
//! (doc/plan_fp_types.md §8.1). Because the RTL and this model are rendered from
//! one source they cannot drift.
//!
//! `TRACTABLE` lists the operators a bit-vector FP solver (z3) discharges
//! quickly. The RNE arithmetic (`mul`/`add`/`sub`/`fma`) is generated identically
//! but its 2^64 miter is not solver-tractable; it stays on the §8.2 differential
//! backstop (see `ARITHMETIC`).

use crate::FpCompat;

/// Operators whose generated miter z3 discharges exhaustively.
pub const TRACTABLE: &[&str] = &[
    "eq", "ne", "lt", "le", "gt", "ge", "narrow", "widen", "to_sint", "to_uint",
];

/// f32 add/sub — machine-proved `unsat` vs `fp.add`/`fp.sub` over all 2^64
/// inputs (~80 s each in z3). Tractable because the bounded adder keeps the
/// datapath ~56-bit (no multiplier) — the SAT instance stays small.
pub const F32_ADD: &[&str] = &["add", "sub"];

/// f32 mul/fma — generated identically from the IR, but their 24x24-multiplier
/// equivalence is SAT-hard at 2^64; z3 times out. These stay on the §8.2
/// differential backstop.
pub const ARITHMETIC: &[&str] = &["mul", "fma"];

/// BF16 comparisons — route through the cheap f32 compare path; prove instantly.
pub const BF16_CMP: &[&str] = &[
    "bf16_eq", "bf16_ne", "bf16_lt", "bf16_le", "bf16_gt", "bf16_ge",
];

/// BF16 RNE arithmetic — the §8.1 primary target. Routed through the f32
/// datapath, but the small input space (2^32) makes the miter solver-tractable:
/// z3 discharges each `unsat` (mul/add/sub in seconds–minutes). `bf16_fma`
/// (2^48) is heavier — included when it converges within the test's cap.
pub const BF16_ARITH: &[&str] = &["bf16_mul", "bf16_add", "bf16_sub"];

fn nan32_hex(p: FpCompat) -> &'static str {
    match p {
        FpCompat::Riscv => "#x7FC00000",
        FpCompat::Cuda => "#x7FFFFFFF",
    }
}
fn nan16_hex(p: FpCompat) -> &'static str {
    match p {
        FpCompat::Riscv => "#x7FC0",
        FpCompat::Cuda => "#x7FFF",
    }
}

/// Full SMT-LIB2 proof query for `op` under `profile`.
pub fn equiv_proof(op: &str, profile: FpCompat) -> String {
    let n32 = nan32_hex(profile);
    let n16 = nan16_hex(profile);
    let mut s = String::new();
    s.push_str("(set-logic QF_FPBV)\n(define-sort F () (_ FloatingPoint 8 24))\n");
    s.push_str(&crate::fp_ir::render_smt(&crate::fp_ops::fp_functions(
        profile,
    )));

    let pre = "(declare-fun a () (_ BitVec 32))\n(declare-fun b () (_ BitVec 32))\n\
               (define-fun fa () F ((_ to_fp 8 24) a))\n(define-fun fb () F ((_ to_fp 8 24) b))\n";
    let cmp = |f: &str, spec: &str| {
        format!("{pre}(assert (not (= (= ({f} a b) #b1) {spec})))\n(check-sat)\n")
    };
    let arith = |f: &str, fpop: &str| {
        format!(
            "{pre}(define-fun fr () F ({fpop} RNE fa fb))\n(define-fun rr () (_ BitVec 32) ({f} a b))\n\
             (assert (not (ite (fp.isNaN fr) (= rr {n32}) (= ((_ to_fp 8 24) rr) fr))))\n(check-sat)\n"
        )
    };
    match op {
        "eq" => s.push_str(&cmp("arch_f32_eq", "(fp.eq fa fb)")),
        "ne" => s.push_str(&cmp("arch_f32_ne", "(not (fp.eq fa fb))")),
        "lt" => s.push_str(&cmp("arch_f32_lt", "(fp.lt fa fb)")),
        "le" => s.push_str(&cmp("arch_f32_le", "(fp.leq fa fb)")),
        "gt" => s.push_str(&cmp("arch_f32_gt", "(fp.gt fa fb)")),
        "ge" => s.push_str(&cmp("arch_f32_ge", "(fp.geq fa fb)")),
        "mul" => s.push_str(&arith("arch_f32_mul", "fp.mul")),
        "add" => s.push_str(&arith("arch_f32_add", "fp.add")),
        "sub" => s.push_str(&arith("arch_f32_sub", "fp.sub")),
        "narrow" => s.push_str(&format!(
            "(declare-fun x () (_ BitVec 32))\n(define-fun fx () F ((_ to_fp 8 24) x))\n\
             (define-fun spec () (_ FloatingPoint 8 8) ((_ to_fp 8 8) RNE fx))\n\
             (define-fun rr () (_ BitVec 16) (arch_f32_to_bf16 x))\n\
             (assert (not (ite (fp.isNaN spec) (= rr {n16}) (= ((_ to_fp 8 8) rr) spec))))\n(check-sat)\n"
        )),
        "widen" => s.push_str(&format!(
            "(declare-fun h () (_ BitVec 16))\n\
             (define-fun spec () F ((_ to_fp 8 24) RNE ((_ to_fp 8 8) h)))\n\
             (define-fun rr () (_ BitVec 32) (arch_bf16_to_f32 h))\n\
             (assert (not (ite (fp.isNaN spec) (= rr {n32}) (= ((_ to_fp 8 24) rr) spec))))\n(check-sat)\n"
        )),
        "to_sint" => s.push_str(
            "(declare-fun x () (_ BitVec 32))\n(define-fun fx () F ((_ to_fp 8 24) x))\n\
             (define-fun n () (_ BitVec 32) (_ bv32 32))\n\
             (define-fun spec () (_ BitVec 64) ((_ fp.to_sbv 64) RTZ fx))\n\
             (define-fun rr () (_ BitVec 64) (arch_f32_to_sint x n))\n\
             (assert (and (not (fp.isNaN fx)) (not (fp.isInfinite fx)) (fp.lt (fp.abs fx) ((_ to_fp 8 24) RNE 2147483648.0))))\n\
             (assert (not (= ((_ sign_extend 32) ((_ extract 31 0) rr)) spec)))\n(check-sat)\n",
        ),
        "to_uint" => s.push_str(
            "(declare-fun x () (_ BitVec 32))\n(define-fun fx () F ((_ to_fp 8 24) x))\n\
             (define-fun n () (_ BitVec 32) (_ bv32 32))\n\
             (define-fun spec () (_ BitVec 64) ((_ fp.to_ubv 64) RTZ fx))\n\
             (define-fun rr () (_ BitVec 64) (arch_f32_to_uint x n))\n\
             (assert (and (not (fp.isNaN fx)) (not (fp.isInfinite fx)) (fp.geq fx ((_ to_fp 8 24) RNE 0.0)) (fp.lt fx ((_ to_fp 8 24) RNE 4294967296.0))))\n\
             (assert (not (= ((_ zero_extend 32) ((_ extract 31 0) rr)) spec)))\n(check-sat)\n",
        ),
        "fma" => s.push_str(&format!(
            "(declare-fun a () (_ BitVec 32))\n(declare-fun b () (_ BitVec 32))\n(declare-fun c () (_ BitVec 32))\n\
             (define-fun fa () F ((_ to_fp 8 24) a))\n(define-fun fb () F ((_ to_fp 8 24) b))\n(define-fun fc () F ((_ to_fp 8 24) c))\n\
             (define-fun fr () F (fp.fma RNE fa fb fc))\n(define-fun rr () (_ BitVec 32) (arch_fma_f32 a b c))\n\
             (assert (not (ite (fp.isNaN fr) (= rr {n32}) (= ((_ to_fp 8 24) rr) fr))))\n(check-sat)\n"
        )),
        // Bounded sticky-fold FMA == exact-wide (470-bit) reference FMA, all
        // inputs. Pure bit-vector: the shared 24x24 `mul` and the identical
        // special-case wrapper appear on both sides, so a CSE-ing bit-blaster
        // cancels them and never solves a multiplier equivalence. `unsat` ⇒ the
        // sticky-fold is bit-identical to the machine-proved exact-wide FMA over
        // the whole 2^96 input space, transferring its correctness.
        "fma_equiv" => {
            s.push_str(&crate::fp_ir::render_smt(&[crate::fp_ops::fma_f32_ref(profile)]));
            s.push_str(
                "(declare-fun a () (_ BitVec 32))\n(declare-fun b () (_ BitVec 32))\n(declare-fun c () (_ BitVec 32))\n\
                 (assert (not (= (arch_fma_f32 a b c) (arch_fma_f32_ref a b c))))\n(check-sat)\n",
            );
        }
        // Multiply-abstracted variant: the product `mp` is a free 48-bit input
        // (not `mul(mant_a, mant_b)`), so the query has no multiplier at all.
        // Proving new == ref for all (a,b,c,mp) is a pure shift/add/round miter
        // (solver-tractable like f32 add) and is strictly stronger than the
        // real-product case. `unsat` ⇒ sticky-fold FMA ≡ exact-wide FMA.
        "fma_equiv_abs" => {
            s.push_str(&crate::fp_ir::render_smt(&[
                crate::fp_ops::fma_param(true, profile),
                crate::fp_ops::fma_param(false, profile),
            ]));
            s.push_str(
                "(declare-fun a () (_ BitVec 32))\n(declare-fun b () (_ BitVec 32))\n(declare-fun c () (_ BitVec 32))\n(declare-fun mp () (_ BitVec 48))\n\
                 (assert (not (= (arch_fma_param_new a b c mp) (arch_fma_param_ref a b c mp))))\n(check-sat)\n",
            );
        }
        // ── bf16: spec on (_ FloatingPoint 8 8); RTL routes widen->f32->narrow ──
        _ if op.starts_with("bf16_") => {
            let bpre = "(declare-fun a () (_ BitVec 16))\n(declare-fun b () (_ BitVec 16))\n\
                        (define-fun ga () (_ FloatingPoint 8 8) ((_ to_fp 8 8) a))\n\
                        (define-fun gb () (_ FloatingPoint 8 8) ((_ to_fp 8 8) b))\n";
            let bcmp = |f: &str, spec: &str| {
                format!("{bpre}(assert (not (= (= ({f} a b) #b1) {spec})))\n(check-sat)\n")
            };
            let barith = |f: &str, fpop: &str| {
                format!(
                    "{bpre}(define-fun gr () (_ FloatingPoint 8 8) ({fpop} RNE ga gb))\n\
                     (define-fun rr () (_ BitVec 16) ({f} a b))\n\
                     (assert (not (ite (fp.isNaN gr) (= rr {n16}) (= ((_ to_fp 8 8) rr) gr))))\n(check-sat)\n"
                )
            };
            match op {
                "bf16_eq" => s.push_str(&bcmp("arch_bf16_eq", "(fp.eq ga gb)")),
                "bf16_ne" => s.push_str(&bcmp("arch_bf16_ne", "(not (fp.eq ga gb))")),
                "bf16_lt" => s.push_str(&bcmp("arch_bf16_lt", "(fp.lt ga gb)")),
                "bf16_le" => s.push_str(&bcmp("arch_bf16_le", "(fp.leq ga gb)")),
                "bf16_gt" => s.push_str(&bcmp("arch_bf16_gt", "(fp.gt ga gb)")),
                "bf16_ge" => s.push_str(&bcmp("arch_bf16_ge", "(fp.geq ga gb)")),
                "bf16_mul" => s.push_str(&barith("arch_bf16_mul", "fp.mul")),
                "bf16_add" => s.push_str(&barith("arch_bf16_add", "fp.add")),
                "bf16_sub" => s.push_str(&barith("arch_bf16_sub", "fp.sub")),
                "bf16_fma" => s.push_str(&format!(
                    "(declare-fun a () (_ BitVec 16))\n(declare-fun b () (_ BitVec 16))\n(declare-fun c () (_ BitVec 16))\n\
                     (define-fun ga () (_ FloatingPoint 8 8) ((_ to_fp 8 8) a))\n\
                     (define-fun gb () (_ FloatingPoint 8 8) ((_ to_fp 8 8) b))\n\
                     (define-fun gc () (_ FloatingPoint 8 8) ((_ to_fp 8 8) c))\n\
                     (define-fun gr () (_ FloatingPoint 8 8) (fp.fma RNE ga gb gc))\n\
                     (define-fun rr () (_ BitVec 16) (arch_fma_bf16 a b c))\n\
                     (assert (not (ite (fp.isNaN gr) (= rr {n16}) (= ((_ to_fp 8 8) rr) gr))))\n(check-sat)\n"
                )),
                other => panic!("unknown bf16 proof op {other}"),
            }
        }
        other => panic!("unknown proof op {other}"),
    }
    s
}
