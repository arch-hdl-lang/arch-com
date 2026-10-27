//! Lean translation-validation certificates for ARCH → SystemVerilog codegen.
//!
//! For every `module` in the certified subset this backend emits a Lean file
//! containing two concrete terms:
//!
//! * `core` — the module as an ARCH-Core program (`ArchCodegenProof/Core.lean`),
//!   built from the checked AST, and
//! * `sv` — the module as an SV-subset program (`ArchCodegenProof/Sv.lean`),
//!   built by reading the *emitted SV text* back with [`sv_reader`],
//!
//! and closes `check core sv = true` by kernel evaluation. The Lean theorem
//! `check_sound` turns that into trace equivalence of the two programs for all
//! inputs and cycles. The codegen itself is not trusted; the trusted base is
//! the two Lean semantics, the AST→Core translation below, `sv_reader`, and
//! the Lean kernel. See `doc/plan_verified_codegen.md`.
//!
//! The covered subset is deliberately small (see [`translate_module`]); a
//! module outside it is reported as skipped with the first reason found.

mod sv_reader;

use crate::ast::{
    BinOp as AstBinOp, ClockEdge, Expr, ExprKind, Item, LitKind, ModuleBodyItem, ModuleDecl,
    ParamKind, RegReset, ResetKind, ResetLevel, Stmt, TypeExpr, UnaryOp,
};
use std::collections::HashMap;
use std::fmt::Write as _;

// ── data model mirrored from the Lean side ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Port {
    sig: u32,
    dir: Dir,
    width: u32,
}

#[derive(Debug, Clone)]
enum CoreExpr {
    Sig(u32),
    Lit(u32, u64),
    Bool(bool),
    Add(Box<CoreExpr>, Box<CoreExpr>),
    Sub(Box<CoreExpr>, Box<CoreExpr>),
    Mul(Box<CoreExpr>, Box<CoreExpr>),
    BinW(BinOp, Box<CoreExpr>, Box<CoreExpr>),
    Bnot(Box<CoreExpr>),
    Cmp(CmpOp, Box<CoreExpr>, Box<CoreExpr>),
    Lnot(Box<CoreExpr>),
    Land(Box<CoreExpr>, Box<CoreExpr>),
    Lor(Box<CoreExpr>, Box<CoreExpr>),
    Mux(Box<CoreExpr>, Box<CoreExpr>, Box<CoreExpr>),
    Resize(u32, Box<CoreExpr>),
    Slice(u64, u64, Box<CoreExpr>),
}

#[derive(Debug, Clone)]
enum CoreStmt {
    Assign(u32, CoreExpr),
    Ite(CoreExpr, Vec<CoreStmt>, Vec<CoreStmt>),
}

#[derive(Debug, Clone)]
struct CoreModule {
    ports: Vec<Port>,
    locals: Vec<(u32, u32)>,
    clk: Option<u32>,
    rst: Option<(u32, bool)>,
    rst_async: bool,
    regs: Vec<(u32, CoreExpr)>,
    combs: Vec<Vec<CoreStmt>>,
    seq: Vec<CoreStmt>,
}

#[derive(Debug, Clone)]
enum SvCw {
    Num(u64),
    Bits(u32),
    Sel(CmpOp, Box<SvCw>, Box<SvCw>, Box<SvCw>, Box<SvCw>),
}

#[derive(Debug, Clone)]
enum SvExpr {
    Ident(u32),
    Lit(Option<u32>, bool, u64),
    Bin(BinOp, Box<SvExpr>, Box<SvExpr>),
    Bnot(Box<SvExpr>),
    Cmp(CmpOp, Box<SvExpr>, Box<SvExpr>),
    Lnot(Box<SvExpr>),
    Land(Box<SvExpr>, Box<SvExpr>),
    Lor(Box<SvExpr>, Box<SvExpr>),
    Cond(Box<SvExpr>, Box<SvExpr>, Box<SvExpr>),
    Cast(SvCw, Box<SvExpr>),
    Unsigned(Box<SvExpr>),
    Slice(u32, u64, u64),
}

#[derive(Debug, Clone)]
enum SvStmt {
    Assign(u32, SvExpr),
    Ite(SvExpr, Vec<SvStmt>, Vec<SvStmt>),
}

#[derive(Debug, Clone)]
enum SvProc {
    Assign(u32, SvExpr),
    Comb(Vec<SvStmt>),
}

