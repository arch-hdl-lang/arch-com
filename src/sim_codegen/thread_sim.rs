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
//   - Scalar Bool/UInt ports + regs (wide UInt/SInt up to 1024 bits use VlWide)
//   - Combinational `let` bindings
//   - Thread body: CombAssign, SeqAssign, WaitUntil, WaitCycles,
//     IfElse (no waits inside), `for i in s..e { … }` (no waits inside)
//   - Predicate / expression shapes: idents, literals, !/~, all binops

use crate::ast::{
    ArbiterPolicy, BinOp, CombAssign, ForRange, Stmt, Direction, Expr, ExprKind, IfElseOf,
    LitKind, ModuleBodyItem, ModuleDecl, RegAssign, ResetLevel,
    ParamDecl, ThreadBlock, ThreadStmt, TypeExpr, UnaryOp,
};
use crate::diagnostics::CompileWarning;
use crate::sim_codegen::SimModel;
use std::collections::HashSet;

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
    hold_comb: Vec<Stmt>,
    /// SeqAssigns / IfElses-of-SeqAssigns that fire ONCE when the wait
    /// completes (i.e., right after the co_await returns). Used by
    /// `do { ... } until cond;` where the body's seq updates fire on
    /// the cycle the until-cond becomes true.
    post_wait_seq: Vec<RegAssign>,
    post_wait_seq_if: Vec<IfElseOf<ThreadStmt>>,
    /// Terminating wait. None ⇒ terminal segment (falls off end of
    /// thread body; for non-once threads the while-loop wraps).
    wait_kind: WaitKind,
    /// If Some(resource_name): release the resource after this
    /// segment's wait completes (set holder back to -1). Used by the
    /// `lock` exit to release the resource at the end of the lock body.
    release_lock: Option<String>,
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
    /// Fork/join segment: launch the listed branches, then wait for
    /// all of them to reach `Done`. Branch indices are into
    /// ThreadInfo.branches.
    ForkJoin(Vec<usize>),
    /// Wait until the named resource's holder is either free (-1) or
    /// already this thread (priority-arbitrated; lower thread index
    /// wins ties because pass 2 of the scheduler resumes slots in
    /// declaration order). On resume the coroutine claims the holder.
    LockAcquire(String /* resource name */, usize /* this thread index */),
}

struct ForLoopInfo {
    var: String,
    start: Expr,
    end: Expr,
}

/// One fork branch with its own segment list.
struct Branch {
    /// Per-thread branch index (used to name slot/seg fields).
    id: usize,
    segs: Vec<Segment>,
}

/// Top-level partition output for one thread.
struct ThreadInfo {
    main_segs: Vec<Segment>,
    branches: Vec<Branch>,
}

pub fn gen_module_thread(m: &ModuleDecl, debug: bool, wave: bool, num_os_threads: u32) -> Result<SimModel, String> {
    let mut sink = Vec::new();
    gen_module_thread_with_warnings(m, debug, wave, num_os_threads, &mut sink)
}

