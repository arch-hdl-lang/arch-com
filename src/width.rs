//! Width / index-bit-count helpers shared across codegen, sim_codegen,
//! typecheck, and elaborate. Replaces ~14 copies of
//! `(n as f64).log2().ceil() as u32` (and ~3 variations with `<= 1` /
//! `<= 2` / `max(1, _)` floors) that had drifted across the compiler.
//!
//! Float arithmetic is avoided — these are compile-time integer
//! computations and should be exact at every input.

/// Pure ceiling log₂.
/// Edge cases: `clog2(0) = 0`, `clog2(1) = 0`, `clog2(2) = 1`,
/// `clog2(3) = 2`, `clog2(4) = 2`, `clog2(5) = 3`.
///
/// Equivalent to "number of bits to *distinguish* `n` values" (which
/// is 0 for n=0 or n=1 — degenerate; most callers want
/// [`index_width`] instead, which floors to 1).
pub fn clog2(n: u64) -> u32 {
    if n <= 1 {
        0
    } else {
        (n - 1).ilog2() + 1
    }
}

/// Width in bits to *address* an array of `n` items, with a 1-bit
/// floor: `index_width(0) = index_width(1) = index_width(2) = 1`,
/// `index_width(3) = 2`, `index_width(5) = 3`. Equivalent to
/// `max(1, clog2(n))`.
///
/// This matches the historical `if n <= 1 { 1 } else { clog2(n) }`
/// and `if n <= 2 { 1 } else { clog2(n) }` patterns scattered through
/// the compiler (both produce the same width for every input — the
/// `<= 1` form was sloppy at n=2 in fewer places, but `clog2(2) = 1`
/// either way).
pub fn index_width(n: u64) -> u32 {
    clog2(n).max(1)
}

/// True when `e` is built only from literals, `$clog2` and names for which
/// `is_param` holds: a compile-time integer expression. The backends
/// evaluate such an expression as an integer (so `(1 << W) - 1` is a mask,
/// not a 1-bit shift), and the typechecker's non-widening `<<` check skips
/// it. One shared rule keeps the three in agreement.
pub fn is_const_int_expr(e: &crate::ast::Expr, is_param: &dyn Fn(&str) -> bool) -> bool {
    use crate::ast::ExprKind;
    match &e.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Ident(n) => is_param(n),
        ExprKind::Clog2(a) | ExprKind::Unary(_, a) => is_const_int_expr(a, is_param),
        ExprKind::Binary(_, a, b) => {
            is_const_int_expr(a, is_param) && is_const_int_expr(b, is_param)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clog2_edge_cases() {
        assert_eq!(clog2(0), 0);
        assert_eq!(clog2(1), 0);
        assert_eq!(clog2(2), 1);
        assert_eq!(clog2(3), 2);
        assert_eq!(clog2(4), 2);
        assert_eq!(clog2(5), 3);
        assert_eq!(clog2(8), 3);
        assert_eq!(clog2(9), 4);
        assert_eq!(clog2(255), 8);
        assert_eq!(clog2(256), 8);
        assert_eq!(clog2(257), 9);
    }

    #[test]
    fn index_width_floors_at_1() {
        assert_eq!(index_width(0), 1);
        assert_eq!(index_width(1), 1);
        assert_eq!(index_width(2), 1);
        assert_eq!(index_width(3), 2);
        assert_eq!(index_width(4), 2);
        assert_eq!(index_width(5), 3);
        assert_eq!(index_width(32), 5);
    }

    #[test]
    fn matches_float_log2_ceil_for_n_geq_2() {
        // Sanity: for n >= 2, integer clog2 matches the historical
        // float-arithmetic shape `(n as f64).log2().ceil() as u32`.
        for n in 2u64..=1024 {
            let float_form = (n as f64).log2().ceil() as u32;
            assert_eq!(clog2(n), float_form, "mismatch at n={n}");
        }
    }
}
