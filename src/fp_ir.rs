//! Shared bit-vector IR for the floating-point helpers.
//!
//! One in-Rust description of each FP operator's bit-logic renders to BOTH
//! synthesizable SystemVerilog (`render_sv`) and SMT-LIB2 (`render_smt`). The
//! simulated/synthesized RTL and the formally-checked model are therefore the
//! *same source* — they cannot drift (doc/plan_fp_types.md §8).
//!
//! The IR is a small DAG of fixed-width bit-vector nodes. Both renderers
//! linearize the DAG into administrative-normal form (one operation per named
//! temporary), which (a) keeps the emitted SV free of part-selects on
//! expressions — every `[hi:lo]` lands on a name — and (b) shares common
//! sub-expressions in both dialects. Predicates are 1-bit vectors so `ite` and
//! the boolean connectives are uniform across both backends.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum Bin {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    Lshr,
}

#[derive(Clone, Copy, PartialEq)]
enum Cmp {
    Eq,
    Ne,
    Ult,
    Ule,
    Ugt,
    Uge,
    Slt,
    Sle,
    Sgt,
    Sge,
}

enum Kind {
    Var(String),
    Const { val: u128 },
    Extract { x: Bv, hi: u32, lo: u32 },
    Concat(Bv, Bv),
    ZeroExt { x: Bv, to: u32 },
    Bin { op: Bin, a: Bv, b: Bv },
    Not(Bv),
    Ite { c: Bv, t: Bv, e: Bv },
    Cmp { op: Cmp, a: Bv, b: Bv },
    Call { name: String, args: Vec<Bv> },
}

struct Node {
    width: u32,
    kind: Kind,
}

/// A width-tracked bit-vector value (reference-counted so the DAG shares
/// sub-expressions; equality of `Rc` pointers drives common-subexpression
/// naming in both renderers).
#[derive(Clone)]
pub struct Bv(Rc<Node>);

impl Bv {
    fn mk(width: u32, kind: Kind) -> Bv {
        Bv(Rc::new(Node { width, kind }))
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
}

/// A bit-vector constant of the given width.
pub fn cst(val: u128, width: u32) -> Bv {
    Bv::mk(width, Kind::Const { val })
}
/// A named input/parameter reference of the given width.
pub fn var(name: &str, width: u32) -> Bv {
    Bv::mk(width, Kind::Var(name.to_string()))
}
/// `x[hi:lo]` — result width `hi-lo+1`.
pub fn extract(x: &Bv, hi: u32, lo: u32) -> Bv {
    assert!(hi >= lo && hi < x.width(), "extract out of range");
    Bv::mk(hi - lo + 1, Kind::Extract { x: x.clone(), hi, lo })
}
/// `{a, b}` — concatenation, `a` is the high part.
pub fn concat(a: &Bv, b: &Bv) -> Bv {
    Bv::mk(a.width() + b.width(), Kind::Concat(a.clone(), b.clone()))
}
/// Zero-extend `x` to `to` bits.
pub fn zext(x: &Bv, to: u32) -> Bv {
    assert!(to >= x.width(), "zext shrinks");
    if to == x.width() {
        return x.clone();
    }
    Bv::mk(to, Kind::ZeroExt { x: x.clone(), to })
}
fn bin(op: Bin, a: &Bv, b: &Bv) -> Bv {
    assert_eq!(a.width(), b.width(), "binop width mismatch");
    Bv::mk(a.width(), Kind::Bin { op, a: a.clone(), b: b.clone() })
}
pub fn add(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::Add, a, b)
}
pub fn sub(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::Sub, a, b)
}
pub fn mul(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::Mul, a, b)
}
pub fn band(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::And, a, b)
}
pub fn bor(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::Or, a, b)
}
pub fn bxor(a: &Bv, b: &Bv) -> Bv {
    bin(Bin::Xor, a, b)
}
/// Logical shift left; the shift amount is zero-extended to `a`'s width.
pub fn shl(a: &Bv, amt: &Bv) -> Bv {
    bin(Bin::Shl, a, &zext(amt, a.width()))
}
/// Logical shift right; the shift amount is zero-extended to `a`'s width.
pub fn lshr(a: &Bv, amt: &Bv) -> Bv {
    bin(Bin::Lshr, a, &zext(amt, a.width()))
}
pub fn bnot(x: &Bv) -> Bv {
    Bv::mk(x.width(), Kind::Not(x.clone()))
}
/// `c ? t : e` — `c` must be 1-bit; `t` and `e` must share a width.
pub fn ite(c: &Bv, t: &Bv, e: &Bv) -> Bv {
    assert_eq!(c.width(), 1, "ite condition must be 1-bit");
    assert_eq!(t.width(), e.width(), "ite arms width mismatch");
    Bv::mk(t.width(), Kind::Ite { c: c.clone(), t: t.clone(), e: e.clone() })
}
fn cmp(op: Cmp, a: &Bv, b: &Bv) -> Bv {
    assert_eq!(a.width(), b.width(), "compare width mismatch");
    Bv::mk(1, Kind::Cmp { op, a: a.clone(), b: b.clone() })
}
pub fn eq(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Eq, a, b)
}
pub fn ne(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Ne, a, b)
}
pub fn ult(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Ult, a, b)
}
pub fn ule(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Ule, a, b)
}
pub fn ugt(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Ugt, a, b)
}
pub fn uge(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Uge, a, b)
}
pub fn slt(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Slt, a, b)
}
pub fn sle(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Sle, a, b)
}
pub fn sgt(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Sgt, a, b)
}
pub fn sge(a: &Bv, b: &Bv) -> Bv {
    cmp(Cmp::Sge, a, b)
}
/// Two's-complement negation.
pub fn neg(x: &Bv) -> Bv {
    sub(&cst(0, x.width()), x)
}
/// Boolean AND/OR/NOT over 1-bit predicates (bitwise on width-1 vectors).
pub fn and(a: &Bv, b: &Bv) -> Bv {
    band(a, b)
}
pub fn or(a: &Bv, b: &Bv) -> Bv {
    bor(a, b)
}
pub fn not(a: &Bv) -> Bv {
    bnot(a)
}
/// Call another `FpFn` by name; `width` is the callee's return width.
pub fn call(name: &str, args: &[Bv], width: u32) -> Bv {
    Bv::mk(width, Kind::Call { name: name.to_string(), args: args.to_vec() })
}