#[derive(Debug, Clone)]
struct SvModule {
    ports: Vec<Port>,
    locals: Vec<(u32, u32)>,
    procs: Vec<SvProc>,
    ff_clk: Option<u32>,
    /// `or posedge s` → `(s, true)`, `or negedge s` → `(s, false)`.
    ff_async: Option<(u32, bool)>,
    ff_body: Vec<SvStmt>,
}

/// Signal names ↔ the natural-number ids used on the Lean side. Both the
/// AST translation and the SV reader resolve names through the same table.
#[derive(Debug, Default)]
struct SigTable {
    names: Vec<String>,
    ids: HashMap<String, u32>,
    /// Default values of the module's integer parameters, as the ARCH side
    /// sees them. The SV reader deliberately does not use these: it
    /// evaluates the `parameter` defaults it reads from the emitted text.
    params: HashMap<String, u64>,
}

impl SigTable {
    fn add(&mut self, name: &str) -> Result<u32, String> {
        if self.ids.contains_key(name) {
            return Err(format!("signal `{name}` is declared twice"));
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        self.ids.insert(name.to_string(), id);
        Ok(id)
    }

    fn get(&self, name: &str) -> Option<u32> {
        self.ids.get(name).copied()
    }
}

// ── public API ─────────────────────────────────────────────────────────────

/// Result of certificate generation over a set of items.
#[derive(Debug)]
pub struct CodegenCert {
    /// The Lean replay file.
    pub lean: String,
    /// Modules covered by a certificate in `lean`.
    pub certified: Vec<String>,
    /// Modules outside the certified subset, with the first reason found.
    pub skipped: Vec<(String, String)>,
    /// Number of non-`module` constructs (fifo, fsm, …) that are not covered.
    pub uncovered_constructs: usize,
}

/// Build codegen certificates for every `module` in `items`, reading each
/// module's emitted SV out of `sv_text` (the complete `arch build` output for
/// those items). When no module could be certified, `certified` is empty and
/// `lean` is empty; the caller reports `skipped` and decides what to do.
pub fn render_lean_items<'a, I>(items: I, sv_text: &str) -> Result<CodegenCert, String>
where
    I: IntoIterator<Item = &'a Item>,
{
    let mut certified = Vec::new();
    let mut skipped = Vec::new();
    let mut uncovered_constructs = 0;
    let mut bodies = String::new();
    for item in items {
        let Item::Module(m) = item else {
            // Hardware constructs that emit their own SV module.
            if matches!(
                item,
                Item::Fsm(_)
                    | Item::Fifo(_)
                    | Item::Ram(_)
                    | Item::Cam(_)
                    | Item::Counter(_)
                    | Item::Arbiter(_)
                    | Item::Regfile(_)
                    | Item::Pipeline(_)
                    | Item::Linklist(_)
                    | Item::Synchronizer(_)
                    | Item::Clkgate(_)
            ) {
                uncovered_constructs += 1;
            }
            continue;
        };
        match certify_module(m, sv_text) {
            Ok(body) => {
                bodies.push_str(&body);
                certified.push(m.name.name.clone());
            }
            Err(reason) => skipped.push((m.name.name.clone(), reason)),
        }
    }
    if certified.is_empty() {
        return Ok(CodegenCert {
            lean: String::new(),
            certified,
            skipped,
            uncovered_constructs,
        });
    }
    let mut lean = String::new();
    lean.push_str("import ArchCodegenProof\n\nopen Arch.CodegenProof\n\n");
    lean.push_str("/-!\nGenerated by `arch build --emit-codegen-proof-lean`.\n\n");
    lean.push_str(
        "Each namespace below replays the codegen validator on one module: `core` is the\n\
         module as ARCH-Core (from the checked AST), `sv` is the emitted SystemVerilog read\n\
         back from the `.sv` text, and `codegen_correct` states that the two have the same\n\
         ports, the same flop clock, and the same traces for all inputs and cycles.\n",
    );
    if !skipped.is_empty() {
        lean.push_str("\nModules outside the certified subset (NOT covered by this file):\n");
        for (name, reason) in &skipped {
            let _ = writeln!(lean, "* `{name}`: {reason}");
        }
    }
    if uncovered_constructs > 0 {
        let _ = writeln!(
            lean,
            "\n{uncovered_constructs} non-`module` construct(s) are NOT covered by this file."
        );
    }
    lean.push_str("-/\n");
    lean.push_str(&bodies);
    Ok(CodegenCert {
        lean,
        certified,
        skipped,
        uncovered_constructs,
    })
}

fn certify_module(m: &ModuleDecl, sv_text: &str) -> Result<String, String> {
    let (core, sigs) = translate_module(m)?;
    let sv = sv_reader::read_module(sv_text, &m.name.name, &sigs)
        .map_err(|e| format!("emitted SV is outside the reader subset: {e}"))?;
    let params: Vec<(String, u64)> = m
        .params
        .iter()
        .map(|p| (p.name.name.clone(), sigs.params[&p.name.name]))
        .collect();
    Ok(render_module(&m.name.name, &params, &core, &sv, &sigs))
}

// ── ARCH AST → ARCH-Core ───────────────────────────────────────────────────

type R<T> = Result<T, String>;

/// Evaluate a compile-time integer expression over the module's parameter
/// defaults. Intermediate results must stay non-negative and below 2^31 —
/// the range of the SV `parameter int` the codegen emits.
fn const_u64(e: &Expr, params: &HashMap<String, u64>) -> R<u64> {
    const LIMIT: u64 = 1 << 31;
    let v = match &e.kind {
        ExprKind::Literal(LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v)) => *v,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
        ExprKind::Ident(name) => *params
            .get(name)
            .ok_or_else(|| format!("`{name}` is not a compile-time integer"))?,
        ExprKind::Binary(op, l, r) => {
            let (l, r) = (const_u64(l, params)?, const_u64(r, params)?);
            match op {
                AstBinOp::Add => l + r,
                AstBinOp::Sub => l
                    .checked_sub(r)
                    .ok_or_else(|| "negative constant expression".to_string())?,
                AstBinOp::Mul => l
                    .checked_mul(r)
                    .ok_or_else(|| "constant expression overflows".to_string())?,
                AstBinOp::Div | AstBinOp::Mod if r == 0 => {
                    return Err("division by zero in constant expression".to_string())
                }
                AstBinOp::Div => l / r,
                AstBinOp::Mod => l % r,
                other => {
                    return Err(format!(
                        "operator `{other}` in a constant expression is not yet covered"
                    ))
                }
            }
        }
        ExprKind::Clog2(a) => {
            let a = const_u64(a, params)?;
            if a <= 1 {
                0
            } else {
                u64::from(64 - (a - 1).leading_zeros())
            }
        }
        _ => return Err("constant expression form is not yet covered".to_string()),
    };
    if v >= LIMIT {
        return Err("constant expression exceeds the 32-bit `parameter int` range".to_string());
    }
    Ok(v)
}

