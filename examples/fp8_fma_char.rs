// Exhaustive FP8 fma characterization: fused f32-accumulate (the RTL/sim
// semantics) vs a TRUE correctly-rounded reference, all 2^24 input triples,
// per format (E4M3/E5M2) and per --fp-compat profile.
//
//   cargo run --release --example fp8_fma_char
//
// The exact value of a*b+c is computed in f64, which is EXACT for fp8
// operands: significands are ≤4 bits, so the product has ≤8 significant
// bits, and the widest alignment span (E5M2: product down to 2^-32 against
// an addend up to 2^15·1.75) needs ≤51 bits — inside f64's 53. The CR
// reference is then a single f64→fp8 rounding (RNE + profile overflow).
// The RTL semantics mirror `_arch_f32_to_fp8`: widen → one f32 fma → one
// f32→fp8 rounding (the second rounding is the characterized deviation).
//
// A zero mismatch count for a format×profile would mean the fused-f32 fma
// is correctly rounded there (upgrading the SMT `fma_cr` miter to expected-
// unsat); nonzero counts get recorded in doc §3.8 like the bf16 0.37% note.

use arch::fp_lit::{e4m3_bits_to_f64, e5m2_bits_to_f64, f64_to_e4m3_bits, f64_to_e5m2_bits};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prof {
    Riscv,
    Cuda,
}

#[derive(Clone, Copy)]
struct Fmt {
    e4m3: bool,
}

fn is_nan8(f: Fmt, h: u8) -> bool {
    if f.e4m3 {
        (h & 0x7F) == 0x7F
    } else {
        (h >> 2) & 0x1F == 0x1F && (h & 3) != 0
    }
}
fn is_inf8(f: Fmt, h: u8) -> bool {
    !f.e4m3 && (h & 0x7F) == 0x7C
}
fn widen(f: Fmt, h: u8) -> f64 {
    // NaN/inf handled by callers; finite decode via the literal helpers.
    if f.e4m3 {
        e4m3_bits_to_f64(h)
    } else {
        e5m2_bits_to_f64(h)
    }
}

/// f64 value → fp8 byte under `prof`, mirroring the RTL narrow semantics
/// (single RNE rounding + profile overflow). `v` must be a real number
/// (callers handle NaN/inf inputs).
fn round8(f: Fmt, p: Prof, v: f64) -> u8 {
    let sgn = if v.is_sign_negative() { 0x80u8 } else { 0 };
    let enc = if f.e4m3 {
        f64_to_e4m3_bits(v)
    } else {
        f64_to_e5m2_bits(v)
    };
    match enc {
        Some(b) => b,
        None => match (f.e4m3, p) {
            (true, Prof::Riscv) => 0x7F,        // NaN, sign dropped (OCP)
            (true, Prof::Cuda) => sgn | 0x7E,   // satfinite ±448
            (false, Prof::Riscv) => sgn | 0x7C, // ±inf
            (false, Prof::Cuda) => sgn | 0x7B,  // satfinite ±57344
        },
    }
}

fn nan8(f: Fmt, p: Prof) -> u8 {
    match (f.e4m3, p) {
        (true, _) => 0x7F,
        (false, Prof::Riscv) => 0x7E,
        (false, Prof::Cuda) => 0x7F,
    }
}

