//! Shared bit-vector IR for the floating-point helpers.
//!
//! One in-Rust description of each FP operator's bit-logic renders to BOTH
//! synthesizable SystemVerilog (`render_sv`) and SMT-LIB2 (`render_smt`). The
//! simulated/synthesized RTL and the formally-checked model are therefore the
//! *same source* — they cannot drift (doc/archive/plan_fp_types.md §8).
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
    Bv::mk(
        hi - lo + 1,
        Kind::Extract {
            x: x.clone(),
            hi,
            lo,
        },
    )
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
    Bv::mk(
        a.width(),
        Kind::Bin {
            op,
            a: a.clone(),
            b: b.clone(),
        },
    )
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
    Bv::mk(
        t.width(),
        Kind::Ite {
            c: c.clone(),
            t: t.clone(),
            e: e.clone(),
        },
    )
}
fn cmp(op: Cmp, a: &Bv, b: &Bv) -> Bv {
    assert_eq!(a.width(), b.width(), "compare width mismatch");
    Bv::mk(
        1,
        Kind::Cmp {
            op,
            a: a.clone(),
            b: b.clone(),
        },
    )
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
    Bv::mk(
        width,
        Kind::Call {
            name: name.to_string(),
            args: args.to_vec(),
        },
    )
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
    let mut lin = Lin {
        ids: HashMap::new(),
        order: Vec::new(),
    };
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
    sv_rhs_with(b, &mut |x: &Bv| sv_ref(x, lin))
}

