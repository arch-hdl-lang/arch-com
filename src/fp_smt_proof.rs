//! SMT-LIB2 equivalence proofs for the FP helpers, generated from the SAME
//! shared IR as the synthesizable SystemVerilog (`crate::fp_ops`).
//!
//! `equiv_proof(op, profile)` returns a complete SMT-LIB2 query: the helper
//! `define-fun`s rendered from the IR, followed by a miter asserting the negation
//! of equivalence to the IEEE-754 `FloatingPoint` theory. `unsat` from a solver
//! ⇒ the emitted RTL operator equals IEEE-754 over its entire input space
//! (doc/archive/plan_fp_types.md §8.1). Because the RTL and this model are rendered from
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

/// FP8 comparisons — E5M2 against the `(_ FloatingPoint 5 3)` theory
/// directly; E4M3 against IEEE compare semantics on the (separately proven)
/// widened values. 2^16 inputs; prove instantly.
pub const FP8_CMP: &[&str] = &[
    "e5m2_eq", "e5m2_ne", "e5m2_lt", "e5m2_le", "e5m2_gt", "e5m2_ge", "e4m3_eq", "e4m3_ne",
    "e4m3_lt", "e4m3_le", "e4m3_gt", "e4m3_ge",
];

/// FP8 conversions. `e5m2_widen`/`e5m2_narrow` are pure `(_ FloatingPoint 5 3)`
/// theory (the cuda narrow adds a small satfinite wrapper). E4M3 is OCP OFP8 —
/// **not** an IEEE format — so its specs are hand-written two-region models:
/// the widen exploits the encoding coincidence with IEEE `(4,4)` below
/// exponent 15 (plus 7 explicit constants for the finite top binade 256..448),
/// and the narrow rounds via `(_ FloatingPoint 8 4)` (4-bit significand, wide
/// exponent — the E4M3 *normal* grid) with a scaled `fp.roundToIntegral` model
/// for the subnormal region and the OCP ≥480 overflow rule. The widen miter
/// grounds the hand spec against the IR; narrow/arith/compare specs then decode
/// results through the proven widen.
pub const FP8_CONV: &[&str] = &["e5m2_widen", "e5m2_narrow", "e4m3_widen", "e4m3_narrow"];

/// FP8 RNE binary arithmetic — expected `unsat` = proven correctly rounded.
/// The inner f32 op is *exact* for E4M3 add/sub/mul (≤19-bit alignment span)
/// and for both formats' mul (≤8-bit products), so widen→f32-op→narrow
/// rounds once from the exact value there. E5M2 add/sub can round in f32
/// first (up to ~35-bit exact sums) — a genuine double rounding whose
/// innocuousness is NOT assumed from any p ≥ 2q+2-style margin (a known
/// fallacy for round-to-nearest; see the bf16_fma discussion in
/// tests/fp_v1/smt_proof/README.md) but PROVED by the miter itself against
/// the exact-wide single-rounding spec.
pub const FP8_ARITH: &[&str] = &[
    "e5m2_mul", "e5m2_add", "e5m2_sub", "e4m3_mul", "e4m3_add", "e4m3_sub",
];

/// FP8 fused-f32-accumulate fma vs a TRUE correctly-rounded reference (exact
/// fma in a wide-significand sort, then one fp8 rounding).
///
/// - `e4m3_fma_cr`: **`unsat` — proved** under both profiles (z3, 2026-08-01).
///   The double rounding is innocuous across E4M3's dynamic range, so the
///   fused f32-accumulate fma IS correctly-rounded E4M3 fma. Confirmed by
///   the exhaustive 2^24 characterization (0 mismatches).
/// - `e5m2_fma_cr`: expected `sat` — the second rounding is real for E5M2
///   (witness fma(0x1E,0x7A,0x01): exact 288+2^-16 → CR 320, fused 256;
///   18960/2^24 mismatches riscv, 15888/2^24 cuda, all 1 ULP — see
///   `examples/fp8_fma_char.rs`). Not asserted by tests.
pub const FP8_FMA_CR: &[&str] = &["e5m2_fma_cr", "e4m3_fma_cr"];