/// A single FP helper: name, typed parameters, and a return expression.
pub struct FpFn {
    pub name: String,
    pub params: Vec<(String, u32)>,
    pub ret_w: u32,
    pub body: Bv,
}

impl FpFn {
    pub fn new(name: &str, params: &[(&str, u32)], ret_w: u32, body: Bv) -> FpFn {
        assert_eq!(body.width(), ret_w, "fn {name}: body width != ret width");
        FpFn {
            name: name.to_string(),
            params: params.iter().map(|(n, w)| (n.to_string(), *w)).collect(),
            ret_w,
            body,
        }
    }
}

// ── DAG linearization (shared by both renderers) ────────────────────────────

struct Lin {
    ids: HashMap<usize, usize>, // Rc ptr -> temp id
    order: Vec<Bv>,             // compound nodes in topological order
}

fn is_leaf(b: &Bv) -> bool {
    matches!(b.0.kind, Kind::Var(_) | Kind::Const { .. })
}

fn linearize(body: &Bv) -> Lin {
    let mut lin = Lin { ids: HashMap::new(), order: Vec::new() };
    fn go(b: &Bv, lin: &mut Lin) {
        if is_leaf(b) {
            return;
        }
        let ptr = Rc::as_ptr(&b.0) as usize;
        if lin.ids.contains_key(&ptr) {
            return;
        }
        match &b.0.kind {
            Kind::Extract { x, .. } | Kind::ZeroExt { x, .. } | Kind::Not(x) => go(x, lin),
            Kind::Concat(a, c) | Kind::Bin { a, b: c, .. } | Kind::Cmp { a, b: c, .. } => {
                go(a, lin);
                go(c, lin);
            }
            Kind::Ite { c, t, e } => {
                go(c, lin);
                go(t, lin);
                go(e, lin);
            }
            Kind::Call { args, .. } => {
                for a in args {
                    go(a, lin);
                }
            }
            Kind::Var(_) | Kind::Const { .. } => {}
        }
        let id = lin.order.len();
        lin.ids.insert(ptr, id);
        lin.order.push(b.clone());
    }
    go(body, &mut lin);
    lin
}

// ── SystemVerilog renderer ──────────────────────────────────────────────────

fn sv_ref(b: &Bv, lin: &Lin) -> String {
    match &b.0.kind {
        Kind::Var(n) => n.clone(),
        Kind::Const { val } => format!("{}'h{:X}", b.width(), val),
        _ => format!("_t{}", lin.ids[&(Rc::as_ptr(&b.0) as usize)]),
    }
}

