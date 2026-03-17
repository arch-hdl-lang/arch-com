/// Verilator-compatible C++ simulation model generator.
///
/// For each synthesizable construct in the ARCH source (module, counter, fsm)
/// this emits:
///   V{Name}.h   – class declaration with public port fields and private state
///   V{Name}.cpp – eval() / eval_posedge() / eval_comb() implementations
///
/// The generated class matches the Verilator interface:
///   VFoo* dut = new VFoo;
///   dut->clk = 0; dut->eval();
///   dut->clk = 1; dut->eval();   // rising edge detected inside eval()
///   dut->final();
///   delete dut;
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::resolve::{Symbol, SymbolTable};
use crate::typecheck::enum_width;

// ── Public API ────────────────────────────────────────────────────────────────

pub struct SimModel {
    pub class_name: String,
    pub header: String,
    pub impl_: String,
}

pub struct SimCodegen<'a> {
    symbols: &'a SymbolTable,
    source: &'a SourceFile,
    #[allow(dead_code)]
    overload_map: HashMap<usize, usize>,
}

impl<'a> SimCodegen<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        source: &'a SourceFile,
        overload_map: HashMap<usize, usize>,
    ) -> Self {
        Self { symbols, source, overload_map }
    }

    /// Generate a SimModel for each synthesizable construct in the source.
    pub fn generate(&self) -> Vec<SimModel> {
        let mut models = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Module(m)  => models.push(self.gen_module(m)),
                Item::Counter(c) => models.push(self.gen_counter(c)),
                Item::Fsm(f)     => models.push(self.gen_fsm(f)),
                _ => {} // fifo/ram/arbiter/regfile: TODO
            }
        }
        models
    }

    /// Return the contents of the `verilated.h` stub and `verilated.cpp`.
    pub fn verilated_h() -> String {
        r#"#pragma once
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

/// Minimal Verilated compatibility shim for arch-generated C++ simulation models.
class Verilated {
public:
    static void commandArgs(int argc, char** argv) {
        for (int i = 1; i < argc; i++) {
            int v;
            if (sscanf(argv[i], "+arch_verbosity=%d", &v) == 1) {
                _s_verbosity = v;
            }
        }
    }
    static int verbosity() { return _s_verbosity; }
    static int _s_verbosity;
};

/// Ceiling log2 helper (included once via verilated.h).
static inline uint32_t _arch_clog2(uint64_t v) {
    if (v <= 1) return 1;
    uint32_t r = 0; v--; while (v) { v >>= 1; r++; } return r;
}
"#.to_string()
    }

    pub fn verilated_cpp() -> String {
        r#"#include "verilated.h"
int Verilated::_s_verbosity = 1;
"#.to_string()
    }
}

// ── Type helpers ──────────────────────────────────────────────────────────────

/// Evaluate a simple constant expression to a u32 bit-width.
/// Returns 32 as a safe fallback for unknown/parametric widths.
fn eval_width(expr: &Expr) -> u32 {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(n)) => *n as u32,
        ExprKind::Literal(LitKind::Hex(n)) => *n as u32,
        ExprKind::Clog2(inner) => {
            let v = eval_width(inner);
            if v <= 1 { 1 } else { 32 - (v - 1).leading_zeros() }
        }
        _ => 32, // param / expression: conservative fallback
    }
}

/// Smallest C++ unsigned integer type that fits `bits`.
fn cpp_uint(bits: u32) -> &'static str {
    if bits <= 8  { "uint8_t" }
    else if bits <= 16 { "uint16_t" }
    else if bits <= 32 { "uint32_t" }
    else               { "uint64_t" }
}

/// Smallest C++ signed integer type that fits `bits`.
fn cpp_sint(bits: u32) -> &'static str {
    if bits <= 8  { "int8_t" }
    else if bits <= 16 { "int16_t" }
    else if bits <= 32 { "int32_t" }
    else               { "int64_t" }
}

/// C++ type for a TypeExpr.
fn cpp_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::UInt(w) => cpp_uint(eval_width(w)).to_string(),
        TypeExpr::SInt(w) => cpp_sint(eval_width(w)).to_string(),
        TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(..) => {
            "uint8_t".to_string()
        }
        TypeExpr::Vec(_, _) => "uint32_t".to_string(), // simplified
        TypeExpr::Named(_)  => "uint32_t".to_string(), // enum/struct: simplified
    }
}

/// Cast expression to `bits`-wide C++ type: `(uint8_t)(expr)`.
fn cast_to_bits(expr: &str, bits: u32) -> String {
    format!("({})({})", cpp_uint(bits), expr)
}

/// Bit-range extraction: `(expr >> lo) & mask`.
fn bit_range(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
    format!("(({} >> {}) & 0x{:X}ULL)", expr, lo, mask)
}

