// thread_sim.rs — Pre-lowering thread sim emitter (Phase 1 spike).
//
// Emits C++20 coroutine-based per-module classes that simulate `thread`
// blocks directly, without lowering them to FSMs. See
// doc/plan_thread_parallel_sim.md and runtime/arch_thread_rt.h for
// design + runtime API.
//
// Scope (Phase 1): handles the spike target — single thread per module,
// scalar Bool/UInt ports, `wait until <ident>` predicates, `wait <const>
// cycle`, plain port assignments. Anything else returns an error so the
// caller can fall back to the lowered-fsm path or report unsupported.

use crate::ast::{
    Direction, Expr, ExprKind, LitKind, ModuleBodyItem, ModuleDecl,
    ResetLevel, ThreadBlock, ThreadStmt, TypeExpr, UnaryOp,
};
use crate::sim_codegen::SimModel;

pub fn gen_module_thread(m: &ModuleDecl) -> Result<SimModel, String> {
    let class = m.name.name.clone();

    // Collect threads and reject unsupported features for Phase 1.
    let threads: Vec<&ThreadBlock> = m.body.iter().filter_map(|i| match i {
        ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).collect();
    if threads.is_empty() {
        return Err(format!("module `{}` has no thread blocks", class));
    }
    if threads.len() > 1 {
        return Err(format!(
            "module `{}` has {} thread blocks; Phase 1 spike supports 1 thread/module",
            class, threads.len()
        ));
    }
    let t = threads[0];
    if t.tlm_target.is_some() || t.implement.is_some() || t.reentrant.is_some() {
        return Err(format!("module `{}`: TLM/implement/reentrant threads not yet supported", class));
    }
    if t.default_when.is_some() {
        return Err(format!("module `{}`: thread `default when` not yet supported", class));
    }

    // Reject non-thread top-level items the spike doesn't handle yet.
    for item in &m.body {
        match item {
            ModuleBodyItem::Thread(_) => {}
            ModuleBodyItem::RegDecl(_) => {} // accepted
            ModuleBodyItem::LetBinding(_) => {} // accepted (assigned in eval)
            _ => return Err(format!(
                "module `{}`: thread sim Phase 1 only supports `thread` + `reg` + `let` items",
                class
            )),
        }
    }

    let mut header = String::new();
    header.push_str("#pragma once\n");
    header.push_str("#include \"arch_thread_rt.h\"\n");
    header.push_str("#include <cstdint>\n");
    header.push_str("#include \"verilated.h\"\n\n");
    header.push_str(&format!("class {} {{\npublic:\n", class));

    // Identify clock and reset ports.
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

    // Emit port fields (scalar Bool/UInt only).
    for p in &m.ports {
        let cpp_ty = match &p.ty {
            TypeExpr::Clock(_) | TypeExpr::Reset(..) | TypeExpr::Bool | TypeExpr::Bit => "uint8_t".to_string(),
            TypeExpr::UInt(w) => uint_cpp_ty(eval_const(w))?,
            other => return Err(format!(
                "module `{}` port `{}`: type {:?} not supported by thread sim Phase 1",
                class, p.name.name, other
            )),
        };
        header.push_str(&format!("  {} {} = 0;\n", cpp_ty, p.name.name));
    }
    header.push('\n');

    // Emit reg fields (scalar UInt/Bool).
    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            let cpp_ty = match &r.ty {
                TypeExpr::Bool | TypeExpr::Bit => "uint8_t".to_string(),
                TypeExpr::UInt(w) => uint_cpp_ty(eval_const(w))?,
                other => return Err(format!(
                    "module `{}` reg `{}`: type {:?} not supported",
                    class, r.name.name, other
                )),
            };
            header.push_str(&format!("  {} {} = 0;\n", cpp_ty, r.name.name));
        }
    }
    header.push('\n');

    // Identify which non-reg ports the thread writes — those get
    // zeroed at the start of each tick (state-local comb semantic).
    let driven_outputs = collect_thread_driven_outputs(t, m);

    // Constructor + lifecycle.
    header.push_str(&format!("  {}() {{\n", class));
    header.push_str("    _slot.thread = make_thread();\n");
    header.push_str("    _sched.slots.push_back(&_slot);\n");
    header.push_str("  }\n");
    header.push_str(&format!("  ~{}() {{ _slot.thread.destroy(); }}\n\n", class));

    // eval(): combinational settle. For modules with `let` bindings we
    // evaluate them here. Phase 1 supports trivial lets only.
    header.push_str("  void eval() {\n");
    for item in &m.body {
        if let ModuleBodyItem::LetBinding(lb) = item {
            let rhs = expr_to_cpp(&lb.value)?;
            header.push_str(&format!("    {} = {};\n", lb.name.name, rhs));
        }
    }
    header.push_str("  }\n\n");

    // posedge handler: reset → recreate coroutine; else → default-zero
    // outputs + tick scheduler.
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
    header.push_str("      _slot.thread.destroy();\n");
    header.push_str("      _slot.thread = make_thread();\n");
    header.push_str("      _slot.kind = arch_rt::WaitKind::Ready;\n");
    header.push_str("      _slot.cycles_remaining = 0;\n");
    header.push_str("      _slot.pred = nullptr;\n");
    header.push_str("      return;\n");
    header.push_str("    }\n");
    // State-local comb default for thread-driven non-reg outputs.
    for n in &driven_outputs {
        header.push_str(&format!("    {} = 0;\n", n));
    }
    header.push_str("    _sched.tick();\n");
    header.push_str("  }\n\n");

    // Suppress unused-port warnings (clk/rst/inputs not read by host).
    let _ = (&clk_name, &rst_name);

    header.push_str("private:\n");
    header.push_str("  arch_rt::ThreadScheduler _sched;\n");
    header.push_str("  arch_rt::ThreadSlot      _slot;\n\n");

    // Coroutine body.
    header.push_str("  arch_rt::ArchThread make_thread() {\n");
    let mut body_cpp = String::new();
    emit_thread_body(&t.body, &mut body_cpp, 4)?;
    header.push_str(&body_cpp);
    header.push_str("    co_return;\n");
    header.push_str("  }\n");

    header.push_str("};\n");

    // No separate .cpp; everything is inline header for the spike.
    Ok(SimModel {
        class_name: class.clone(),
        header,
        impl_: format!("// {} thread-sim: header-only (Phase 1 spike)\n", class),
    })
}