fn const_u32(e: &Expr, params: &HashMap<String, u64>) -> R<u32> {
    u32::try_from(const_u64(e, params)?).map_err(|_| "width too large".to_string())
}

fn data_width(ty: &TypeExpr, params: &HashMap<String, u64>) -> R<u32> {
    match ty {
        TypeExpr::Bool | TypeExpr::Bit => Ok(1),
        TypeExpr::UInt(w) => {
            let w = const_u32(w, params)?;
            if w == 0 {
                return Err("zero-width UInt".to_string());
            }
            Ok(w)
        }
        TypeExpr::SInt(_) => Err("signed types are not yet covered".to_string()),
        _ => Err("only `UInt<N>`/`Bool` data types are covered".to_string()),
    }
}

fn bitlen(v: u64) -> u32 {
    (64 - v.leading_zeros()).max(1)
}

/// Translate a module in the certified subset:
///
/// * ports: `Clock`, synchronous `Reset`, `UInt<N>`/`Bool` data (no bus,
///   `port reg`, `shared`, `unpacked`, or defaults);
/// * body: `reg` (with a synchronous reset on a reset port, no `init`,
///   `guard` or `multicycle`), `wire`, typed `let`, `comb` blocks and at most
///   one `seq on clk rising` block;
/// * statements: assignment to a plain signal and `if`/`elsif`/`else`;
/// * expressions: literals, signals, `+ - * +% -% *% & | ^ ~`, comparisons,
///   `and or not`, `?:`, `.trunc/.zext/.resize<N>()` and constant `[hi:lo]`.
fn translate_module(m: &ModuleDecl) -> R<(CoreModule, SigTable)> {
    if m.implements.is_some() || !m.hooks.is_empty() {
        return Err("`implements`/hooks are not yet covered".to_string());
    }
    let mut sigs = SigTable::default();
    // Parameters are certified at their default values — the module exactly
    // as `arch build` emits it. Instances that override them are separate
    // elaborations and are not covered by this certificate.
    for p in &m.params {
        let name = &p.name.name;
        if !matches!(p.kind, ParamKind::Const) || p.unpacked_size.is_some() {
            return Err(format!(
                "param `{name}`: only integer `const` parameters are covered"
            ));
        }
        let default = p
            .default
            .as_ref()
            .ok_or_else(|| format!("param `{name}` has no default value"))?;
        let v = const_u64(default, &sigs.params).map_err(|e| format!("param `{name}`: {e}"))?;
        sigs.params.insert(name.clone(), v);
    }
    let mut ports = Vec::new();
    let mut clocks = Vec::new();
    // reset port name → active-high?
    // reset port name → (active-high?, asynchronous?)
    let mut resets: HashMap<String, (bool, bool)> = HashMap::new();
    let mut outputs = Vec::new();
    for p in &m.ports {
        let name = &p.name.name;
        let unsupported_port = if p.bus_info.is_some() {
            Some("bus ports")
        } else if p.reg_info.is_some() {
            Some("`port reg` / `pipe_reg` ports")
        } else if p.shared.is_some() {
            Some("`shared` ports")
        } else if p.unpacked {
            Some("`unpacked` ports")
        } else {
            None
        };
        if let Some(kind) = unsupported_port {
            return Err(format!("port `{name}`: {kind} are not yet covered"));
        }
        if p.default.is_some() {
            return Err(format!("port `{name}`: port defaults are not yet covered"));
        }
        let width = match &p.ty {
            TypeExpr::Clock(_) => {
                clocks.push(name.clone());
                1
            }
            TypeExpr::Reset(kind, level) => {
                resets.insert(
                    name.clone(),
                    (*level == ResetLevel::High, *kind == ResetKind::Async),
                );
                1
            }
            ty => data_width(ty, &sigs.params).map_err(|e| format!("port `{name}`: {e}"))?,
        };
        let sig = sigs.add(name)?;
        let dir = match p.direction {
            crate::ast::Direction::In => Dir::Input,
            crate::ast::Direction::Out => {
                outputs.push(sig);
                Dir::Output
            }
        };
        ports.push(Port { sig, dir, width });
    }

    // Declarations first, so processes may reference signals declared later.
    let mut locals = Vec::new();
    let mut reg_sigs = Vec::new();
    let mut wire_sigs = Vec::new();
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                let w = data_width(&r.ty, &sigs.params)
                    .map_err(|e| format!("reg `{}`: {e}", r.name.name))?;
                let sig = sigs.add(&r.name.name)?;
                locals.push((sig, w));
                reg_sigs.push(sig);
            }
            ModuleBodyItem::WireDecl(wd) => {
                if wd.unpacked || !wd.bus_params.is_empty() {
                    return Err(format!(
                        "wire `{}`: unpacked/bus wires are not yet covered",
                        wd.name.name
                    ));
                }
                let w = data_width(&wd.ty, &sigs.params)
                    .map_err(|e| format!("wire `{}`: {e}", wd.name.name))?;
                let sig = sigs.add(&wd.name.name)?;
                locals.push((sig, w));
                wire_sigs.push(sig);
            }
            ModuleBodyItem::LetBinding(l) => {
                if !l.destructure_fields.is_empty() {
                    return Err("destructuring `let` is not yet covered".to_string());
                }
                let ty = l.ty.as_ref().ok_or_else(|| {
                    format!("let `{}`: untyped `let` is not yet covered", l.name.name)
                })?;
                let w = data_width(ty, &sigs.params)
                    .map_err(|e| format!("let `{}`: {e}", l.name.name))?;
                let sig = sigs.add(&l.name.name)?;
                locals.push((sig, w));
            }
            _ => {}
        }
    }

    let comb_targets: Vec<u32> = outputs.iter().chain(&wire_sigs).copied().collect();
    let mut combs = Vec::new();
    let mut seq = None;
    let mut clk = None;
    let mut regs = Vec::new();
    let mut rst: Option<(u32, bool)> = None;
    let mut rst_async = false;
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                let name = &r.name.name;
                for (present, what) in [
                    (r.init.is_some(), "`init`"),
                    (r.guard.is_some(), "`guard`"),
                    (r.multicycle.is_some(), "`multicycle`"),
                ] {
                    if present {
                        return Err(format!("reg `{name}`: {what} is not yet covered"));
                    }
                }
                let (rst_name, high, is_async, value) = match &r.reset {
                    RegReset::None => {
                        return Err(format!(
                            "reg `{name}`: registers without reset are not yet covered"
                        ))
                    }
                    RegReset::Inherit(rs, v) => {
                        let (high, is_async) = *resets.get(&rs.name).ok_or_else(|| {
                            format!("reg `{name}`: reset `{}` is not a reset port", rs.name)
                        })?;
                        (rs.name.clone(), high, is_async, v)
                    }
                    RegReset::Explicit(rs, kind, level, v) => {
                        let Some(&(_, port_async)) = resets.get(&rs.name) else {
                            return Err(format!(
                                "reg `{name}`: reset `{}` is not a reset port",
                                rs.name
                            ));
                        };
                        let is_async = *kind == ResetKind::Async;
                        if is_async != port_async {
                            return Err(format!(
                                "reg `{name}`: reset kind differs from its port's; not covered"
                            ));
                        }
                        (rs.name.clone(), *level == ResetLevel::High, is_async, v)
                    }
                };
                let rs = sigs
                    .get(&rst_name)
                    .expect("reset port is in the signal table");
                if let Some(prev) = rst {
                    if prev != (rs, high) || rst_async != is_async {
                        return Err(
                            "registers with different resets are not yet covered".to_string()
                        );
                    }
                }
                rst = Some((rs, high));
                rst_async = is_async;
                let sig = sigs.get(name).expect("reg is in the signal table");
                regs.push((sig, expr(value, &sigs)?));
            }
            ModuleBodyItem::LetBinding(l) => {
                let sig = sigs.get(&l.name.name).expect("let is in the signal table");
                combs.push(vec![CoreStmt::Assign(sig, expr(&l.value, &sigs)?)]);
            }
            ModuleBodyItem::CombBlock(c) => combs.push(stmts(&c.stmts, &sigs, &comb_targets)?),
            ModuleBodyItem::RegBlock(b) => {
                if seq.is_some() {
                    return Err("more than one `seq` block is not yet covered".to_string());
                }
                if b.clock_edge != ClockEdge::Rising {
                    return Err("falling-edge `seq` blocks are not yet covered".to_string());
                }
                if !clocks.contains(&b.clock.name) {
                    return Err(format!(
                        "`seq` clock `{}` is not a Clock port",
                        b.clock.name
                    ));
                }
                clk = sigs.get(&b.clock.name);
                seq = Some(stmts(&b.stmts, &sigs, &reg_sigs)?);
            }
            ModuleBodyItem::WireDecl(_) => {}
            other => {
                return Err(format!(
                    "{} items are not yet covered",
                    body_item_kind(other)
                ))
            }
        }
    }
    if !regs.is_empty() && seq.is_none() {
        return Err("registers without a `seq` block are not yet covered".to_string());
    }
    let core = CoreModule {
        ports,
        locals,
        clk,
        rst,
        rst_async,
        regs,
        combs,
        seq: seq.unwrap_or_default(),
    };
    Ok((core, sigs))
}