/// OCP MX sub-8-bit storage formats — conversions only (no arithmetic exists
/// to prove: `Ty::is_float_arith` rejects operators on these types).
///
/// None of the three is an IEEE format — all three are **all-finite**: no Inf,
/// no NaN, every encoding is a value. So each gets a hand-written spec in the
/// same two-region shape as E4M3, with two differences that follow from
/// all-finiteness:
///
/// - the widen has **no NaN arm at all** (E4M3's spec needs one for `S.1111.111`);
/// - the narrow **saturates under both `--fp-compat` profiles**, because the
///   encoding space has nowhere else to go. That is not a modelling choice —
///   it is the one place the profiles provably cannot differ, and these miters
///   are what pins it.
///
/// The widen exploits the same encoding coincidence E4M3 does: below the
/// all-ones exponent an all-finite format is bit-for-bit IEEE `(eb, mb+1)`
/// (same bias, same subnormal rule), so only the finite top binade needs
/// explicit constants. The narrow rounds via `(_ FloatingPoint 8 mb+1)` — the
/// format's *normal* grid at unbounded-for-f32 exponent range — with a scaled
/// `fp.roundToIntegral` for the subnormal region and a saturate-above rule.
///
/// At 4 bits this is unusually strong: `e2m1_widen` is exhaustive over all 16
/// encodings and `e2m1_narrow` over all 2^32 FP32 inputs.
pub const MX_CONV: &[&str] = &[
    "e2m1_widen",
    "e2m1_narrow",
    "e2m3_widen",
    "e2m3_narrow",
    "e3m2_widen",
    "e3m2_narrow",
];

/// An OCP all-finite storage format, described by constants transcribed from
/// the OCP spec's value tables rather than recomputed from `(eb, mb)`.
///
/// That transcription is the point. A spec derived by the same arithmetic the
/// IR uses would agree with a wrong IR; these are the published numbers, so a
/// sign/bias/shift error in `fp8_round` has nothing to hide behind.
struct AllFinite {
    /// Encoding width in bits (`1 + eb + mb`).
    w: u32,
    eb: u32,
    mb: u32,
    widen_fn: &'static str,
    narrow_fn: &'static str,
    /// Smallest positive normal, `2^(1-bias)`.
    min_normal: &'static str,
    /// Reciprocal of the subnormal grid spacing `2^(1-bias-mb)`. The grid runs
    /// through the min-normal binade (same spacing there), so splitting the two
    /// regions at `min_normal` is sound.
    sub_scale: &'static str,
    /// First point ABOVE the top binade on the `(8, mb+1)` grid. Reaching it is
    /// overflow — and with no Inf in the format, overflow means saturation.
    over: &'static str,
    /// The finite top binade in magnitude order, one constant per mantissa
    /// code: the OCP table verbatim.
    topmag: &'static [&'static str],
}

fn all_finite_fmt(op: &str) -> Option<AllFinite> {
    // ±{0, 0.5, 1, 1.5, 2, 3, 4, 6} — the entire FP4 E2M1 value set.
    let e2m1 = AllFinite {
        w: 4,
        eb: 2,
        mb: 1,
        widen_fn: "arch_e2m1_to_f32",
        narrow_fn: "arch_f32_to_e2m1",
        min_normal: "1.0",
        sub_scale: "2.0",
        over: "8.0",
        topmag: &["4.0", "6.0"],
    };
    let e2m3 = AllFinite {
        w: 6,
        eb: 2,
        mb: 3,
        widen_fn: "arch_e2m3_to_f32",
        narrow_fn: "arch_f32_to_e2m3",
        min_normal: "1.0",
        sub_scale: "8.0",
        over: "8.0",
        topmag: &["4.0", "4.5", "5.0", "5.5", "6.0", "6.5", "7.0", "7.5"],
    };
    let e3m2 = AllFinite {
        w: 6,
        eb: 3,
        mb: 2,
        widen_fn: "arch_e3m2_to_f32",
        narrow_fn: "arch_f32_to_e3m2",
        min_normal: "0.25",
        sub_scale: "16.0",
        over: "32.0",
        topmag: &["16.0", "20.0", "24.0", "28.0"],
    };
    match op {
        _ if op.starts_with("e2m1_") => Some(e2m1),
        _ if op.starts_with("e2m3_") => Some(e2m3),
        _ if op.starts_with("e3m2_") => Some(e3m2),
        _ => None,
    }
}