/// Same as `gen_module_thread`, but routes thread-sim-specific warnings
/// into `warnings` instead of dropping them.
/// The main entry (`gen_module_thread`) forwards to this with a throwaway
/// sink so existing callers keep working unchanged.
pub fn gen_module_thread_with_warnings(
    m: &ModuleDecl,
    debug: bool,
    wave: bool,
    num_os_threads: u32,
    _warnings: &mut Vec<CompileWarning>,
) -> Result<SimModel, String> {
    // Match the Verilator/fsm convention: V<ModuleName>. Lets the same
    // TB drive either --thread-sim path without changing #includes,
    // which is what makes --thread-sim=both cross-check practical.
    let class = format!("V{}", m.name.name);

    let threads: Vec<&ThreadBlock> = m.body.iter().filter_map(|i| match i {
        ModuleBodyItem::Thread(t) => Some(t),
        _ => None,
    }).collect();
    if threads.is_empty() {
        return Err(format!("module `{}` has no thread blocks", class));
    }
    validate_wide_thread_sim_exprs(m, &class)?;
    for (i, t) in threads.iter().enumerate() {
        if t.tlm_target.is_some() || t.implement.is_some() {
            return Err(format!("module `{}` thread #{}: TLM/implement not yet supported", class, i));
        }
    }

    for item in &m.body {
        match item {
            ModuleBodyItem::Thread(_)
            | ModuleBodyItem::RegDecl(_)
            | ModuleBodyItem::LetBinding(_)
            | ModuleBodyItem::CombBlock(_)
            | ModuleBodyItem::RegBlock(_)
            | ModuleBodyItem::Resource(_) => {}
            _ => return Err(format!(
                "module `{}`: thread sim does not yet support this body item kind",
                class
            )),
        }
    }
    // Partition each thread body into segments + collected branches.
    let mut thread_infos: Vec<ThreadInfo> = Vec::new();
    for (ti, t) in threads.iter().enumerate() {
        let mut branches: Vec<Branch> = Vec::new();
        let main_segs = partition(&t.body, &mut branches, ti)
            .map_err(|e| format!("module `{}` thread #{}: {}", class, ti, e))?;
        thread_infos.push(ThreadInfo { main_segs, branches });
    }
    let resource_count = m.body.iter()
        .filter(|item| matches!(item, ModuleBodyItem::Resource(_)))
        .count();
    if resource_count > 0 && threads.len() > 64 {
        return Err(format!(
            "module `{}`: thread sim resource mutexes support at most 64 user threads (got {})",
            class,
            threads.len()
        ));
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
    if m.body.iter().any(|item| matches!(item, ModuleBodyItem::Resource(r) if matches!(r.policy, ArbiterPolicy::Custom(_)))) {
        header.push_str("#include \"VFunctions.h\"\n");
    }
    header.push_str("#include \"verilated.h\"\n\n");
    header.push_str(&format!("class {} {{\npublic:\n", class));

    for p in &m.params {
        if let Some(def) = &p.default {
            let val = eval_const_with_params(def, &m.params);
            header.push_str(&format!("  static constexpr uint64_t {} = {}ULL;\n", p.name.name, val));
        }
    }
    if !m.params.is_empty() {
        header.push('\n');
    }

    for p in &m.ports {
        let cpp_ty = port_or_reg_cpp_ty_with_params(&p.ty, &m.params)
            .map_err(|e| format!("module `{}` port `{}`: {}", class, p.name.name, e))?;
        header.push_str(&format!("  {}\n", field_decl(&cpp_ty, &p.name.name)));
    }
    header.push('\n');

    let mut vec_reg_info: Vec<(String, String, u64)> = Vec::new(); // (name, elem_ty, count)
    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            if let TypeExpr::Vec(elem, count_expr) = &r.ty {
                let elem_ty = port_or_reg_cpp_ty_with_params(elem, &m.params)
                    .map_err(|e| format!("module `{}` reg `{}` element: {}", class, r.name.name, e))?;
                let count = eval_const_with_params(count_expr, &m.params);
                if count == 0 {
                    return Err(format!("module `{}` reg `{}`: Vec count = 0 (param resolution not yet supported)", class, r.name.name));
                }
                header.push_str(&format!("  {} {}[{}] = {{}};\n", elem_ty, r.name.name, count));
                vec_reg_info.push((r.name.name.clone(), elem_ty, count));
            } else {
                let cpp_ty = port_or_reg_cpp_ty_with_params(&r.ty, &m.params)
                    .map_err(|e| format!("module `{}` reg `{}`: {}", class, r.name.name, e))?;
                header.push_str(&format!("  {}\n", field_decl(&cpp_ty, &r.name.name)));
            }
        }
    }
    // Let-binding fields: declare any let whose name isn't already a
    // port (lets aliasing a port skip declaration — eval() just writes
    // to the port). Without this, eval()'s `<let_name> = expr;`
    // assignments fail to compile when an out-of-line method body
    // (e.g. trace_open under --wave) actually exercises the .h.
    let port_names_set: std::collections::HashSet<&str> =
        m.ports.iter().map(|p| p.name.name.as_str()).collect();
    for item in &m.body {
        if let ModuleBodyItem::LetBinding(lb) = item {
            if port_names_set.contains(lb.name.name.as_str()) { continue; }
            // For typed lets, infer width; for untyped (target=port),
            // the previous filter already skipped them.
            let cpp_ty = match &lb.ty {
                Some(t) => port_or_reg_cpp_ty_with_params(t, &m.params)
                    .map_err(|e| format!("module `{}` let `{}`: {}", class, lb.name.name, e))?,
                None => continue, // untyped lets must alias a port — handled by the filter above
            };
            header.push_str(&format!("  {}\n", field_decl(&cpp_ty, &lb.name.name)));
        }
    }
    header.push('\n');

    let driven_outputs = collect_thread_driven_outputs(&threads, m);

    // Constructor: register one slot per thread coroutine, then run an
    // initial scheduler tick to advance every thread past its entry
    // wait. Without this, parallel sim takes 1 cycle longer than fsm
    // to "warm up" (parent coroutine wouldn't run its fork-launch
    // until first posedge, branches wouldn't reach their first wait
    // until the cycle after that). The initial tick doesn't increment
    // _dbg_cycle (cycle counter lives in eval()) so cycle alignment
    // matches fsm's "state register starts at entry state" semantic.
    let mt = num_os_threads > 1;
    header.push_str(&format!("  {}() {{\n", class));
    for (i, info) in thread_infos.iter().enumerate() {
        header.push_str(&format!("    _slot_{i}.thread = _make_thread_{i}();\n"));
        header.push_str(&format!("    _sched_{i}.slots.push_back(&_slot_{i});\n"));
        for br in &info.branches {
            // Branch slot starts in Done so the scheduler skips it
            // until the parent's fork-launch resets it.
            header.push_str(&format!("    _t{i}_br{}_slot.kind = arch_rt::WaitKind::Done;\n", br.id));
            header.push_str(&format!("    _sched_{i}.slots.push_back(&_t{i}_br{}_slot);\n", br.id));
        }
    }
    // Initial settle — run each per-thread scheduler in declaration
    // order so thread N can read signals set by threads 0..N-1 during
    // their entry segments. ALWAYS sequential in the constructor (workers
    // not yet spawned).
    for (i, _) in thread_infos.iter().enumerate() {
        header.push_str(&format!("    _sched_{i}.tick();\n"));
    }
    if mt {
        // Spawn one worker OS thread per user thread. Each worker waits
        // at _start_barrier, ticks its scheduler when signaled, then
        // signals _end_barrier. The main TB-driving thread (caller of
        // eval()) coordinates by hitting both barriers per posedge.
        for (i, _) in thread_infos.iter().enumerate() {
            header.push_str(&format!("    _worker_{i} = std::thread([this]{{ _worker_loop_{i}(); }});\n"));
        }
    }
    header.push_str("  }\n");
    header.push_str(&format!("  ~{}() {{\n", class));
    if mt {
        // Wake workers via _start_barrier so they observe _shutdown=true
        // and break out of their loops, then join them.
        header.push_str("    _shutdown.store(true, std::memory_order_release);\n");
        header.push_str("    _start_barrier.wait();\n");
        for (i, _) in thread_infos.iter().enumerate() {
            header.push_str(&format!("    if (_worker_{i}.joinable()) _worker_{i}.join();\n"));
        }
    }
    for (i, info) in thread_infos.iter().enumerate() {
        header.push_str(&format!("    _slot_{i}.thread.destroy();\n"));
        for br in &info.branches {
            header.push_str(&format!("    _t{i}_br{}_slot.thread.destroy();\n", br.id));
        }
    }
    header.push_str("  }\n\n");

    // eval(): zero thread-driven outputs, run each thread's segment
    // hold-comb, then evaluate combinational lets.
    header.push_str("  void eval() {\n");
    if wave {
        // Auto-open VCD on first eval if Verilated::traceFile() is set
        // (TB convention: arch sim --wave out.vcd passes +trace+out.vcd
        // which the verilated.cpp stub captures into traceFile()).
        header.push_str("    if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        header.push_str("      trace_open(Verilated::traceFile());\n");
    }
    // Detect rising edge of clk (Verilator convention) and run the
    // posedge logic. Update _clk_prev BEFORE the dispatch so the
    // recursive eval() call inside _do_posedge doesn't re-trigger.
    header.push_str(&format!("    bool _rising = ({clk_name} && !_clk_prev);\n"));
    header.push_str(&format!("    _clk_prev = {clk_name};\n"));
    header.push_str("    if (_rising) {\n");
    header.push_str("      _do_posedge();\n");
    if debug {
        // Match fsm sim's --debug pattern: log FIRST with current
        // cycle counter, THEN increment. So a port change logged on
        // the Nth rising edge reports cycle N (counting from 0 at
        // the initial pre-clock eval).
        header.push_str("      _dbg_log_ports();\n");
        header.push_str("      _dbg_cycle++;\n");
    }
    if wave {
        header.push_str("      if (_trace_fp) trace_dump(_trace_time++);\n");
    }
    header.push_str("      return;\n");  // _do_posedge settled comb via its own eval() at the end
    header.push_str("    }\n");
    // Comb-only eval (clk falling or steady) — falls through to the
    // segment switches and module-level comb below, then logs at end.
    for n in &driven_outputs {
        header.push_str(&format!("    {} = 0;\n", n));
    }
    for (ti, info) in thread_infos.iter().enumerate() {
        if !info.main_segs.is_empty() {
            header.push_str(&format!("    switch (_seg_{ti}) {{\n"));
            for (si, seg) in info.main_segs.iter().enumerate() {
                header.push_str(&format!("      case {si}: {{\n"));
                for cs in &seg.hold_comb {
                    emit_comb_stmt(cs, &mut header, 8)?;
                }
                header.push_str("        break;\n");
                header.push_str("      }\n");
            }
            header.push_str("      default: break;\n");
            header.push_str("    }\n");
        }
        // Per-fork-branch segment switches; only contribute when the
        // branch slot isn't Done (avoids stale segment outputs after
        // join completes).
        for br in &info.branches {
            header.push_str(&format!(
                "    if (_t{ti}_br{}_slot.kind != arch_rt::WaitKind::Done) {{\n",
                br.id
            ));
            header.push_str(&format!("      switch (_t{ti}_br{}_seg) {{\n", br.id));
            for (si, seg) in br.segs.iter().enumerate() {
                header.push_str(&format!("        case {si}: {{\n"));
                for cs in &seg.hold_comb {
                    emit_comb_stmt(cs, &mut header, 10)?;
                }
                header.push_str("          break;\n");
                header.push_str("        }\n");
            }
            header.push_str("        default: break;\n");
            header.push_str("      }\n");
            header.push_str("    }\n");
        }
    }
    for item in &m.body {
        if let ModuleBodyItem::LetBinding(lb) = item {
            let rhs = expr_to_cpp(&lb.value)?;
            header.push_str(&format!("    {} = {};\n", lb.name.name, rhs));
        }
    }
    // Module-level `comb` blocks run every eval() (after thread segment
    // hold-comb but the order shouldn't matter for non-overlapping
    // assignments — both are "always_comb" semantics).
    for item in &m.body {
        if let ModuleBodyItem::CombBlock(cb) = item {
            for cs in &cb.stmts {
                emit_comb_stmt(cs, &mut header, 4)?;
            }
        }
    }
    if debug {
        header.push_str("    _dbg_log_ports();\n");
    }
    if wave {
        header.push_str("    if (_trace_fp) trace_dump(_trace_time++);\n");
    }
    header.push_str("  }\n\n");

    // posedge handler: reset → recreate; else → tick + eval.
    // Posedge handler — called from eval() on rising edge of clk.
    // Kept as a private method for internal call; previously was
    // public posedge_<clk>() and called explicitly by TB.
    header.push_str("  void _do_posedge() {\n");
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
            if let TypeExpr::Vec(_, count_expr) = &r.ty {
                let count = eval_const_with_params(count_expr, &m.params);
                header.push_str(&format!("      for (uint64_t _i = 0; _i < {count}; _i++) {} [_i] = 0;\n", r.name.name));
            } else {
                header.push_str(&format!("      {} = 0;\n", r.name.name));
            }
        }
    }
    for (i, info) in thread_infos.iter().enumerate() {
        header.push_str(&format!("      _slot_{i}.thread.destroy();\n"));
        header.push_str(&format!("      _slot_{i}.thread = _make_thread_{i}();\n"));
        header.push_str(&format!("      _slot_{i}.kind = arch_rt::WaitKind::Ready;\n"));
        header.push_str(&format!("      _slot_{i}.cycles_remaining = 0;\n"));
        header.push_str(&format!("      _slot_{i}.pred = nullptr;\n"));
        header.push_str(&format!("      _seg_{i} = 0;\n"));
        // Branch slots return to Done so they don't run until next fork-launch.
        for br in &info.branches {
            header.push_str(&format!("      _t{i}_br{}_slot.thread.destroy();\n", br.id));
            header.push_str(&format!("      _t{i}_br{}_slot.kind = arch_rt::WaitKind::Done;\n", br.id));
            header.push_str(&format!("      _t{i}_br{}_slot.cycles_remaining = 0;\n", br.id));
            header.push_str(&format!("      _t{i}_br{}_slot.pred = nullptr;\n", br.id));
            header.push_str(&format!("      _t{i}_br{}_seg = 0;\n", br.id));
        }
    }
    // Resource arbitration state back to reset values.
    for item in &m.body {
        if let ModuleBodyItem::Resource(r) = item {
            emit_resource_reset(&mut header, r, &m.params, threads.len(), "      ");
        }
    }
    // Initial settle after reset: same logic as constructor — advances
    // every thread past its entry wait so post-reset eval() shows the
    // initial state's hold-comb (matching fsm's "state register reset
    // to entry state, comb runs immediately" semantic).
    for (i, _) in thread_infos.iter().enumerate() {
        header.push_str(&format!("      _sched_{i}.tick();\n"));
    }
    header.push_str("      eval();\n");
    header.push_str("      return;\n");
    header.push_str("    }\n");
    // `default when <cond>` clauses (priority soft-reset per thread):
    // checked AFTER hard reset and BEFORE the scheduler tick. When the
    // condition is true, fire the clause's seq assigns and reset the
    // thread's coroutine to its entry segment — same shape as the
    // lowered-fsm wrapping behavior in elaborate.rs.
    for (ti, t) in threads.iter().enumerate() {
        if let Some((dw_cond, dw_stmts)) = &t.default_when {
            let cond_cpp = expr_to_cpp_bool(dw_cond)?;
            header.push_str(&format!("    if ({cond_cpp}) {{\n"));
            for s in dw_stmts {
                if let ThreadStmt::SeqAssign(a) = s {
                    let lhs = expr_to_cpp(&a.target)?;
                    let rhs = expr_to_cpp(&a.value)?;
                    header.push_str(&format!("      {lhs} = {rhs};\n"));
                } else if let ThreadStmt::CombAssign(a) = s {
                    // CombAssign in default-when block: also fire once
                    // (treat like a seq assign for soft-reset purposes).
                    let lhs = expr_to_cpp(&a.target)?;
                    let rhs = expr_to_cpp(&a.value)?;
                    header.push_str(&format!("      {lhs} = {rhs};\n"));
                }
                // Other ThreadStmt kinds (waits, control flow) are
                // illegal inside `default when` per arch grammar; ignored
                // here defensively.
            }
            header.push_str(&format!("      _slot_{ti}.thread.destroy();\n"));
            header.push_str(&format!("      _slot_{ti}.thread = _make_thread_{ti}();\n"));
            header.push_str(&format!("      _slot_{ti}.kind = arch_rt::WaitKind::Ready;\n"));
            header.push_str(&format!("      _slot_{ti}.cycles_remaining = 0;\n"));
            header.push_str(&format!("      _slot_{ti}.pred = nullptr;\n"));
            header.push_str(&format!("      _seg_{ti} = 0;\n"));
            // Reset this thread's branches too.
            for br in &thread_infos[ti].branches {
                header.push_str(&format!("      _t{ti}_br{}_slot.thread.destroy();\n", br.id));
                header.push_str(&format!("      _t{ti}_br{}_slot.kind = arch_rt::WaitKind::Done;\n", br.id));
                header.push_str(&format!("      _t{ti}_br{}_seg = 0;\n", br.id));
            }
            header.push_str("    }\n");
        }
    }
    // Module-level `seq on clk rising` blocks run at every posedge,
    // BEFORE the scheduler tick — so threads that read the just-updated
    // reg state see the new value (matches lowered-fsm always_ff order).
    for item in &m.body {
        if let ModuleBodyItem::RegBlock(rb) = item {
            for s in &rb.stmts {
                emit_seq_stmt(s, &mut header, 4)?;
            }
        }
    }
    if mt {
        // Multi-OS-thread tick: workers wait at _start_barrier; signal
        // them to begin their per-thread tick, then wait at _end_barrier
        // for them to complete.
        header.push_str("    _start_barrier.wait();\n");
        header.push_str("    _end_barrier.wait();\n");
    } else {
        // Sequential tick (single OS thread, current default).
        for (i, _) in thread_infos.iter().enumerate() {
            header.push_str(&format!("    _sched_{i}.tick();\n"));
        }
    }
    header.push_str("    eval();\n");
    // Note: --debug logging fires from eval() at end (matches fsm
    // pattern). _dbg_cycle increments on rising edge in eval() too.
    header.push_str("  }\n\n");

    if debug {
        // Initial log: capture port-state at construction (cycle 0
        // before any posedge). Called once from constructor or first
        // eval — for parity with fsm, we emit it as a public method
        // the TB can call after reset.
        header.push_str(&format!("  void _dbg_log_ports() {{\n"));
        for p in &m.ports {
            if matches!(&p.ty, TypeExpr::Clock(_)) { continue; }
            let pname = &p.name.name;
            let dir = match p.direction { Direction::In => "in", Direction::Out => "out" };
            let bits = match &p.ty {
                TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Reset(..) => 1,
                TypeExpr::UInt(w) => eval_const_with_params(w, &m.params),
                _ => continue,  // Vec / wide / bus skipped in Phase 5 spike
            };
            if bits == 0 || bits > 64 { continue; }
            header.push_str(&format!("    if ({pname} != _dbg_prev_{pname}) {{\n"));
            header.push_str(&format!(
                "      printf(\"[%llu][{mod}.{pname}]({dir}) 0x%llx -> 0x%llx\\n\", \
                 (unsigned long long)_dbg_cycle, \
                 (unsigned long long)_dbg_prev_{pname}, \
                 (unsigned long long){pname});\n",
                mod = m.name.name
            ));
            header.push_str(&format!("      _dbg_prev_{pname} = {pname};\n"));
            header.push_str("    }\n");
        }
        header.push_str("  }\n\n");
    }

    let _ = (&clk_name, &rst_name);

    // VCD trace methods (auto-emit when --wave is set).
    let trace_impl_str = if wave {
        let mut signals: Vec<crate::sim_codegen::TraceSignal> = Vec::new();
        // Top-level ports — include clocks too so the VCD timeline is
        // visible (matches fsm sim's --wave output).
        for p in &m.ports {
            let width = match &p.ty {
                TypeExpr::Clock(_) | TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Reset(..) => 1,
                TypeExpr::UInt(w) => eval_const_with_params(w, &m.params) as u32,
                _ => continue,
            };
            if width == 0 || width > 64 { continue; }
            signals.push(crate::sim_codegen::TraceSignal {
                vcd_name: p.name.name.clone(),
                cpp_expr: p.name.name.clone(),
                width,
                is_wide: false,
            });
        }
        // Reg fields. Scalars trace as a single VCD signal; Vec<T,N>
        // traces as N separate signals named `<name>[i]` so each
        // element is independently visible in the waveform viewer.
        // (fsm sim currently skips Vec regs entirely — gap to close
        // separately if the consistency matters.)
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let TypeExpr::Vec(elem, count_expr) = &r.ty {
                    let elem_width = match elem.as_ref() {
                        TypeExpr::Bool | TypeExpr::Bit => 1,
                        TypeExpr::UInt(w) => eval_const_with_params(w, &m.params) as u32,
                        _ => continue,
                    };
                    if elem_width == 0 || elem_width > 64 { continue; }
                    let count = eval_const_with_params(count_expr, &m.params);
                    for i in 0..count {
                        signals.push(crate::sim_codegen::TraceSignal {
                            vcd_name: format!("{}[{}]", r.name.name, i),
                            cpp_expr: format!("{}[{}]", r.name.name, i),
                            width: elem_width,
                            is_wide: false,
                        });
                    }
                } else {
                    let width = match &r.ty {
                        TypeExpr::Bool | TypeExpr::Bit => 1,
                        TypeExpr::UInt(w) => eval_const_with_params(w, &m.params) as u32,
                        _ => continue,
                    };
                    if width == 0 || width > 64 { continue; }
                    signals.push(crate::sim_codegen::TraceSignal {
                        vcd_name: r.name.name.clone(),
                        cpp_expr: r.name.name.clone(),
                        width,
                        is_wide: false,
                    });
                }
            }
        }
        // Coroutine state — exposed to VCD with leading underscore so
        // they show up as "reg" in the VCD scope. Useful for debugging
        // why a thread is parked at a given segment / which thread holds
        // a resource. Width: 8 bits is plenty for segment ids (Phase
        // limits well under 256), 32 bits for resource holders (-1
        // sentinel needs full int32_t but we cast to uint32 for VCD).
        for (ti, info) in thread_infos.iter().enumerate() {
            // Per-thread main segment id
            signals.push(crate::sim_codegen::TraceSignal {
                vcd_name: format!("_seg_{ti}"),
                cpp_expr: format!("_seg_{ti}"),
                width: 8,
                is_wide: false,
            });
            // Per-fork-branch segment ids
            for br in &info.branches {
                signals.push(crate::sim_codegen::TraceSignal {
                    vcd_name: format!("_t{ti}_br{}_seg", br.id),
                    cpp_expr: format!("_t{ti}_br{}_seg", br.id),
                    width: 8,
                    is_wide: false,
                });
            }
        }
        // Per-resource holder fields (-1 = free, otherwise thread index).
        for item in &m.body {
            if let ModuleBodyItem::Resource(r) = item {
                signals.push(crate::sim_codegen::TraceSignal {
                    vcd_name: format!("_resource_{}_holder", r.name.name),
                    cpp_expr: format!("(uint32_t)_resource_{}_holder", r.name.name),
                    width: 32,
                    is_wide: false,
                });
            }
        }
        let (decls, _impl) = crate::sim_codegen::emit_trace_methods(&class, &m.name.name, &signals);
        header.push_str(&decls);
        // emit_trace_methods returns standalone-class-method impls
        // (e.g. `void Foo::trace_open(...)`); since our class is
        // header-only, we splice them in inline at the bottom rather
        // than putting them in a separate .cpp.
        _impl
    } else {
        String::new()
    };

    if mt {
        // Public cycle-batch API: run K cycles in workers without
        // returning to caller between them. Trade-off: per-cycle
        // observability is sacrificed (no segment-switch eval, no
        // module-level comb/seq, no debug log, no VCD dump inside
        // the batch — only at the end). Use when running long
        // input-stable simulations where amortizing barrier cost
        // matters more than per-cycle observability.
        header.push_str("  void run_cycles(uint64_t k) {\n");
        header.push_str("    if (k == 0) return;\n");
        header.push_str("    _batch_count.store(k, std::memory_order_release);\n");
        header.push_str("    _start_barrier.wait();\n");
        header.push_str("    _end_barrier.wait();\n");
        header.push_str("    _batch_count.store(0, std::memory_order_release);\n");
        header.push_str("    eval();  // settle outputs after the batch\n");
        header.push_str("  }\n\n");
    }

    header.push_str("private:\n");
    header.push_str("  uint8_t _clk_prev = 0;\n");
    if wave {
        header.push_str("  FILE* _trace_fp = nullptr;\n");
        header.push_str("  uint64_t _trace_time = 0;\n");
    }
    if debug {
        header.push_str("  uint64_t _dbg_cycle = 0;\n");
        for p in &m.ports {
            if matches!(&p.ty, TypeExpr::Clock(_)) { continue; }
            let cpp_ty = port_or_reg_cpp_ty_with_params(&p.ty, &m.params)
                .map_err(|e| format!("module `{}` port `{}`: {}", class, p.name.name, e))?;
            header.push_str(&format!("  {}\n", field_decl(&cpp_ty, &format!("_dbg_prev_{}", p.name.name))));
        }
    }
    // Per-user-thread scheduler. Each owns its main slot + its fork
    // branches. At N=1 OS thread, all schedulers tick sequentially in
    // the calling OS thread; at N>1 each tick runs in a dedicated
    // worker OS thread synchronized via Barrier.
    for (i, _) in thread_infos.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ThreadScheduler _sched_{i};\n"));
    }
    if mt {
        // Multi-OS-thread coordination state. The +1 in barrier targets
        // is the calling thread (TB driver) which hits both barriers
        // each posedge.
        let total = thread_infos.len() + 1;
        header.push_str("  std::atomic<bool> _shutdown{false};\n");
        // Cycle-batch count: 0 = per-cycle mode (workers run 1 tick
        // per barrier round-trip from eval()); >0 = cycle-batch mode
        // (workers run K ticks per round-trip, set by run_cycles(K)).
        header.push_str("  std::atomic<uint64_t> _batch_count{0};\n");
        header.push_str(&format!("  arch_rt::Barrier _start_barrier{{{total}}};\n"));
        header.push_str(&format!("  arch_rt::Barrier _end_barrier{{{total}}};\n"));
        for (i, _) in thread_infos.iter().enumerate() {
            header.push_str(&format!("  std::thread _worker_{i};\n"));
        }
    }
    // One arbitration bundle per resource. Request bits are set by
    // threads parked on `lock`, and the generated selector mirrors the
    // declared mutex policy so --thread-sim observes the same contract as
    // lowered lock arbiters.
    for item in &m.body {
        if let ModuleBodyItem::Resource(r) = item {
            header.push_str(&format!("  uint64_t _resource_{}_req_mask = 0;\n", r.name.name));
            header.push_str(&format!("  int32_t _resource_{}_holder = -1;\n", r.name.name));
            match &r.policy {
                ArbiterPolicy::Priority => {}
                ArbiterPolicy::RoundRobin => {
                    header.push_str(&format!(
                        "  uint32_t _resource_{}_last_grant = {};\n",
                        r.name.name,
                        threads.len().saturating_sub(1)
                    ));
                }
                ArbiterPolicy::Lru => {
                    header.push_str(&format!(
                        "  uint32_t _resource_{}_lru_order[{}] = {{{}}};\n",
                        r.name.name,
                        threads.len(),
                        (0..threads.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
                    ));
                }
                ArbiterPolicy::Weighted(weight) => {
                    let credit = eval_const_with_params(weight, &m.params).max(1);
                    header.push_str(&format!(
                        "  uint32_t _resource_{}_last_grant = {};\n",
                        r.name.name,
                        threads.len().saturating_sub(1)
                    ));
                    header.push_str(&format!(
                        "  uint32_t _resource_{}_credits[{}] = {{{}}};\n",
                        r.name.name,
                        threads.len(),
                        std::iter::repeat(credit.to_string()).take(threads.len()).collect::<Vec<_>>().join(", ")
                    ));
                }
                ArbiterPolicy::Custom(_) => {
                    header.push_str(&format!("  uint64_t _resource_{}_last_grant_onehot = 0;\n", r.name.name));
                }
            }
        }
    }
    emit_resource_helpers(&mut header, m, threads.len())?;
    for (i, info) in thread_infos.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ThreadSlot _slot_{i};\n"));
        header.push_str(&format!("  uint32_t _seg_{i} = 0;\n"));
        for br in &info.branches {
            header.push_str(&format!("  arch_rt::ThreadSlot _t{i}_br{}_slot;\n", br.id));
            header.push_str(&format!("  uint32_t _t{i}_br{}_seg = 0;\n", br.id));
        }
    }
    header.push('\n');

    // Coroutine bodies.
    for (ti, t) in threads.iter().enumerate() {
        header.push_str(&format!("  arch_rt::ArchThread _make_thread_{ti}() {{\n"));
        let mut body_cpp = String::new();
        let info = &thread_infos[ti];
        let segs = &info.main_segs;
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
                    let n_str = expr_to_cpp(n)?;
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
                WaitKind::ForkJoin(branch_ids) => {
                    // Launch each branch: destroy any prior coroutine,
                    // create a fresh one, mark slot Ready so the next
                    // tick resumes it. Then wait until all branch slots
                    // are Done.
                    for &bid in branch_ids {
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_slot.thread.destroy();\n"));
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_slot.thread = _t{ti}_br{bid}_make();\n"));
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_slot.kind = arch_rt::WaitKind::Ready;\n"));
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_slot.cycles_remaining = 0;\n"));
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_slot.pred = nullptr;\n"));
                        body_cpp.push_str(&format!("{pad2}_t{ti}_br{bid}_seg = 0;\n"));
                    }
                    let pred = branch_ids.iter()
                        .map(|b| format!("_t{ti}_br{b}_slot.kind == arch_rt::WaitKind::Done"))
                        .collect::<Vec<_>>()
                        .join(" && ");
                    body_cpp.push_str(&format!(
                        "{pad2}co_await arch_rt::wait_until(&_slot_{ti}, [this]{{ return {pred}; }});\n"
                    ));
                }
                WaitKind::LockAcquire(res, my_id) => {
                    // Register this thread as a requester, then wait until
                    // the resource's policy selector chooses it. Re-check
                    // after resume because another scheduler tick may have
                    // claimed first in multi-OS-thread mode.
                    body_cpp.push_str(&format!("{pad2}_resource_{res}_note_request({my_id});\n"));
                    body_cpp.push_str(&format!("{pad2}while (true) {{\n"));
                    body_cpp.push_str(&format!(
                        "{pad2}  co_await arch_rt::wait_until(&_slot_{ti}, [this]{{ return _resource_{res}_can_acquire({my_id}); }});\n"
                    ));
                    body_cpp.push_str(&format!("{pad2}  if (_resource_{res}_can_acquire({my_id})) {{\n"));
                    body_cpp.push_str(&format!("{pad2}    _resource_{res}_claim({my_id});\n"));
                    body_cpp.push_str(&format!("{pad2}    break;\n"));
                    body_cpp.push_str(&format!("{pad2}  }}\n"));
                    body_cpp.push_str(&format!("{pad2}}}\n"));
                }
            }
            // Release lock if this segment was the last one inside a
            // `lock <name> { ... }` body.
            if let Some(res) = &seg.release_lock {
                body_cpp.push_str(&format!("{pad2}_resource_{res}_release({ti});\n"));
            }
            // Post-wait seq stmts (used by `do { ... } until cond;`):
            // fire AFTER the co_await returns, on the cycle the wait
            // condition became true.
            for sa in &seg.post_wait_seq {
                let lhs = expr_to_cpp(&sa.target)?;
                let rhs = expr_to_cpp(&sa.value)?;
                body_cpp.push_str(&format!("{pad2}{lhs} = {rhs};\n"));
            }
            for ie in &seg.post_wait_seq_if {
                emit_seq_if(ie, &mut body_cpp, ind + if seg.for_loop.is_some() {2} else {0})?;
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

        // Branch coroutines for this thread (one per fork branch).
        // Each is a one-shot (no while-true) — branches complete then
        // sit at Done until the parent's next fork-launch resets them.
        for br in &info.branches {
            header.push_str(&format!(
                "  arch_rt::ArchThread _t{ti}_br{}_make() {{\n", br.id
            ));
            for (si, seg) in br.segs.iter().enumerate() {
                let pad = "    ";
                if let Some(fl) = &seg.for_loop {
                    let s = expr_to_cpp(&fl.start)?;
                    let e = expr_to_cpp(&fl.end)?;
                    header.push_str(&format!(
                        "{pad}for (uint64_t {v} = {s}; {v} <= {e}; {v}++) {{\n",
                        v = fl.var
                    ));
                }
                let pad2 = if seg.for_loop.is_some() { "      " } else { "    " };
                header.push_str(&format!("{pad2}_t{ti}_br{}_seg = {si};\n", br.id));
                for sa in &seg.entry_seq {
                    let lhs = expr_to_cpp(&sa.target)?;
                    let rhs = expr_to_cpp(&sa.value)?;
                    header.push_str(&format!("{pad2}{lhs} = {rhs};\n"));
                }
                for ie in &seg.entry_seq_if {
                    emit_seq_if(ie, &mut header, if seg.for_loop.is_some() { 6 } else { 4 })?;
                }
                match &seg.wait_kind {
                    WaitKind::Until(cond) => {
                        let pred = expr_to_cpp_bool(cond)?;
                        header.push_str(&format!(
                            "{pad2}co_await arch_rt::wait_until(&_t{ti}_br{}_slot, [this]{{ return {pred}; }});\n", br.id
                        ));
                    }
                    WaitKind::Cycles(n) => {
                        let n_str = expr_to_cpp(n)?;
                        header.push_str(&format!(
                            "{pad2}co_await arch_rt::wait_cycles(&_t{ti}_br{}_slot, {n_str});\n", br.id
                        ));
                    }
                    WaitKind::Terminal => {
                        // Terminal in branch body. If the segment has held
                        // outputs the user wrote (e.g. `aw_valid = 0;` after
                        // a wait), they need one cycle of visibility before
                        // the branch goes Done — otherwise the assignment is
                        // invisible because eval() skips segment switches
                        // for Done branches. A 1-cycle yield here matches
                        // the lowered-fsm behavior (each trailing segment
                        // becomes its own state with one posedge of comb
                        // visibility).
                        if !seg.hold_comb.is_empty() {
                            header.push_str(&format!(
                                "{pad2}co_await arch_rt::wait_cycles(&_t{ti}_br{}_slot, 1);\n", br.id
                            ));
                        }
                    }
                    WaitKind::ForkJoin(_) => {
                        return Err("nested fork/join inside fork branch not yet supported".into());
                    }
                    WaitKind::LockAcquire(_, _) => {
                        return Err("`lock` inside fork branch not yet supported".into());
                    }
                }
                if seg.for_loop.is_some() {
                    header.push_str("    }\n");
                }
            }
            header.push_str("    co_return;\n");
            header.push_str("  }\n\n");
        }
    }

    if mt {
        // Per-worker loop method. Each worker waits at _start_barrier,
        // checks _shutdown, runs the requested number of ticks, then
        // waits at _end_barrier. _batch_count==0 ⇒ regular per-cycle
        // mode (one tick per barrier round-trip from eval()).
        // _batch_count>0 ⇒ cycle-batch mode (workers run K ticks
        // before signaling done, amortizing barrier overhead over K
        // cycles). Caller invokes batch mode via run_cycles(K).
        for (i, _) in thread_infos.iter().enumerate() {
            header.push_str(&format!("  void _worker_loop_{i}() {{\n"));
            header.push_str("    while (true) {\n");
            header.push_str("      _start_barrier.wait();\n");
            header.push_str("      if (_shutdown.load(std::memory_order_acquire)) break;\n");
            header.push_str("      uint64_t k = _batch_count.load(std::memory_order_acquire);\n");
            header.push_str("      if (k == 0) k = 1;\n");
            header.push_str("      for (uint64_t _j = 0; _j < k; _j++) {\n");
            header.push_str(&format!("        _sched_{i}.tick();\n"));
            header.push_str("      }\n");
            header.push_str("      _end_barrier.wait();\n");
            header.push_str("    }\n");
            header.push_str("  }\n\n");
        }
    }

    header.push_str("};\n");

    let impl_ = if !trace_impl_str.is_empty() {
        // Trace impls reference class-private fields, so they must be
        // class methods (not free functions). emit_trace_methods returns
        // out-of-class definitions like `void Foo::trace_open(...)`,
        // which we put in the .cpp alongside an #include of the .h.
        format!("#include \"{class}.h\"\n#include <cstdio>\n\n{trace_impl_str}")
    } else {
        format!("// {} thread-sim: header-only\n", class)
    };

    Ok(SimModel {
        class_name: class.clone(),
        header,
        impl_,
    })
}