fn body_item_kind(item: &ModuleBodyItem) -> &'static str {
    match item {
        ModuleBodyItem::RegDecl(_) => "reg",
        ModuleBodyItem::RegBlock(_) => "seq",
        ModuleBodyItem::LatchBlock(_) => "latch",
        ModuleBodyItem::CombBlock(_) => "comb",
        ModuleBodyItem::LetBinding(_) => "let",
        ModuleBodyItem::Inst(_) => "inst",
        ModuleBodyItem::Generate(_) => "generate",
        ModuleBodyItem::PipeRegDecl(_) => "pipe_reg",
        ModuleBodyItem::WireDecl(_) => "wire",
        ModuleBodyItem::Thread(_) => "thread",
        ModuleBodyItem::Resource(_) => "resource",
        ModuleBodyItem::Assert(_) => "assert/cover",
        ModuleBodyItem::Function(_) => "function",
        ModuleBodyItem::TlmConnect(_) => "tlm connect",
        ModuleBodyItem::TypeAlias(_) => "type alias",
    }
}

fn stmts(ss: &[Stmt], sigs: &SigTable, targets: &[u32]) -> R<Vec<CoreStmt>> {
    ss.iter().map(|s| stmt(s, sigs, targets)).collect()
}

fn stmt(s: &Stmt, sigs: &SigTable, targets: &[u32]) -> R<CoreStmt> {
    match s {
        Stmt::Assign(a) => {
            let ExprKind::Ident(name) = &a.target.kind else {
                return Err("assignment to a non-signal target is not yet covered".to_string());
            };
            let x = sigs
                .get(name)
                .ok_or_else(|| format!("assignment to unknown signal `{name}`"))?;
            if !targets.contains(&x) {
                return Err(format!(
                    "assignment to `{name}` from this block kind is not covered"
                ));
            }
            Ok(CoreStmt::Assign(x, expr(&a.value, sigs)?))
        }
        Stmt::IfElse(ie) => {
            if ie.unique {
                return Err("`unique if` is not yet covered".to_string());
            }
            Ok(CoreStmt::Ite(
                expr(&ie.cond, sigs)?,
                stmts(&ie.then_stmts, sigs, targets)?,
                stmts(&ie.else_stmts, sigs, targets)?,
            ))
        }
        _ => Err("only assignments and if/else statements are covered".to_string()),
    }
}