/// Widen + narrow miters for one all-finite storage format.
fn all_finite_proof(op: &str, f: &AllFinite) -> String {
    let AllFinite { w, eb, mb, .. } = *f;
    let ones = |n: u32| "1".repeat(n as usize);
    // `topmag`: the OCP top-binade table as a mantissa-indexed ite chain.
    let mut top = format!("((_ to_fp 8 24) RNE {})", f.topmag[f.topmag.len() - 1]);
    for (m, v) in f.topmag.iter().enumerate().rev().skip(1) {
        top = format!(
            "(ite (= mf #b{:0width$b}) ((_ to_fp 8 24) RNE {v}) {top})",
            m,
            width = mb as usize
        );
    }
    match op {
        _ if op.ends_with("_widen") => format!(
            "(declare-fun h () (_ BitVec {w}))\n\
             (define-fun rr () (_ BitVec 32) ({} h))\n\
             (define-fun sneg () Bool (= ((_ extract {} {}) h) #b1))\n\
             (define-fun ef () (_ BitVec {eb}) ((_ extract {} {mb}) h))\n\
             (define-fun mf () (_ BitVec {mb}) ((_ extract {} 0) h))\n\
             (define-fun topmag () F {top})\n\
             (define-fun spec () F (ite (= ef #b{})\n\
                                       (ite sneg (fp.neg topmag) topmag)\n\
                                       ((_ to_fp 8 24) RNE ((_ to_fp {eb} {}) h))))\n\
             (assert (not (= ((_ to_fp 8 24) rr) spec)))\n(check-sat)\n",
            f.widen_fn,
            w - 1,
            w - 1,
            w - 2,
            mb - 1,
            ones(eb),
            mb + 1
        ),
        _ if op.ends_with("_narrow") => format!(
            "(declare-fun x () (_ BitVec 32))\n\
             (define-fun v () F ((_ to_fp 8 24) x))\n\
             (define-fun rr () (_ BitVec {w}) ({} x))\n\
             (define-fun cS () F ((_ to_fp 8 24) RNE {}))\n\
             (define-fun minn () F ((_ to_fp 8 24) RNE {}))\n\
             (define-fun rN () (_ FloatingPoint 8 {sb}) ((_ to_fp 8 {sb}) RNE v))\n\
             (define-fun rNf () F ((_ to_fp 8 24) RNE rN))\n\
             (define-fun subv () F (fp.div RNE (fp.roundToIntegral RNE (fp.mul RNE v cS)) cS))\n\
             (define-fun is_sub () Bool (fp.lt (fp.abs v) minn))\n\
             (define-fun specv () F (ite is_sub subv rNf))\n\
             (define-fun ovf () Bool (and (not is_sub)\n\
                                     (or (fp.isInfinite rN)\n\
                                         (fp.geq (fp.abs rN) ((_ to_fp 8 {sb}) RNE {})))))\n\
             (define-fun sat () (_ BitVec {w})\n\
               (ite (= ((_ extract 31 31) x) #b1) #b1{mag} #b0{mag}))\n\
             (assert (not (ite (fp.isNaN v) (= rr sat)\n\
                           (ite ovf (= rr sat)\n\
                           (= ((_ to_fp 8 24) ({} rr)) specv)))))\n(check-sat)\n",
            f.narrow_fn,
            f.sub_scale,
            f.min_normal,
            f.over,
            f.widen_fn,
            sb = mb + 1,
            mag = ones(w - 1),
        ),
        other => panic!("unknown all-finite proof op {other}"),
    }
}

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
fn nan8_e5m2_hex(p: FpCompat) -> &'static str {
    match p {
        FpCompat::Riscv => "#x7E",
        FpCompat::Cuda => "#x7F",
    }
}
/// E4M3 has exactly one NaN encoding (OFP8) — no profile choice.
fn nan8_e4m3_hex(_p: FpCompat) -> &'static str {
    "#x7F"
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
        // ── e5m2: direct (_ FloatingPoint 5 3) theory (IEEE-style format) ──
        _ if op.starts_with("e5m2_") => {
            let n8 = nan8_e5m2_hex(profile);
            let epre = "(declare-fun a () (_ BitVec 8))\n(declare-fun b () (_ BitVec 8))\n\
                        (define-fun ga () (_ FloatingPoint 5 3) ((_ to_fp 5 3) a))\n\
                        (define-fun gb () (_ FloatingPoint 5 3) ((_ to_fp 5 3) b))\n";
            let ecmp = |f: &str, spec: &str| {
                format!("{epre}(assert (not (= (= ({f} a b) #b1) {spec})))\n(check-sat)\n")
            };
            // riscv: RNE overflow lands on ±inf exactly as the theory does,
            // so `=` on the decoded result is the whole spec.
            // cuda: satfinite — an infinite ideal result (overflow OR ±inf
            // inputs propagating) maps to ±max-finite 0x7B/0xFB instead.
            //
            // `gr` construction: `direct` uses fp.<op> on (5,3) itself;
            // `wide` computes the EXACT result in (_ FloatingPoint 8 53)
            // (3-bit significands, ≤32-binade alignment span → ≤35 bits, so
            // fp.add/sub/mul there is exact) and rounds ONCE into (5,3) —
            // semantically the same correctly-rounded spec. add/sub use
            // `wide` because z3 (4.15) returns `unknown` on (5,3) fp.add
            // miters even with pinned operands (a rewriter incompleteness,
            // not search hardness — mul and to_fp on the same sort
            // discharge fine); the wide form also has the nice property of
            // not depending on z3's (5,3) fp.add semantics at all.
            let earith = |f: &str, fpop: &str, wide: bool| {
                let gr_def = if wide {
                    format!(
                        "(define-fun wa () (_ FloatingPoint 8 53) ((_ to_fp 8 53) RNE ga))\n\
                         (define-fun wb () (_ FloatingPoint 8 53) ((_ to_fp 8 53) RNE gb))\n\
                         (define-fun exact () (_ FloatingPoint 8 53) ({fpop} RNE wa wb))\n\
                         (define-fun gr () (_ FloatingPoint 5 3) ((_ to_fp 5 3) RNE exact))\n"
                    )
                } else {
                    format!("(define-fun gr () (_ FloatingPoint 5 3) ({fpop} RNE ga gb))\n")
                };
                match profile {
                    FpCompat::Riscv => format!(
                        "{epre}{gr_def}(define-fun rr () (_ BitVec 8) ({f} a b))\n\
                         (assert (not (ite (fp.isNaN gr) (= rr {n8}) (= ((_ to_fp 5 3) rr) gr))))\n(check-sat)\n"
                    ),
                    FpCompat::Cuda => format!(
                        "{epre}{gr_def}(define-fun rr () (_ BitVec 8) ({f} a b))\n\
                         (assert (not (ite (fp.isNaN gr) (= rr {n8})\n\
                                      (ite (fp.isInfinite gr) (= rr (ite (fp.isNegative gr) #xFB #x7B))\n\
                                      (= ((_ to_fp 5 3) rr) gr)))))\n(check-sat)\n"
                    ),
                }
            };
            match op {
                "e5m2_eq" => s.push_str(&ecmp("arch_e5m2_eq", "(fp.eq ga gb)")),
                "e5m2_ne" => s.push_str(&ecmp("arch_e5m2_ne", "(not (fp.eq ga gb))")),
                "e5m2_lt" => s.push_str(&ecmp("arch_e5m2_lt", "(fp.lt ga gb)")),
                "e5m2_le" => s.push_str(&ecmp("arch_e5m2_le", "(fp.leq ga gb)")),
                "e5m2_gt" => s.push_str(&ecmp("arch_e5m2_gt", "(fp.gt ga gb)")),
                "e5m2_ge" => s.push_str(&ecmp("arch_e5m2_ge", "(fp.geq ga gb)")),
                "e5m2_mul" => s.push_str(&earith("arch_e5m2_mul", "fp.mul", false)),
                "e5m2_add" => s.push_str(&earith("arch_e5m2_add", "fp.add", true)),
                "e5m2_sub" => s.push_str(&earith("arch_e5m2_sub", "fp.sub", true)),
                "e5m2_widen" => s.push_str(&format!(
                    "(declare-fun h () (_ BitVec 8))\n\
                     (define-fun spec () F ((_ to_fp 8 24) RNE ((_ to_fp 5 3) h)))\n\
                     (define-fun rr () (_ BitVec 32) (arch_e5m2_to_f32 h))\n\
                     (assert (not (ite (fp.isNaN spec) (= rr {n32}) (= ((_ to_fp 8 24) rr) spec))))\n(check-sat)\n"
                )),
                "e5m2_narrow" => match profile {
                    FpCompat::Riscv => s.push_str(&format!(
                        "(declare-fun x () (_ BitVec 32))\n(define-fun fx () F ((_ to_fp 8 24) x))\n\
                         (define-fun spec () (_ FloatingPoint 5 3) ((_ to_fp 5 3) RNE fx))\n\
                         (define-fun rr () (_ BitVec 8) (arch_f32_to_e5m2 x))\n\
                         (assert (not (ite (fp.isNaN spec) (= rr {n8}) (= ((_ to_fp 5 3) rr) spec))))\n(check-sat)\n"
                    )),
                    FpCompat::Cuda => s.push_str(&format!(
                        "(declare-fun x () (_ BitVec 32))\n(define-fun fx () F ((_ to_fp 8 24) x))\n\
                         (define-fun spec () (_ FloatingPoint 5 3) ((_ to_fp 5 3) RNE fx))\n\
                         (define-fun rr () (_ BitVec 8) (arch_f32_to_e5m2 x))\n\
                         (assert (not (ite (fp.isNaN spec) (= rr {n8})\n\
                                      (ite (fp.isInfinite spec) (= rr (ite (fp.isNegative spec) #xFB #x7B))\n\
                                      (= ((_ to_fp 5 3) rr) spec)))))\n(check-sat)\n"
                    )),
                },
                // TRUE-CR fma reference: exact fma in (_ FloatingPoint 8 53)
                // (product of two 3-bit sigs + a ≤47-binade alignment span
                // fits 53 bits), then one (5,3) rounding. Expected `sat` —
                // the RTL is fused f32-accumulate.
                "e5m2_fma_cr" => {
                    let ovf = match profile {
                        FpCompat::Riscv => "(= ((_ to_fp 5 3) rr) gr)".to_string(),
                        FpCompat::Cuda =>
                            "(ite (fp.isInfinite gr) (= rr (ite (fp.isNegative gr) #xFB #x7B)) (= ((_ to_fp 5 3) rr) gr))".to_string(),
                    };
                    s.push_str(&format!(
                        "(declare-fun a () (_ BitVec 8))\n(declare-fun b () (_ BitVec 8))\n(declare-fun c () (_ BitVec 8))\n\
                         (define-fun wa () (_ FloatingPoint 8 53) ((_ to_fp 8 53) RNE ((_ to_fp 5 3) a)))\n\
                         (define-fun wb () (_ FloatingPoint 8 53) ((_ to_fp 8 53) RNE ((_ to_fp 5 3) b)))\n\
                         (define-fun wc () (_ FloatingPoint 8 53) ((_ to_fp 8 53) RNE ((_ to_fp 5 3) c)))\n\
                         (define-fun exact () (_ FloatingPoint 8 53) (fp.fma RNE wa wb wc))\n\
                         (define-fun gr () (_ FloatingPoint 5 3) ((_ to_fp 5 3) RNE exact))\n\
                         (define-fun rr () (_ BitVec 8) (arch_fma_e5m2 a b c))\n\
                         (assert (not (ite (fp.isNaN gr) (= rr {n8}) {ovf})))\n(check-sat)\n"
                    ));
                }
                other => panic!("unknown e5m2 proof op {other}"),
            }
        }
        // ── e4m3: OCP OFP8 — hand-written two-region spec (NOT an IEEE
        // format; there is no (_ FloatingPoint 4 4)-shaped sort for it) ──
        _ if op.starts_with("e4m3_") => {
            let n8 = nan8_e4m3_hex(profile);
            // Decode an e4m3 result byte to its f32 value through the IR
            // widen (grounded by the e4m3_widen miter below).
            let wr = |bv: &str| format!("((_ to_fp 8 24) (arch_e4m3_to_f32 {bv}))");
            // Two-region OCP round of an exact value `v` (sort F):
            //  - subnormal region |v| < 2^-6: fixed 2^-9 grid via scaled
            //    fp.roundToIntegral (RNE ties-to-even on the grid index —
            //    identical to mantissa tie-to-even since the index IS the
            //    mantissa). The grid extends through the min-normal binade
            //    (spacing there is also 2^-9), so any split point inside
            //    [2^-6, 2^-5) is sound; we split at 2^-6.
            //  - normal region: (_ to_fp 8 4) RNE — the 4-bit-significand
            //    grid with unbounded-for-f32 exponent range. This handles
            //    the 248 tie (→256, even) and the 464 tie (→448, even) by
            //    plain RNE; overflow ⟺ |rounded| ≥ 480 (the next grid point
            //    above max-finite 448), which also covers ±inf `v`.
            // The exact value `v` lives in sort `(_ FloatingPoint {ebsb})` —
            // (8 24) for the binary ops (exact there), (8 37) for the fma
            // reference. `specv` is materialized in that sort; the result
            // byte decodes into it exactly (all fp8 values are exact in
            // either sort), so `=` (FP identity — distinguishes ±0) is the
            // full non-overflow spec.
            let round_spec_in = |ebsb: &str| {
                let vs = format!("(_ FloatingPoint {ebsb})");
                format!(
                    "(define-fun c512 () {vs} ((_ to_fp {ebsb}) RNE 512.0))\n\
                     (define-fun minn () {vs} ((_ to_fp {ebsb}) RNE 0.015625))\n\
                     (define-fun r84 () (_ FloatingPoint 8 4) ((_ to_fp 8 4) RNE v))\n\
                     (define-fun r84f () {vs} ((_ to_fp {ebsb}) RNE r84))\n\
                     (define-fun subv () {vs} (fp.div RNE (fp.roundToIntegral RNE (fp.mul RNE v c512)) c512))\n\
                     (define-fun is_sub () Bool (fp.lt (fp.abs v) minn))\n\
                     (define-fun specv () {vs} (ite is_sub subv r84f))\n\
                     (define-fun ovf () Bool (and (not is_sub) (or (fp.isInfinite r84) (fp.geq (fp.abs r84) ((_ to_fp 8 4) RNE 480.0)))))\n"
                )
            };
            let round_spec = round_spec_in("8 24");
            let ovf_res = match profile {
                FpCompat::Riscv => format!("(= rr {n8})"),
                FpCompat::Cuda => "(= rr (ite (fp.isNegative v) #xFE #x7E))".to_string(),
            };
            let round_assert_in = |ebsb: &str| {
                let wrr = format!("((_ to_fp {ebsb}) RNE {})", wr("rr"));
                format!(
                    "(assert (not (ite (fp.isNaN v) (= rr {n8})\n\
                                  (ite ovf {ovf_res}\n\
                                  (= {wrr} specv)))))\n(check-sat)\n"
                )
            };
            let round_assert = round_assert_in("8 24");
            let epre4 = format!(
                "(declare-fun a () (_ BitVec 8))\n(declare-fun b () (_ BitVec 8))\n\
                 (define-fun va () F {})\n(define-fun vb () F {})\n",
                wr("a"),
                wr("b")
            );
            let ecmp4 = |f: &str, spec: &str| {
                format!("{epre4}(assert (not (= (= ({f} a b) #b1) {spec})))\n(check-sat)\n")
            };
            // The inner f32 op on widened e4m3 values is EXACT (≤8-bit
            // products; ≤19-bit alignment span for add/sub, both well under
            // f32's 24-bit significand), so `v` is the infinitely-precise
            // result and `unsat` proves the op correctly rounded per OCP.
            let earith4 = |f: &str, fpop: &str| {
                format!(
                    "{epre4}(define-fun v () F ({fpop} RNE va vb))\n\
                     (define-fun rr () (_ BitVec 8) ({f} a b))\n{round_spec}{round_assert}"
                )
            };
            match op {
                "e4m3_eq" => s.push_str(&ecmp4("arch_e4m3_eq", "(fp.eq va vb)")),
                "e4m3_ne" => s.push_str(&ecmp4("arch_e4m3_ne", "(not (fp.eq va vb))")),
                "e4m3_lt" => s.push_str(&ecmp4("arch_e4m3_lt", "(fp.lt va vb)")),
                "e4m3_le" => s.push_str(&ecmp4("arch_e4m3_le", "(fp.leq va vb)")),
                "e4m3_gt" => s.push_str(&ecmp4("arch_e4m3_gt", "(fp.gt va vb)")),
                "e4m3_ge" => s.push_str(&ecmp4("arch_e4m3_ge", "(fp.geq va vb)")),
                "e4m3_mul" => s.push_str(&earith4("arch_e4m3_mul", "fp.mul")),
                "e4m3_add" => s.push_str(&earith4("arch_e4m3_add", "fp.add")),
                "e4m3_sub" => s.push_str(&earith4("arch_e4m3_sub", "fp.sub")),
                // Hand spec grounding: below exponent 15 the E4M3 encoding
                // coincides bit-for-bit with IEEE (4,4) (same bias 7, same
                // subnormal rule); exponent 15 with mantissa < 7 are the OCP
                // finite normals 256..448 (7 explicit constants); mantissa 7
                // is the sole NaN.
                "e4m3_widen" => s.push_str(&format!(
                    "(declare-fun h () (_ BitVec 8))\n\
                     (define-fun rr () (_ BitVec 32) (arch_e4m3_to_f32 h))\n\
                     (define-fun sneg () Bool (= ((_ extract 7 7) h) #b1))\n\
                     (define-fun ef () (_ BitVec 4) ((_ extract 6 3) h))\n\
                     (define-fun mf () (_ BitVec 3) ((_ extract 2 0) h))\n\
                     (define-fun isn () Bool (= ((_ extract 6 0) h) #b1111111))\n\
                     (define-fun topmag () F\n\
                       (ite (= mf #b000) ((_ to_fp 8 24) RNE 256.0)\n\
                       (ite (= mf #b001) ((_ to_fp 8 24) RNE 288.0)\n\
                       (ite (= mf #b010) ((_ to_fp 8 24) RNE 320.0)\n\
                       (ite (= mf #b011) ((_ to_fp 8 24) RNE 352.0)\n\
                       (ite (= mf #b100) ((_ to_fp 8 24) RNE 384.0)\n\
                       (ite (= mf #b101) ((_ to_fp 8 24) RNE 416.0)\n\
                                         ((_ to_fp 8 24) RNE 448.0))))))))\n\
                     (define-fun spec () F (ite (= ef #b1111) (ite sneg (fp.neg topmag) topmag)\n\
                                               ((_ to_fp 8 24) RNE ((_ to_fp 4 4) h))))\n\
                     (assert (not (ite isn (= rr {n32}) (= ((_ to_fp 8 24) rr) spec))))\n(check-sat)\n"
                )),
                "e4m3_narrow" => s.push_str(&format!(
                    "(declare-fun x () (_ BitVec 32))\n\
                     (define-fun v () F ((_ to_fp 8 24) x))\n\
                     (define-fun rr () (_ BitVec 8) (arch_f32_to_e4m3 x))\n{round_spec}{round_assert}"
                )),
                // TRUE-CR fma reference: exact fma in (_ FloatingPoint 8 37)
                // (6-bit products on a 2^-18 grid, magnitude ≤ 448²+448 →
                // ≤36 bits, so `fp.fma` in that sort is exact), then ONE OCP
                // rounding — the whole round spec runs in the wide sort so
                // no intermediate f32 rounding sneaks into the reference.
                // Expected `sat` (the RTL is fused f32-accumulate).
                "e4m3_fma_cr" => s.push_str(&format!(
                    "(declare-fun a () (_ BitVec 8))\n(declare-fun b () (_ BitVec 8))\n(declare-fun c () (_ BitVec 8))\n\
                     (define-fun wa () (_ FloatingPoint 8 37) ((_ to_fp 8 37) RNE {}))\n\
                     (define-fun wb () (_ FloatingPoint 8 37) ((_ to_fp 8 37) RNE {}))\n\
                     (define-fun wc () (_ FloatingPoint 8 37) ((_ to_fp 8 37) RNE {}))\n\
                     (define-fun v () (_ FloatingPoint 8 37) (fp.fma RNE wa wb wc))\n\
                     (define-fun rr () (_ BitVec 8) (arch_fma_e4m3 a b c))\n{}{}",
                    wr("a"), wr("b"), wr("c"),
                    round_spec_in("8 37"), round_assert_in("8 37")
                )),
                other => panic!("unknown e4m3 proof op {other}"),
            }
        }
        // ── OCP MX all-finite storage formats: FP4 E2M1, FP6 E2M3/E3M2 ──
        _ if all_finite_fmt(op).is_some() => {
            let f = all_finite_fmt(op).expect("guarded above");
            s.push_str(&all_finite_proof(op, &f));
        }
        other => panic!("unknown proof op {other}"),
    }
    s
}
