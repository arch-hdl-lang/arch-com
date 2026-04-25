// thread_sim.rs — Pre-lowering thread sim emitter.
//
// Emits C++20 coroutine-based per-module classes that simulate `thread`
// blocks directly, without lowering them to FSMs. See
// doc/plan_thread_parallel_sim.md and runtime/arch_thread_rt.h for
// design + runtime API.
//
// Phase 1.5 scope:
//   - N threads per module (named or anonymous, share one scheduler)
//   - Scalar Bool/UInt<≤64> ports + regs
//   - Combinational `let` bindings (evaluated each eval())
//   - Thread body statements:
//       CombAssign / SeqAssign  (port = / reg <=, both lower to `=`)
//       WaitUntil / WaitCycles  (literal cycle count for now)
//       IfElse                  (with elsif/else chains)
//   - Thread predicate / RHS expression shapes: idents, literals,
//     unary !/~, binary == != < > <= >= && || & | + -
//
// Anything else returns a clean error so the caller can fall back to
// the lowered-fsm path or report unsupported.

use crate::ast::{
    BinOp, Direction, Expr, ExprKind, LitKind, ModuleBodyItem, ModuleDecl,
    ResetLevel, ThreadBlock, ThreadStmt, TypeExpr, UnaryOp,
};
use crate::sim_codegen::SimModel;