fn sv_decl_width(w: u32) -> String {
    if w == 1 {
        String::new()
    } else {
        format!("[{}:0] ", w - 1)
    }
}

fn sv_rhs(b: &Bv, lin: &Lin) -> String {
    let r = |x: &Bv| sv_ref(x, lin);
    match &b.0.kind {
        Kind::Var(_) | Kind::Const { .. } => sv_ref(b, lin),
        Kind::Extract { x, hi, lo } => {
            if hi == lo {
                format!("{}[{}]", r(x), hi)
            } else {
                format!("{}[{}:{}]", r(x), hi, lo)
            }
        }
        Kind::Concat(a, c) => format!("{{{}, {}}}", r(a), r(c)),
        Kind::ZeroExt { x, to } => format!("{{{}'b0, {}}}", to - x.width(), r(x)),
        Kind::Not(x) => format!("~{}", r(x)),
        Kind::Bin { op, a, b: c } => {
            let o = match op {
                Bin::Add => "+",
                Bin::Sub => "-",
                Bin::Mul => "*",
                Bin::And => "&",
                Bin::Or => "|",
                Bin::Xor => "^",
                Bin::Shl => "<<",
                Bin::Lshr => ">>",
            };
            format!("{} {} {}", r(a), o, r(c))
        }
        Kind::Cmp { op, a, b: c } => {
            // Unsigned operands are plain `logic`, so SV relops are unsigned;
            // signed compares wrap both sides in `$signed`.
            match op {
                Cmp::Eq => format!("{} == {}", r(a), r(c)),
                Cmp::Ne => format!("{} != {}", r(a), r(c)),
                Cmp::Ult => format!("{} < {}", r(a), r(c)),
                Cmp::Ugt => format!("{} > {}", r(a), r(c)),
                Cmp::Ule => format!("{} <= {}", r(a), r(c)),
                Cmp::Uge => format!("{} >= {}", r(a), r(c)),
                Cmp::Slt => format!("$signed({}) < $signed({})", r(a), r(c)),
                Cmp::Sgt => format!("$signed({}) > $signed({})", r(a), r(c)),
                Cmp::Sle => format!("$signed({}) <= $signed({})", r(a), r(c)),
                Cmp::Sge => format!("$signed({}) >= $signed({})", r(a), r(c)),
            }
        }
        Kind::Ite { c, t, e } => format!("{} ? {} : {}", r(c), r(t), r(e)),
        Kind::Call { name, args } => {
            let a: Vec<String> = args.iter().map(|x| r(x)).collect();
            format!("{}({})", name, a.join(", "))
        }
    }
}

fn render_sv_fn(f: &FpFn) -> String {
    let lin = linearize(&f.body);
    let mut s = String::new();
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(n, w)| format!("input logic {}{}", sv_decl_width(*w), n))
        .collect();
    let _ = writeln!(
        s,
        "function automatic logic {}{}({});",
        sv_decl_width(f.ret_w),
        f.name,
        params.join(", ")
    );
    for b in &lin.order {
        let id = lin.ids[&(Rc::as_ptr(&b.0) as usize)];
        let _ = writeln!(s, "  logic {}_t{} = {};", sv_decl_width(b.width()), id, sv_rhs(b, &lin));
    }
    let _ = writeln!(s, "  {} = {};", f.name, sv_ref(&f.body, &lin));
    let _ = writeln!(s, "endfunction");
    s
}

/// Render a set of helper functions to one SystemVerilog block.
pub fn render_sv(funcs: &[FpFn]) -> String {
    funcs.iter().map(render_sv_fn).collect::<Vec<_>>().join("")
}

// ── SMT-LIB2 renderer ───────────────────────────────────────────────────────

fn smt_sort(w: u32) -> String {
    format!("(_ BitVec {w})")
}

fn smt_ref(b: &Bv, lin: &Lin) -> String {
    match &b.0.kind {
        Kind::Var(n) => n.clone(),
        Kind::Const { val } => format!("(_ bv{} {})", val, b.width()),
        _ => format!("_t{}", lin.ids[&(Rc::as_ptr(&b.0) as usize)]),
    }
}