/// Convert SV/ARCH format string tokens to printf equivalents.
/// `%0d` → `%d`, `%0h` → `%x`, `%0b` → `%u`, `%0t` → `%lu`
fn sv_fmt_to_printf(s: &str) -> String {
    s.replace("%0t", "%lu")
     .replace("%0d", "%d")
     .replace("%0h", "%x")
     .replace("%0b", "%u")
     .replace("%t",  "%lu")
     .replace("%d",  "%d")
     .replace("%h",  "%x")
}

// ── Expression context ────────────────────────────────────────────────────────

/// Holds name-role mappings for the current construct being emitted.
struct Ctx<'a> {
    /// Names that are reg declarations (private `_name` fields).
    reg_names: &'a HashSet<String>,
    /// Names that are let bindings (private `_let_name` fields).
    let_names: &'a HashSet<String>,
    /// Names that are sub-instance variables (`_inst_name`).
    inst_names: &'a HashSet<String>,
    /// When true, reg-name LHS assignments use `_n_` prefix (NBA next value).
    posedge_lhs: bool,
    /// Enum name → list of variant names (for integer mapping).
    enum_map: &'a HashMap<String, Vec<String>>,
}

impl<'a> Ctx<'a> {
    fn new(
        reg_names: &'a HashSet<String>,
        _port_names: &'a HashSet<String>,
        let_names: &'a HashSet<String>,
        inst_names: &'a HashSet<String>,
        enum_map: &'a HashMap<String, Vec<String>>,
    ) -> Self {
        Ctx { reg_names, let_names, inst_names, posedge_lhs: false, enum_map }
    }

    fn posedge(mut self) -> Self { self.posedge_lhs = true; self }

    /// Resolve a plain identifier to its C++ name.
    fn resolve_name(&self, name: &str, is_lhs: bool) -> String {
        if self.reg_names.contains(name) {
            if is_lhs && self.posedge_lhs {
                format!("_n_{}", name)
            } else {
                format!("_{}", name)
            }
        } else if self.let_names.contains(name) {
            format!("_let_{}", name)
        } else if self.inst_names.contains(name) {
            format!("_inst_{}", name)
        } else {
            // Port name or param — use as-is
            name.to_string()
        }
    }
}

// ── Expression emitter ────────────────────────────────────────────────────────

fn cpp_expr(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, false)
}

fn cpp_expr_lhs(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, true)
}