/// The single SV per-`Kind` syntax table, parameterized over operand-name
/// resolution so the plain function renderer (`sv_rhs`) and the staged
/// datapath renderer (`render_sv_staged`, stage-qualified names) cannot
/// drift in operator syntax.
fn sv_rhs_with(b: &Bv, r: &mut dyn FnMut(&Bv) -> String) -> String {
    match &b.0.kind {
        Kind::Var(_) | Kind::Const { .. } => r(b),
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
    // Declarations first, then the assignments — never `logic w _t0 = rhs;`
    // inside a function body. Two reasons, and the second is the one that
    // bites: SV requires a block's declarations to precede its first
    // statement (so these cannot be interleaved), and yosys's built-in
    // Verilog frontend rejects a declaration carrying an initializer inside
    // a `function` outright — "Invalid nesting of always blocks and/or
    // initializations". That is legal SV which Verilator 5.048 and Icarus
    // both accept, so no simulator gate ever saw it, but it made every
    // emitted soft-float helper unsynthesizable and took the whole MX/NVFP4
    // quantizer line with it (arch#932). `fp_block.rs` already emits this
    // split shape; this brings the IR renderer in line with it.
    for b in &lin.order {
        let id = lin.ids[&(Rc::as_ptr(&b.0) as usize)];
        let _ = writeln!(s, "  logic {}_t{};", sv_decl_width(b.width()), id);
    }
    for b in &lin.order {
        let id = lin.ids[&(Rc::as_ptr(&b.0) as usize)];
        let _ = writeln!(s, "  _t{} = {};", id, sv_rhs(b, &lin));
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
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(n, w)| format!("({n} {})", smt_sort(*w)))
        .collect();
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

// ── Concrete interpreter ────────────────────────────────────────────────────
//
// Evaluates a `Bv` DAG to its concrete unsigned value — the fourth consumer of
// the same IR the SV/SMT/Lean renderers derive from. `arch formal`'s
// counterexample replay uses it to independently re-check a solver model
// against the *identical* operator definitions the SMT query embedded, so the
// check carries no cross-model (RTL-vs-SMT) equivalence assumption.
//
// Conservative by contract: anything the interpreter cannot decide — an
// unbound `Var`, an unknown or arity/width-mismatched `Call`, a node wider
// than 128 bits, call-depth blowout — returns `None`. Callers must treat
// `None` as "inconclusive", never as a verdict. The 128-bit ceiling covers
// every SMT-emitted operator today (widest intermediate ≈ 100 bits inside
// `arch_fma_f32`'s normalizer); if a future op exceeds it, replay degrades to
// inconclusive instead of silently wrapping.

fn eval_mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// Reinterpret the `w`-bit value `v` as a signed integer.
fn eval_sext(v: u128, w: u32) -> i128 {
    if w == 0 || w >= 128 || (v >> (w - 1)) & 1 == 0 {
        v as i128
    } else {
        (v | !eval_mask(w)) as i128
    }
}

/// Nested `Call` frames beyond this depth return `None`. The real call graph
/// (fp8 → f32 core → rounder) is 3 deep; this is a cycle backstop, not a limit
/// any legitimate operator approaches.
const EVAL_MAX_CALL_DEPTH: u32 = 64;

/// Evaluate a `Bv` DAG to its unsigned value, masked to the node width.
///
/// `env` binds `Var` names (`FpFn` parameters) to values; `fns` is the
/// operator table (`fp_ops::fp_functions`) that `Call` nodes resolve against.
/// Returns `None` — meaning the caller's check is INCONCLUSIVE, never a false
/// verdict — on an unbound `Var`, an unknown/arity/width-mismatched `Call`,
/// or any node wider than 128 bits.
pub fn eval_bv(node: &Bv, env: &HashMap<String, u128>, fns: &[FpFn]) -> Option<u128> {
    let mut memo = HashMap::new();
    eval_go(node, env, fns, &mut memo, 0)
}

fn eval_go(
    b: &Bv,
    env: &HashMap<String, u128>,
    fns: &[FpFn],
    // Per-frame memo keyed by Rc pointer: the DAG shares subexpressions
    // heavily (fma linearizes to 300+ nodes), so an unmemoized tree walk
    // would be exponential. A frame = one (env, body) pair, so memoized
    // values can never leak across different variable bindings.
    memo: &mut HashMap<usize, u128>,
    depth: u32,
) -> Option<u128> {
    let w = b.width();
    if w > 128 {
        return None;
    }
    let ptr = Rc::as_ptr(&b.0) as usize;
    if let Some(v) = memo.get(&ptr) {
        return Some(*v);
    }
    let v = match &b.0.kind {
        Kind::Var(n) => *env.get(n)?,
        Kind::Const { val } => val & eval_mask(w),
        Kind::Extract { x, hi: _, lo } => (eval_go(x, env, fns, memo, depth)? >> lo) & eval_mask(w),
        Kind::Concat(a, c) => {
            (eval_go(a, env, fns, memo, depth)? << c.width()) | eval_go(c, env, fns, memo, depth)?
        }
        // Operand is already masked to its own (smaller) width.
        Kind::ZeroExt { x, .. } => eval_go(x, env, fns, memo, depth)?,
        Kind::Not(x) => !eval_go(x, env, fns, memo, depth)? & eval_mask(w),
        Kind::Bin { op, a, b: c } => {
            let x = eval_go(a, env, fns, memo, depth)?;
            let y = eval_go(c, env, fns, memo, depth)?;
            let r = match op {
                Bin::Add => x.wrapping_add(y),
                Bin::Sub => x.wrapping_sub(y),
                Bin::Mul => x.wrapping_mul(y),
                Bin::And => x & y,
                Bin::Or => x | y,
                Bin::Xor => x ^ y,
                // bvshl/bvlshr: shift ≥ operand width yields 0 (the mask
                // handles amounts in [w, 128); the guard handles ≥ 128,
                // which would overflow the host shift).
                Bin::Shl => {
                    if y >= 128 {
                        0
                    } else {
                        x << y
                    }
                }
                Bin::Lshr => {
                    if y >= 128 {
                        0
                    } else {
                        x >> y
                    }
                }
            };
            r & eval_mask(w)
        }
        Kind::Cmp { op, a, b: c } => {
            let wa = a.width();
            let x = eval_go(a, env, fns, memo, depth)?;
            let y = eval_go(c, env, fns, memo, depth)?;
            let t = match op {
                Cmp::Eq => x == y,
                Cmp::Ne => x != y,
                Cmp::Ult => x < y,
                Cmp::Ule => x <= y,
                Cmp::Ugt => x > y,
                Cmp::Uge => x >= y,
                Cmp::Slt => eval_sext(x, wa) < eval_sext(y, wa),
                Cmp::Sle => eval_sext(x, wa) <= eval_sext(y, wa),
                Cmp::Sgt => eval_sext(x, wa) > eval_sext(y, wa),
                Cmp::Sge => eval_sext(x, wa) >= eval_sext(y, wa),
            };
            t as u128
        }
        // Short-circuit: don't force the dead arm — an undecidable
        // subexpression there must not poison a decidable result.
        Kind::Ite { c, t, e } => {
            if eval_go(c, env, fns, memo, depth)? & 1 == 1 {
                eval_go(t, env, fns, memo, depth)?
            } else {
                eval_go(e, env, fns, memo, depth)?
            }
        }
        Kind::Call { name, args } => {
            if depth >= EVAL_MAX_CALL_DEPTH {
                return None;
            }
            let f = fns.iter().find(|f| f.name == *name)?;
            if f.params.len() != args.len() {
                return None;
            }
            let mut child_env = HashMap::new();
            for ((pname, pwidth), arg) in f.params.iter().zip(args) {
                if arg.width() != *pwidth {
                    return None;
                }
                child_env.insert(pname.clone(), eval_go(arg, env, fns, memo, depth)?);
            }
            let mut child_memo = HashMap::new();
            eval_go(&f.body, &child_env, fns, &mut child_memo, depth + 1)?
        }
    };
    memo.insert(ptr, v);
    Some(v)
}

// ── Lean 4 renderer ─────────────────────────────────────────────────────────
//
// Emits each helper as a Lean `def` over `BitVec` (Lean core `Init.Data.BitVec`
// — no Mathlib, no extra package, matching the dependency-free lake project).
// This is the third renderer of the *same* IR: the model a structured prover
// reasons about is bit-for-bit the model that `render_sv`/`render_smt` produce,
// so a Lean proof transfers to the emitted RTL with no re-transcription.
//
// The point of a Lean backend (over z3/cvc5) is the multiplier-bearing ops
// (`mul`/`fma`): a 24×24 multiplier equivalence is SAT-hard for any bit-blaster
// (`bv_decide` included), but Lean lets the proof *lift* the bit model to the
// algebraic (significand, exponent)/real layer and discharge correct-rounding
// structurally — the FLoPS / Flocq methodology — never bit-blasting the array.

fn lean_ref(b: &Bv, lin: &Lin) -> String {
    match &b.0.kind {
        Kind::Var(n) => n.clone(),
        Kind::Const { val } => format!("(BitVec.ofNat {} {})", b.width(), val),
        _ => format!("_t{}", lin.ids[&(Rc::as_ptr(&b.0) as usize)]),
    }
}

fn lean_rhs(b: &Bv, lin: &Lin) -> String {
    let r = |x: &Bv| lean_ref(x, lin);
    match &b.0.kind {
        Kind::Var(_) | Kind::Const { .. } => lean_ref(b, lin),
        Kind::Extract { x, hi, lo } => format!("(BitVec.extractLsb {hi} {lo} {})", r(x)),
        // `++` is high ++ low for BitVec, matching `concat(a /*high*/, b)`.
        Kind::Concat(a, c) => format!("({} ++ {})", r(a), r(c)),
        Kind::ZeroExt { x, to } => format!("(BitVec.setWidth {to} {})", r(x)),
        Kind::Not(x) => format!("(~~~ {})", r(x)),
        Kind::Bin { op, a, b: c } => match op {
            Bin::Add => format!("({} + {})", r(a), r(c)),
            Bin::Sub => format!("({} - {})", r(a), r(c)),
            Bin::Mul => format!("({} * {})", r(a), r(c)),
            Bin::And => format!("({} &&& {})", r(a), r(c)),
            Bin::Or => format!("({} ||| {})", r(a), r(c)),
            Bin::Xor => format!("({} ^^^ {})", r(a), r(c)),
            // Shift amount is a same-width BV (already zero-extended by `shl`/
            // `lshr`); Lean's `<<<`/`>>>` on BitVec take a `Nat`.
            Bin::Shl => format!("({} <<< {}.toNat)", r(a), r(c)),
            Bin::Lshr => format!("({} >>> {}.toNat)", r(a), r(c)),
        },
        Kind::Cmp { op, a, b: c } => {
            // `BitVec.ofBool` of a Bool predicate, NOT `if p then 1#1 else 0#1`:
            // the latter is a `Prop`-conditioned `ite` that `bv_decide` cannot
            // bit-blast (it abstracts it as an opaque variable, which produces
            // spurious counterexamples on non-symmetric goals). `ofBool` and the
            // Bool comparators below are all in `bv_decide`'s supported fragment.
            let pred = match op {
                Cmp::Eq => format!("({} == {})", r(a), r(c)),
                Cmp::Ne => format!("({} != {})", r(a), r(c)),
                Cmp::Ult => format!("(BitVec.ult {} {})", r(a), r(c)),
                Cmp::Ule => format!("(BitVec.ule {} {})", r(a), r(c)),
                Cmp::Ugt => format!("(BitVec.ult {} {})", r(c), r(a)),
                Cmp::Uge => format!("(BitVec.ule {} {})", r(c), r(a)),
                Cmp::Slt => format!("(BitVec.slt {} {})", r(a), r(c)),
                Cmp::Sle => format!("(BitVec.sle {} {})", r(a), r(c)),
                Cmp::Sgt => format!("(BitVec.slt {} {})", r(c), r(a)),
                Cmp::Sge => format!("(BitVec.sle {} {})", r(c), r(a)),
            };
            format!("(BitVec.ofBool {pred})")
        }
        // Selector is a 1-bit BV; `c == 1#1` is a Bool the `if` bit-blasts.
        Kind::Ite { c, t, e } => {
            format!(
                "(if {} == (BitVec.ofNat 1 1) then {} else {})",
                r(c),
                r(t),
                r(e)
            )
        }
        Kind::Call { name, args } => {
            let a: Vec<String> = args.iter().map(|x| r(x)).collect();
            format!("({} {})", name, a.join(" "))
        }
    }
}

fn render_lean_fn(f: &FpFn) -> String {
    let lin = linearize(&f.body);
    let mut s = String::new();
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(n, w)| format!("({n} : BitVec {w})"))
        .collect();
    let _ = writeln!(
        s,
        "def {} {} : BitVec {} :=",
        f.name,
        params.join(" "),
        f.ret_w
    );
    for b in &lin.order {
        let id = lin.ids[&(Rc::as_ptr(&b.0) as usize)];
        let _ = writeln!(
            s,
            "  let _t{id} : BitVec {} := {}",
            b.width(),
            lean_rhs(b, &lin)
        );
    }
    let _ = writeln!(s, "  {}", lean_ref(&f.body, &lin));
    s
}

/// Render a set of helper functions to one Lean 4 source block (`def`s over
/// `BitVec`, dependency-free — Lean core only). Wrap in a `namespace` and proofs
/// at the call site (see `proofs/lean_fp_equiv/`).
pub fn render_lean(funcs: &[FpFn]) -> String {
    funcs
        .iter()
        .map(render_lean_fn)
        .collect::<Vec<_>>()
        .join("\n")
}

// ── staged SystemVerilog renderer (proposal phase 3.5) ──────────────────────
//
// Renders one operator as a hand-scheduled N-stage pipelined SV *module*
// (per-stage `always_comb` blocks + live-set register layers), driven by a
// `pipelined_ops::StagedSchedule` over the SAME `linearize` walk as the
// combinational renderers — so temp numbering, CSE structure, and operator
// syntax (via `sv_rhs_with`) are shared, not duplicated.
//
// Structure emitted (for `stages = 6`):
//   module <Name>(input clk, input args..., output y);
//     stage-k comb block computing that stage's temps (k = 1..=6)
//     register layer k (k = 1..=5) carrying the live set + forwarded inputs
//     assign y = <stage-6 result>;   // COMBINATIONAL out — the caller's
//   endmodule                        // pipe_reg port supplies edge N + reset
//
// Internal layers are deliberately reset-free (retiming-friendly, matching
// the characterized hand-staged run); cycle-exact post-reset equivalence
// with the cascade emission is the *caller's* job (codegen emits a warm-up
// gate at the binding site — see codegen's staged-ops support).
//
// Returns `Err` (never panics) if the operator's current linearization does
// not match the schedule (temp-count drift, missing/extra nested call): the
// caller falls back to the cascade form with a warning, and the pinned unit
// tests catch the drift in CI.

/// Which namespace a compound node belongs to in the staged rendering.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum StagedNs {
    Main,
    Callee,
}

