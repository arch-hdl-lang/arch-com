// thread_sim.rs — Pre-lowering thread sim emitter.
//
// Emits C++20 coroutine-based per-module classes that simulate `thread`
// blocks directly, without lowering them to FSMs. See
// doc/plan_thread_parallel_sim.md and runtime/arch_thread_rt.h for
// design + runtime API.
//
// Phase 2 model — segment-based held-comb semantics matching the
// (post-PR-145) lowered-fsm behavior:
//
//   - The thread body is partitioned into SEGMENTS at every `wait`
//     point. Each segment owns:
//       * `entry_seq`  — SeqAssigns that fire ONCE on entry (`<=`)
//       * `hold_comb`  — CombAssigns that HOLD across the wait (`=`)
//       * `wait_kind`  — WaitUntil(cond) | WaitCycles(n) | Terminal
//   - The coroutine sets a per-thread `_seg_<i>` field at each segment
//     boundary, fires SeqAssigns once, and `co_await`s the wait.
//   - `eval()` zeroes thread-driven outputs, then for each thread
//     switches on `_seg_<i>` and re-runs that segment's hold-comb.
//     This re-evaluates each cycle (matching `always_comb`), so
//     CombAssigns track input changes during the wait.
//
// Phase 2 scope:
//   - Multiple threads/module
//   - Scalar Bool/UInt<≤64> ports + regs
//   - Combinational `let` bindings
//   - Thread body: CombAssign, SeqAssign, WaitUntil, WaitCycles,
//     IfElse (no waits inside), `for i in s..e { … }` (no waits inside)
//   - Predicate / expression shapes: idents, literals, !/~, all binops

use crate::ast::{
    BinOp, CombAssign, CombStmt, Direction, Expr, ExprKind, IfElseOf,
    LitKind, ModuleBodyItem, ModuleDecl, RegAssign, ResetLevel,
    ThreadBlock, ThreadStmt, TypeExpr, UnaryOp,
};
use crate::sim_codegen::SimModel;

/// One segment of a thread, demarcated by a `wait` boundary.
struct Segment {
    /// SeqAssigns that fire once on entry (lowered as `=` writes since
    /// arch sim is single-process / immediate-effect).
    entry_seq: Vec<RegAssign>,
    /// IfElse statements (with only SeqAssign children) that fire once
    /// on entry. For Phase 2 we accept pure-seq IfElses as a separate
    /// list to keep the structure simple.
    entry_seq_if: Vec<IfElseOf<ThreadStmt>>,
    /// CombAssigns held while in this segment. Re-evaluated each
    /// `eval()` so they track input changes during the wait.
    hold_comb: Vec<CombStmt>,
    /// Terminating wait. None ⇒ terminal segment (falls off end of
    /// thread body; for non-once threads the while-loop wraps).
    wait_kind: WaitKind,
    /// For loops (no waits inside): emitted as a C++ for around the
    /// segment's await. For Phase 2 we attach the for-loop bounds here
    /// and use the loop body as the segment's content.
    /// (Set when this segment was synthesized by partitioning a
    /// `for` containing a wait.)
    for_loop: Option<ForLoopInfo>,
}

#[derive(Clone)]
enum WaitKind {
    Until(Expr),
    Cycles(Expr),
    Terminal,
}