pub fn gen_module_thread(m: &ModuleDecl) -> Result<SimModel, String> {
    let class = m.name.name.clone();

    let threads: Vec<&ThreadBlock> = m.body.iter().filter_map(|i| match i {
        ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).collect();
    if threads.is_empty() {
        return Err(format!("module `{}` has no thread blocks", class));
    }
    for (i, t) in threads.iter().enumerate() {
        if t.tlm_target.is_some() || t.implement.is_some() || t.reentrant.is_some() {
            return Err(format!("module `{}` thread #{}: TLM/implement/reentrant not yet supported", class, i));
        }
        if t.default_when.is_some() {
            return Err(format!("module `{}` thread #{}: `default when` not yet supported", class, i));
        }
    }

    for item in &m.body {
        match item {
            ModuleBodyItem::Thread(_)
            | ModuleBodyItem::RegDecl(_)
            | ModuleBodyItem::LetBinding(_) => {}
            _ => return Err(format!(
                "module `{}`: thread sim only supports `thread` + `reg` + `let` items",
                class
            )),
        }
    }

    // Ports + regs as fields.
    let clk_name = m.ports.iter()
        .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
        .map(|p| p.name.name.clone())
        .ok_or_else(|| format!("module `{}` has no clock port", class))?;
    let (rst_name, rst_active_low) = m.ports.iter()
        .find_map(|p| match &p.ty {
            TypeExpr::Reset(_, lvl) => Some((p.name.name.clone(), matches!(lvl, ResetLevel::Low))),
            _ => None,
        })
        .ok_or_else(|| format!("module `{}` has no reset port", class))?;

    let mut header = String::new();
    header.push_str("#pragma once\n");
    header.push_str("#include \"arch_thread_rt.h\"\n");
    header.push_str("#include <cstdint>\n");
    header.push_str("#include \"verilated.h\"\n\n");
    header.push_str(&format!("class {} {{\npublic:\n", class));

    // Port fields.
    for p in &m.ports {
        let cpp_ty = port_or_reg_cpp_ty(&p.ty)
            .map_err(|e| format!("module `{}` port `{}`: {}", class, p.name.name, e))?;
        header.push_str(&format!("  {} {} = 0;\n", cpp_ty, p.name.name));
    }
    header.push('\n');

    // Reg fields.
    let mut reg_resets: Vec<(String, String)> = Vec::new();
    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            let cpp_ty = port_or_reg_cpp_ty(&r.ty)
                .map_err(|e| format!("module `{}` reg `{}`: {}", class, r.name.name, e))?;
            header.push_str(&format!("  {} {} = 0;\n", cpp_ty, r.name.name));
            // Track reset value (default 0) so reset clears regs.
            reg_resets.push((r.name.name.clone(), "0".to_string()));
        }
    }
    header.push('\n');

    // Collect outputs that any thread writes — these are zeroed at the
    // start of each tick (state-local comb default semantic).
    let driven_outputs = collect_thread_driven_outputs(&threads, m);

    // Constructor: register one slot per thread coroutine.
    header.push_str(&format!("  {}() {{\n", class));
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("    _slot_{i}.thread = _make_thread_{i}();\n"));
        header.push_str(&format!("    _sched.slots.push_back(&_slot_{i});\n"));
    }
    header.push_str("  }\n");
    header.push_str(&format!("  ~{}() {{\n", class));
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("    _slot_{i}.thread.destroy();\n"));
    }
    header.push_str("  }\n\n");

    // eval(): combinational settle. Lets are pure-functional combinational.
    header.push_str("  void eval() {\n");
    for item in &m.body {
        if let ModuleBodyItem::LetBinding(lb) = item {
            let rhs = expr_to_cpp(&lb.value)?;
            header.push_str(&format!("    {} = {};\n", lb.name.name, rhs));
        }
    }
    header.push_str("  }\n\n");

    // posedge handler.
    header.push_str(&format!("  void posedge_{}() {{\n", clk_name));
    let rst_check = if rst_active_low {
        format!("if (!{}) {{", rst_name)
    } else {
        format!("if ({}) {{", rst_name)
    };
    header.push_str(&format!("    {}\n", rst_check));
    for n in &driven_outputs {
        header.push_str(&format!("      {} = 0;\n", n));
    }
    for (n, v) in &reg_resets {
        header.push_str(&format!("      {} = {};\n", n, v));
    }
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("      _slot_{i}.thread.destroy();\n"));
        header.push_str(&format!("      _slot_{i}.thread = _make_thread_{i}();\n"));
        header.push_str(&format!("      _slot_{i}.kind = arch_rt::WaitKind::Ready;\n"));
        header.push_str(&format!("      _slot_{i}.cycles_remaining = 0;\n"));
        header.push_str(&format!("      _slot_{i}.pred = nullptr;\n"));
    }
    header.push_str("      return;\n");
    header.push_str("    }\n");
    for n in &driven_outputs {
        header.push_str(&format!("    {} = 0;\n", n));
    }
    header.push_str("    _sched.tick();\n");
    // Re-evaluate combinational lets so outputs derived from regs (e.g.
    // `let data_out = data_r;`) reflect the writes the thread just made.
    header.push_str("    eval();\n");
    header.push_str("  }\n\n");

    let _ = (&clk_name, &rst_name);

    header.push_str("private:\n");
    header.push_str("  arch_rt::ThreadScheduler _sched;\n");
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ThreadSlot _slot_{i};\n"));
    }
    header.push('\n');

    // Coroutine bodies — one per thread. Each captures `&_slot_<i>` so
    // the awaiter knows where to write its parked state.
    //
    // `thread once` runs body to completion and stays Done. A regular
    // thread re-runs from the top after its body completes — matching
    // the lowered-fsm behavior where state advances to Done then back
    // to the entry state — so we wrap the body in `while (true) { ... }`.
    for (i, t) in threads.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ArchThread _make_thread_{i}() {{\n"));
        let mut body_cpp = String::new();
        if !t.once {
            body_cpp.push_str("    while (true) {\n");
            emit_thread_body(&t.body, &format!("_slot_{i}"), &mut body_cpp, 6)?;
            body_cpp.push_str("    }\n");
        } else {
            emit_thread_body(&t.body, &format!("_slot_{i}"), &mut body_cpp, 4)?;
        }
        header.push_str(&body_cpp);
        header.push_str("    co_return;\n");
        header.push_str("  }\n\n");
    }

    header.push_str("};\n");

    Ok(SimModel {
        class_name: class.clone(),
        header,
        impl_: format!("// {} thread-sim: header-only\n", class),
    })
}