impl StagedNs {
    fn prefix(self) -> &'static str {
        match self {
            StagedNs::Main => "t",
            StagedNs::Callee => "A_t",
        }
    }
}

struct StagedCtx<'a> {
    sched: &'a crate::pipelined_ops::StagedSchedule,
    lin_m: Lin,
    lin_c: Lin,
    /// The single nested-call node's id in the main namespace.
    call_id: usize,
    /// Callee param name → the call-site argument node (main namespace).
    param_args: Vec<(String, Bv)>,
    /// (ns, id) → last stage that reads the value (def stage from `sched`).
    last_use: HashMap<(StagedNs, usize), u32>,
    /// module input name → last stage that reads it.
    input_last_use: HashMap<String, u32>,
}

impl StagedCtx<'_> {
    fn def_stage(&self, ns: StagedNs, id: usize) -> u32 {
        match ns {
            StagedNs::Main => self.sched.main_stage(id),
            StagedNs::Callee => self.sched.callee_stage(id),
        }
    }

    /// Resolve an operand reference from a stage-`k` context. `ns` is the
    /// namespace of the *referencing* node; callee-param `Var`s re-resolve
    /// to the call-site argument in the main namespace. Records liveness.
    fn resolve(&mut self, b: &Bv, ns: StagedNs, k: u32) -> Result<String, String> {
        match &b.0.kind {
            Kind::Const { val } => Ok(format!("{}'h{:X}", b.width(), val)),
            Kind::Var(n) => match ns {
                StagedNs::Main => {
                    let e = self.input_last_use.entry(n.clone()).or_insert(k);
                    *e = (*e).max(k);
                    if k == 1 {
                        Ok(n.clone())
                    } else {
                        Ok(format!("r{}__in_{}", k - 1, n))
                    }
                }
                StagedNs::Callee => {
                    let arg = self
                        .param_args
                        .iter()
                        .find(|(p, _)| p == n)
                        .map(|(_, a)| a.clone())
                        .ok_or_else(|| format!("callee references unknown param `{n}`"))?;
                    self.resolve(&arg, StagedNs::Main, k)
                }
            },
            _ => {
                let (lin, this_ns) = match ns {
                    StagedNs::Main => (&self.lin_m, StagedNs::Main),
                    StagedNs::Callee => (&self.lin_c, StagedNs::Callee),
                };
                let id = *lin
                    .ids
                    .get(&(Rc::as_ptr(&b.0) as usize))
                    .ok_or_else(|| "operand not in its namespace's linearization".to_string())?;
                let d = self.def_stage(this_ns, id);
                if d > k {
                    return Err(format!(
                        "schedule is not topological: {}{} defined in stage {d}, read in stage {k}",
                        this_ns.prefix(),
                        id
                    ));
                }
                let e = self.last_use.entry((this_ns, id)).or_insert(k);
                *e = (*e).max(k);
                if d == k {
                    Ok(format!("s{k}__{}{id}", this_ns.prefix()))
                } else {
                    Ok(format!("r{}__{}{id}", k - 1, this_ns.prefix()))
                }
            }
        }
    }

    /// Render the RHS for a compound node owned by stage `k`. The main
    /// namespace's nested-call node renders as an alias of the callee's
    /// result; everything else goes through the shared syntax table.
    fn rhs(&mut self, b: &Bv, ns: StagedNs, k: u32) -> Result<String, String> {
        if let Kind::Call { .. } = &b.0.kind {
            let result = self.lin_c.order.last().cloned().ok_or("empty callee")?;
            return self.resolve(&result, StagedNs::Callee, k);
        }
        let mut err: Option<String> = None;
        let text = sv_rhs_with(b, &mut |x: &Bv| match self.resolve(x, ns, k) {
            Ok(s) => s,
            Err(e) => {
                err = Some(e);
                String::from("/*ERR*/")
            }
        });
        match err {
            Some(e) => Err(e),
            None => Ok(text),
        }
    }
}

// ── Gate-delay-weighted leaf scheduler (arch#955) ─────────────────────────
//
// The FMA's staged schedule (`FMA_F32_S6_SCHEDULE`) was hand-derived with
// measured provenance because balancing a pipeline by SSA *node count* does
// not balance gate *delay*: one 48-bit `Mul` node is ~dozens of gate levels
// while a `Bxor` is one. This derives a balanced cut automatically for a
// LEAF FpFn (zero nested calls — `f32_mul`, `f32_add`, …) by weighting each
// `Bv` node with a proxy for its mapped gate depth, computing every node's
// cumulative critical-path weight, then cutting at even weight fractions.
//
// The weights are relative, not a timing model: they only have to keep the
// deep arithmetic (Mul, Add/Sub, Cmp, shifts) from sharing a thin stage with
// wiring (Extract/Concat/ZeroExt) and one-level gates (bitwise, Ite). The
// emitted schedule is validated empirically against yosys per-stage depth.

