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
pub const TRACTABLE: &[&str] =
    &["eq", "ne", "lt", "le", "gt", "ge", "narrow", "widen", "to_sint", "to_uint"];

/// Generated identically from the IR, but not solver-tractable (2^64 / fused).
pub const ARITHMETIC: &[&str] = &["mul", "add", "sub", "fma"];

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
    s.push_str(&crate::fp_ir::render_smt(&crate::fp_ops::fp_functions(profile)));

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
        other => panic!("unknown proof op {other}"),
    }
    s
}