fn emit_thread_body(stmts: &[ThreadStmt], out: &mut String, indent: usize) -> Result<(), String> {
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
                    "{}co_await arch_rt::wait_until(&_slot, [this]{{ return {}; }});\n",
                    pad, pred
                ));
            }
            ThreadStmt::WaitCycles(n, _) => {
                let n_str = match &n.kind {
                    ExprKind::Literal(LitKind::Dec(v)) => format!("{}", v),
                    ExprKind::Literal(LitKind::Sized(_, v)) => format!("{}", v),
                    _ => return Err("wait <N> cycle: Phase 1 supports only literal N".into()),
                };
                out.push_str(&format!("{}co_await arch_rt::wait_cycles(&_slot, {});\n", pad, n_str));
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
        ExprKind::Unary(UnaryOp::Not, inner) => Ok(format!("!({})", expr_to_cpp(inner)?)),
        _ => Err(format!("expr shape not supported by thread sim Phase 1: {:?}", std::mem::discriminant(&e.kind))),
    }
}

// Predicate context: coerce ident/expr to a C++ bool comparison.
fn expr_to_cpp_bool(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(format!("{} != 0", n)),
        ExprKind::Bool(true) => Ok("true".into()),
        ExprKind::Bool(false) => Ok("false".into()),
        ExprKind::Unary(UnaryOp::Not, inner) => Ok(format!("!({})", expr_to_cpp_bool(inner)?)),
        _ => expr_to_cpp(e),
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
        _ => return Err(format!("UInt<{}> > 64 bits not supported by thread sim Phase 1", bits)),
    })
}

fn collect_thread_driven_outputs(t: &ThreadBlock, m: &ModuleDecl) -> Vec<String> {
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
                ThreadStmt::For { body, .. } => walk(body, port_outs, out),
                ThreadStmt::Lock { body, .. } => walk(body, port_outs, out),
                ThreadStmt::DoUntil { body, .. } => walk(body, port_outs, out),
                ThreadStmt::ForkJoin(branches, _) => {
                    for b in branches { walk(b, port_outs, out); }
                }
                _ => {}
            }
        }
    }
    walk(&t.body, &port_outs, &mut out);
    let mut v: Vec<String> = out.into_iter().collect();
    v.sort();
    v
}

pub fn arch_thread_rt_h() -> &'static str {
    include_str!("../../runtime/arch_thread_rt.h")
}