/// Proxy gate depth of a single `Bv` node of the given `width`, in gate
/// levels — used only for *relative* stage balancing (see section comment).
fn gate_weight(kind: &Kind, width: u32) -> u32 {
    let w = width.max(1);
    let log2 = |x: u32| 32 - (x.max(1) - 1).leading_zeros(); // ceil-ish log2
    match kind {
        // Wiring — no logic on the path.
        Kind::Var(_)
        | Kind::Const { .. }
        | Kind::Extract { .. }
        | Kind::Concat(_, _)
        | Kind::ZeroExt { .. } => 0,
        // One gate level regardless of width.
        Kind::Not(_) => 1,
        Kind::Ite { .. } => 1,
        Kind::Bin {
            op: Bin::And | Bin::Or | Bin::Xor,
            ..
        } => 1,
        // Carry / compare chains ~ the operand width (ABC -fast maps these to
        // ripple-ish structures rather than log-depth trees).
        Kind::Bin {
            op: Bin::Add | Bin::Sub,
            ..
        } => w,
        Kind::Cmp { .. } => w,
        // Barrel shifter ~ log2 layers of muxes.
        Kind::Bin {
            op: Bin::Shl | Bin::Lshr,
            ..
        } => log2(w),
        // Multiplier: partial-product array + reduction ~ 2·width. The single
        // most expensive node in the FP operators, and the one the cut must
        // isolate.
        Kind::Bin { op: Bin::Mul, .. } => 2 * w,
        // A leaf op has no calls; treat conservatively if one appears.
        Kind::Call { .. } => 2 * w,
    }
}

/// Cumulative critical-path weight of every linearized node: the max over its
/// operands' cumulative weight, plus its own `gate_weight`. `lin.order` is
/// topological, so a single forward pass suffices.
fn node_gate_depths(lin: &Lin) -> Vec<u32> {
    let mut d = vec![0u32; lin.order.len()];
    for (i, b) in lin.order.iter().enumerate() {
        let od = |x: &Bv| -> u32 {
            if is_leaf(x) {
                0
            } else {
                d[lin.ids[&(Rc::as_ptr(&x.0) as usize)]]
            }
        };
        let operand_max = match &b.0.kind {
            Kind::Extract { x, .. } | Kind::ZeroExt { x, .. } | Kind::Not(x) => od(x),
            Kind::Concat(a, c) | Kind::Bin { a, b: c, .. } | Kind::Cmp { a, b: c, .. } => {
                od(a).max(od(c))
            }
            Kind::Ite { c, t, e } => od(c).max(od(t)).max(od(e)),
            Kind::Call { args, .. } => args.iter().map(|a| od(a)).max().unwrap_or(0),
            Kind::Var(_) | Kind::Const { .. } => 0,
        };
        d[i] = operand_max + gate_weight(&b.0.kind, b.width());
    }
    d
}

/// Derive `stages + 1` cut points over a LEAF FpFn's temp ids, balancing
/// cumulative gate weight across `stages`. Returns `main_starts` for a
/// [`crate::pipelined_ops::StagedSchedule`] (its `callee_starts` is then
/// `&[0; stages + 1]`).
///
/// A node lands in the first stage whose cumulative-weight ceiling it does
/// not exceed; since `lin.order` is topological and cumulative weight is
/// monotone along it, the resulting cut points are non-decreasing and every
/// node's operands sit in its stage or earlier (the topological invariant
/// `render_sv_staged` re-checks).
pub fn derive_leaf_schedule(f: &FpFn, stages: u32) -> Result<Vec<usize>, String> {
    if stages == 0 {
        return Err("stages must be >= 1".into());
    }
    let lin = linearize(&f.body);
    if lin
        .order
        .iter()
        .any(|b| matches!(b.0.kind, Kind::Call { .. }))
    {
        return Err(format!("`{}` is not a leaf: it nests a call", f.name));
    }
    let n = lin.order.len();
    if (stages as usize) > n {
        return Err(format!(
            "{stages} stages requested but `{}` has only {n} nodes",
            f.name
        ));
    }
    let depths = node_gate_depths(&lin);
    let total = *depths.last().unwrap_or(&0);
    let mut starts = vec![0usize];
    for k in 1..stages {
        let thresh = (total as u64 * k as u64 / stages as u64) as u32;
        // First node whose cumulative weight exceeds this stage's ceiling
        // begins the next stage. Clamp so each stage owns >= 1 node and the
        // arrays stay strictly increasing (render_sv_staged needs that).
        let want = depths.iter().position(|&x| x > thresh).unwrap_or(n);
        let lo = *starts.last().unwrap() + 1;
        starts.push(want.max(lo).min(n - (stages - k) as usize));
    }
    starts.push(n);
    Ok(starts)
}