/// RTL semantics: widen → f32 fma → f32→fp8 narrow (fused f32-accumulate).
fn rtl_fma(f: Fmt, p: Prof, a: u8, b: u8, c: u8) -> u8 {
    if is_nan8(f, a) || is_nan8(f, b) || is_nan8(f, c) {
        return nan8(f, p);
    }
    // E5M2 infinities propagate through the f32 fma; inf*0 → NaN.
    let (fa, fb, fc) = (widen_f32(f, a), widen_f32(f, b), widen_f32(f, c));
    let r = f32::mul_add(fa, fb, fc);
    if r.is_nan() {
        return nan8(f, p);
    }
    if r.is_infinite() {
        let sgn = if r < 0.0 { 0x80u8 } else { 0 };
        return match (f.e4m3, p) {
            (true, Prof::Riscv) => 0x7F,
            (true, Prof::Cuda) => sgn | 0x7E,
            (false, Prof::Riscv) => sgn | 0x7C,
            (false, Prof::Cuda) => sgn | 0x7B,
        };
    }
    round8(f, p, r as f64)
}
fn widen_f32(f: Fmt, h: u8) -> f32 {
    if is_inf8(f, h) {
        return if h & 0x80 != 0 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    widen(f, h) as f32
}

/// Correctly-rounded reference: exact a*b+c in f64 (see header), ONE fp8
/// rounding.
fn cr_fma(f: Fmt, p: Prof, a: u8, b: u8, c: u8) -> u8 {
    if is_nan8(f, a) || is_nan8(f, b) || is_nan8(f, c) {
        return nan8(f, p);
    }
    if !f.e4m3 {
        // E5M2 infinity semantics (exact, no rounding involved).
        let (ia, ib, ic) = (is_inf8(f, a), is_inf8(f, b), is_inf8(f, c));
        if ia || ib || ic {
            let wa = widen_f32(f, a) as f64;
            let wb = widen_f32(f, b) as f64;
            let wc = widen_f32(f, c) as f64;
            let r = wa * wb + wc; // inf arithmetic in f64 is exact here
            if r.is_nan() {
                return nan8(f, p);
            }
            let sgn = if r < 0.0 { 0x80u8 } else { 0 };
            return match p {
                Prof::Riscv => sgn | 0x7C,
                Prof::Cuda => sgn | 0x7B,
            };
        }
    }
    let exact = widen(f, a) * widen(f, b) + widen(f, c); // exact in f64
    round8(f, p, exact)
}

/// Total order rank of a non-NaN fp8 byte: sign-magnitude → signed rank, so
/// |rank(x) − rank(y)| is the ULP distance on the format's value grid. For
/// E5M2 the encoding is IEEE-ordered with ±inf (0x7C) one step above max
/// finite — counting overflow-boundary flips (0x7B vs 0x7C) as 1 ULP. Both
/// zeros map to rank 0 (they compare equal in value).
fn rank(h: u8) -> i32 {
    let mag = (h & 0x7F) as i32;
    if h & 0x80 != 0 {
        -mag
    } else {
        mag
    }
}

fn main() {
    for (fname, fmt) in [("E4M3", Fmt { e4m3: true }), ("E5M2", Fmt { e4m3: false })] {
        for (pname, prof) in [("riscv", Prof::Riscv), ("cuda", Prof::Cuda)] {
            let mut total = 0u64;
            let mut mismatch = 0u64;
            let mut first: Option<(u8, u8, u8, u8, u8)> = None;
            // ULP-distance histogram over mismatches; index 0 counts cases
            // where either side is NaN (not a distance) — expected 0, since
            // both paths canonicalize NaN identically.
            let mut hist = [0u64; 4]; // [nan-involved, 1, 2, >=3]
            for a in 0..=255u8 {
                for b in 0..=255u8 {
                    for c in 0..=255u8 {
                        let r = rtl_fma(fmt, prof, a, b, c);
                        let g = cr_fma(fmt, prof, a, b, c);
                        total += 1;
                        if r != g {
                            mismatch += 1;
                            if first.is_none() {
                                first = Some((a, b, c, r, g));
                            }
                            if is_nan8(fmt, r) || is_nan8(fmt, g) {
                                hist[0] += 1;
                            } else {
                                let d = (rank(r) - rank(g)).unsigned_abs();
                                hist[(d.min(3)) as usize] += 1;
                            }
                        }
                    }
                }
            }
            let pct = 100.0 * mismatch as f64 / total as f64;
            print!("{fname}/{pname}: {mismatch}/{total} mismatches ({pct:.4}%)");
            if let Some((a, b, c, r, g)) = first {
                print!("  first: fma(0x{a:02X},0x{b:02X},0x{c:02X}) rtl=0x{r:02X} cr=0x{g:02X}");
            }
            if mismatch > 0 {
                print!(
                    "  ulp-hist: 1={} 2={} >=3={} nan-involved={}",
                    hist[1], hist[2], hist[3], hist[0]
                );
            }
            println!();
        }
    }
}