fn emit_thread_body(stmts: &[ThreadStmt], slot: &str, out: &mut String, indent: usize) -> Result<(), String> {
    let pad = " ".repeat(indent);
    for s in stmts {
        match s {
            ThreadStmt::CombAssign(a) => {
                let lhs = expr_to_cpp(&a.target)?;
                let rhs = expr_to_cpp(&a.value)?;
                out.push_str(&format!("{}{} = {};\n", pad, lhs, rhs));
            }
            ThreadStmt::SeqAssign(a) => {
                let lhs = expr_to_cpp(&a.target)?;
                let rhs = expr_to_cpp(&a.value)?;
                out.push_str(&format!("{}{} = {};\n", pad, lhs, rhs));
            }
            ThreadStmt::WaitUntil(cond, _) => {
                let pred = expr_to_cpp_bool(cond)?;
                out.push_str(&format!(
                    "{}co_await arch_rt::wait_until(&{}, [this]{{ return {}; }});\n",
                    pad, slot, pred
                ));
            }
            ThreadStmt::WaitCycles(n, _) => {
                let n_str = match &n.kind {
                    ExprKind::Literal(LitKind::Dec(v)) => format!("{}", v),
                    ExprKind::Literal(LitKind::Sized(_, v)) => format!("{}", v),
                    _ => return Err("wait <N> cycle: thread sim supports only literal N".into()),
                };
                out.push_str(&format!("{}co_await arch_rt::wait_cycles(&{}, {});\n", pad, slot, n_str));
            }
            ThreadStmt::IfElse(ie) => {
                let cond = expr_to_cpp_bool(&ie.cond)?;
                out.push_str(&format!("{}if ({}) {{\n", pad, cond));
                emit_thread_body(&ie.then_stmts, slot, out, indent + 2)?;
                if !ie.else_stmts.is_empty() {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    emit_thread_body(&ie.else_stmts, slot, out, indent + 2)?;
                }
                out.push_str(&format!("{}}}\n", pad));
            }
            ThreadStmt::For { var, start, end, body, .. } => {
                // ARCH `for i in start..end` is inclusive on both ends;
                // codegen.rs and existing sim emit `for (..; i <= end; ..)`.
                let s = expr_to_cpp(start)?;
                let e = expr_to_cpp(end)?;
                out.push_str(&format!("{}for (uint64_t {v} = {s}; {v} <= {e}; {v}++) {{\n",
                    pad, v = var.name));
                emit_thread_body(body, slot, out, indent + 2)?;
                out.push_str(&format!("{}}}\n", pad));
            }
            other => return Err(format!("thread stmt not yet supported: {:?}", std::mem::discriminant(other))),
        }
    }
    Ok(())
}

fn expr_to_cpp(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(n.clone()),
        ExprKind::Literal(LitKind::Dec(v)) => Ok(format!("{}", v)),
        ExprKind::Literal(LitKind::Hex(v)) => Ok(format!("0x{:X}", v)),
        ExprKind::Literal(LitKind::Bin(v)) => Ok(format!("{}", v)),
        ExprKind::Literal(LitKind::Sized(_, v)) => Ok(format!("{}", v)),
        ExprKind::Bool(true) => Ok("1".into()),
        ExprKind::Bool(false) => Ok("0".into()),
        ExprKind::Unary(UnaryOp::Not, inner) => Ok(format!("!({})", expr_to_cpp_bool(inner)?)),
        ExprKind::Unary(UnaryOp::BitNot, inner) => Ok(format!("(~({}))", expr_to_cpp(inner)?)),
        ExprKind::Unary(UnaryOp::Neg, inner) => Ok(format!("(-({}))", expr_to_cpp(inner)?)),
        ExprKind::Binary(op, lhs, rhs) => {
            let l = expr_to_cpp(lhs)?;
            let r = expr_to_cpp(rhs)?;
            let op_str = match op {
                BinOp::Add | BinOp::AddWrap => "+",
                BinOp::Sub | BinOp::SubWrap => "-",
                BinOp::Mul | BinOp::MulWrap => "*",
                BinOp::Eq => "==", BinOp::Neq => "!=",
                BinOp::Lt => "<",  BinOp::Gt => ">",
                BinOp::Lte => "<=", BinOp::Gte => ">=",
                BinOp::And => "&&", BinOp::Or => "||",
                BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^",
                BinOp::Shl => "<<", BinOp::Shr => ">>",
                _ => return Err(format!("binop {:?} not yet supported by thread sim", op)),
            };
            Ok(format!("({} {} {})", l, op_str, r))
        }
        _ => Err(format!("expr shape not supported by thread sim: {:?}", std::mem::discriminant(&e.kind))),
    }
}