fn cpp_expr_inner(expr: &Expr, ctx: &Ctx, is_lhs: bool) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match lit {
            LitKind::Dec(v) => format!("{v}"),
            LitKind::Hex(v) => format!("0x{v:X}"),
            LitKind::Bin(v) => format!("{v}"),
            LitKind::Sized(_, v) => format!("{v}"),
        },
        ExprKind::Bool(true)  => "1".to_string(),
        ExprKind::Bool(false) => "0".to_string(),

        ExprKind::Ident(name) => ctx.resolve_name(name, is_lhs),

        ExprKind::Binary(op, lhs, rhs) => {
            let l = cpp_expr(lhs, ctx);
            let r = cpp_expr(rhs, ctx);
            let op_str = match op {
                BinOp::Add    => "+",  BinOp::Sub   => "-",
                BinOp::Mul    => "*",  BinOp::Div   => "/",
                BinOp::Mod    => "%",
                BinOp::Eq     => "==", BinOp::Neq  => "!=",
                BinOp::Lt     => "<",  BinOp::Gt   => ">",
                BinOp::Lte    => "<=", BinOp::Gte  => ">=",
                BinOp::And    => "&&", BinOp::Or   => "||",
                BinOp::BitAnd => "&",  BinOp::BitOr => "|",
                BinOp::BitXor => "^",
                BinOp::Shl    => "<<", BinOp::Shr  => ">>",
            };
            format!("({l} {op_str} {r})")
        }

        ExprKind::Unary(op, operand) => {
            let o = cpp_expr(operand, ctx);
            match op {
                UnaryOp::Not    => format!("(!{o})"),
                UnaryOp::BitNot => format!("(~{o})"),
                UnaryOp::Neg    => format!("(-{o})"),
            }
        }

        ExprKind::FieldAccess(base, field) => {
            // Check if base is an instance reference
            if let ExprKind::Ident(base_name) = &base.kind {
                if ctx.inst_names.contains(base_name.as_str()) {
                    return format!("_inst_{}.{}", base_name, field.name);
                }
            }
            let b = cpp_expr(base, ctx);
            format!("{b}.{}", field.name)
        }

        ExprKind::MethodCall(base, method, args) => {
            let b = cpp_expr(base, ctx);
            match method.name.as_str() {
                "trunc" if args.len() == 2 => {
                    // bit-range select: expr.trunc<hi,lo>()
                    let hi = eval_width(&args[0]);
                    let lo = eval_width(&args[1]);
                    bit_range(&b, hi, lo)
                }
                "trunc" => {
                    if let Some(w_expr) = args.first() {
                        let bits = eval_width(w_expr);
                        cast_to_bits(&b, bits)
                    } else {
                        b
                    }
                }
                "zext" | "sext" => {
                    if let Some(w_expr) = args.first() {
                        let bits = eval_width(w_expr);
                        // Cast is sufficient — C++ zero/sign extends automatically
                        // for signed→unsigned, use the signed type
                        format!("({})({})", cpp_uint(bits), b)
                    } else {
                        b
                    }
                }
                _ => format!("{b}.{}()", method.name),
            }
        }

        ExprKind::Cast(inner, ty) => {
            let e = cpp_expr(inner, ctx);
            let t = cpp_type(ty);
            format!("({t})({e})")
        }

        ExprKind::Index(base, idx) => {
            let b = cpp_expr(base, ctx);
            let i = cpp_expr(idx, ctx);
            format!("{b}[{i}]")
        }

        ExprKind::EnumVariant(enum_name, variant) => {
            // Map to integer based on variant position in the enum
            if let Some(variants) = ctx.enum_map.get(&enum_name.name) {
                let idx = variants.iter().position(|v| *v == variant.name).unwrap_or(0);
                format!("{}", idx)
            } else {
                format!("/* {}::{} */ 0", enum_name.name, variant.name)
            }
        }

        ExprKind::StructLiteral(_, _) => "0 /* struct literal */".to_string(),

        ExprKind::Todo => "0 /* todo! */".to_string(),

        ExprKind::Concat(parts) => {
            // Bit concatenation: emit as shift+or
            // For C++ we can't easily do packed bit concat without knowing widths.
            // Use a comment and 0 as placeholder for now.
            if parts.is_empty() {
                "0".to_string()
            } else {
                // Simple 2-part concat: (hi << lo_bits) | lo
                // Without width info, just emit addition as approximation
                let strs: Vec<String> = parts.iter().map(|p| cpp_expr(p, ctx)).collect();
                format!("/* concat */ ({})", strs.join(" | "))
            }
        }

        ExprKind::Clog2(arg) => {
            // Use __builtin_clz for compile-time-evaluable expressions, or a helper
            let a = cpp_expr(arg, ctx);
            format!("_arch_clog2({a})")
        }

        ExprKind::FunctionCall(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(|a| cpp_expr(a, ctx)).collect();
            format!("{}({})", name, arg_strs.join(", "))
        }

        ExprKind::ExprMatch(scrutinee, arms) => {
            // Nested ternary
            let s = cpp_expr(scrutinee, ctx);
            let mut result = "0".to_string();
            for arm in arms.iter().rev() {
                let val = cpp_expr(&arm.value, ctx);
                let cond = match &arm.pattern {
                    Pattern::Wildcard => { result = val; continue; }
                    Pattern::Ident(id) if id.name == "_" => { result = val; continue; }
                    Pattern::Literal(e) => {
                        let lit = cpp_expr(e, ctx);
                        format!("({s} == {lit})")
                    }
                    Pattern::Ident(id) => format!("({s} == {})", id.name),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().position(|v| *v == vr.name).unwrap_or(0);
                            format!("({s} == {idx})")
                        } else {
                            format!("({s} == /* {}::{} */ 0)", en.name, vr.name)
                        }
                    }
                };
                result = format!("({cond} ? {val} : {result})");
            }
            result
        }

        ExprKind::Match(scrutinee, _) => {
            // Statement-level match embedded in expression context: fallback
            let s = cpp_expr(scrutinee, ctx);
            format!("/* match({s}) */ 0")
        }
    }
}

// ── Statement emitters ────────────────────────────────────────────────────────

fn ind(n: usize) -> String { "  ".repeat(n) }

fn emit_reg_stmts(stmts: &[Stmt], ctx: &Ctx, out: &mut String, indent: usize) {
    for stmt in stmts {
        emit_reg_stmt(stmt, ctx, out, indent);
    }
}