fn expr(e: &Expr, sigs: &SigTable) -> R<CoreExpr> {
    let b = |x: &Expr| expr(x, sigs).map(Box::new);
    Ok(match &e.kind {
        ExprKind::Literal(LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v)) => {
            CoreExpr::Lit(bitlen(*v), *v)
        }
        ExprKind::Literal(LitKind::Sized(w, v)) => CoreExpr::Lit(*w, *v),
        ExprKind::Bool(v) => CoreExpr::Bool(*v),
        ExprKind::Ident(name) if sigs.params.contains_key(name) => {
            let v = sigs.params[name];
            CoreExpr::Lit(bitlen(v), v)
        }
        ExprKind::Ident(name) => CoreExpr::Sig(
            sigs.get(name)
                .ok_or_else(|| format!("reference to unknown signal `{name}`"))?,
        ),
        ExprKind::Binary(op, l, r) => {
            let (l, r) = (b(l)?, b(r)?);
            match op {
                AstBinOp::Add => CoreExpr::Add(l, r),
                AstBinOp::Sub => CoreExpr::Sub(l, r),
                AstBinOp::Mul => CoreExpr::Mul(l, r),
                AstBinOp::AddWrap => CoreExpr::BinW(BinOp::Add, l, r),
                AstBinOp::SubWrap => CoreExpr::BinW(BinOp::Sub, l, r),
                AstBinOp::MulWrap => CoreExpr::BinW(BinOp::Mul, l, r),
                AstBinOp::BitAnd => CoreExpr::BinW(BinOp::And, l, r),
                AstBinOp::BitOr => CoreExpr::BinW(BinOp::Or, l, r),
                AstBinOp::BitXor => CoreExpr::BinW(BinOp::Xor, l, r),
                AstBinOp::Eq => CoreExpr::Cmp(CmpOp::Eq, l, r),
                AstBinOp::Neq => CoreExpr::Cmp(CmpOp::Ne, l, r),
                AstBinOp::Lt => CoreExpr::Cmp(CmpOp::Lt, l, r),
                AstBinOp::Lte => CoreExpr::Cmp(CmpOp::Le, l, r),
                AstBinOp::Gt => CoreExpr::Cmp(CmpOp::Gt, l, r),
                AstBinOp::Gte => CoreExpr::Cmp(CmpOp::Ge, l, r),
                AstBinOp::And => CoreExpr::Land(l, r),
                AstBinOp::Or => CoreExpr::Lor(l, r),
                other => return Err(format!("operator `{other}` is not yet covered")),
            }
        }
        ExprKind::Unary(UnaryOp::Not, a) => CoreExpr::Lnot(b(a)?),
        ExprKind::Unary(UnaryOp::BitNot, a) => CoreExpr::Bnot(b(a)?),
        ExprKind::Ternary(c, t, f) => CoreExpr::Mux(b(c)?, b(t)?, b(f)?),
        ExprKind::MethodCall(recv, method, args)
            if matches!(method.name.as_str(), "trunc" | "zext" | "resize") && args.len() == 1 =>
        {
            CoreExpr::Resize(const_u32(&args[0], &sigs.params)?, b(recv)?)
        }
        ExprKind::BitSlice(base, hi, lo) => {
            let (hi, lo) = (const_u64(hi, &sigs.params)?, const_u64(lo, &sigs.params)?);
            if lo > hi {
                return Err("descending bit slice".to_string());
            }
            CoreExpr::Slice(hi, lo, b(base)?)
        }
        _ => return Err("expression form is not yet covered".to_string()),
    })
}