// Predicate context: convert ident to `ident != 0` so the lambda
// returns bool, but otherwise reuse expr_to_cpp.
fn expr_to_cpp_bool(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(format!("({} != 0)", n)),
        ExprKind::Bool(true) => Ok("true".into()),
        ExprKind::Bool(false) => Ok("false".into()),
        ExprKind::Unary(UnaryOp::Not, inner) => Ok(format!("!({})", expr_to_cpp_bool(inner)?)),
        // Binary ops that already return bool (==, !=, <, etc.) pass through.
        // Bitwise/arithmetic ops are wrapped in `!= 0` so they coerce.
        ExprKind::Binary(op, _, _) => {
            let s = expr_to_cpp(e)?;
            match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt
                | BinOp::Lte | BinOp::Gte | BinOp::And | BinOp::Or => Ok(s),
                _ => Ok(format!("({} != 0)", s)),
            }
        }
        _ => expr_to_cpp(e),
    }
}

fn port_or_reg_cpp_ty(ty: &TypeExpr) -> Result<String, String> {
    match ty {
        TypeExpr::Clock(_) | TypeExpr::Reset(..) | TypeExpr::Bool | TypeExpr::Bit => Ok("uint8_t".to_string()),
        TypeExpr::UInt(w) => uint_cpp_ty(eval_const(w)),
        other => Err(format!("type {:?} not supported by thread sim", other)),
    }
}

fn eval_const(e: &Expr) -> u64 {
    match &e.kind {
        ExprKind::Literal(LitKind::Dec(v)) => *v,
        ExprKind::Literal(LitKind::Hex(v)) => *v,
        ExprKind::Literal(LitKind::Bin(v)) => *v,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
        _ => 0,
    }
}

fn uint_cpp_ty(bits: u64) -> Result<String, String> {
    Ok(match bits {
        0 => return Err("UInt<0> not supported".into()),
        1..=8 => "uint8_t".to_string(),
        9..=16 => "uint16_t".to_string(),
        17..=32 => "uint32_t".to_string(),
        33..=64 => "uint64_t".to_string(),
        _ => return Err(format!("UInt<{}> > 64 bits not supported by thread sim", bits)),
    })
}

fn collect_thread_driven_outputs(threads: &[&ThreadBlock], m: &ModuleDecl) -> Vec<String> {
    use std::collections::HashSet;
    let mut out: HashSet<String> = HashSet::new();
    let port_outs: HashSet<&str> = m.ports.iter()
        .filter(|p| p.direction == Direction::Out && p.reg_info.is_none())
        .map(|p| p.name.name.as_str())
        .collect();
    fn walk(stmts: &[ThreadStmt], port_outs: &HashSet<&str>, out: &mut HashSet<String>) {
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(a) => {
                    if let ExprKind::Ident(n) = &a.target.kind {
                        if port_outs.contains(n.as_str()) { out.insert(n.clone()); }
                    }
                }
                ThreadStmt::IfElse(ie) => {
                    walk(&ie.then_stmts, port_outs, out);
                    walk(&ie.else_stmts, port_outs, out);
                }
                ThreadStmt::For { body, .. }
                | ThreadStmt::Lock { body, .. }
                | ThreadStmt::DoUntil { body, .. } => walk(body, port_outs, out),
                ThreadStmt::ForkJoin(branches, _) => {
                    for b in branches { walk(b, port_outs, out); }
                }
                _ => {}
            }
        }
    }
    for t in threads { walk(&t.body, &port_outs, &mut out); }
    let mut v: Vec<String> = out.into_iter().collect();
    v.sort();
    v
}

pub fn arch_thread_rt_h() -> &'static str {
    include_str!("../../runtime/arch_thread_rt.h")
}