fn emit_reg_stmt(stmt: &Stmt, ctx: &Ctx, out: &mut String, indent: usize) {
    match stmt {
        Stmt::Assign(a) => {
            let lhs = cpp_expr_lhs(&a.target, ctx);
            let rhs = cpp_expr(&a.value, ctx);
            out.push_str(&format!("{}{}  = {};\n", ind(indent), lhs, rhs));
        }
        Stmt::IfElse(ie) => {
            emit_reg_if_else(ie, ctx, out, indent, false);
        }
        Stmt::Match(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let case_str = match &arm.pattern {
                    Pattern::Wildcard | Pattern::Ident(_) => "default".to_string(),
                    Pattern::Literal(e) => {
                        let v = cpp_expr(e, ctx);
                        format!("case {v}")
                    }
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().position(|v| *v == vr.name).unwrap_or(0);
                            format!("case {idx}")
                        } else {
                            "default".to_string()
                        }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                emit_reg_stmts(&arm.body, ctx, out, indent + 2);
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        Stmt::Log(l) => {
            emit_log_stmt(l, ctx, out, indent);
        }
    }
}

fn emit_reg_if_else(ie: &IfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    let cond = cpp_expr(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if ({}) {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
    }
    emit_reg_stmts(&ie.then_stmts, ctx, out, indent + 1);

    // Check for else-if chain
    if ie.else_stmts.len() == 1 {
        if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_reg_if_else(nested, ctx, out, indent, true);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        emit_reg_stmts(&ie.else_stmts, ctx, out, indent + 1);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

fn emit_comb_stmts(stmts: &[CombStmt], ctx: &Ctx, out: &mut String, indent: usize) {
    for stmt in stmts {
        emit_comb_stmt(stmt, ctx, out, indent);
    }
}

fn emit_comb_stmt(stmt: &CombStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    match stmt {
        CombStmt::Assign(a) => {
            // In comb context, LHS is a port (not a reg) — just assign directly
            let rhs = cpp_expr(&a.value, ctx);
            out.push_str(&format!("{}{}  = {};\n", ind(indent), a.target.name, rhs));
        }
        CombStmt::IfElse(ie) => {
            emit_comb_if_else(ie, ctx, out, indent, false);
        }
        CombStmt::MatchExpr(m) => {
            let scrut = cpp_expr(&m.scrutinee, ctx);
            out.push_str(&format!("{}switch ({}) {{\n", ind(indent), scrut));
            for arm in &m.arms {
                let case_str = match &arm.pattern {
                    Pattern::Wildcard | Pattern::Ident(_) => "default".to_string(),
                    Pattern::Literal(e) => format!("case {}", cpp_expr(e, ctx)),
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants.iter().position(|v| *v == vr.name).unwrap_or(0);
                            format!("case {idx}")
                        } else {
                            "default".to_string()
                        }
                    }
                };
                out.push_str(&format!("{}{}: {{\n", ind(indent + 1), case_str));
                for s in &arm.body {
                    // Arms here are Stmt (reg-style), emit as comb assigns
                    match s {
                        Stmt::Assign(a) => {
                            let rhs = cpp_expr(&a.value, ctx);
                            let lhs = cpp_expr(&a.target, ctx);
                            out.push_str(&format!("{}{} = {};\n", ind(indent + 2), lhs, rhs));
                        }
                        _ => {}
                    }
                }
                out.push_str(&format!("{}  break;\n", ind(indent + 1)));
                out.push_str(&format!("{}}}\n", ind(indent + 1)));
            }
            out.push_str(&format!("{}}}\n", ind(indent)));
        }
        CombStmt::Log(l) => {
            emit_log_stmt(l, ctx, out, indent);
        }
    }
}

fn emit_comb_if_else(ie: &CombIfElse, ctx: &Ctx, out: &mut String, indent: usize, is_chain: bool) {
    let cond = cpp_expr(&ie.cond, ctx);
    if is_chain {
        out.push_str(&format!("{}}} else if ({}) {{\n", ind(indent), cond));
    } else {
        out.push_str(&format!("{}if ({}) {{\n", ind(indent), cond));
    }
    emit_comb_stmts(&ie.then_stmts, ctx, out, indent + 1);

    if ie.else_stmts.len() == 1 {
        if let CombStmt::IfElse(nested) = &ie.else_stmts[0] {
            emit_comb_if_else(nested, ctx, out, indent, true);
            return;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{}}} else {{\n", ind(indent)));
        emit_comb_stmts(&ie.else_stmts, ctx, out, indent + 1);
    }
    out.push_str(&format!("{}}}\n", ind(indent)));
}

fn emit_log_stmt(l: &LogStmt, ctx: &Ctx, out: &mut String, indent: usize) {
    let args_str: String = l.args.iter()
        .map(|a| format!(", (long long)({})", cpp_expr(a, ctx)))
        .collect();
    let fmt = sv_fmt_to_printf(&l.fmt);
    let printf_line = format!(
        "{}printf(\"[{}][{}] {}\\n\"{});",
        ind(indent), l.level.name(), l.tag, fmt, args_str
    );
    if l.level == LogLevel::Always {
        out.push_str(&printf_line);
        out.push('\n');
    } else {
        out.push_str(&format!(
            "{}if (Verilated::verbosity() >= {}) {{ {} }}\n",
            ind(indent), l.level.value(), printf_line
        ));
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Collect all register names from a module body.
fn collect_reg_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r.name.name.clone()) } else { None })
        .collect()
}

/// Collect all let binding names from a module body.
fn collect_let_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| if let ModuleBodyItem::LetBinding(l) = i { Some(l.name.name.clone()) } else { None })
        .collect()
}

/// Collect all inst names from a module body.
fn collect_inst_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| if let ModuleBodyItem::Inst(inst) = i { Some(inst.name.name.clone()) } else { None })
        .collect()
}