// ── Lean rendering ─────────────────────────────────────────────────────────

fn lean_bin(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => ".add",
        BinOp::Sub => ".sub",
        BinOp::Mul => ".mul",
        BinOp::And => ".and",
        BinOp::Or => ".or",
        BinOp::Xor => ".xor",
    }
}

fn lean_cmp(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => ".eq",
        CmpOp::Ne => ".ne",
        CmpOp::Lt => ".lt",
        CmpOp::Le => ".le",
        CmpOp::Gt => ".gt",
        CmpOp::Ge => ".ge",
    }
}

fn lean_ports(ports: &[Port]) -> String {
    let items: Vec<String> = ports
        .iter()
        .map(|p| {
            let dir = match p.dir {
                Dir::Input => ".input",
                Dir::Output => ".output",
            };
            format!("⟨{}, {dir}, {}⟩", p.sig, p.width)
        })
        .collect();
    format!("[{}]", items.join(", "))
}

fn lean_locals(locals: &[(u32, u32)]) -> String {
    let items: Vec<String> = locals.iter().map(|(s, w)| format!("({s}, {w})")).collect();
    format!("[{}]", items.join(", "))
}

fn lean_core_expr(e: &CoreExpr) -> String {
    let p = |x: &CoreExpr| format!("({})", lean_core_expr(x));
    match e {
        CoreExpr::Sig(x) => format!(".sig {x}"),
        CoreExpr::Lit(w, n) => format!(".lit {w} {n}"),
        CoreExpr::Bool(v) => format!(".bool {v}"),
        CoreExpr::Add(a, b) => format!(".add {} {}", p(a), p(b)),
        CoreExpr::Sub(a, b) => format!(".sub {} {}", p(a), p(b)),
        CoreExpr::Mul(a, b) => format!(".mul {} {}", p(a), p(b)),
        CoreExpr::BinW(op, a, b) => format!(".binW {} {} {}", lean_bin(*op), p(a), p(b)),
        CoreExpr::Bnot(a) => format!(".bnot {}", p(a)),
        CoreExpr::Cmp(op, a, b) => format!(".cmp {} {} {}", lean_cmp(*op), p(a), p(b)),
        CoreExpr::Lnot(a) => format!(".lnot {}", p(a)),
        CoreExpr::Land(a, b) => format!(".land {} {}", p(a), p(b)),
        CoreExpr::Lor(a, b) => format!(".lor {} {}", p(a), p(b)),
        CoreExpr::Mux(c, a, b) => format!(".mux {} {} {}", p(c), p(a), p(b)),
        CoreExpr::Resize(n, a) => format!(".resize {n} {}", p(a)),
        CoreExpr::Slice(hi, lo, a) => format!(".slice {hi} {lo} {}", p(a)),
    }
}