fn smt_rhs(b: &Bv, lin: &Lin) -> String {
    let r = |x: &Bv| smt_ref(x, lin);
    match &b.0.kind {
        Kind::Var(_) | Kind::Const { .. } => smt_ref(b, lin),
        Kind::Extract { x, hi, lo } => format!("((_ extract {hi} {lo}) {})", r(x)),
        Kind::Concat(a, c) => format!("(concat {} {})", r(a), r(c)),
        Kind::ZeroExt { x, to } => format!("((_ zero_extend {}) {})", to - x.width(), r(x)),
        Kind::Not(x) => format!("(bvnot {})", r(x)),
        Kind::Bin { op, a, b: c } => {
            let o = match op {
                Bin::Add => "bvadd",
                Bin::Sub => "bvsub",
                Bin::Mul => "bvmul",
                Bin::And => "bvand",
                Bin::Or => "bvor",
                Bin::Xor => "bvxor",
                Bin::Shl => "bvshl",
                Bin::Lshr => "bvlshr",
            };
            format!("({} {} {})", o, r(a), r(c))
        }
        Kind::Cmp { op, a, b: c } => {
            let p = match op {
                Cmp::Eq => format!("(= {} {})", r(a), r(c)),
                Cmp::Ne => format!("(not (= {} {}))", r(a), r(c)),
                Cmp::Ult => format!("(bvult {} {})", r(a), r(c)),
                Cmp::Ule => format!("(bvule {} {})", r(a), r(c)),
                Cmp::Ugt => format!("(bvugt {} {})", r(a), r(c)),
                Cmp::Uge => format!("(bvuge {} {})", r(a), r(c)),
                Cmp::Slt => format!("(bvslt {} {})", r(a), r(c)),
                Cmp::Sle => format!("(bvsle {} {})", r(a), r(c)),
                Cmp::Sgt => format!("(bvsgt {} {})", r(a), r(c)),
                Cmp::Sge => format!("(bvsge {} {})", r(a), r(c)),
            };
            format!("(ite {p} #b1 #b0)")
        }
        Kind::Ite { c, t, e } => format!("(ite (= {} #b1) {} {})", r(c), r(t), r(e)),
        Kind::Call { name, args } => {
            let a: Vec<String> = args.iter().map(|x| r(x)).collect();
            format!("({} {})", name, a.join(" "))
        }
    }
}

fn render_smt_fn(f: &FpFn) -> String {
    let lin = linearize(&f.body);
    let params: Vec<String> =
        f.params.iter().map(|(n, w)| format!("({n} {})", smt_sort(*w))).collect();
    let mut body = smt_ref(&f.body, &lin);
    // Wrap the temporaries as nested `let`s, innermost last.
    for b in lin.order.iter().rev() {
        let id = lin.ids[&(Rc::as_ptr(&b.0) as usize)];
        body = format!("(let ((_t{id} {})) {body})", smt_rhs(b, &lin));
    }
    format!(
        "(define-fun {} ({}) {} {body})\n",
        f.name,
        params.join(" "),
        smt_sort(f.ret_w)
    )
}

/// Render a set of helper functions to one SMT-LIB2 block of `define-fun`s.
pub fn render_smt(funcs: &[FpFn]) -> String {
    funcs.iter().map(render_smt_fn).collect::<Vec<_>>().join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_both_dialects() {
        // f(a,b) = (a + b) with the low bit forced, then compared.
        let a = var("a", 8);
        let b = var("b", 8);
        let s = add(&a, &b);
        let lo = extract(&s, 0, 0);
        let body = ite(&eq(&lo, &cst(0, 1)), &s, &cst(0xFF, 8));
        let f = FpFn::new("t", &[("a", 8), ("b", 8)], 8, body);

        let sv = render_sv(&[f]);
        assert!(sv.contains("function automatic logic [7:0] t(input logic [7:0] a, input logic [7:0] b);"));
        assert!(sv.contains(" + "));
        assert!(sv.contains("[0]"));
        assert!(sv.contains("? "));

        // rebuild for smt (Bv was moved into the fn)
        let a = var("a", 8);
        let b = var("b", 8);
        let s = add(&a, &b);
        let lo = extract(&s, 0, 0);
        let body = ite(&eq(&lo, &cst(0, 1)), &s, &cst(0xFF, 8));
        let f = FpFn::new("t", &[("a", 8), ("b", 8)], 8, body);
        let smt = render_smt(&[f]);
        assert!(smt.contains("(define-fun t ((a (_ BitVec 8)) (b (_ BitVec 8))) (_ BitVec 8)"));
        assert!(smt.contains("(bvadd a b)"));
        assert!(smt.contains("(let ("));
    }
}