struct ForLoopInfo {
    var: String,
    start: Expr,
    end: Expr,
}

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

    // Partition each thread body into segments.
    let mut thread_segments: Vec<Vec<Segment>> = Vec::new();
    for (ti, t) in threads.iter().enumerate() {
        let segs = partition(&t.body)
            .map_err(|e| format!("module `{}` thread #{}: {}", class, ti, e))?;
        thread_segments.push(segs);
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

    for p in &m.ports {
        let cpp_ty = port_or_reg_cpp_ty(&p.ty)
            .map_err(|e| format!("module `{}` port `{}`: {}", class, p.name.name, e))?;
        header.push_str(&format!("  {} {} = 0;\n", cpp_ty, p.name.name));
    }
    header.push('\n');

    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            let cpp_ty = port_or_reg_cpp_ty(&r.ty)
                .map_err(|e| format!("module `{}` reg `{}`: {}", class, r.name.name, e))?;
            header.push_str(&format!("  {} {} = 0;\n", cpp_ty, r.name.name));
        }
    }
    header.push('\n');

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

    // eval(): zero thread-driven outputs, run each thread's segment
    // hold-comb, then evaluate combinational lets.
    header.push_str("  void eval() {\n");
    for n in &driven_outputs {
        header.push_str(&format!("    {} = 0;\n", n));
    }
    for (ti, segs) in thread_segments.iter().enumerate() {
        if segs.is_empty() { continue; }
        header.push_str(&format!("    switch (_seg_{ti}) {{\n"));
        for (si, seg) in segs.iter().enumerate() {
            header.push_str(&format!("      case {si}: {{\n"));
            for cs in &seg.hold_comb {
                emit_comb_stmt(cs, &mut header, 8);
            }
            header.push_str("        break;\n");
            header.push_str("      }\n");
        }
        header.push_str("      default: break;\n");
        header.push_str("    }\n");
    }
    for item in &m.body {
        if let ModuleBodyItem::LetBinding(lb) = item {
            let rhs = expr_to_cpp(&lb.value)?;
            header.push_str(&format!("    {} = {};\n", lb.name.name, rhs));
        }
    }
    header.push_str("  }\n\n");

    // posedge handler: reset → recreate; else → tick + eval.
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
    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            header.push_str(&format!("      {} = 0;\n", r.name.name));
        }
    }
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("      _slot_{i}.thread.destroy();\n"));
        header.push_str(&format!("      _slot_{i}.thread = _make_thread_{i}();\n"));
        header.push_str(&format!("      _slot_{i}.kind = arch_rt::WaitKind::Ready;\n"));
        header.push_str(&format!("      _slot_{i}.cycles_remaining = 0;\n"));
        header.push_str(&format!("      _slot_{i}.pred = nullptr;\n"));
        header.push_str(&format!("      _seg_{i} = 0;\n"));
    }
    header.push_str("      eval();\n");
    header.push_str("      return;\n");
    header.push_str("    }\n");
    header.push_str("    _sched.tick();\n");
    header.push_str("    eval();\n");
    header.push_str("  }\n\n");

    let _ = (&clk_name, &rst_name);

    header.push_str("private:\n");
    header.push_str("  arch_rt::ThreadScheduler _sched;\n");
    for (i, _) in threads.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ThreadSlot _slot_{i};\n"));
        header.push_str(&format!("  uint32_t _seg_{i} = 0;\n"));
    }
    header.push('\n');

    // Coroutine bodies.
    for (ti, t) in threads.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ArchThread _make_thread_{ti}() {{\n"));
        let mut body_cpp = String::new();
        let segs = &thread_segments[ti];
        if !t.once {
            body_cpp.push_str("    while (true) {\n");
        }
        for (si, seg) in segs.iter().enumerate() {
            let ind = if !t.once { 6 } else { 4 };
            let pad = " ".repeat(ind);
            // Open for-loop wrapper if this segment is inside a for.
            if let Some(fl) = &seg.for_loop {
                let s = expr_to_cpp(&fl.start)?;
                let e = expr_to_cpp(&fl.end)?;
                body_cpp.push_str(&format!(
                    "{pad}for (uint64_t {v} = {s}; {v} <= {e}; {v}++) {{\n",
                    v = fl.var
                ));
            }
            let pad2 = " ".repeat(if seg.for_loop.is_some() { ind + 2 } else { ind });
            // Set segment id.
            body_cpp.push_str(&format!("{pad2}_seg_{ti} = {si};\n"));
            // Fire entry SeqAssigns once.
            for sa in &seg.entry_seq {
                let lhs = expr_to_cpp(&sa.target)?;
                let rhs = expr_to_cpp(&sa.value)?;
                body_cpp.push_str(&format!("{pad2}{lhs} = {rhs};\n"));
            }
            for ie in &seg.entry_seq_if {
                emit_seq_if(ie, &mut body_cpp, ind + if seg.for_loop.is_some() {2} else {0})?;
            }
            // Wait.
            match &seg.wait_kind {
                WaitKind::Until(cond) => {
                    let pred = expr_to_cpp_bool(cond)?;
                    body_cpp.push_str(&format!(
                        "{pad2}co_await arch_rt::wait_until(&_slot_{ti}, [this]{{ return {pred}; }});\n"
                    ));
                }
                WaitKind::Cycles(n) => {
                    let n_str = match &n.kind {
                        ExprKind::Literal(LitKind::Dec(v)) => format!("{}", v),
                        ExprKind::Literal(LitKind::Sized(_, v)) => format!("{}", v),
                        _ => return Err("wait <N> cycle: only literal N supported".into()),
                    };
                    body_cpp.push_str(&format!(
                        "{pad2}co_await arch_rt::wait_cycles(&_slot_{ti}, {n_str});\n"
                    ));
                }
                WaitKind::Terminal => {
                    // Terminal segment: yield 1 cycle so the loop wraps
                    // cleanly without busy-looping.
                    body_cpp.push_str(&format!(
                        "{pad2}co_await arch_rt::wait_cycles(&_slot_{ti}, 1);\n"
                    ));
                }
            }
            if seg.for_loop.is_some() {
                let pad_close = " ".repeat(ind);
                body_cpp.push_str(&format!("{pad_close}}}\n"));
            }
        }
        if !t.once {
            body_cpp.push_str("    }\n");
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

// Walk thread body and partition into segments at each wait point.
fn partition(body: &[ThreadStmt]) -> Result<Vec<Segment>, String> {
    let mut segs: Vec<Segment> = Vec::new();
    let mut cur = new_segment();

    for s in body {
        match s {
            ThreadStmt::CombAssign(a) => {
                cur.hold_comb.push(CombStmt::Assign(a.clone()));
            }
            ThreadStmt::SeqAssign(a) => {
                cur.entry_seq.push(a.clone());
            }
            ThreadStmt::IfElse(ie) => {
                if contains_wait(&ie.then_stmts) || contains_wait(&ie.else_stmts) {
                    return Err("`if/else` containing `wait` not yet supported by thread sim".into());
                }
                let kind = classify_ifelse(ie);
                match kind {
                    IfKind::PureComb => {
                        let comb_ie = lower_thread_ifelse_to_comb(ie)?;
                        cur.hold_comb.push(CombStmt::IfElse(comb_ie));
                    }
                    IfKind::PureSeq => {
                        cur.entry_seq_if.push(ie.clone());
                    }
                    IfKind::Mixed => {
                        return Err("mixed CombAssign + SeqAssign inside `if/else` not yet supported".into());
                    }
                    IfKind::Empty => {}
                }
            }
            ThreadStmt::WaitUntil(cond, _) => {
                cur.wait_kind = WaitKind::Until(cond.clone());
                segs.push(std::mem::replace(&mut cur, new_segment()));
            }
            ThreadStmt::WaitCycles(n, _) => {
                cur.wait_kind = WaitKind::Cycles(n.clone());
                segs.push(std::mem::replace(&mut cur, new_segment()));
            }
            ThreadStmt::For { var, start, end, body, .. } => {
                if !contains_wait(body) {
                    return Err("`for` without internal `wait` is unusual; use `for` in comb instead. \
                        Phase 2 thread-sim only handles `for` containing a wait.".into());
                }
                // Flush any pending pre-loop hold/seq into a 1-cycle yield
                // segment if non-empty (so the held outputs get a cycle to
                // settle before the loop). For Phase 2, error if pending.
                if !cur.hold_comb.is_empty() || !cur.entry_seq.is_empty() {
                    return Err("`for` preceded by un-flushed comb/seq assigns not yet supported \
                        (insert a `wait` before the for loop)".into());
                }
                // Partition the loop body — Phase 2 supports a body with
                // exactly one wait (like BurstRead). The loop body is then
                // a single segment that gets wrapped in a C++ for loop.
                let inner = partition(body)?;
                if inner.len() != 1 {
                    return Err("`for` body must contain exactly one wait segment in Phase 2".into());
                }
                let mut inner_seg = inner.into_iter().next().unwrap();
                inner_seg.for_loop = Some(ForLoopInfo {
                    var: var.name.clone(),
                    start: start.clone(),
                    end: end.clone(),
                });
                segs.push(inner_seg);
            }
            other => return Err(format!("thread stmt not yet supported: {:?}", std::mem::discriminant(other))),
        }
    }
    // Trailing segment: terminal yield (if non-empty).
    if !cur.hold_comb.is_empty() || !cur.entry_seq.is_empty() || !cur.entry_seq_if.is_empty() {
        cur.wait_kind = WaitKind::Terminal;
        segs.push(cur);
    }
    if segs.is_empty() {
        return Err("thread body has no statements".into());
    }
    Ok(segs)
}

fn new_segment() -> Segment {
    Segment {
        entry_seq: Vec::new(),
        entry_seq_if: Vec::new(),
        hold_comb: Vec::new(),
        wait_kind: WaitKind::Terminal,
        for_loop: None,
    }
}

enum IfKind { PureComb, PureSeq, Mixed, Empty }

fn classify_ifelse(ie: &IfElseOf<ThreadStmt>) -> IfKind {
    let (mut has_comb, mut has_seq) = (false, false);
    fn walk(stmts: &[ThreadStmt], hc: &mut bool, hs: &mut bool) {
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(_) => *hc = true,
                ThreadStmt::SeqAssign(_) => *hs = true,
                ThreadStmt::IfElse(ie) => {
                    walk(&ie.then_stmts, hc, hs);
                    walk(&ie.else_stmts, hc, hs);
                }
                _ => {}
            }
        }
    }
    walk(&ie.then_stmts, &mut has_comb, &mut has_seq);
    walk(&ie.else_stmts, &mut has_comb, &mut has_seq);
    match (has_comb, has_seq) {
        (true, false) => IfKind::PureComb,
        (false, true) => IfKind::PureSeq,
        (true, true) => IfKind::Mixed,
        (false, false) => IfKind::Empty,
    }
}

fn lower_thread_ifelse_to_comb(ie: &IfElseOf<ThreadStmt>) -> Result<IfElseOf<CombStmt>, String> {
    fn lower_stmts(stmts: &[ThreadStmt]) -> Result<Vec<CombStmt>, String> {
        let mut out = Vec::new();
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(a) => out.push(CombStmt::Assign(a.clone())),
                ThreadStmt::IfElse(inner) => {
                    out.push(CombStmt::IfElse(lower_thread_ifelse_to_comb(inner)?));
                }
                _ => return Err("non-comb stmt inside pure-comb IfElse (shouldn't happen)".into()),
            }
        }
        Ok(out)
    }
    Ok(IfElseOf {
        cond: ie.cond.clone(),
        then_stmts: lower_stmts(&ie.then_stmts)?,
        else_stmts: lower_stmts(&ie.else_stmts)?,
        unique: ie.unique,
        span: ie.span,
    })
}