/// Extract reset info from a port list: (signal_name, is_async, is_low)
fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
    for p in ports {
        if let TypeExpr::Reset(kind, level) = &p.ty {
            return (
                p.name.name.clone(),
                *kind == ResetKind::Async,
                *level == ResetLevel::Low,
            );
        }
    }
    ("rst".to_string(), false, false)
}

/// Resolve a module's RegDecl reset against its ports.
fn resolve_reg_reset_info(reset: &RegReset, ports: &[PortDecl]) -> Option<(String, bool, bool)> {
    match reset {
        RegReset::None => None,
        RegReset::Explicit(sig, kind, level) => Some((
            sig.name.clone(),
            *kind == ResetKind::Async,
            *level == ResetLevel::Low,
        )),
        RegReset::Inherit(sig) => {
            if let Some(p) = ports.iter().find(|p| p.name.name == sig.name) {
                if let TypeExpr::Reset(kind, level) = &p.ty {
                    Some((sig.name.clone(), *kind == ResetKind::Async, *level == ResetLevel::Low))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Build a map of enum_name → variant_name_list from the symbol table.
fn build_enum_map(symbols: &SymbolTable) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    for (name, (sym, _)) in &symbols.globals {
        if let Symbol::Enum(info) = sym {
            m.insert(name.clone(), info.variants.clone());
        }
    }
    m
}

// ── Module codegen ────────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_module(&self, m: &ModuleDecl) -> SimModel {
        let name = &m.name.name;
        let class = format!("V{name}");
        let enum_map = build_enum_map(self.symbols);

        let port_names: HashSet<String> = m.ports.iter().map(|p| p.name.name.clone()).collect();
        let reg_names  = collect_reg_names(&m.body);
        let let_names  = collect_let_names(&m.body);
        let inst_names = collect_inst_names(&m.body);

        // Collect instances with their module types
        let insts: Vec<&InstDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::Inst(inst) = i { Some(inst) } else { None })
            .collect();

        // ── Header ───────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str(&format!("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n"));

        // Include sub-instance headers
        for inst in &insts {
            h.push_str(&format!("#include \"V{}.h\"\n", inst.module_name.name));
        }
        h.push('\n');

        h.push_str(&format!("class {class} {{\npublic:\n"));

        // Public port fields
        for p in &m.ports {
            let ty = cpp_type(&p.ty);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        h.push('\n');

        // Constructor: zero-init all ports, state fields
        let port_inits: Vec<String> = m.ports.iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        let reg_inits: Vec<String> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i {
                let init_val = match &r.init.kind {
                    ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                    ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                    _ => "0".to_string(),
                };
                Some(format!("_{}({})", r.name.name, init_val))
            } else { None })
            .collect();
        let clk_inits = vec!["_clk_prev(0)".to_string()];
        let all_inits: Vec<String> = port_inits.into_iter()
            .chain(reg_inits)
            .chain(clk_inits)
            .collect();

        h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        h.push_str("  void eval();\n");
        h.push_str("  void final() {}\n\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");

        // Private reg fields
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                let ty = cpp_type(&r.ty);
                h.push_str(&format!("  {ty} _{};\n", r.name.name));
            }
        }

        // Sub-instance private fields
        for inst in &insts {
            h.push_str(&format!("  V{} _inst_{};\n", inst.module_name.name, inst.name.name));
        }

        h.push_str("\n  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("};\n");

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        // Detect rising edge on the first Clock port
        let clk_port = m.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (_rising) eval_posedge();\n");

        // Sub-instance connections + eval calls
        for inst in &insts {
            cpp.push('\n');
            // Write inputs to sub-instance
            for conn in &inst.connections {
                if conn.direction == ConnectDir::Input {
                    let ctx = Ctx::new(&reg_names, &port_names, &let_names, &inst_names, &enum_map);
                    let sig = cpp_expr(&conn.signal, &ctx);
                    cpp.push_str(&format!("  _inst_{}.{} = {};\n", inst.name.name, conn.port_name.name, sig));
                }
            }
            cpp.push_str(&format!("  _inst_{}.eval();\n", inst.name.name));
            // Read outputs from sub-instance
            for conn in &inst.connections {
                if conn.direction == ConnectDir::Output {
                    let ctx = Ctx::new(&reg_names, &port_names, &let_names, &inst_names, &enum_map);
                    let sig = cpp_expr(&conn.signal, &ctx);
                    cpp.push_str(&format!("  {} = _inst_{}.{};\n", sig, inst.name.name, conn.port_name.name));
                }
            }
        }

        cpp.push_str("  eval_comb();\n");
        cpp.push_str("}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));

        // Find all reg blocks
        let reg_blocks: Vec<&RegBlock> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegBlock(rb) = i { Some(rb) } else { None })
            .collect();

        let reg_decls: Vec<&RegDecl> = m.body.iter()
            .filter_map(|i| if let ModuleBodyItem::RegDecl(r) = i { Some(r) } else { None })
            .collect();

        if !reg_blocks.is_empty() {
            // Declare _n_ next-value temporaries for all regs
            for rd in &reg_decls {
                let ty = cpp_type(&rd.ty);
                cpp.push_str(&format!("  {ty} _n_{} = _{};\n", rd.name.name, rd.name.name));
            }
            cpp.push('\n');

            // Build context for posedge (LHS uses _n_ prefix)
            let ctx = Ctx::new(&reg_names, &port_names, &let_names, &inst_names, &enum_map).posedge();

            // For each reg block, emit the reset guard + body
            for rb in &reg_blocks {
                // Collect regs assigned in this block
                let mut assigned = std::collections::BTreeSet::new();
                collect_stmt_assigns(&rb.stmts, &mut assigned);

                // Find reset info from any assigned reg that has a reset
                let mut reset_sig: Option<(String, bool, bool)> = None;
                let mut reset_regs: Vec<(&str, String)> = Vec::new();

                for name in &assigned {
                    if let Some(rd) = reg_decls.iter().find(|r| r.name.name == *name) {
                        if let Some(info) = resolve_reg_reset_info(&rd.reset, &m.ports) {
                            if reset_sig.is_none() {
                                reset_sig = Some(info.clone());
                            }
                            // Compute init value
                            let init_val = match &rd.init.kind {
                                ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                                ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                                _ => "0".to_string(),
                            };
                            reset_regs.push((&rd.name.name, init_val));
                        }
                    }
                }

                if let Some((rst_name, _is_async, is_low)) = &reset_sig {
                    let cond = if *is_low {
                        format!("(!{})", rst_name)
                    } else {
                        rst_name.clone()
                    };
                    cpp.push_str(&format!("  if ({cond}) {{\n"));
                    for (reg_name, init) in &reset_regs {
                        cpp.push_str(&format!("    _n_{reg_name} = {init};\n"));
                    }
                    cpp.push_str("  } else {\n");
                    let mut body = String::new();
                    emit_reg_stmts(&rb.stmts, &ctx, &mut body, 2);
                    cpp.push_str(&body);
                    cpp.push_str("  }\n");
                } else {
                    let mut body = String::new();
                    emit_reg_stmts(&rb.stmts, &ctx, &mut body, 1);
                    cpp.push_str(&body);
                }
            }

            cpp.push('\n');
            // Commit next values
            for rd in &reg_decls {
                cpp.push_str(&format!("  _{} = _n_{};\n", rd.name.name, rd.name.name));
            }
        }
        cpp.push_str("}\n\n");

        // eval_comb()
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));

        // Emit let bindings first (wires)
        let ctx_comb = Ctx::new(&reg_names, &port_names, &let_names, &inst_names, &enum_map);
        for item in &m.body {
            if let ModuleBodyItem::LetBinding(l) = item {
                let val = cpp_expr(&l.value, &ctx_comb);
                let ty = l.ty.as_ref().map(|t| cpp_type(t)).unwrap_or_else(|| "uint32_t".to_string());
                cpp.push_str(&format!("  {ty} _let_{} = {};\n", l.name.name, val));
            }
        }

        for item in &m.body {
            if let ModuleBodyItem::CombBlock(cb) = item {
                let mut body = String::new();
                emit_comb_stmts(&cb.stmts, &ctx_comb, &mut body, 1);
                cpp.push_str(&body);
            }
        }
        cpp.push_str("}\n");

        SimModel { class_name: class.clone(), header: h, impl_: cpp }
    }
}