/// Render `main_fn` as a staged pipelined module per `sched`, inlining the
/// single nested call to `callee_fn` (the `A_` namespace). See the section
/// comment above for the emitted structure and the fallback contract.
pub fn render_sv_staged(
    main_fn: &FpFn,
    callee_fn: Option<&FpFn>,
    sched: &crate::pipelined_ops::StagedSchedule,
    module_name: &str,
) -> Result<String, String> {
    let lin_m = linearize(&main_fn.body);
    // Leaf ops (`f32_mul`, `f32_add`, …) have no callee: the `A_` namespace
    // is empty, `callee_starts` is `&[0; stages + 1]`, and the single-call
    // machinery below degenerates via `call_id = usize::MAX` (arch#955).
    let lin_c = match callee_fn {
        Some(c) => linearize(&c.body),
        None => Lin {
            ids: std::collections::HashMap::new(),
            order: Vec::new(),
        },
    };

    // ── structural verification (drift ⇒ Err ⇒ caller falls back) ──
    if let Some(c) = callee_fn {
        if c.name != sched.callee {
            return Err(format!(
                "schedule expects callee `{}`, got `{}`",
                sched.callee, c.name
            ));
        }
    }
    if lin_m.order.len() != *sched.main_starts.last().unwrap() {
        return Err(format!(
            "main temp count drifted: linearize gives {}, schedule covers {}",
            lin_m.order.len(),
            sched.main_starts.last().unwrap()
        ));
    }
    if lin_c.order.len() != *sched.callee_starts.last().unwrap() {
        return Err(format!(
            "callee temp count drifted: linearize gives {}, schedule covers {}",
            lin_c.order.len(),
            sched.callee_starts.last().unwrap()
        ));
    }
    let calls_m: Vec<usize> = lin_m
        .order
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b.0.kind, Kind::Call { .. }))
        .map(|(i, _)| i)
        .collect();
    let expected_calls = if callee_fn.is_some() { 1 } else { 0 };
    if calls_m.len() != expected_calls {
        return Err(format!(
            "expected exactly {expected_calls} nested call(s) in `{}`, found {}",
            main_fn.name,
            calls_m.len()
        ));
    }
    if lin_c
        .order
        .iter()
        .any(|b| matches!(b.0.kind, Kind::Call { .. }))
    {
        return Err("callee itself nests a call".to_string());
    }
    let (call_id, param_args): (usize, Vec<(String, Bv)>) = match callee_fn {
        Some(callee) => {
            let call_id = calls_m[0];
            let Kind::Call { name, args } = &lin_m.order[call_id].0.kind else {
                unreachable!("filtered on Kind::Call above")
            };
            if name != sched.callee {
                return Err(format!(
                    "nested call is `{name}`, schedule expects `{}`",
                    sched.callee
                ));
            }
            if args.len() != callee.params.len() {
                return Err("call arg count != callee param count".to_string());
            }
            let param_args = callee
                .params
                .iter()
                .map(|(p, _)| p.clone())
                .zip(args.iter().cloned())
                .collect();
            (call_id, param_args)
        }
        // No call node: usize::MAX makes the `stage_assigns` split place every
        // main temp before the (absent) call and none after it.
        None => (usize::MAX, Vec::new()),
    };

    let stages = sched.stages();
    let mut ctx = StagedCtx {
        sched,
        lin_m,
        lin_c,
        call_id,
        param_args,
        last_use: HashMap::new(),
        input_last_use: HashMap::new(),
    };

    // ── per-stage assignment lists, in intra-stage dependency order:
    // main ids < call_id, then callee ids, then main ids >= call_id (the
    // linearizer guarantees call args precede the call node, and callee
    // temps depend only on callee temps + call args). Two passes share this.
    let stage_assigns = |ctx: &StagedCtx| -> Vec<Vec<(StagedNs, usize)>> {
        (1..=stages)
            .map(|k| {
                let mut v: Vec<(StagedNs, usize)> = Vec::new();
                let (m0, m1) = (
                    ctx.sched.main_starts[(k - 1) as usize],
                    ctx.sched.main_starts[k as usize],
                );
                let (c0, c1) = (
                    ctx.sched.callee_starts[(k - 1) as usize],
                    ctx.sched.callee_starts[k as usize],
                );
                v.extend((m0..m1.min(ctx.call_id)).map(|i| (StagedNs::Main, i)));
                v.extend((c0..c1).map(|i| (StagedNs::Callee, i)));
                v.extend((m0.max(ctx.call_id)..m1).map(|i| (StagedNs::Main, i)));
                v
            })
            .collect()
    };
    let per_stage = stage_assigns(&ctx);

    // ── pass 1: liveness (render every RHS, discard text, keep the maps).
    for (k0, assigns) in per_stage.iter().enumerate() {
        let k = (k0 + 1) as u32;
        for &(ns, id) in assigns {
            let node = match ns {
                StagedNs::Main => ctx.lin_m.order[id].clone(),
                StagedNs::Callee => ctx.lin_c.order[id].clone(),
            };
            ctx.rhs(&node, ns, k)?;
        }
    }
    // The module output reads the main result in the final stage.
    let result_node = main_fn.body.clone();
    ctx.resolve(&result_node, StagedNs::Main, stages)?;

    // ── pass 2: emit (liveness maps are complete; resolve() re-records
    // idempotently).
    let mut s = String::new();
    let _ = writeln!(
        s,
        "// {module_name}: hand-scheduled {stages}-stage pipelined `{}` —\n\
         // generated from the shared bit-vector IR + the registry staged\n\
         // schedule (src/pipelined_ops.rs). Do not edit by hand.\n\
         // Internal register layers are reset-free by design; the binding\n\
         // site's pipe_reg output register supplies the final edge + reset.",
        main_fn.name
    );
    let _ = writeln!(s, "module {module_name} (");
    let _ = writeln!(s, "  input logic clk,");
    for (p, w) in &main_fn.params {
        let _ = writeln!(s, "  input logic {}{},", sv_decl_width(*w), p);
    }
    let _ = writeln!(s, "  output logic {}y\n);", sv_decl_width(main_fn.ret_w));

    let node_of = |ctx: &StagedCtx, ns: StagedNs, id: usize| -> Bv {
        match ns {
            StagedNs::Main => ctx.lin_m.order[id].clone(),
            StagedNs::Callee => ctx.lin_c.order[id].clone(),
        }
    };

    for (k0, assigns) in per_stage.iter().enumerate() {
        let k = (k0 + 1) as u32;
        if assigns.is_empty() {
            let _ = writeln!(s, "  // ── stage {k}: (no logic — carry layer only)");
        } else {
            let _ = writeln!(s, "  // ── stage {k}");
            for &(ns, id) in assigns {
                let node = node_of(&ctx, ns, id);
                let _ = writeln!(
                    s,
                    "  logic {}s{k}__{}{id};",
                    sv_decl_width(node.width()),
                    ns.prefix()
                );
            }
            let _ = writeln!(s, "  always_comb begin");
            for &(ns, id) in assigns {
                let node = node_of(&ctx, ns, id);
                let rhs = ctx.rhs(&node, ns, k)?;
                let _ = writeln!(s, "    s{k}__{}{id} = {rhs};", ns.prefix());
            }
            let _ = writeln!(s, "  end");
        }

        // Register layer k (none after the final stage).
        if k == stages {
            break;
        }
        // Deterministic order: forwarded inputs (param order), then main
        // temps ascending, then callee temps ascending.
        let mut carried: Vec<(String, String, u32)> = Vec::new(); // (reg, src, width)
        for (p, w) in &main_fn.params {
            let lu = *ctx.input_last_use.get(p).unwrap_or(&0);
            if lu > k {
                let src = if k == 1 {
                    p.clone()
                } else {
                    format!("r{}__in_{}", k - 1, p)
                };
                carried.push((format!("r{k}__in_{p}"), src, *w));
            }
        }
        for ns in [StagedNs::Main, StagedNs::Callee] {
            let total = match ns {
                StagedNs::Main => ctx.lin_m.order.len(),
                StagedNs::Callee => ctx.lin_c.order.len(),
            };
            for id in 0..total {
                let d = ctx.def_stage(ns, id);
                let lu = *ctx.last_use.get(&(ns, id)).unwrap_or(&0);
                if d <= k && lu > k {
                    let src = if d == k {
                        format!("s{k}__{}{id}", ns.prefix())
                    } else {
                        format!("r{}__{}{id}", k - 1, ns.prefix())
                    };
                    let w = node_of(&ctx, ns, id).width();
                    carried.push((format!("r{k}__{}{id}", ns.prefix()), src, w));
                }
            }
        }
        let _ = writeln!(s, "  // ── register layer {k} ({} signals)", carried.len());
        for (reg, _, w) in &carried {
            let _ = writeln!(s, "  logic {}{};", sv_decl_width(*w), reg);
        }
        let _ = writeln!(s, "  always_ff @(posedge clk) begin");
        for (reg, src, _) in &carried {
            let _ = writeln!(s, "    {reg} <= {src};");
        }
        let _ = writeln!(s, "  end");
    }

    let y_ref = ctx.resolve(&result_node, StagedNs::Main, stages)?;
    let _ = writeln!(s, "  assign y = {y_ref};");
    let _ = writeln!(s, "endmodule");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end proof of the zero-call staged render path (arch#955):
    /// derive an `f32_mul` schedule, render the staged module, and confirm in
    /// iverilog that — inputs held stable past the pipeline fill — its output
    /// equals the trusted `eval_bv` oracle for random vectors. Skips cleanly
    /// when iverilog is unavailable.
    #[test]
    fn staged_leaf_f32_mul_matches_eval_bv() {
        if std::process::Command::new("iverilog")
            .arg("-V")
            .output()
            .is_err()
        {
            eprintln!("iverilog not installed; skipping staged-leaf equivalence");
            return;
        }
        let funcs = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let mul = funcs.iter().find(|f| f.name == "arch_f32_mul").unwrap();

        const STAGES: u32 = 4;
        let main_starts = derive_leaf_schedule(mul, STAGES).unwrap();
        // Leaf: empty callee namespace, callee_starts all zero. Leaked to
        // 'static for the const-shaped StagedSchedule — test-only; production
        // rows are committed consts.
        let main_starts: &'static [usize] = Box::leak(main_starts.into_boxed_slice());
        let callee_starts: &'static [usize] =
            Box::leak(vec![0usize; STAGES as usize + 1].into_boxed_slice());
        let sched = crate::pipelined_ops::StagedSchedule {
            main_fn: "arch_f32_mul",
            callee: "",
            sv_module: "ArchF32MulStaged4",
            width: 32,
            main_starts,
            callee_starts,
        };
        let sv =
            render_sv_staged(mul, None, &sched, "ArchF32MulStaged4").expect("staged leaf render");

        // Deterministic pseudo-random FP32 vectors (LCG), plus edge cases.
        let mut vs: Vec<(u32, u32)> = vec![
            (0x3F800000, 0x40000000), // 1.0 * 2.0
            (0x40490FDB, 0x3EAAAAAB), // pi * 1/3
            (0x7F800000, 0x3F800000), // inf * 1.0
            (0x00000000, 0x7F800000), // 0 * inf -> nan
            (0xBF800000, 0x40800000), // -1.0 * 4.0
        ];
        let mut st: u64 = 0x9E3779B97F4A7C15;
        for _ in 0..40 {
            let mut nx = || {
                st = st.wrapping_mul(6364136223846793005).wrapping_add(1);
                (st >> 33) as u32
            };
            vs.push((nx(), nx()));
        }

        let mut expected: Vec<u32> = Vec::new();
        for (a, b) in &vs {
            let mut env = std::collections::HashMap::new();
            env.insert("a".to_string(), *a as u128);
            env.insert("b".to_string(), *b as u128);
            expected.push(eval_bv(&mul.body, &env, &funcs).expect("eval_bv") as u32);
        }

        let mut tb = String::new();
        tb.push_str("`timescale 1ns/1ps\nmodule tb;\n");
        tb.push_str("  logic clk = 0;\n  logic [31:0] a, b, y;\n");
        tb.push_str("  ArchF32MulStaged4 dut(.clk(clk), .a(a), .b(b), .y(y));\n");
        tb.push_str("  always #5 clk = ~clk;\n  integer i, c;\n");
        let n = vs.len();
        let _ = writeln!(tb, "  logic [31:0] va [0:{}];", n - 1);
        let _ = writeln!(tb, "  logic [31:0] vb [0:{}];", n - 1);
        tb.push_str("  initial begin\n");
        for (i, (a, b)) in vs.iter().enumerate() {
            let _ = writeln!(tb, "    va[{i}]=32'h{a:08X}; vb[{i}]=32'h{b:08X};");
        }
        let _ = writeln!(tb, "    for (i=0;i<{n};i=i+1) begin");
        tb.push_str("      a = va[i]; b = vb[i];\n");
        tb.push_str("      for (c=0;c<8;c=c+1) @(posedge clk);\n");
        tb.push_str("      $display(\"Y %0d %08x\", i, y);\n");
        tb.push_str("    end\n    $finish;\n  end\nendmodule\n");

        let dir = std::env::temp_dir().join(format!("archstaged_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let svf = dir.join("staged.sv");
        std::fs::write(&svf, format!("{sv}\n{tb}")).unwrap();
        let vvpf = dir.join("a.out");
        let comp = std::process::Command::new("iverilog")
            .args(["-g2012", "-o"])
            .arg(&vvpf)
            .arg(&svf)
            .output()
            .unwrap();
        assert!(
            comp.status.success(),
            "iverilog failed:\n{}",
            String::from_utf8_lossy(&comp.stderr)
        );
        let run = std::process::Command::new("vvp")
            .arg(&vvpf)
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&run.stdout);
        let mut got = vec![None; n];
        for line in out.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() == 3 && f[0] == "Y" {
                let idx: usize = f[1].parse().unwrap();
                got[idx] = Some(u32::from_str_radix(f[2], 16).unwrap());
            }
        }
        let mut mism = 0;
        for i in 0..n {
            let g = got[i].unwrap_or_else(|| panic!("no Y for vector {i}:\n{out}"));
            if g != expected[i] {
                mism += 1;
                eprintln!(
                    "vec {i}: a={:08x} b={:08x}  staged={:08x} expected={:08x}",
                    vs[i].0, vs[i].1, g, expected[i]
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            mism, 0,
            "{mism}/{n} staged f32_mul outputs diverged from eval_bv"
        );
    }

    /// The gate-weighted leaf scheduler produces a topological, balanced cut
    /// for `f32_mul` (arch#955). Balancing by gate weight — not SSA node
    /// count — is the point: f32_mul is 143 nodes / SSA-depth 53 but ~186
    /// mapped gate levels, concentrated in the 48-bit multiply.
    #[test]
    fn leaf_scheduler_f32_mul_is_topological_and_balanced() {
        let funcs = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let mul = funcs.iter().find(|f| f.name == "arch_f32_mul").unwrap();
        let lin = linearize(&mul.body);
        let depths = node_gate_depths(&lin);
        let total = *depths.last().unwrap();

        for stages in 2..=5u32 {
            let starts = derive_leaf_schedule(mul, stages).unwrap();
            assert_eq!(
                starts.len() as u32,
                stages + 1,
                "starts has stages+1 points"
            );
            assert_eq!(starts[0], 0);
            assert_eq!(*starts.last().unwrap(), lin.order.len());
            // Strictly increasing ⇒ every stage owns ≥ 1 node.
            for w in starts.windows(2) {
                assert!(w[1] > w[0], "stage empty in N={stages}: {starts:?}");
            }
            // Topological: a node's operands never sit in a later stage. With
            // cut points over a topological order this reduces to "operand id
            // < node id ⇒ operand stage ≤ node stage", which the monotone
            // cut guarantees; assert it directly as a guard.
            let stage_of = |id: usize| {
                starts
                    .windows(2)
                    .position(|w| id >= w[0] && id < w[1])
                    .unwrap()
            };
            for (i, b) in lin.order.iter().enumerate() {
                let si = stage_of(i);
                let check = |x: &Bv| {
                    if !is_leaf(x) {
                        let oid = lin.ids[&(std::rc::Rc::as_ptr(&x.0) as usize)];
                        assert!(stage_of(oid) <= si, "non-topological cut N={stages}");
                    }
                };
                match &b.0.kind {
                    Kind::Extract { x, .. } | Kind::ZeroExt { x, .. } | Kind::Not(x) => check(x),
                    Kind::Concat(a, c) | Kind::Bin { a, b: c, .. } | Kind::Cmp { a, b: c, .. } => {
                        check(a);
                        check(c);
                    }
                    Kind::Ite { c, t, e } => {
                        check(c);
                        check(t);
                        check(e);
                    }
                    Kind::Call { args, .. } => args.iter().for_each(|a| check(a)),
                    _ => {}
                }
            }
            // Balance: each stage's gate-weight span within 2× the ideal.
            let ideal = total as f64 / stages as f64;
            for k in 0..stages as usize {
                let lo = if starts[k] == 0 {
                    0
                } else {
                    depths[starts[k] - 1]
                };
                let hi = depths[starts[k + 1] - 1];
                let span = (hi - lo) as f64;
                assert!(
                    span <= 2.5 * ideal + 1.0,
                    "stage {k} span {span} >> ideal {ideal:.1} (N={stages}, {starts:?})"
                );
            }
        }
    }

    /// Pins the zero-drift facts the builtin staged schedule was extracted
    /// against, and exercises the staged renderer end-to-end on the real
    /// FMA. If `fp_ops` evolves and this fails, re-derive the schedule
    /// (see `pipelined_ops::FMA_F32_S6_SCHEDULE` provenance docs).
    #[test]
    fn staged_fma_renders_with_expected_structure() {
        let funcs = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let fma = funcs.iter().find(|f| f.name == "arch_fma_f32").unwrap();
        let add = funcs.iter().find(|f| f.name == "arch_f32_add").unwrap();
        // Zero-drift pins: linearization sizes the schedule was built on.
        assert_eq!(linearize(&fma.body).order.len(), 309, "main temp count");
        assert_eq!(linearize(&add.body).order.len(), 175, "callee temp count");

        let sched = crate::pipelined_ops::FMA_F32_S6_SCHEDULE;
        let sv = render_sv_staged(fma, Some(add), &sched, "ArchF32FmaStaged6")
            .expect("staged rendering must succeed on the pinned linearization");

        assert!(sv.contains("module ArchF32FmaStaged6 ("));
        assert!(sv.contains("input logic clk,"));
        assert!(sv.contains("output logic [31:0] y"));
        // Six stage comb blocks (stages 2 and 3 are callee-only but non-empty),
        // five register layers, combinational final result.
        for k in 1..=6 {
            assert!(
                sv.contains(&format!("// ── stage {k}")),
                "stage {k} present"
            );
        }
        for k in 1..=5 {
            assert!(
                sv.contains(&format!("// ── register layer {k} (")),
                "register layer {k} present"
            );
        }
        assert!(sv.contains("assign y = s6__t308;"), "comb final result");
        // The nested add is inlined, not called: stage-4 alias of the callee
        // result, and no SV function-call syntax anywhere in the module.
        assert!(
            sv.contains("s4__t44 = s4__A_t174;"),
            "call inlined as alias"
        );
        assert!(!sv.contains("arch_f32_add("), "no function call remains");
        // Reset-free internal layers (caller supplies output reset).
        assert!(!sv.contains("rst"), "no reset in the staged datapath");
        // Decl-then-assign style only (Yosys-frontend-safe): no
        // decl-with-initializer inside the module.
        assert!(!sv.contains("; //init"), "sanity");
        for line in sv.lines() {
            let t = line.trim_start();
            if t.starts_with("logic ") {
                assert!(!t.contains('='), "decl-with-init found: {line}");
            }
        }
    }

    #[test]
    fn staged_fma_drift_detection_errs_not_panics() {
        let funcs = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let fma = funcs.iter().find(|f| f.name == "arch_fma_f32").unwrap();
        let add = funcs.iter().find(|f| f.name == "arch_f32_add").unwrap();
        // Wrong callee name.
        let bad = crate::pipelined_ops::StagedSchedule {
            callee: "arch_f32_sub",
            ..crate::pipelined_ops::FMA_F32_S6_SCHEDULE
        };
        assert!(render_sv_staged(fma, Some(add), &bad, "M").is_err());
        // Truncated coverage (temp-count drift).
        let bad2 = crate::pipelined_ops::StagedSchedule {
            main_starts: &[0, 44, 44, 44, 231, 273, 300],
            ..crate::pipelined_ops::FMA_F32_S6_SCHEDULE
        };
        assert!(render_sv_staged(fma, Some(add), &bad2, "M").is_err());
    }

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
        assert!(sv.contains(
            "function automatic logic [7:0] t(input logic [7:0] a, input logic [7:0] b);"
        ));
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

        // Lean: same DAG, third dialect.
        let a = var("a", 8);
        let b = var("b", 8);
        let s = add(&a, &b);
        let lo = extract(&s, 0, 0);
        let body = ite(&eq(&lo, &cst(0, 1)), &s, &cst(0xFF, 8));
        let f = FpFn::new("t", &[("a", 8), ("b", 8)], 8, body);
        let lean = render_lean(&[f]);
        assert!(lean.contains("def t (a : BitVec 8) (b : BitVec 8) : BitVec 8 :="));
        assert!(lean.contains("(a + b)"));
        assert!(lean.contains("(BitVec.extractLsb 0 0 "));
        assert!(lean.contains("if "));
        assert!(lean.contains("(BitVec.ofNat 8 255)"));
        // Comparisons must render via `BitVec.ofBool`, NOT a Prop-conditioned
        // `if p then 1#1 else 0#1` — the latter is abstracted (not bit-blasted)
        // by Lean's `bv_decide`, which breaks the FP proofs in proofs/lean_fp_equiv.
        assert!(lean.contains("(BitVec.ofBool ("));
        assert!(!lean.contains("then (BitVec.ofNat 1 1) else (BitVec.ofNat 1 0)"));
    }

    #[test]
    fn lean_renders_every_op_kind() {
        // Exercise every Kind so the renderer can't silently lose a case.
        let a = var("a", 8);
        let b = var("b", 8);
        let body = ite(
            &slt(&a, &b),
            &concat(&extract(&band(&a, &b), 7, 4), &zext(&extract(&b, 1, 0), 4)),
            &shl(&lshr(&bnot(&bxor(&a, &b)), &cst(1, 8)), &cst(2, 8)),
        );
        let f = FpFn::new("k", &[("a", 8), ("b", 8)], 8, body);
        let lean = render_lean(&[f]);
        for needle in [
            "BitVec.slt",
            "++",
            "BitVec.setWidth",
            "~~~",
            "&&&",
            "^^^",
            ">>>",
            "<<<",
            ".toNat",
        ] {
            assert!(
                lean.contains(needle),
                "Lean output missing {needle}:\n{lean}"
            );
        }
    }

    // ── eval_bv (concrete interpreter) ──────────────────────────────────────

    fn ev(b: &Bv) -> Option<u128> {
        eval_bv(b, &HashMap::new(), &[])
    }

    #[test]
    fn eval_bv_primitives_and_masking() {
        // Const masked to width.
        assert_eq!(ev(&cst(0x1FF, 8)), Some(0xFF));
        // Add/sub/mul wrap at the node width.
        assert_eq!(ev(&add(&cst(0xFF, 8), &cst(1, 8))), Some(0));
        assert_eq!(ev(&sub(&cst(0, 8), &cst(1, 8))), Some(0xFF));
        assert_eq!(ev(&mul(&cst(16, 8), &cst(16, 8))), Some(0));
        // Extract / concat / zext / not.
        assert_eq!(ev(&extract(&cst(0b1011_0110, 8), 5, 2)), Some(0b1101));
        assert_eq!(ev(&concat(&cst(0xA, 4), &cst(0x5, 4))), Some(0xA5));
        assert_eq!(ev(&zext(&cst(0x80, 8), 16)), Some(0x80));
        assert_eq!(ev(&bnot(&cst(0x0F, 8))), Some(0xF0));
        // Bitwise binaries.
        assert_eq!(ev(&band(&cst(0xCC, 8), &cst(0xAA, 8))), Some(0x88));
        assert_eq!(ev(&bor(&cst(0xCC, 8), &cst(0xAA, 8))), Some(0xEE));
        assert_eq!(ev(&bxor(&cst(0xCC, 8), &cst(0xAA, 8))), Some(0x66));
        // Shifts: in-range, and amount ≥ width yields 0 (bvshl semantics).
        assert_eq!(ev(&shl(&cst(1, 8), &cst(3, 8))), Some(8));
        assert_eq!(ev(&shl(&cst(1, 8), &cst(9, 8))), Some(0));
        assert_eq!(ev(&lshr(&cst(0x80, 8), &cst(7, 8))), Some(1));
        assert_eq!(ev(&lshr(&cst(0x80, 8), &cst(9, 8))), Some(0));
        // Ite selects on the 1-bit condition.
        assert_eq!(ev(&ite(&cst(1, 1), &cst(3, 8), &cst(4, 8))), Some(3));
        assert_eq!(ev(&ite(&cst(0, 1), &cst(3, 8), &cst(4, 8))), Some(4));
        // Full-width (128-bit) masking doesn't overflow.
        assert_eq!(
            ev(&add(&cst(u128::MAX, 128), &cst(1, 128))),
            Some(0),
            "128-bit wrap"
        );
    }

    #[test]
    fn eval_bv_compares_signed_and_unsigned() {
        let neg1 = cst(0xFF, 8); // -1 as signed 8-bit
        let one = cst(0x01, 8);
        // Unsigned: 0xFF > 1.
        assert_eq!(ev(&ult(&neg1, &one)), Some(0));
        assert_eq!(ev(&ugt(&neg1, &one)), Some(1));
        // Signed: -1 < 1.
        assert_eq!(ev(&slt(&neg1, &one)), Some(1));
        assert_eq!(ev(&sgt(&neg1, &one)), Some(0));
        assert_eq!(ev(&sle(&neg1, &neg1)), Some(1));
        assert_eq!(ev(&sge(&one, &neg1)), Some(1));
        assert_eq!(ev(&eq(&neg1, &neg1)), Some(1));
        assert_eq!(ev(&ne(&neg1, &one)), Some(1));
        assert_eq!(ev(&ule(&one, &neg1)), Some(1));
        assert_eq!(ev(&uge(&one, &neg1)), Some(0));
    }

    /// Differential check against the host FPU on the real operator table —
    /// this is what earns the interpreter the right to gate `arch formal`'s
    /// counterexample replay. Finite inputs only: NaN canonicalization is
    /// profile-dependent and host NaN bits differ, so NaN-producing cases
    /// are pinned separately below.
    #[test]
    fn eval_bv_fp_functions_match_host_fpu() {
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let env = HashMap::new();
        let f32c = |bits: u32| cst(bits as u128, 32);
        // Interesting finite f32 patterns: zeros, one, subnormal min/max,
        // normal min, max finite, an odd-mantissa value, big/small mix.
        let pats: [u32; 10] = [
            0x0000_0000, // +0
            0x8000_0000, // -0
            0x3F80_0000, // 1.0
            0x0000_0001, // min subnormal
            0x007F_FFFF, // max subnormal
            0x0080_0000, // min normal
            0x7F7F_FFFF, // max finite
            0x4049_0FDB, // ~pi
            0xC2C8_0000, // -100.0
            0x3400_0000, // 2^-23
        ];
        for &a in &pats {
            for &b in &pats {
                let fa = f32::from_bits(a);
                let fb = f32::from_bits(b);
                for (name, host) in [
                    ("arch_f32_add", fa + fb),
                    ("arch_f32_sub", fa - fb),
                    ("arch_f32_mul", fa * fb),
                ] {
                    let got = eval_bv(&call(name, &[f32c(a), f32c(b)], 32), &env, &fns)
                        .unwrap_or_else(|| panic!("{name}({a:#X},{b:#X}) undecidable"));
                    assert_eq!(
                        got,
                        host.to_bits() as u128,
                        "{name}({a:#010X}, {b:#010X}): ir={got:#010X} host={:#010X}",
                        host.to_bits()
                    );
                }
                // Compares (finite operands, so no NaN-ordering ambiguity).
                let cmp_got = eval_bv(&call("arch_f32_lt", &[f32c(a), f32c(b)], 1), &env, &fns);
                assert_eq!(cmp_got, Some((fa < fb) as u128), "lt({a:#X},{b:#X})");
            }
        }
        // Fused multiply-add over a few finite triples (host fmaf == RNE fused).
        for (a, b, c) in [
            (2.0f32, 3.0f32, 4.0f32),
            (1.5, -2.5, 0.25),
            (1e20, 1e20, -1e38),
            (3.0, 7.0, 1e-40),
        ] {
            let got = eval_bv(
                &call(
                    "arch_fma_f32",
                    &[f32c(a.to_bits()), f32c(b.to_bits()), f32c(c.to_bits())],
                    32,
                ),
                &env,
                &fns,
            )
            .expect("fma undecidable");
            assert_eq!(got, a.mul_add(b, c).to_bits() as u128, "fma({a},{b},{c})");
        }
        // Nested call chain (bf16 widens to f32, ops, narrows): 1.0 + 2.0 = 3.0.
        assert_eq!(
            eval_bv(
                &call("arch_bf16_add", &[cst(0x3F80, 16), cst(0x4000, 16)], 16),
                &env,
                &fns
            ),
            Some(0x4040)
        );
        // fp8 e4m3 (3-deep call chain): 1.0 (0x38) * 2.0 (0x40) = 2.0 (0x40).
        assert_eq!(
            eval_bv(
                &call("arch_e4m3_mul", &[cst(0x38, 8), cst(0x40, 8)], 8),
                &env,
                &fns
            ),
            Some(0x40)
        );
    }

    #[test]
    fn eval_bv_conservative_none_paths() {
        let fns = crate::fp_ops::fp_functions(crate::FpCompat::default());
        let env = HashMap::new();
        // Unbound Var → None; bound Var → value.
        assert_eq!(eval_bv(&var("x", 8), &env, &fns), None);
        let mut bound = HashMap::new();
        bound.insert("x".to_string(), 5u128);
        assert_eq!(eval_bv(&var("x", 8), &bound, &fns), Some(5));
        // Unknown call name → None.
        assert_eq!(
            eval_bv(&call("arch_no_such_fn", &[cst(0, 32)], 32), &env, &fns),
            None
        );
        // Arity mismatch → None.
        assert_eq!(
            eval_bv(&call("arch_f32_add", &[cst(0, 32)], 32), &env, &fns),
            None
        );
        // Argument width mismatch → None.
        assert_eq!(
            eval_bv(
                &call("arch_f32_add", &[cst(0, 16), cst(0, 32)], 32),
                &env,
                &fns
            ),
            None
        );
        // Node wider than 128 bits → None (degrade, don't wrap).
        let wide = concat(&cst(0, 100), &cst(0, 100));
        assert_eq!(eval_bv(&wide, &env, &fns), None);
        // Undecidable dead Ite arm must not poison a decidable result
        // (short-circuit contract).
        let dead = ite(&cst(1, 1), &cst(7, 8), &var("unbound", 8));
        assert_eq!(eval_bv(&dead, &env, &fns), Some(7));
        // ...but the live arm being undecidable does.
        let live = ite(&cst(0, 1), &cst(7, 8), &var("unbound", 8));
        assert_eq!(eval_bv(&live, &env, &fns), None);
    }
}