fn emit_comb_stmt(cs: &CombStmt, out: &mut String, indent: usize) {
    let pad = " ".repeat(indent);
    match cs {
        CombStmt::Assign(a) => {
            let lhs = expr_to_cpp(&a.target).unwrap_or_else(|e| format!("/* err: {e} */"));
            let rhs = expr_to_cpp(&a.value).unwrap_or_else(|e| format!("/* err: {e} */"));
            out.push_str(&format!("{pad}{lhs} = {rhs};\n"));
        }
        CombStmt::IfElse(ie) => {
            let cond = expr_to_cpp_bool(&ie.cond).unwrap_or_else(|e| format!("/* err: {e} */"));
            out.push_str(&format!("{pad}if ({cond}) {{\n"));
            for s in &ie.then_stmts { emit_comb_stmt(s, out, indent + 2); }
            if !ie.else_stmts.is_empty() {
                out.push_str(&format!("{pad}}} else {{\n"));
                for s in &ie.else_stmts { emit_comb_stmt(s, out, indent + 2); }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        _ => out.push_str(&format!("{pad}/* unsupported CombStmt */\n")),
    }
}

fn emit_seq_if(ie: &IfElseOf<ThreadStmt>, out: &mut String, indent: usize) -> Result<(), String> {
    let pad = " ".repeat(indent);
    let cond = expr_to_cpp_bool(&ie.cond)?;
    out.push_str(&format!("{pad}if ({cond}) {{\n"));
    for s in &ie.then_stmts {
        if let ThreadStmt::SeqAssign(a) = s {
            let lhs = expr_to_cpp(&a.target)?;
            let rhs = expr_to_cpp(&a.value)?;
            out.push_str(&format!("{pad}  {lhs} = {rhs};\n"));
        } else if let ThreadStmt::IfElse(inner) = s {
            emit_seq_if(inner, out, indent + 2)?;
        }
    }
    if !ie.else_stmts.is_empty() {
        out.push_str(&format!("{pad}}} else {{\n"));
        for s in &ie.else_stmts {
            if let ThreadStmt::SeqAssign(a) = s {
                let lhs = expr_to_cpp(&a.target)?;
                let rhs = expr_to_cpp(&a.value)?;
                out.push_str(&format!("{pad}  {lhs} = {rhs};\n"));
            } else if let ThreadStmt::IfElse(inner) = s {
                emit_seq_if(inner, out, indent + 2)?;
            }
        }
    }
    out.push_str(&format!("{pad}}}\n"));
    Ok(())
}

fn contains_wait(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::WaitUntil(..) | ThreadStmt::WaitCycles(..) => true,
        ThreadStmt::IfElse(ie) => contains_wait(&ie.then_stmts) || contains_wait(&ie.else_stmts),
        ThreadStmt::For { body, .. } => contains_wait(body),
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => contains_wait(body),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|b| contains_wait(b)),
        _ => false,
    })
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
                _ => return Err(format!("binop {:?} not yet supported", op)),
            };
            Ok(format!("({} {} {})", l, op_str, r))
        }
        _ => Err(format!("expr shape not supported: {:?}", std::mem::discriminant(&e.kind))),
    }
}

fn expr_to_cpp_bool(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(format!("({} != 0)", n)),
        ExprKind::Bool(true) => Ok("true".into()),
        ExprKind::Bool(false) => Ok("false".into()),
        ExprKind::Unary(UnaryOp::Not, inner) => Ok(format!("!({})", expr_to_cpp_bool(inner)?)),
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
        other => Err(format!("type {:?} not supported", other)),
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
        _ => return Err(format!("UInt<{}> > 64 bits not supported", bits)),
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

// Suppress unused-import warnings while CombAssign/RegAssign are
// referenced only via paths.
#[allow(dead_code)]
fn _unused(_a: &CombAssign, _r: &RegAssign) {}

pub fn arch_thread_rt_h() -> &'static str {
    include_str!("../../runtime/arch_thread_rt.h")
}