/// Collect all LHS root names from a list of Stmts.
fn collect_stmt_assigns(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign(a) => {
                if let ExprKind::Ident(n) = &a.target.kind {
                    out.insert(n.clone());
                }
            }
            Stmt::IfElse(ie) => {
                collect_stmt_assigns(&ie.then_stmts, out);
                collect_stmt_assigns(&ie.else_stmts, out);
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    collect_stmt_assigns(&arm.body, out);
                }
            }
            Stmt::Log(_) => {}
        }
    }
}

// ── Counter codegen ───────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_counter(&self, c: &CounterDecl) -> SimModel {
        let name = &c.name.name;
        let class = format!("V{name}");

        // Parameter defaults
        let max_param = c.params.iter()
            .find(|p| p.name.name == "MAX")
            .and_then(|p| p.default.as_ref())
            .map(|e| match &e.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                _ => 255,
            });

        let value_port = c.ports.iter().find(|p| p.name.name == "value");
        let count_bits = value_port
            .and_then(|vp| if let TypeExpr::UInt(w) = &vp.ty { Some(eval_width(w)) } else { None })
            .unwrap_or(8);
        let count_ty = cpp_uint(count_bits);

        let has_inc   = c.ports.iter().any(|p| p.name.name == "inc");
        let has_dec   = c.ports.iter().any(|p| p.name.name == "dec");
        let has_at_max = c.ports.iter().any(|p| p.name.name == "at_max");
        let has_at_min = c.ports.iter().any(|p| p.name.name == "at_min");

        let (rst_name, _is_async, is_low) = extract_reset_info(&c.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };

        let init_val: u64 = c.init.as_ref()
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
            .unwrap_or(0);

        // ── Header ───────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));

        for p in &c.ports {
            let ty = cpp_type(&p.ty);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        h.push('\n');

        // Constructor
        let port_inits: Vec<String> = c.ports.iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        let state_inits = vec![
            "_clk_prev(0)".to_string(),
            format!("_count_r({})", init_val),
        ];
        let all_inits: Vec<String> = port_inits.into_iter().chain(state_inits).collect();
        h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        h.push_str("  void eval();\n  void final() {}\nprivate:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {count_ty} _count_r;\n"));
        h.push_str("  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("};\n");

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = c.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (_rising) eval_posedge();\n");
        cpp.push_str("  eval_comb();\n}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  {count_ty} _n = _count_r;\n"));
        cpp.push_str(&format!("  if ({rst_cond}) {{\n"));
        cpp.push_str(&format!("    _n = {init_val};\n"));
        cpp.push_str("  } else {\n");

        // Counter mode logic
        use CounterDirection::*;
        use CounterMode::*;
        match (c.direction, c.mode) {
            (Up, Wrap) => {
                let inc_cond = if has_inc { "    if (inc) {\n" } else { "    {\n" };
                cpp.push_str(inc_cond);
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r == ({count_ty}){max}) _n = {init_val};\n"));
                    cpp.push_str("      else _n = _count_r + 1;\n");
                } else {
                    // wrap at bit boundary
                    cpp.push_str(&format!("      _n = ({count_ty})(_count_r + 1);\n"));
                }
                cpp.push_str("    }\n");
            }
            (Down, Wrap) => {
                let dec_cond = if has_dec { "    if (dec) {\n" } else { "    {\n" };
                cpp.push_str(dec_cond);
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r == {init_val}) _n = ({count_ty}){max};\n"));
                    cpp.push_str("      else _n = _count_r - 1;\n");
                } else {
                    cpp.push_str(&format!("      _n = ({count_ty})(_count_r - 1);\n"));
                }
                cpp.push_str("    }\n");
            }
            (Up, Saturate) => {
                let inc_cond = if has_inc { "    if (inc) {\n" } else { "    {\n" };
                cpp.push_str(inc_cond);
                if let Some(max) = max_param {
                    cpp.push_str(&format!("      if (_count_r < ({count_ty}){max}) _n = _count_r + 1;\n"));
                } else {
                    let max_val = (1u64 << count_bits) - 1;
                    cpp.push_str(&format!("      if (_count_r < ({count_ty})0x{max_val:X}ULL) _n = _count_r + 1;\n"));
                }
                cpp.push_str("    }\n");
            }
            (Down, Saturate) => {
                let dec_cond = if has_dec { "    if (dec) {\n" } else { "    {\n" };
                cpp.push_str(dec_cond);
                cpp.push_str("      if (_count_r > 0) _n = _count_r - 1;\n");
                cpp.push_str("    }\n");
            }
            (Up, Gray) => {
                cpp.push_str("    if (inc) {\n");
                cpp.push_str("      uint32_t _bin = _count_r + 1;\n");
                cpp.push_str(&format!("      _n = ({count_ty})(_bin ^ (_bin >> 1));\n"));
                cpp.push_str("    }\n");
            }
            (Up, OneHot) => {
                let inc_cond = if has_inc { "    if (inc) {\n" } else { "    {\n" };
                cpp.push_str(inc_cond);
                cpp.push_str(&format!(
                    "      _n = ({count_ty})((_count_r >> 1) | (_count_r << ({count_bits} - 1)));\n"
                ));
                cpp.push_str("    }\n");
            }
            (Up, Johnson) => {
                let inc_cond = if has_inc { "    if (inc) {\n" } else { "    {\n" };
                cpp.push_str(inc_cond);
                cpp.push_str(&format!(
                    "      _n = ({count_ty})((~_count_r & 1) << ({count_bits}-1)) | (_count_r >> 1);\n"
                ));
                cpp.push_str("    }\n");
            }
            (UpDown, _) => {
                cpp.push_str("    if (inc && !dec) _n = _count_r + 1;\n");
                cpp.push_str("    else if (dec && !inc) _n = _count_r - 1;\n");
            }
            _ => {
                let inc_cond = if has_inc { "    if (inc)" } else { "" };
                cpp.push_str(&format!("    {inc_cond} _n = ({count_ty})(_count_r + 1);\n"));
            }
        }

        cpp.push_str("  }\n");
        cpp.push_str("  _count_r = _n;\n");
        cpp.push_str("}\n\n");

        // eval_comb()
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        if value_port.is_some() {
            cpp.push_str("  value = _count_r;\n");
        }
        if has_at_max {
            if let Some(max) = max_param {
                cpp.push_str(&format!("  at_max = (_count_r == ({count_ty}){max}) ? 1 : 0;\n"));
            } else {
                let all_ones = (1u64 << count_bits) - 1;
                cpp.push_str(&format!("  at_max = (_count_r == 0x{all_ones:X}ULL) ? 1 : 0;\n"));
            }
        }
        if has_at_min {
            cpp.push_str(&format!("  at_min = (_count_r == {init_val}) ? 1 : 0;\n"));
        }
        cpp.push_str("}\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}

// ── FSM codegen ───────────────────────────────────────────────────────────────

impl<'a> SimCodegen<'a> {
    fn gen_fsm(&self, f: &FsmDecl) -> SimModel {
        let name = &f.name.name;
        let class = format!("V{name}");
        let enum_map = build_enum_map(self.symbols);

        let port_names: HashSet<String> = f.ports.iter().map(|p| p.name.name.clone()).collect();
        let empty_regs  = HashSet::new();
        let empty_lets  = HashSet::new();
        let empty_insts = HashSet::new();

        let n_states  = f.state_names.len();
        let state_bits = enum_width(n_states);
        let state_ty  = cpp_uint(state_bits as u32);

        // State index map
        let state_idx: HashMap<String, usize> = f.state_names.iter()
            .enumerate()
            .map(|(i, s)| (s.name.clone(), i))
            .collect();

        let default_idx = state_idx.get(&f.default_state.name).copied().unwrap_or(0);

        let (rst_name, _is_async, is_low) = extract_reset_info(&f.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };

        // ── Header ───────────────────────────────────────────────────────────
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n\n");

        h.push_str(&format!("class {class} {{\npublic:\n"));

        // State enum as public constants
        h.push_str("  // State constants\n");
        for (i, sn) in f.state_names.iter().enumerate() {
            h.push_str(&format!("  static const {state_ty} STATE_{} = {i};\n", sn.name.to_uppercase()));
        }
        h.push('\n');

        // Ports
        for p in &f.ports {
            let ty = cpp_type(&p.ty);
            h.push_str(&format!("  {ty} {};\n", p.name.name));
        }
        h.push('\n');

        // Constructor
        let port_inits: Vec<String> = f.ports.iter()
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        let state_inits = vec![
            "_clk_prev(0)".to_string(),
            format!("_state_r({default_idx})"),
        ];
        let all_inits: Vec<String> = port_inits.into_iter().chain(state_inits).collect();
        h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        h.push_str("  void eval();\n  void final() {}\nprivate:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {state_ty} _state_r;\n"));
        h.push_str("  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("};\n");

        // ── Implementation ────────────────────────────────────────────────────
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str())
            .unwrap_or("clk");

        // eval()
        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (_rising) eval_posedge();\n");
        cpp.push_str("  eval_comb();\n}\n\n");

        // eval_posedge(): state register update + transitions
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  {state_ty} _n_state = _state_r;\n"));
        cpp.push_str(&format!("  if ({rst_cond}) {{\n"));
        cpp.push_str(&format!("    _n_state = {default_idx};\n"));
        cpp.push_str("  } else {\n");
        cpp.push_str("    switch (_state_r) {\n");

        let ctx_fsm = Ctx::new(&empty_regs, &port_names, &empty_lets, &empty_insts, &enum_map);

        for sb in &f.states {
            let idx = state_idx.get(&sb.name.name).copied().unwrap_or(0);
            cpp.push_str(&format!("      case {idx}: // {}\n", sb.name.name));
            for tr in &sb.transitions {
                let cond = cpp_expr(&tr.condition, &ctx_fsm);
                let target_idx = state_idx.get(&tr.target.name).copied().unwrap_or(0);
                cpp.push_str(&format!("        if ({cond}) {{ _n_state = {target_idx}; break; }}\n"));
            }
            cpp.push_str("        break;\n");
        }

        cpp.push_str("    }\n  }\n");
        cpp.push_str("  _state_r = _n_state;\n");
        cpp.push_str("}\n\n");

        // eval_comb(): outputs based on current state
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));

        // Emit default values for output ports
        for p in &f.ports {
            if p.direction == Direction::Out {
                let default_val = p.default.as_ref()
                    .map(|e| match &e.kind {
                        ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                        ExprKind::Literal(LitKind::Dec(v)) => v.to_string(),
                        _ => "0".to_string(),
                    })
                    .unwrap_or_else(|| "0".to_string());
                cpp.push_str(&format!("  {} = {};\n", p.name.name, default_val));
            }
        }

        cpp.push_str("  switch (_state_r) {\n");
        for sb in &f.states {
            let idx = state_idx.get(&sb.name.name).copied().unwrap_or(0);
            cpp.push_str(&format!("    case {idx}: {{ // {}\n", sb.name.name));
            let mut body = String::new();
            emit_comb_stmts(&sb.comb_stmts, &ctx_fsm, &mut body, 3);
            cpp.push_str(&body);
            cpp.push_str("      break;\n    }\n");
        }
        cpp.push_str("  }\n}\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}