// Walk thread body and partition into segments at each wait point.
// Branches collected from any nested fork/join sites are appended to
// `branches`; the caller passes a fresh Vec at the top level.
// `thread_id` identifies the owning thread (used as priority for
// LockAcquire — lower id wins).
fn partition(body: &[ThreadStmt], branches: &mut Vec<Branch>, thread_id: usize) -> Result<Vec<Segment>, String> {
    let mut segs: Vec<Segment> = Vec::new();
    let mut cur = new_segment();

    for s in body {
        match s {
            ThreadStmt::CombAssign(a) => {
                cur.hold_comb.push(Stmt::Assign(a.clone()));
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
                        cur.hold_comb.push(Stmt::IfElse(comb_ie));
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
                let inner = partition(body, branches, thread_id)?;
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
            ThreadStmt::ForkJoin(branch_bodies, _) => {
                // Flush any pending into a 1-cycle yield segment so the
                // pre-fork holds get a chance to settle. Phase 3a: error
                // if pending (force user to put a wait before fork).
                if !cur.hold_comb.is_empty() || !cur.entry_seq.is_empty() || !cur.entry_seq_if.is_empty() {
                    return Err("fork preceded by un-flushed comb/seq assigns not yet supported \
                        (insert a wait before the fork)".into());
                }
                // Allocate a Branch per branch body, partition each.
                let mut branch_ids: Vec<usize> = Vec::new();
                for body in branch_bodies {
                    let id = branches.len();
                    let segs = partition(body, branches, thread_id)?;
                    branches.push(Branch { id, segs });
                    branch_ids.push(id);
                }
                // Emit a fork segment whose wait waits for all branches Done.
                cur.wait_kind = WaitKind::ForkJoin(branch_ids);
                segs.push(std::mem::replace(&mut cur, new_segment()));
            }
            ThreadStmt::Lock { resource, body, .. } => {
                // Flush pending into a state before the lock, since the
                // pre-lock CombAssigns/SeqAssigns shouldn't be held while
                // waiting to acquire the resource.
                if !cur.hold_comb.is_empty() || !cur.entry_seq.is_empty() || !cur.entry_seq_if.is_empty() {
                    return Err("`lock` preceded by un-flushed comb/seq assigns not yet supported \
                        (insert a wait before the lock)".into());
                }
                // Acquire segment: wait until lock is free or already held by us.
                cur.wait_kind = WaitKind::LockAcquire(resource.name.clone(), thread_id);
                segs.push(std::mem::replace(&mut cur, new_segment()));
                // Recursively partition the lock body.
                let mut body_segs = partition(body, branches, thread_id)?;
                // The last segment of the body is where the lock release fires.
                if let Some(last) = body_segs.last_mut() {
                    last.release_lock = Some(resource.name.clone());
                } else {
                    return Err("`lock` body partitioned to zero segments".into());
                }
                segs.extend(body_segs);
            }
            ThreadStmt::DoUntil { body, cond, .. } => {
                // do { body } until cond — hold body's CombAssigns
                // while waiting for cond; when cond fires, also fire
                // body's SeqAssigns (and IfElses-of-SeqAssigns).
                //
                // Phase 4 scope: body must contain only CombAssign,
                // SeqAssign, IfElse (no nested waits, no for-loops,
                // no fork/join). This covers the axi_dma_thread case.
                if !cur.hold_comb.is_empty() || !cur.entry_seq.is_empty() || !cur.entry_seq_if.is_empty() {
                    return Err("`do until` preceded by un-flushed comb/seq assigns not yet supported".into());
                }
                if contains_wait(body) {
                    return Err("`do until` body containing nested `wait` not yet supported".into());
                }
                let mut hold_comb: Vec<Stmt> = Vec::new();
                let mut post_seq: Vec<RegAssign> = Vec::new();
                let mut post_seq_if: Vec<IfElseOf<ThreadStmt>> = Vec::new();
                for s in body {
                    match s {
                        ThreadStmt::CombAssign(a) => hold_comb.push(Stmt::Assign(a.clone())),
                        ThreadStmt::SeqAssign(a) => post_seq.push(a.clone()),
                        ThreadStmt::IfElse(ie) => {
                            match classify_ifelse(ie) {
                                IfKind::PureComb => {
                                    let comb_ie = lower_thread_ifelse_to_comb(ie)?;
                                    hold_comb.push(Stmt::IfElse(comb_ie));
                                }
                                IfKind::PureSeq => post_seq_if.push(ie.clone()),
                                IfKind::Mixed => return Err(
                                    "mixed comb+seq IfElse inside `do until` body not yet supported".into()),
                                IfKind::Empty => {}
                            }
                        }
                        _ => return Err(format!(
                            "stmt kind not supported inside `do until` body: {:?}",
                            std::mem::discriminant(s)
                        )),
                    }
                }
                cur.hold_comb = hold_comb;
                cur.post_wait_seq = post_seq;
                cur.post_wait_seq_if = post_seq_if;
                cur.wait_kind = WaitKind::Until(cond.clone());
                segs.push(std::mem::replace(&mut cur, new_segment()));
            }
            other => return Err(format!("thread stmt not yet supported: {:?}", std::mem::discriminant(other))),
        }
    }
    // Trailing segment: if it's pure-seq (no held comb), fold its
    // assigns into the previous wait segment's post_wait_seq — they
    // fire on the cycle the previous wait completes, no extra cycle
    // of latency. This matches the lowered-fsm trailing-statement
    // optimization (elaborate.rs §4 "Trailing statements"). For
    // trailing segments with held comb, we still need a Terminal
    // segment with a 1-cycle yield so the comb is visible.
    if !cur.hold_comb.is_empty() {
        cur.wait_kind = WaitKind::Terminal;
        segs.push(cur);
    } else if !cur.entry_seq.is_empty() || !cur.entry_seq_if.is_empty() {
        if let Some(prev) = segs.last_mut() {
            // Only fold into a wait segment (not Terminal/ForkJoin etc.).
            if matches!(prev.wait_kind, WaitKind::Until(_) | WaitKind::Cycles(_)) {
                prev.post_wait_seq.extend(std::mem::take(&mut cur.entry_seq));
                prev.post_wait_seq_if.extend(std::mem::take(&mut cur.entry_seq_if));
            } else {
                cur.wait_kind = WaitKind::Terminal;
                segs.push(cur);
            }
        } else {
            cur.wait_kind = WaitKind::Terminal;
            segs.push(cur);
        }
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
        post_wait_seq: Vec::new(),
        post_wait_seq_if: Vec::new(),
        wait_kind: WaitKind::Terminal,
        release_lock: None,
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

fn lower_thread_ifelse_to_comb(ie: &IfElseOf<ThreadStmt>) -> Result<IfElseOf<Stmt>, String> {
    fn lower_stmts(stmts: &[ThreadStmt]) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(a) => out.push(Stmt::Assign(a.clone())),
                ThreadStmt::IfElse(inner) => {
                    out.push(Stmt::IfElse(lower_thread_ifelse_to_comb(inner)?));
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

// Emit a sequential statement (from a module-level `seq on clk rising`
// block) as C++ inside posedge_clk. Non-blocking `<=` lowers to `=`
// because arch sim is single-process / immediate-effect.
fn emit_seq_stmt(s: &crate::ast::Stmt, out: &mut String, indent: usize) -> Result<(), String> {
    use crate::ast::Stmt;
    let pad = " ".repeat(indent);
    match s {
        Stmt::Assign(a) => {
            let lhs = expr_to_cpp(&a.target)?;
            let rhs = expr_to_cpp(&a.value)?;
            out.push_str(&format!("{pad}{lhs} = {rhs};\n"));
        }
        Stmt::IfElse(ie) => {
            let cond = expr_to_cpp_bool(&ie.cond)?;
            out.push_str(&format!("{pad}if ({cond}) {{\n"));
            for s in &ie.then_stmts { emit_seq_stmt(s, out, indent + 2)?; }
            if !ie.else_stmts.is_empty() {
                out.push_str(&format!("{pad}}} else {{\n"));
                for s in &ie.else_stmts { emit_seq_stmt(s, out, indent + 2)?; }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        _ => return Err(format!("module-level seq stmt kind not yet supported by thread sim")),
    }
    Ok(())
}

fn emit_comb_stmt(cs: &Stmt, out: &mut String, indent: usize) -> Result<(), String> {
    let pad = " ".repeat(indent);
    match cs {
        Stmt::Assign(a) => {
            let lhs = expr_to_cpp(&a.target)?;
            let rhs = expr_to_cpp(&a.value)?;
            out.push_str(&format!("{pad}{lhs} = {rhs};\n"));
        }
        Stmt::IfElse(ie) => {
            let cond = expr_to_cpp_bool(&ie.cond)?;
            out.push_str(&format!("{pad}if ({cond}) {{\n"));
            for s in &ie.then_stmts { emit_comb_stmt(s, out, indent + 2)?; }
            if !ie.else_stmts.is_empty() {
                out.push_str(&format!("{pad}}} else {{\n"));
                for s in &ie.else_stmts { emit_comb_stmt(s, out, indent + 2)?; }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        other => {
            return Err(format!(
                "comb stmt kind not yet supported by thread sim: {:?}",
                std::mem::discriminant(other)
            ));
        }
    }
    Ok(())
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
        // DoUntil is itself a wait — the until-cond gates progress.
        ThreadStmt::WaitUntil(..) | ThreadStmt::WaitCycles(..) | ThreadStmt::DoUntil { .. } => true,
        ThreadStmt::IfElse(ie) => contains_wait(&ie.then_stmts) || contains_wait(&ie.else_stmts),
        ThreadStmt::For { body, .. } => contains_wait(body),
        ThreadStmt::Lock { body, .. } => contains_wait(body),
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
        ExprKind::Index(base, idx) => {
            // Vec/array indexing: lower as C++ subscript. The base is
            // typically a Vec reg (`thread_complete[i]`).
            let b = expr_to_cpp(base)?;
            let i = expr_to_cpp(idx)?;
            Ok(format!("{b}[{i}]"))
        }
        ExprKind::MethodCall(recv, method, args) => {
            // Width-cast methods are no-ops for sim (C++ types already
            // hold the right width). Emit the receiver verbatim.
            match method.name.as_str() {
                "trunc" | "zext" | "sext" | "resize" => expr_to_cpp(recv),
                _ => Err(format!("method `.{}()` not yet supported by thread sim", method.name)),
            }.map(|s| { let _ = args; s })
        }
        ExprKind::BitSlice(base, hi, lo) => {
            // base[hi:lo] — extract bits. C++ equivalent:
            // (base >> lo) & ((1 << (hi - lo + 1)) - 1)
            let b = expr_to_cpp(base)?;
            let h = expr_to_cpp(hi)?;
            let l = expr_to_cpp(lo)?;
            Ok(format!("(({b}) >> ({l}) & ((1ull << (({h}) - ({l}) + 1)) - 1))"))
        }
        ExprKind::Binary(op, lhs, rhs) => {
            let l = expr_to_cpp(lhs)?;
            let r = expr_to_cpp(rhs)?;
            let op_str = match op {
                BinOp::Add | BinOp::AddWrap => "+",
                BinOp::Sub | BinOp::SubWrap => "-",
                BinOp::Mul | BinOp::MulWrap => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
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

fn port_or_reg_cpp_ty_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> Result<String, String> {
    match ty {
        TypeExpr::Clock(_) | TypeExpr::Reset(..) | TypeExpr::Bool | TypeExpr::Bit => Ok("uint8_t".to_string()),
        TypeExpr::UInt(w) => uint_cpp_ty(eval_const_with_params(w, params)),
        TypeExpr::SInt(w) => int_cpp_ty(eval_const_with_params(w, params)),
        other => Err(format!("type {:?} not supported", other)),
    }
}

fn wide_words(bits: u64) -> u64 {
    (bits + 31) / 32
}

fn scalar_width_with_params(ty: &TypeExpr, params: &[ParamDecl]) -> Option<u64> {
    match ty {
        TypeExpr::Bool | TypeExpr::Bit => Some(1),
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => Some(eval_const_with_params(w, params)),
        _ => None,
    }
}

fn collect_wide_scalar_names(m: &ModuleDecl) -> HashSet<String> {
    let mut wide = HashSet::new();
    for p in &m.ports {
        if scalar_width_with_params(&p.ty, &m.params).is_some_and(|w| w > 64) {
            wide.insert(p.name.name.clone());
        }
    }
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if scalar_width_with_params(&r.ty, &m.params).is_some_and(|w| w > 64) {
                    wide.insert(r.name.name.clone());
                }
            }
            ModuleBodyItem::LetBinding(lb) => {
                if let Some(ty) = &lb.ty {
                    if scalar_width_with_params(ty, &m.params).is_some_and(|w| w > 64) {
                        wide.insert(lb.name.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    wide
}

fn expr_uses_wide_name(expr: &Expr, wide_names: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => wide_names.contains(name),
        ExprKind::Unary(_, inner)
        | ExprKind::Signed(inner)
        | ExprKind::Unsigned(inner)
        | ExprKind::Cast(inner, _)
        | ExprKind::Clog2(inner) => expr_uses_wide_name(inner, wide_names),
        ExprKind::Index(base, idx)
        | ExprKind::BitSlice(base, idx, _)
        | ExprKind::PartSelect(base, idx, _, _) => {
            expr_uses_wide_name(base, wide_names) || expr_uses_wide_name(idx, wide_names)
        }
        ExprKind::FieldAccess(base, _) => expr_uses_wide_name(base, wide_names),
        ExprKind::Binary(_, lhs, rhs) => {
            expr_uses_wide_name(lhs, wide_names) || expr_uses_wide_name(rhs, wide_names)
        }
        ExprKind::Ternary(cond, then_expr, else_expr) => {
            expr_uses_wide_name(cond, wide_names)
                || expr_uses_wide_name(then_expr, wide_names)
                || expr_uses_wide_name(else_expr, wide_names)
        }
        ExprKind::MethodCall(base, _, args) => {
            expr_uses_wide_name(base, wide_names)
                || args.iter().any(|arg| expr_uses_wide_name(arg, wide_names))
        }
        ExprKind::Concat(parts) => parts.iter().any(|part| expr_uses_wide_name(part, wide_names)),
        ExprKind::Literal(_) | ExprKind::Bool(_) => false,
        _ => false,
    }
}

fn wide_expr_is_direct_copy(expr: &Expr, wide_names: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => wide_names.contains(name),
        ExprKind::MethodCall(base, method, _)
            if matches!(method.name.as_str(), "trunc" | "zext" | "sext" | "resize") =>
        {
            wide_expr_is_direct_copy(base, wide_names)
        }
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) | ExprKind::Cast(inner, _) => {
            wide_expr_is_direct_copy(inner, wide_names)
        }
        _ => false,
    }
}

fn validate_wide_expr(
    expr: &Expr,
    wide_names: &HashSet<String>,
    class: &str,
    context: &str,
) -> Result<(), String> {
    if expr_uses_wide_name(expr, wide_names) && !wide_expr_is_direct_copy(expr, wide_names) {
        return Err(format!(
            "module `{class}`: thread sim currently supports wide (>64-bit) values only as direct copies; unsupported {context}: {expr:?}"
        ));
    }
    Ok(())
}

fn validate_stmt_wide_exprs(
    stmt: &Stmt,
    wide_names: &HashSet<String>,
    class: &str,
) -> Result<(), String> {
    match stmt {
        Stmt::Assign(a) => {
            validate_wide_expr(&a.target, wide_names, class, "assignment target")?;
            validate_wide_expr(&a.value, wide_names, class, "assignment value")?;
        }
        Stmt::IfElse(ie) => {
            validate_wide_expr(&ie.cond, wide_names, class, "if condition")?;
            for stmt in &ie.then_stmts {
                validate_stmt_wide_exprs(stmt, wide_names, class)?;
            }
            for stmt in &ie.else_stmts {
                validate_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        Stmt::For(fl) => {
            match &fl.range {
                ForRange::Range(start, end) => {
                    validate_wide_expr(start, wide_names, class, "for-range start")?;
                    validate_wide_expr(end, wide_names, class, "for-range end")?;
                }
                ForRange::ValueList(values) => {
                    for value in values {
                        validate_wide_expr(value, wide_names, class, "for-range value")?;
                    }
                }
            }
            for stmt in &fl.body {
                validate_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        Stmt::WaitUntil(cond, _) => {
            validate_wide_expr(cond, wide_names, class, "wait-until condition")?;
        }
        Stmt::DoUntil { body, cond, .. } => {
            for stmt in body {
                validate_stmt_wide_exprs(stmt, wide_names, class)?;
            }
            validate_wide_expr(cond, wide_names, class, "do-until condition")?;
        }
        Stmt::Match(m) => {
            validate_wide_expr(&m.scrutinee, wide_names, class, "match scrutinee")?;
            for arm in &m.arms {
                for stmt in &arm.body {
                    validate_stmt_wide_exprs(stmt, wide_names, class)?;
                }
            }
        }
        Stmt::Init(ib) => {
            for stmt in &ib.body {
                validate_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        Stmt::Log(_) => {}
    }
    Ok(())
}

fn validate_thread_stmt_wide_exprs(
    stmt: &ThreadStmt,
    wide_names: &HashSet<String>,
    class: &str,
) -> Result<(), String> {
    match stmt {
        ThreadStmt::CombAssign(a) | ThreadStmt::SeqAssign(a) => {
            validate_wide_expr(&a.target, wide_names, class, "assignment target")?;
            validate_wide_expr(&a.value, wide_names, class, "assignment value")?;
        }
        ThreadStmt::IfElse(ie) => {
            validate_wide_expr(&ie.cond, wide_names, class, "if condition")?;
            for stmt in &ie.then_stmts {
                validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
            }
            for stmt in &ie.else_stmts {
                validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        ThreadStmt::WaitUntil(cond, _) => {
            validate_wide_expr(cond, wide_names, class, "wait-until condition")?;
        }
        ThreadStmt::WaitCycles(expr, _) => {
            validate_wide_expr(expr, wide_names, class, "wait-cycles count")?;
        }
        ThreadStmt::For { start, end, body, .. } => {
            validate_wide_expr(start, wide_names, class, "for-range start")?;
            validate_wide_expr(end, wide_names, class, "for-range end")?;
            for stmt in body {
                validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        ThreadStmt::DoUntil { body, cond, .. } => {
            for stmt in body {
                validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
            }
            validate_wide_expr(cond, wide_names, class, "do-until condition")?;
        }
        ThreadStmt::Lock { body, .. } => {
            for stmt in body {
                validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
            }
        }
        ThreadStmt::ForkJoin(branches, _) => {
            for branch in branches {
                for stmt in branch {
                    validate_thread_stmt_wide_exprs(stmt, wide_names, class)?;
                }
            }
        }
        ThreadStmt::ForkTlmAssign(a) => {
            validate_wide_expr(&a.target, wide_names, class, "assignment target")?;
            validate_wide_expr(&a.value, wide_names, class, "assignment value")?;
        }
        ThreadStmt::Return(expr, _) => {
            validate_wide_expr(expr, wide_names, class, "return value")?;
        }
        ThreadStmt::JoinAll(_) | ThreadStmt::Log(_) => {}
    }
    Ok(())
}

fn validate_wide_thread_sim_exprs(m: &ModuleDecl, class: &str) -> Result<(), String> {
    let wide_names = collect_wide_scalar_names(m);
    if wide_names.is_empty() {
        return Ok(());
    }
    for item in &m.body {
        match item {
            ModuleBodyItem::LetBinding(lb) => {
                validate_wide_expr(&lb.value, &wide_names, class, "let binding value")?;
            }
            ModuleBodyItem::CombBlock(cb) => {
                for stmt in &cb.stmts {
                    validate_stmt_wide_exprs(stmt, &wide_names, class)?;
                }
            }
            ModuleBodyItem::RegBlock(rb) => {
                for stmt in &rb.stmts {
                    validate_stmt_wide_exprs(stmt, &wide_names, class)?;
                }
            }
            ModuleBodyItem::Thread(t) => {
                for stmt in &t.body {
                    validate_thread_stmt_wide_exprs(stmt, &wide_names, class)?;
                }
                if let Some((cond, body)) = &t.default_when {
                    validate_wide_expr(cond, &wide_names, class, "default-when condition")?;
                    for stmt in body {
                        validate_thread_stmt_wide_exprs(stmt, &wide_names, class)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_wide_cpp_ty(ty: &str) -> bool {
    ty.starts_with("VlWide<")
}

fn field_decl(ty: &str, name: &str) -> String {
    if is_wide_cpp_ty(ty) {
        format!("{ty} {name}{{}};")
    } else {
        format!("{ty} {name} = 0;")
    }
}

fn eval_const_with_params(e: &Expr, params: &[ParamDecl]) -> u64 {
    match &e.kind {
        ExprKind::Literal(LitKind::Dec(v)) => *v,
        ExprKind::Literal(LitKind::Hex(v)) => *v,
        ExprKind::Literal(LitKind::Bin(v)) => *v,
        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
        ExprKind::Ident(name) => {
            params.iter()
                .find(|p| p.name.name == *name)
                .and_then(|p| p.default.as_ref())
                .map(|d| eval_const_with_params(d, params))
                .unwrap_or(0)
        }
        ExprKind::Clog2(a) => {
            let v = eval_const_with_params(a, params);
            if v <= 1 { 0 } else { 64 - (v - 1).leading_zeros() as u64 }
        }
        ExprKind::Unary(op, a) => {
            let v = eval_const_with_params(a, params);
            match op {
                UnaryOp::Not => !v,
                UnaryOp::BitNot => !v,
                UnaryOp::Neg => v.wrapping_neg(),
                UnaryOp::RedAnd | UnaryOp::RedOr | UnaryOp::RedXor => 0,
            }
        }
        ExprKind::Binary(op, l, r) => {
            let lv = eval_const_with_params(l, params);
            let rv = eval_const_with_params(r, params);
            match op {
                BinOp::Add | BinOp::AddWrap => lv.wrapping_add(rv),
                BinOp::Sub | BinOp::SubWrap => lv.wrapping_sub(rv),
                BinOp::Mul | BinOp::MulWrap => lv.wrapping_mul(rv),
                BinOp::Div => if rv == 0 { 0 } else { lv / rv },
                BinOp::Mod => if rv == 0 { 0 } else { lv % rv },
                BinOp::Shl => lv.wrapping_shl(rv as u32),
                BinOp::Shr => lv.wrapping_shr(rv as u32),
                BinOp::BitAnd => lv & rv,
                BinOp::BitOr => lv | rv,
                BinOp::BitXor => lv ^ rv,
                _ => 0,
            }
        }
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
        65..=1024 => format!("VlWide<{}>", wide_words(bits)),
        _ => return Err(format!("UInt<{}> > 1024 bits not supported by thread sim", bits)),
    })
}

fn int_cpp_ty(bits: u64) -> Result<String, String> {
    Ok(match bits {
        0 => return Err("SInt<0> not supported".into()),
        1..=8 => "int8_t".to_string(),
        9..=16 => "int16_t".to_string(),
        17..=32 => "int32_t".to_string(),
        33..=64 => "int64_t".to_string(),
        65..=1024 => format!("VlWide<{}>", wide_words(bits)),
        _ => return Err(format!("SInt<{}> > 1024 bits not supported by thread sim", bits)),
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

fn emit_resource_helpers(header: &mut String, m: &ModuleDecl, num_threads: usize) -> Result<(), String> {
    for item in &m.body {
        let ModuleBodyItem::Resource(r) = item else { continue; };
        let res = &r.name.name;
        header.push_str(&format!("  uint32_t _resource_{res}_select() {{\n"));
        header.push_str(&format!("    uint64_t _req = _resource_{res}_req_mask;\n"));
        header.push_str("    if (_req == 0) return 0xffffffffu;\n");
        match &r.policy {
            ArbiterPolicy::Priority => {
                header.push_str(&format!("    for (uint32_t _idx = 0; _idx < {num_threads}; _idx++) {{\n"));
                header.push_str("      if ((_req >> _idx) & 1ULL) return _idx;\n");
                header.push_str("    }\n");
                header.push_str("    return 0xffffffffu;\n");
            }
            ArbiterPolicy::RoundRobin => {
                header.push_str(&format!("    for (uint32_t _step = 0; _step < {num_threads}; _step++) {{\n"));
                header.push_str(&format!("      uint32_t _idx = (_resource_{res}_last_grant + 1 + _step) % {num_threads};\n"));
                header.push_str("      if ((_req >> _idx) & 1ULL) return _idx;\n");
                header.push_str("    }\n");
                header.push_str("    return 0xffffffffu;\n");
            }
            ArbiterPolicy::Lru => {
                header.push_str(&format!("    for (uint32_t _rank = 0; _rank < {num_threads}; _rank++) {{\n"));
                header.push_str(&format!("      uint32_t _idx = _resource_{res}_lru_order[_rank];\n"));
                header.push_str("      if ((_req >> _idx) & 1ULL) return _idx;\n");
                header.push_str("    }\n");
                header.push_str("    return 0xffffffffu;\n");
            }
            ArbiterPolicy::Weighted(weight) => {
                let credit = eval_const_with_params(weight, &m.params).max(1);
                header.push_str("    bool _has_credit = false;\n");
                header.push_str(&format!("    for (uint32_t _idx = 0; _idx < {num_threads}; _idx++) {{\n"));
                header.push_str(&format!("      if (((_req >> _idx) & 1ULL) && _resource_{res}_credits[_idx] != 0) _has_credit = true;\n"));
                header.push_str("    }\n");
                header.push_str("    if (!_has_credit) {\n");
                header.push_str(&format!("      for (uint32_t _idx = 0; _idx < {num_threads}; _idx++) _resource_{res}_credits[_idx] = {credit};\n"));
                header.push_str("    }\n");
                header.push_str(&format!("    for (uint32_t _step = 0; _step < {num_threads}; _step++) {{\n"));
                header.push_str(&format!("      uint32_t _idx = (_resource_{res}_last_grant + 1 + _step) % {num_threads};\n"));
                header.push_str(&format!("      if (((_req >> _idx) & 1ULL) && _resource_{res}_credits[_idx] != 0) return _idx;\n"));
                header.push_str("    }\n");
                header.push_str("    return 0xffffffffu;\n");
            }
            ArbiterPolicy::Custom(fn_ident) => {
                let hook = r.hook.as_ref().ok_or_else(|| {
                    format!(
                        "module `{}` resource `{}`: custom mutex policy `{}` requires a hook",
                        m.name.name, res, fn_ident.name
                    )
                })?;
                let mut args: Vec<String> = Vec::new();
                for arg in &hook.fn_args {
                    let is_hook_param = hook.params.iter().any(|p| p.name.name == arg.name);
                    let mapped = if is_hook_param {
                        match arg.name.as_str() {
                            "req_mask" => "_req".to_string(),
                            "last_grant" => format!("_resource_{res}_last_grant_onehot"),
                            _ => arg.name.clone(),
                        }
                    } else {
                        arg.name.clone()
                    };
                    args.push(mapped);
                }
                header.push_str(&format!(
                    "    uint64_t _grant_onehot = (uint64_t)({}({})) & _req;\n",
                    fn_ident.name,
                    args.join(", ")
                ));
                header.push_str(&format!("    for (int32_t _idx = (int32_t){num_threads} - 1; _idx >= 0; --_idx) {{\n"));
                header.push_str("      if ((_grant_onehot >> (uint32_t)_idx) & 1ULL) return (uint32_t)_idx;\n");
                header.push_str("    }\n");
                header.push_str("    return 0xffffffffu;\n");
            }
        }
        header.push_str("  }\n");
        header.push_str(&format!("  bool _resource_{res}_can_acquire(uint32_t _tid) {{\n"));
        header.push_str(&format!("    if (_resource_{res}_holder == (int32_t)_tid) return true;\n"));
        header.push_str(&format!("    if (_resource_{res}_holder != -1) return false;\n"));
        header.push_str(&format!("    return _resource_{res}_select() == _tid;\n"));
        header.push_str("  }\n");
        header.push_str(&format!("  void _resource_{res}_note_request(uint32_t _tid) {{\n"));
        header.push_str(&format!("    _resource_{res}_req_mask |= (1ULL << _tid);\n"));
        header.push_str("  }\n");
        header.push_str(&format!("  void _resource_{res}_claim(uint32_t _tid) {{\n"));
        header.push_str(&format!("    _resource_{res}_req_mask &= ~(1ULL << _tid);\n"));
        header.push_str(&format!("    if (_resource_{res}_holder == (int32_t)_tid) return;\n"));
        header.push_str(&format!("    _resource_{res}_holder = (int32_t)_tid;\n"));
        match &r.policy {
            ArbiterPolicy::Priority => {}
            ArbiterPolicy::RoundRobin => {
                header.push_str(&format!("    _resource_{res}_last_grant = _tid;\n"));
            }
            ArbiterPolicy::Lru => {
                header.push_str("    uint32_t _pos = 0;\n");
                header.push_str(&format!("    while (_pos < {num_threads} && _resource_{res}_lru_order[_pos] != _tid) _pos++;\n"));
                header.push_str(&format!("    while (_pos + 1 < {num_threads}) {{\n"));
                header.push_str(&format!("      _resource_{res}_lru_order[_pos] = _resource_{res}_lru_order[_pos + 1];\n"));
                header.push_str("      _pos++;\n");
                header.push_str("    }\n");
                header.push_str(&format!("    _resource_{res}_lru_order[{num_threads} - 1] = _tid;\n"));
            }
            ArbiterPolicy::Weighted(_) => {
                header.push_str(&format!("    if (_resource_{res}_credits[_tid] != 0) _resource_{res}_credits[_tid]--;\n"));
                header.push_str(&format!("    _resource_{res}_last_grant = _tid;\n"));
            }
            ArbiterPolicy::Custom(_) => {
                header.push_str(&format!("    _resource_{res}_last_grant_onehot = (1ULL << _tid);\n"));
            }
        }
        header.push_str("  }\n");
        header.push_str(&format!("  void _resource_{res}_release(uint32_t _tid) {{\n"));
        header.push_str(&format!("    if (_resource_{res}_holder == (int32_t)_tid) _resource_{res}_holder = -1;\n"));
        header.push_str("  }\n");
    }
    Ok(())
}

fn emit_resource_reset(
    header: &mut String,
    r: &crate::ast::ResourceDecl,
    params: &[ParamDecl],
    num_threads: usize,
    pad: &str,
) {
    let res = &r.name.name;
    header.push_str(&format!("{pad}_resource_{res}_req_mask = 0;\n"));
    header.push_str(&format!("{pad}_resource_{res}_holder = -1;\n"));
    match &r.policy {
        ArbiterPolicy::Priority => {}
        ArbiterPolicy::RoundRobin => {
            header.push_str(&format!("{pad}_resource_{res}_last_grant = {};\n", num_threads.saturating_sub(1)));
        }
        ArbiterPolicy::Lru => {
            for idx in 0..num_threads {
                header.push_str(&format!("{pad}_resource_{res}_lru_order[{idx}] = {idx};\n"));
            }
        }
        ArbiterPolicy::Weighted(weight) => {
            let credit = eval_const_with_params(weight, params).max(1);
            header.push_str(&format!("{pad}_resource_{res}_last_grant = {};\n", num_threads.saturating_sub(1)));
            for idx in 0..num_threads {
                header.push_str(&format!("{pad}_resource_{res}_credits[{idx}] = {credit};\n"));
            }
        }
        ArbiterPolicy::Custom(_) => {
            header.push_str(&format!("{pad}_resource_{res}_last_grant_onehot = 0;\n"));
        }
    }
}

// Suppress unused-import warnings while CombAssign/RegAssign are
// referenced only via paths.
#[allow(dead_code)]
fn _unused(_a: &CombAssign, _r: &RegAssign) {}

pub fn arch_thread_rt_h() -> &'static str {
    include_str!("../../runtime/arch_thread_rt.h")
}