fn lean_core_stmts(ss: &[CoreStmt]) -> String {
    match ss.split_first() {
        None => ".nil".to_string(),
        Some((CoreStmt::Assign(x, e), rest)) => format!(
            ".assign {x} ({}) ({})",
            lean_core_expr(e),
            lean_core_stmts(rest)
        ),
        Some((CoreStmt::Ite(c, t, f), rest)) => format!(
            ".ite ({}) ({}) ({}) ({})",
            lean_core_expr(c),
            lean_core_stmts(t),
            lean_core_stmts(f),
            lean_core_stmts(rest)
        ),
    }
}

fn lean_cw(cw: &SvCw) -> String {
    match cw {
        SvCw::Num(n) => format!(".num {n}"),
        SvCw::Bits(x) => format!(".bits {x}"),
        SvCw::Sel(op, a, b, t, f) => format!(
            ".sel {} ({}) ({}) ({}) ({})",
            lean_cmp(*op),
            lean_cw(a),
            lean_cw(b),
            lean_cw(t),
            lean_cw(f)
        ),
    }
}

fn lean_sv_expr(e: &SvExpr) -> String {
    let p = |x: &SvExpr| format!("({})", lean_sv_expr(x));
    match e {
        SvExpr::Ident(x) => format!(".ident {x}"),
        SvExpr::Lit(size, signed, n) => {
            let size = match size {
                None => "none".to_string(),
                Some(w) => format!("(some {w})"),
            };
            format!(".lit {size} {signed} {n}")
        }
        SvExpr::Bin(op, a, b) => format!(".bin {} {} {}", lean_bin(*op), p(a), p(b)),
        SvExpr::Bnot(a) => format!(".bnot {}", p(a)),
        SvExpr::Cmp(op, a, b) => format!(".cmp {} {} {}", lean_cmp(*op), p(a), p(b)),
        SvExpr::Lnot(a) => format!(".lnot {}", p(a)),
        SvExpr::Land(a, b) => format!(".land {} {}", p(a), p(b)),
        SvExpr::Lor(a, b) => format!(".lor {} {}", p(a), p(b)),
        SvExpr::Cond(c, a, b) => format!(".cond {} {} {}", p(c), p(a), p(b)),
        SvExpr::Cast(cw, a) => format!(".cast ({}) {}", lean_cw(cw), p(a)),
        SvExpr::Unsigned(a) => format!(".unsigned {}", p(a)),
        SvExpr::Slice(x, hi, lo) => format!(".slice {x} {hi} {lo}"),
    }
}

fn lean_sv_stmts(ss: &[SvStmt]) -> String {
    match ss.split_first() {
        None => ".nil".to_string(),
        Some((SvStmt::Assign(x, e), rest)) => {
            format!(
                ".assign {x} ({}) ({})",
                lean_sv_expr(e),
                lean_sv_stmts(rest)
            )
        }
        Some((SvStmt::Ite(c, t, f), rest)) => format!(
            ".ite ({}) ({}) ({}) ({})",
            lean_sv_expr(c),
            lean_sv_stmts(t),
            lean_sv_stmts(f),
            lean_sv_stmts(rest)
        ),
    }
}

fn lean_opt(x: Option<u32>) -> String {
    x.map_or("none".to_string(), |v| format!("some {v}"))
}

fn render_module(
    name: &str,
    params: &[(String, u64)],
    core: &CoreModule,
    sv: &SvModule,
    sigs: &SigTable,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "\n/-! ## module `{name}`\n");
    if !params.is_empty() {
        let list: Vec<String> = params.iter().map(|(n, v)| format!("`{n}` = {v}")).collect();
        let _ = writeln!(
            out,
            "Certified at the default parameter values {}. Instances that override \
             them are NOT covered by this certificate.\n",
            list.join(", ")
        );
    }
    out.push_str("Signal ids:\n");
    for (i, n) in sigs.names.iter().enumerate() {
        let _ = writeln!(out, "* {i} = `{n}`");
    }
    out.push_str("-/\n");
    let _ = writeln!(out, "namespace ArchCodegenCert.«{name}»\n");

    out.push_str("def core : Core.Module where\n");
    let _ = writeln!(out, "  ports := {}", lean_ports(&core.ports));
    let _ = writeln!(out, "  locals := {}", lean_locals(&core.locals));
    let _ = writeln!(out, "  clk := {}", lean_opt(core.clk));
    let rst = core
        .rst
        .map_or("none".to_string(), |(s, h)| format!("some ({s}, {h})"));
    let _ = writeln!(out, "  rst := {rst}");
    let _ = writeln!(out, "  rstAsync := {}", core.rst_async);
    let regs: Vec<String> = core
        .regs
        .iter()
        .map(|(s, v)| format!("({s}, {})", lean_core_expr(v)))
        .collect();
    let _ = writeln!(out, "  regs := [{}]", regs.join(", "));
    let combs: Vec<String> = core.combs.iter().map(|c| lean_core_stmts(c)).collect();
    let _ = writeln!(out, "  combs := [{}]", combs.join(",\n    "));
    let _ = writeln!(out, "  seq := {}\n", lean_core_stmts(&core.seq));

    out.push_str("def sv : Sv.Module where\n");
    let _ = writeln!(out, "  ports := {}", lean_ports(&sv.ports));
    let _ = writeln!(out, "  locals := {}", lean_locals(&sv.locals));
    let procs: Vec<String> = sv
        .procs
        .iter()
        .map(|p| match p {
            SvProc::Assign(x, e) => format!(".assign {x} ({})", lean_sv_expr(e)),
            SvProc::Comb(b) => format!(".comb ({})", lean_sv_stmts(b)),
        })
        .collect();
    let _ = writeln!(out, "  procs := [{}]", procs.join(",\n    "));
    let _ = writeln!(out, "  ffClk := {}", lean_opt(sv.ff_clk));
    let ff_async = sv
        .ff_async
        .map_or("none".to_string(), |(s, pos)| format!("some ({s}, {pos})"));
    let _ = writeln!(out, "  ffAsync := {ff_async}");
    let _ = writeln!(out, "  ffBody := {}\n", lean_sv_stmts(&sv.ff_body));

    // `+kernel` evaluates the checker in the kernel directly; the elaborator's
    // own evaluator hits `maxRecDepth` on large modules (e.g. e203_core).
    out.push_str("theorem check_ok : check core sv = true := by decide +kernel\n\n");
    out.push_str(
        "theorem codegen_correct :\n    core.ports = sv.ports ∧ core.clk = sv.ffClk ∧ \
         ∀ tr, core.Run tr ↔ sv.Run tr :=\n  check_sound check_ok\n\n",
    );
    let _ = writeln!(out, "end ArchCodegenCert.«{name}»");
    out
}
