//! Thread → FSM lowering — extracted from `elaborate.rs` (P4 phase 2b,
//! move-only). This module owns the generic (non-TLM) `thread` block family:
//! turning a module's `thread` blocks into a single merged per-module FSM
//! (`lower_threads` / `lower_threads_with_opts` / `lower_module_threads`),
//! partitioning a thread body into FSM states around `wait until` / `wait N
//! cycle` / `fork...join` / `for` / `lock` control (`partition_thread_body_*`
//! family), lowering `fork`/`join` branches and `for` loops to state
//! transitions (`lower_fork_join`, `lower_thread_for`), synthesizing the
//! lock/semaphore/arbiter machinery backing `lock <resource> ... end lock`
//! (`lower_thread_lock`, `synthesize_lock_arbiter`, `split_lock_req_comb`),
//! reducing `shared(or)` / `shared(and)` cross-thread signals
//! (`rewrite_shared_or_seq_stmts`, `transform_shared_or_assigns`), and the
//! auto-thread-assert SVA emission gated by `ThreadLowerOpts::auto_asserts`
//! (`--auto-thread-asserts`) that lives inside `lower_module_threads` itself.
//!
//! ## 2b/2c boundary: what stayed in `elaborate::mod`
//!
//! **TLM lowering is a separate, larger family — deliberately left for a
//! future Phase 2c, not moved here.** `lower_tlm_connects` (bus-initiator/
//! target connect rewriting), `lower_tlm_target_threads` (`thread
//! port.method(...)` target bodies), and `lower_tlm_initiator_calls` (the
//! `d <= m.read(addr);` initiator-call cohort/arbiter/router synthesis, plus
//! its ~4500-line helper cluster) all stay in `elaborate::mod`. Per
//! `src/main.rs`'s pipeline order, TLM lowering runs *before* generic
//! `lower_threads` and the two are structurally disjoint passes — a module's
//! threads are either TLM-bound (routed through the TLM passes, which strip
//! them before generic lowering ever sees them) or plain (routed through this
//! module) — so this is a clean seam, not an arbitrary cut.
//!
//! `check_fork_join_uniform_tlm_class` (rejects a `fork...join` TLM issue
//! group that mixes `blocking` and `out_of_order tags N` calls, PR #761)
//! stays in `elaborate::mod`: it operates on `DirectTlmThread`/
//! `TlmMethodMeta` — TLM call-class metadata, not thread-FSM shape — and has
//! no caller in this module.
//!
//! A handful of functions sit in `elaborate::mod` because they are shared by
//! *three* lowering families, not just this one — `lower_tlm_connects`
//! (before thread lowering in file order), thread lowering (this module),
//! and TLM target/initiator lowering (after, in file order): bus/const-eval
//! utilities `build_module_type_map`, `build_module_type_map_with_buses`,
//! `bus_effective_signals`, `tlm_effective_methods_for_bus`,
//! `tlm_method_effective_signals`, `gen_if_cond_truthy`,
//! `eval_const_expr_from_param_map_for_lower`, `eval_const_expr_for_lower`,
//! `subst_type_expr_for_lower`, and the `SignalInfo` type they traffic in.
//! Moving them here would have meant bouncing visibility both ways across
//! two module boundaries for no benefit; they stay put and this module
//! reaches them (and `try_eval_i64`/`try_eval_bool`/generate-expansion/
//! `ParentShapeInfo`, per the phase-2a precedent) via `use super::*;` below,
//! the same descendant-sees-ancestor privacy rule `elaborate::params`
//! relies on.
//!
//! Two unrelated constructs sit immediately after this module's old location
//! in file order and were never part of this family: `pipe_reg<T, N>` port
//! lowering (`lower_pipe_reg_ports`, a `pipeline`-adjacent concern) and
//! `credit_channel` method dispatch (`lower_credit_channel_dispatch`, a
//! `bus`-adjacent concern). Neither touches `thread`.
//!
//! ## Visibility bumps (mechanical, not a design change)
//!
//! TLM target/initiator lowering (`elaborate::mod`, staying for 2c) reuses
//! several thread-FSM primitives from this module rather than duplicating
//! them — a TLM target thread body (`thread s.read(addr) on clk rising ...`)
//! is still a thread body and goes through the same partition/rename/
//! wait-state machinery. Eleven functions were bumped `fn` → `pub(crate) fn`
//! purely so `elaborate::mod` can call back into them; no signature or body
//! changed:
//! `synthesize_lock_arbiter`, `partition_tlm_target_thread_body_with_loop_ids`,
//! `rewrite_loop_var`, `rewrite_var_expr`, `rename_ident_in_expr`,
//! `rename_ident_in_stmts`, `rename_ident_in_comb_stmts`,
//! `thread_target_return_idx`, `infer_for_cnt_width`, `contains_return`,
//! `thread_block_always_returns`. The same reuse forces `ThreadFsmState`
//! (the per-state partition result these functions build) and six of its
//! fields — `comb_stmts`, `seq_stmts`, `transition_cond`, `wait_cycles`,
//! `multi_transitions`, `terminal_return` — from private to `pub(crate)`
//! too: `elaborate::mod`'s `inline_lower_tlm_target_with_io` reads and
//! rewrites them directly on the `Vec<ThreadFsmState>` it gets back from
//! `partition_tlm_target_thread_body_with_loop_ids`. The remaining fields,
//! untouched outside this module, stay private. `lower_threads`,
//! `lower_threads_with_opts`, and `ThreadLowerOpts` were already `pub`
//! (external callers: `main.rs`'s pipeline driver,
//! `tests/integration_test.rs`, `tests/param_where_constraints.rs`) and are
//! re-exported from `elaborate::mod` so every existing
//! `crate::elaborate::lower_threads`-style call site keeps resolving
//! unchanged.
//!
//! `split_lock_req_comb` (added by PR #775, "emit lock request wires from
//! their own always_comb") is a private helper called only from
//! `lower_module_threads`, entirely internal to this module — no visibility
//! change needed.

use super::*;

// ── Thread → FSM lowering ───────────────────────────────────────────────────

/// Lower all `thread` blocks in modules to FSM + inst.
///
/// For each module containing ThreadBlock items, this pass:
/// 1. Analyzes signals read/written by the thread
/// 2. Creates a top-level FsmDecl with auto-generated states
/// 3. Replaces the ThreadBlock with an InstDecl wiring up the FSM
pub fn lower_threads(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    lower_threads_with_opts(ast, &ThreadLowerOpts::default())
}

/// Options that tune `lower_threads` behavior. The default disables every
/// optional behavior so existing callers (tests, sim, etc.) see no diff.
#[derive(Debug, Clone, Default)]
pub struct ThreadLowerOpts {
    /// Auto-emit SVA spec-contract properties at lowering time
    /// (`wait_until` progress, `wait N cycle` bounded liveness, fork/join
    /// branch transitions). Wrapped in `synopsys translate_off/on` so they
    /// don't reach synthesis. CLI: `--auto-thread-asserts`.
    pub auto_asserts: bool,
    /// Optional sidecar collection populated with source-to-state metadata
    /// for `arch build --emit-thread-map`. Normal lowering leaves this unset.
    pub thread_map: Option<Rc<RefCell<crate::thread_map::ThreadMap>>>,
}

pub fn lower_threads_with_opts(
    ast: SourceFile,
    opts: &ThreadLowerOpts,
) -> Result<SourceFile, Vec<CompileError>> {
    let mut new_items: Vec<Item> = Vec::new();
    let mut extra_fsms: Vec<Item> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    // Pre-collect bus definitions so lower_module_threads can resolve
    // bus port FieldAccess targets in thread bodies to flattened signal
    // names (e.g. `b.v = true;` → drives `b_v`). Without this, threads
    // that write to bus signals leave the corresponding flat output
    // undriven post-lowering.
    let bus_defs: HashMap<String, BusDecl> = ast
        .items
        .iter()
        .filter_map(|it| {
            if let Item::Bus(b) = it {
                Some((b.name.name.clone(), b.clone()))
            } else {
                None
            }
        })
        .collect();

    for item in ast.items {
        match item {
            Item::Module(m) => {
                let has_threads = m
                    .body
                    .iter()
                    .any(|i| matches!(i, ModuleBodyItem::Thread(_)));
                if !has_threads {
                    new_items.push(Item::Module(m));
                    continue;
                }
                match lower_module_threads(m, opts, &bus_defs) {
                    Ok((new_module, fsms)) => {
                        new_items.push(Item::Module(new_module));
                        extra_fsms.extend(fsms);
                    }
                    Err(mut errs) => errors.append(&mut errs),
                }
            }
            other => new_items.push(other),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Insert generated FSMs before the modules that use them
    let mut result = extra_fsms;
    result.extend(new_items);
    Ok(SourceFile {
        items: result,
        inner_doc: None,
        frontmatter: None,
    })
}

/// Lower all threads in a single module to a SINGLE merged module.
///
/// All threads become per-thread state machines within one module.
/// Shared registers, lock arbitration, and output muxing are all
/// handled internally — no multi-driver issues.
fn lower_module_threads(
    m: ModuleDecl,
    opts: &ThreadLowerOpts,
    bus_defs: &HashMap<String, BusDecl>,
) -> Result<(ModuleDecl, Vec<Item>), Vec<CompileError>> {
    let sp = m.span;
    let type_map = build_module_type_map_with_buses(&m, bus_defs);
    // Mapping bus port name → bus name. Used to resolve `b.v = ...` thread
    // targets to the flat signal name `b_v` so the lowering registers
    // them as outputs of the synthesized `_<mod>_threads` sub-module.
    let bus_port_map: HashMap<String, String> = m
        .ports
        .iter()
        .filter_map(|p| {
            p.bus_info
                .as_ref()
                .map(|bi| (p.name.name.clone(), bi.bus_name.name.clone()))
        })
        .collect();
    // Vec-of-bus port name → element count N. A *variable* index into one
    // of these (`o[sel].ready`) can't be resolved to a single static lane
    // at lowering time, so reads of such expressions are lowered to a
    // runtime mux over the N flattened per-lane signals (`o_0_ready` …
    // `o_{N-1}_ready`). Constant indices keep the existing single-lane path.
    let vob_counts: HashMap<String, u32> = {
        let param_vals: HashMap<String, i64> = m
            .params
            .iter()
            .filter_map(|p| {
                p.default.as_ref().and_then(|d| {
                    try_eval_i64(d, &HashMap::new()).map(|v| (p.name.name.clone(), v))
                })
            })
            .collect();
        let mut out = HashMap::new();
        for p in &m.ports {
            if let Some(bi) = p.bus_info.as_ref() {
                if let Some(count_expr) = bi.count.as_ref() {
                    let n = try_eval_i64(count_expr, &param_vals).unwrap_or(0) as u32;
                    if n > 0 {
                        out.insert(p.name.name.clone(), n);
                    }
                }
            }
        }
        out
    };
    let _reg_map = build_module_reg_map(&m);
    let mut errors: Vec<CompileError> = Vec::new();

    // Collect threads and non-thread body items
    let mut threads: Vec<(String, ThreadBlock)> = Vec::new();
    let mut new_body: Vec<ModuleBodyItem> = Vec::new();
    let mut thread_idx = 0usize;
    let mut resource_decls: HashMap<String, ResourceDecl> = HashMap::new();
    // Functions defined in the parent module are also visible to thread
    // bodies. Since the lowering moves thread states into a separate
    // `_<module>_threads` submodule, the function declarations must be
    // cloned into that submodule's body too — SV functions are local to
    // the module they're declared in. Without this, any thread-state body
    // that calls a parent-module function emits as an unresolved
    // task/function reference inside the threads submodule.
    let mut parent_functions: Vec<ModuleBodyItem> = Vec::new();
    // Module-scope params (e.g. `local param X[W-1:0]: const = N`) are
    // similarly visible to thread bodies. Without cloning them into the
    // submodule, any thread-body reference (match arm, concat, comparison)
    // emits as an unresolved identifier in the threads codegen. Clone the
    // whole list — params are cheap, and any unused ones are inert.
    let parent_params: Vec<ParamDecl> = m.params.clone();

    for item in m.body {
        match item {
            ModuleBodyItem::Function(_) => {
                // Keep the function in the parent module body AND clone it
                // for the threads submodule. Both modules need their own copy.
                parent_functions.push(item.clone());
                new_body.push(item);
            }
            ModuleBodyItem::Thread(t) => {
                // TLM target threads are rewritten into regular threads
                // by lower_tlm_target_threads (runs before lower_threads).
                // Any surviving tlm_target here means the pass wasn't
                // invoked — defensive error to catch a caller that
                // skipped the transform.
                if let Some(ref t_binding) = t.tlm_target {
                    return Err(vec![CompileError::general(
                        &format!(
                            "internal error: TLM target thread `{}.{}(...)` reached lower_threads without being rewritten. Call `lower_tlm_target_threads` first.",
                            t_binding.port.name, t_binding.method.name
                        ),
                        t.span,
                    )]);
                }
                // `implement target` threads should have been consumed by
                // lower_tlm_target_threads (which now treats them like
                // v1 tlm_target). If one reaches here, it's an internal
                // error. `implement` (initiator) threads are handled by
                // ordinary TLM initiator call-site/cohort lowering.
                if let Some(ref b) = t.implement {
                    if b.kind == TlmImplementKind::Target {
                        return Err(vec![CompileError::general(
                            &format!(
                                "internal error: `implement target {}.{}(...)` reached lower_threads without being consumed by lower_tlm_target_threads.",
                                b.port.name, b.method.name
                            ),
                            t.span,
                        )]);
                    }
                    return Err(vec![CompileError::general(
                        &format!(
                            "initiator-side `implement {}.{}()` reached ordinary thread lowering. `implement` is an annotation over TLM call-site lowering, so the thread body must contain supported direct calls to `{}.{}(...)`.",
                            b.port.name, b.method.name,
                            b.port.name, b.method.name
                        ),
                        t.span,
                    )]);
                }
                let name = t.name.as_ref().map(|n| n.name.clone()).unwrap_or_else(|| {
                    let n = if thread_idx == 0 {
                        "thread".to_string()
                    } else {
                        format!("thread{}", thread_idx)
                    };
                    thread_idx += 1;
                    n
                });
                if t.name.is_some() {
                    thread_idx += 1;
                }
                threads.push((name, t));
            }
            ModuleBodyItem::Resource(r) => {
                // Resource declarations are consumed here; their policy + hook
                // are stashed in `resource_decls` and used to synthesize a
                // per-resource arbiter further below.
                resource_decls.insert(r.name.name.clone(), r);
            }
            other => new_body.push(other),
        }
    }

    if threads.is_empty() {
        return Ok((
            ModuleDecl {
                body: new_body,
                ..m
            },
            Vec::new(),
        ));
    }

    // ── Build merged thread module ─────────────────────────────────────
    let merged_name = format!("_{}_threads", m.name.name);
    let mut merged_ports: Vec<PortDecl> = Vec::new();
    let mut merged_body: Vec<ModuleBodyItem> = Vec::new();

    // Collect ALL signals read/written across all threads
    let mut all_comb_driven: HashSet<String> = HashSet::new();
    let mut all_seq_driven: HashSet<String> = HashSet::new();
    let mut all_read: HashSet<String> = HashSet::new();
    for (_, t) in &threads {
        let (cd, sd, ar) = collect_thread_signals_with_buses(&t.body, &bus_port_map, &vob_counts);
        all_comb_driven.extend(cd);
        all_seq_driven.extend(sd);
        all_read.extend(ar);
        // Also seed flat bus-signal read names (`b.r` → `b_r`) so the
        // sub-module declares them as inputs.
        collect_thread_bus_reads(&t.body, &bus_port_map, &vob_counts, &mut all_read);
        // Also collect signals referenced in the `default when` clause
        if let Some((dw_cond, dw_stmts)) = &t.default_when {
            let (dw_cd, dw_sd, dw_ar) =
                collect_thread_signals_with_buses(dw_stmts, &bus_port_map, &vob_counts);
            all_comb_driven.extend(dw_cd);
            all_seq_driven.extend(dw_sd);
            all_read.extend(dw_ar);
            collect_thread_bus_reads(dw_stmts, &bus_port_map, &vob_counts, &mut all_read);
            collect_expr_reads(dw_cond, &mut all_read);
            collect_expr_bus_reads(dw_cond, &bus_port_map, &vob_counts, &mut all_read);
        }
    }
    for (_, t) in &threads {
        let (dc_targets, dc_reads) =
            collect_comb_stmt_signals_with_buses(&t.default_comb, &bus_port_map);
        for target in &dc_targets {
            if all_seq_driven.contains(target) {
                return Err(vec![CompileError::general(
                    &format!(
                        "thread `default comb` drives `{target}`, but that signal is also \
                         assigned with `<=` in a thread. Use `default comb` only for \
                         combinational thread outputs."
                    ),
                    t.span,
                )]);
            }
        }
        all_comb_driven.extend(dc_targets);
        all_read.extend(dc_reads);
    }

    // Clock and reset ports (from first thread)
    let (clk_name, rst_name, _rst_level) = {
        let t = &threads[0].1;
        let rk = type_map
            .get(&t.reset.name)
            .and_then(|si| {
                if let TypeExpr::Reset(k, _) = &si.ty {
                    Some(*k)
                } else {
                    None
                }
            })
            .unwrap_or(ResetKind::Async);
        merged_ports.push(PortDecl {
            name: t.clock.clone(),
            direction: Direction::In,
            ty: type_map
                .get(&t.clock.name)
                .map(|si| si.ty.clone())
                .unwrap_or(TypeExpr::Clock(Ident::new("SysDomain".to_string(), sp))),
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        });
        merged_ports.push(PortDecl {
            name: t.reset.clone(),
            direction: Direction::In,
            ty: TypeExpr::Reset(rk, t.reset_level),
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        });
        (t.clock.name.clone(), t.reset.name.clone(), t.reset_level)
    };

    // Collect lock signal names (internal, not ports)
    let mut lock_internal: HashSet<String> = HashSet::new();
    for (_, t) in &threads {
        for res in collect_locked_resources(&t.body) {
            lock_internal.insert(format!("_{}_req", res));
            lock_internal.insert(format!("_{}_grant", res));
        }
    }

    // Input ports (read-only signals, excluding internal lock signals).
    // Bus port roots (like `b` for `port b: initiator B`) are filtered
    // out because the sub-module surfaces only the flattened signals
    // (`b_v`, `b_r`, ...) — never the bus name itself. The bus-aware
    // collectors emit both root and flat entries; the rewrite pass
    // converts in-body references to flat names so the root would dangle.
    let read_only: HashSet<String> = all_read
        .iter()
        .filter(|n| {
            !all_comb_driven.contains(*n) && !all_seq_driven.contains(*n)
                && **n != clk_name && **n != rst_name
                && !n.starts_with("_t") // per-thread counters (_t0_cnt, _t0_loop_cnt_0, etc.)
                && **n != "_cnt" && !n.starts_with("_loop_cnt")
                && !lock_internal.contains(*n)
                && !bus_port_map.contains_key(*n)
        })
        .cloned()
        .collect();
    let mut sorted_reads: Vec<&String> = read_only.iter().collect();
    sorted_reads.sort();
    for name in sorted_reads {
        if let Some(info) = type_map.get(name.as_str()) {
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp),
                direction: Direction::In,
                ty: info.ty.clone(),
                default: None,
                reg_info: None,
                bus_info: None,
                shared: None,
                unpacked: info.unpacked,
                unpacked_ascending: info.unpacked_ascending,
                split: false,
                comb_deps: None,
                span: sp,
            });
        }
    }

    // Output ports (comb-driven, excluding internal lock signals)
    let mut sorted_comb: Vec<&String> = all_comb_driven
        .iter()
        .filter(|n| !lock_internal.contains(*n))
        .collect();
    sorted_comb.sort();
    for name in sorted_comb {
        if let Some(info) = type_map.get(name.as_str()) {
            // shared(and) idles at the identity element (all-ones) so a
            // thread that hasn't yet driven its state doesn't force the
            // AND-reduction low; every other comb-driven output (including
            // shared(or), whose identity is 0) keeps the plain zero default.
            let default_expr = match info.shared {
                Some(SharedReduction::And) => make_ones_expr(sp),
                _ => make_zero_expr(sp),
            };
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp),
                direction: Direction::Out,
                ty: info.ty.clone(),
                default: Some(default_expr),
                reg_info: None,
                bus_info: None,
                shared: info.shared,
                unpacked: info.unpacked,
                unpacked_ascending: info.unpacked_ascending,
                split: false,
                comb_deps: None,
                span: sp,
            });
        }
    }

    // Output ports (seq-driven) — these are port-regs in the merged module
    let mut sorted_seq: Vec<&String> = all_seq_driven.iter().collect();
    sorted_seq.sort();
    for name in sorted_seq {
        if let Some(info) = type_map.get(name.as_str()) {
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp),
                direction: Direction::Out,
                ty: info.ty.clone(),
                default: None,
                reg_info: Some(PortRegInfo {
                    init: info.reg_init.clone(),
                    reset: info.reg_reset.clone(),
                    guard: None,
                    latency: 1,
                    // Synthesized by thread lowering, not user-written;
                    // don't deprecate internal artifacts.
                    legacy_port_reg: false,
                }),
                bus_info: None,
                shared: None,
                unpacked: false,
                unpacked_ascending: false,
                split: false,
                comb_deps: None,
                span: sp,
            });
        }
    }

    // ── Lock arbiter — one synthesized `arbiter` Item per resource ──────
    //
    // For each locked resource we synthesize an `ArbiterDecl` carrying the
    // user's chosen `policy` + optional `hook` (default = `priority`), and
    // instantiate it inside the merged threads module. Per-thread `_req_i`
    // / `_grant_i` scalar wires are packed/unpacked through the arbiter's
    // `request_valid[N]` / `request_ready[N]` ports.
    //
    // This makes the existing `arbiter` construct's full policy support
    // (round_robin / priority / lru / weighted / custom-via-hook) available
    // for `lock`-block arbitration without duplicating arbitration logic.
    let mut all_resources: HashSet<String> = HashSet::new();
    for (_, t) in &threads {
        all_resources.extend(collect_locked_resources(&t.body));
    }
    let mut synthesized_arbiters: Vec<Item> = Vec::new();
    // Sort for deterministic output — HashSet iteration order is not stable.
    let mut sorted_resources: Vec<&String> = all_resources.iter().collect();
    sorted_resources.sort();
    for res_name in sorted_resources {
        let n_threads = threads.len();
        // Resource kind: `mutex` = 1 slot; `semaphore<N>` = N slots (N is a
        // const expr, module-param references allowed, evaluated here). A
        // `lock` referencing a resource with no explicit `resource`
        // declaration defaults to `mutex<priority>` (1 slot) — unchanged
        // from the pre-semaphore default.
        let (policy, hook, n_slots) = match resource_decls.get(res_name) {
            Some(rd) => {
                let n = match &rd.kind {
                    ResourceKind::Mutex => 1u64,
                    ResourceKind::Semaphore(n_expr) => {
                        let n = eval_const_expr_for_lower(n_expr, &parent_params);
                        if n == 0 {
                            errors.push(CompileError::general(
                                &format!(
                                    "resource `{}`: semaphore<N> requires N >= 1 (got 0)",
                                    res_name
                                ),
                                rd.span,
                            ));
                            1
                        } else {
                            n
                        }
                    }
                };
                (rd.policy.clone(), rd.hook.clone(), n as usize)
            }
            None => (ArbiterPolicy::Priority, None, 1usize),
        };
        // Per-thread scalar req/grant wires (internal to the merged module).
        // `req_{ti}` is asserted for the whole lock body (unchanged from
        // mutex); `grant_{ti}` is the final signal `lower_thread_lock`
        // gates output-driving and transitions on.
        for ti in 0..n_threads {
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: Ident::new(format!("_{}_req_{}", res_name, ti), sp),
                ty: TypeExpr::Bool,
                unpacked: false,
                unpacked_ascending: false,
                span: sp,
            }));
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: Ident::new(format!("_{}_grant_{}", res_name, ti), sp),
                ty: TypeExpr::Bool,
                unpacked: false,
                unpacked_ascending: false,
                span: sp,
            }));
            // Release is an edge-qualified event. The arbiter-facing copy is
            // registered so a lock exit condition can depend on grant
            // without creating a release -> grant -> release comb loop
            // (arch#709). A separate combinational release intent below is
            // used for semaphore holder bookkeeping.
            merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(format!("_{}_release_{}", res_name, ti), sp),
                ty: TypeExpr::Bool,
                init: Some(make_zero_expr(sp)),
                reset: RegReset::Inherit(Ident::new(rst_name.clone(), sp), make_zero_expr(sp)),
                guard: None,
                multicycle: None,
                span: sp,
            }));
            // Combinational release intent is used only by semaphore holder
            // bookkeeping. The registered release event above remains the
            // arbiter-facing signal so tight re-locks cannot form a
            // release -> grant -> release combinational loop.
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: Ident::new(format!("_{}_release_pending_{}", res_name, ti), sp),
                ty: TypeExpr::Bool,
                unpacked: false,
                unpacked_ascending: false,
                span: sp,
            }));
        }
        // ── semaphore<N> (N > 1): holder tracking ────────────────────────
        //
        // Unlike mutex (where the arbiter's own sticky `ready` IS the hold
        // state — `req` stays high the whole lock body, so `ready` stays
        // high until `req` deasserts at lock exit), a semaphore needs up
        // to N *simultaneous* holders. The arbiter construct itself only
        // ever grants one winner per cycle, so we generalize by feeding it
        // only *waiting* (not-yet-admitted) requesters, and track admitted
        // holders in a separate per-thread `held` register:
        //
        //   waiting_i  = req_i & !held_i      (comb — competes only pre-admission)
        //   [arbiter]  admits one waiting_i per cycle, subject to:
        //   admit_i    = arb_grant_i & waiting_i & (holder_count < N)
        //   held_i     <= admit_i ? 1 : (held_i & req_i ? held_i : 0)  (seq)
        //   grant_i    = held_i | admit_i     (comb — fed to lower_thread_lock)
        //   holder_count = popcount(held_0..held_{n-1})
        //
        // `admit_i` is combinational, so a thread that wins arbitration on
        // an empty/undersubscribed semaphore sees `grant_i` go high the
        // same cycle — the same "zero-cycle lock" property mutex documents
        // (`lower_thread_lock`'s doc comment), now also true per-holder for
        // semaphores. `held_i` register is what makes the property persist
        // across cycles once a slot is occupied by another thread's grant.
        //
        // `semaphore<1, policy>` does NOT enter this branch — `n_slots > 1`
        // is false for N==1, so it falls through to the exact same
        // raw-arbiter-ready path `mutex` uses below (bit-identical
        // codegen, not just behaviorally equivalent). See
        // `test_semaphore_1_is_bit_identical_to_mutex` in
        // tests/integration_test.rs.
        if n_slots > 1 {
            for ti in 0..n_threads {
                merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                    name: Ident::new(format!("_{}_held_{}", res_name, ti), sp),
                    ty: TypeExpr::Bool,
                    init: Some(make_zero_expr(sp)),
                    reset: RegReset::Inherit(Ident::new(rst_name.clone(), sp), make_zero_expr(sp)),
                    guard: None,
                    multicycle: None,
                    span: sp,
                }));
                merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: Ident::new(format!("_{}_waiting_{}", res_name, ti), sp),
                    ty: TypeExpr::Bool,
                    unpacked: false,
                    unpacked_ascending: false,
                    span: sp,
                }));
                merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: Ident::new(format!("_{}_admit_{}", res_name, ti), sp),
                    ty: TypeExpr::Bool,
                    unpacked: false,
                    unpacked_ascending: false,
                    span: sp,
                }));
            }
            // Width must match the natural IEEE-1800 §11.6 ripple-widening
            // result of summing `n_threads` 1-bit ternary terms: each `+`
            // widens by 1 bit, so an n-term chain settles at n bits (not
            // clog2(n+1) — that would require an explicit `.trunc<W>()`
            // the compiler doesn't insert here).
            let count_width = (n_threads as u32).max(1);
            let holder_count = format!("_{}_holder_count", res_name);
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: Ident::new(holder_count.clone(), sp),
                ty: TypeExpr::UInt(Box::new(Expr::new(
                    ExprKind::Literal(LitKind::Dec(count_width as u64)),
                    sp,
                ))),
                unpacked: false,
                unpacked_ascending: false,
                span: sp,
            }));
            // holder_count = (held_0 ? 1 : 0) + (held_1 ? 1 : 0) + ...
            let mut count_expr: Option<Expr> = None;
            for ti in 0..n_threads {
                let held_ident =
                    Expr::new(ExprKind::Ident(format!("_{}_held_{}", res_name, ti)), sp);
                let term = Expr::new(
                    ExprKind::Ternary(
                        Box::new(held_ident),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(1)), sp)),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
                    ),
                    sp,
                );
                count_expr = Some(match count_expr {
                    Some(acc) => Expr::new(
                        ExprKind::Binary(BinOp::Add, Box::new(acc), Box::new(term)),
                        sp,
                    ),
                    None => term,
                });
            }
            let mut sem_comb: Vec<Stmt> = Vec::new();
            sem_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(holder_count.clone()), sp),
                value: count_expr
                    .unwrap_or_else(|| Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
                span: sp,
            }));
            let n_slots_lit = Expr::new(ExprKind::Literal(LitKind::Dec(n_slots as u64)), sp);
            for ti in 0..n_threads {
                let req_ident = Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp);
                let held_ident =
                    Expr::new(ExprKind::Ident(format!("_{}_held_{}", res_name, ti)), sp);
                // waiting_i = req_i & !held_i
                sem_comb.push(Stmt::Assign(CombAssign {
                    target: Expr::new(ExprKind::Ident(format!("_{}_waiting_{}", res_name, ti)), sp),
                    value: Expr::new(
                        ExprKind::Binary(
                            BinOp::And,
                            Box::new(req_ident.clone()),
                            Box::new(Expr::new(
                                ExprKind::Unary(UnaryOp::Not, Box::new(held_ident.clone())),
                                sp,
                            )),
                        ),
                        sp,
                    ),
                    span: sp,
                }));
                // admit_i = grant_packed[i] & waiting_i & (holder_count < N)
                let arb_grant_i = Expr::new(
                    ExprKind::Index(
                        Box::new(Expr::new(
                            ExprKind::Ident(format!("_{}_grant_packed", res_name)),
                            sp,
                        )),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                    ),
                    sp,
                );
                let waiting_i =
                    Expr::new(ExprKind::Ident(format!("_{}_waiting_{}", res_name, ti)), sp);
                let count_lt_n = Expr::new(
                    ExprKind::Binary(
                        BinOp::Lt,
                        Box::new(Expr::new(ExprKind::Ident(holder_count.clone()), sp)),
                        Box::new(n_slots_lit.clone()),
                    ),
                    sp,
                );
                let admit_val = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(Expr::new(
                            ExprKind::Binary(
                                BinOp::And,
                                Box::new(arb_grant_i),
                                Box::new(waiting_i),
                            ),
                            sp,
                        )),
                        Box::new(count_lt_n),
                    ),
                    sp,
                );
                sem_comb.push(Stmt::Assign(CombAssign {
                    target: Expr::new(ExprKind::Ident(format!("_{}_admit_{}", res_name, ti)), sp),
                    value: admit_val,
                    span: sp,
                }));
                // grant_i = held_i | admit_i
                sem_comb.push(Stmt::Assign(CombAssign {
                    target: Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, ti)), sp),
                    value: Expr::new(
                        ExprKind::Binary(
                            BinOp::BitOr,
                            Box::new(held_ident),
                            Box::new(Expr::new(
                                ExprKind::Ident(format!("_{}_admit_{}", res_name, ti)),
                                sp,
                            )),
                        ),
                        sp,
                    ),
                    span: sp,
                }));
            }
            merged_body.push(ModuleBodyItem::CombBlock(CombBlock {
                stmts: sem_comb,
                span: sp,
            }));
            // held_i <= admit_i ? 1 : (held_i & req_i ? held_i : 0)
            // i.e.: admit -> hold; still requesting while held -> keep holding;
            // req deasserted (lock body exited) -> release.
            let mut sem_seq: Vec<Stmt> = Vec::new();
            for ti in 0..n_threads {
                let held_name = format!("_{}_held_{}", res_name, ti);
                let admit_ident =
                    Expr::new(ExprKind::Ident(format!("_{}_admit_{}", res_name, ti)), sp);
                let held_ident = Expr::new(ExprKind::Ident(held_name.clone()), sp);
                let req_ident = Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp);
                // #696: a semaphore slot is freed by the end-of-lock-body release
                // pulse (`_<res>_release_pending_<ti>`), NOT only by the request wire
                // deasserting. A thread that re-locks back-to-back (tight loop)
                // never deasserts `req_i` across `end lock`, so `held_i & req_i`
                // alone would pin the slot forever and starve waiting contenders.
                // Gate the hold on combinational release intent so the slot
                // is free immediately after the releasing edge. The
                // arbiter-facing release register is intentionally separate:
                // it breaks the tight re-lock comb loop without adding a
                // semaphore handoff bubble.
                let not_release = Expr::new(
                    ExprKind::Unary(
                        UnaryOp::Not,
                        Box::new(Expr::new(
                            ExprKind::Ident(format!("_{}_release_pending_{}", res_name, ti)),
                            sp,
                        )),
                    ),
                    sp,
                );
                let held_and_req = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(held_ident.clone()),
                        Box::new(req_ident),
                    ),
                    sp,
                );
                let still_held = Expr::new(
                    ExprKind::Binary(BinOp::And, Box::new(held_and_req), Box::new(not_release)),
                    sp,
                );
                let next_val = Expr::new(
                    ExprKind::Ternary(
                        Box::new(admit_ident),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(1)), sp)),
                        Box::new(still_held),
                    ),
                    sp,
                );
                sem_seq.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(held_name), sp),
                    value: next_val,
                    span: sp,
                }));
            }
            merged_body.push(ModuleBodyItem::RegBlock(RegBlock {
                clock: Ident::new(clk_name.clone(), sp),
                clock_edge: ClockEdge::Rising,
                stmts: sem_seq,
                span: sp,
            }));
        }
        // Build packed req/grant/release vectors used by the arbiter inst.
        let req_packed = format!("_{}_req_packed", res_name);
        let grant_packed = format!("_{}_grant_packed", res_name);
        let release_packed = format!("_{}_release_packed", res_name);
        let n_threads_expr = Expr::new(ExprKind::Literal(LitKind::Dec(n_threads as u64)), sp);
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: Ident::new(req_packed.clone(), sp),
            ty: TypeExpr::UInt(Box::new(n_threads_expr.clone())),
            unpacked: false,
            unpacked_ascending: false,
            span: sp,
        }));
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: Ident::new(grant_packed.clone(), sp),
            ty: TypeExpr::UInt(Box::new(n_threads_expr.clone())),
            unpacked: false,
            unpacked_ascending: false,
            span: sp,
        }));
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: Ident::new(release_packed.clone(), sp),
            ty: TypeExpr::UInt(Box::new(n_threads_expr.clone())),
            unpacked: false,
            unpacked_ascending: false,
            span: sp,
        }));
        // Throwaway sinks for arbiter scalar outputs (the lock idiom only
        // consumes the per-thread grant ready bits, not the scalar grant
        // index/valid).
        let gv_sink = format!("_{}_grant_valid", res_name);
        let gr_sink = format!("_{}_grant_requester", res_name);
        let gr_width = crate::width::index_width(n_threads as u64);
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: Ident::new(gv_sink.clone(), sp),
            ty: TypeExpr::Bool,
            unpacked: false,
            unpacked_ascending: false,
            span: sp,
        }));
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: Ident::new(gr_sink.clone(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(gr_width as u64)),
                sp,
            ))),
            unpacked: false,
            unpacked_ascending: false,
            span: sp,
        }));

        // Pack/unpack between scalar wires and packed vectors.
        //
        // n_slots <= 1 (`mutex<policy>`, or `semaphore<1, policy>` — the
        // `n_slots > 1` branch above is skipped entirely for N==1, so
        // `semaphore<1, policy>` takes this exact same path as `mutex`,
        // making the two bit-identical in the merged module, not merely
        // behaviorally equivalent): the arbiter's own sticky `ready` IS
        // the grant — `req_i` feeds the arbiter directly and `grant_i` is
        // the raw per-thread `ready` bit.
        //
        // n_slots > 1: the arbiter only sees *waiting* (not-yet-admitted)
        // requesters (`waiting_i`, computed above); `grant_i` is already
        // fully driven by the `sem_comb` block above (`held_i | admit_i`),
        // so only the request side is packed here.
        let mut pack_stmts: Vec<Stmt> = Vec::new();
        for ti in 0..n_threads {
            let req_source = if n_slots > 1 {
                format!("_{}_waiting_{}", res_name, ti)
            } else {
                format!("_{}_req_{}", res_name, ti)
            };
            // _packed[ti] = <req_source>
            pack_stmts.push(Stmt::Assign(CombAssign {
                target: Expr::new(
                    ExprKind::Index(
                        Box::new(Expr::new(ExprKind::Ident(req_packed.clone()), sp)),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                    ),
                    sp,
                ),
                value: Expr::new(ExprKind::Ident(req_source), sp),
                span: sp,
            }));
            if n_slots <= 1 {
                // _grant_ti = _grant_packed[ti]
                pack_stmts.push(Stmt::Assign(CombAssign {
                    target: Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, ti)), sp),
                    value: Expr::new(
                        ExprKind::Index(
                            Box::new(Expr::new(ExprKind::Ident(grant_packed.clone()), sp)),
                            Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                        ),
                        sp,
                    ),
                    span: sp,
                }));
            }
            // _release_packed[ti] = _release_ti (all resource kinds: the
            // synthesized lock arbiter always has the request_release port;
            // for semaphore<N>, the held-register logic doesn't consume it
            // but the arbiter's hold latch still clears harmlessly).
            pack_stmts.push(Stmt::Assign(CombAssign {
                target: Expr::new(
                    ExprKind::Index(
                        Box::new(Expr::new(ExprKind::Ident(release_packed.clone()), sp)),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                    ),
                    sp,
                ),
                value: Expr::new(ExprKind::Ident(format!("_{}_release_{}", res_name, ti)), sp),
                span: sp,
            }));
        }
        merged_body.push(ModuleBodyItem::CombBlock(CombBlock {
            stmts: pack_stmts,
            span: sp,
        }));

        let arb_module_name = format!("_arb_{}_{}", m.name.name, res_name);
        let arb_decl = synthesize_lock_arbiter(
            &arb_module_name,
            n_threads,
            policy,
            hook,
            &clk_name,
            &rst_name,
            _rst_level,
            sp,
        );
        synthesized_arbiters.push(Item::Arbiter(arb_decl));

        // Instantiate the arbiter inside the merged module.
        let inst_name = format!("_arb_inst_{}", res_name);
        merged_body.push(ModuleBodyItem::Inst(InstDecl {
            name: Ident::new(inst_name, sp),
            module_name: Ident::new(arb_module_name, sp),
            param_assigns: Vec::new(),
            auto_connect: None,
            connections: vec![
                Connection {
                    port_name: Ident::new("clk".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(clk_name.clone()), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("rst".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(rst_name.clone()), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("request_valid".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(req_packed.clone()), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("request_ready".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(grant_packed.clone()), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("request_release".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(release_packed.clone()), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("grant_valid".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(gv_sink), sp),
                    reset_override: None,
                    span: sp,
                },
                Connection {
                    port_name: Ident::new("grant_requester".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(gr_sink), sp),
                    reset_override: None,
                    span: sp,
                },
            ],
            for_loops: Vec::new(),
            span: sp,
        }));
    }

    // ── Collect shared(or)/shared(and) signal names for reduction ──────
    // Maps signal name -> its reduction kind. Both `or` and `and` follow
    // the same shadow-wire-plus-reduction lowering; only the fold operator
    // and the idle-thread identity element differ (0 for or, 1 for and —
    // see doc/thread_multi_outstanding_spec.md "Default value").
    let shared_signals: HashMap<String, SharedReduction> = type_map
        .iter()
        .filter_map(|(name, info)| info.shared.map(|r| (name.clone(), r)))
        .collect();

    // shared signals that are seq-driven need per-thread shadow wires + reduction
    let shared_seq: HashMap<String, SharedReduction> = shared_signals
        .iter()
        .filter(|(n, _)| all_seq_driven.contains(n.as_str()))
        .map(|(n, r)| (n.clone(), *r))
        .collect();
    let shared_seq_names: HashSet<String> = shared_seq.keys().cloned().collect();
    // shared signals that are comb-driven use inline accumulation (existing behavior)
    let _shared_comb: HashMap<String, SharedReduction> = shared_signals
        .iter()
        .filter(|(n, _)| all_comb_driven.contains(n.as_str()))
        .map(|(n, r)| (n.clone(), *r))
        .collect();

    // For seq shared signals, create per-thread input wires and the reduction
    let n_threads = threads.len();
    for (sig_name, reduction) in &shared_seq {
        if let Some(info) = type_map.get(sig_name.as_str()) {
            // Per-thread input wires: _sig_in_0, _sig_in_1, ...
            for ti in 0..n_threads {
                let wire_name = format!("_{}_in_{}", sig_name, ti);
                merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: Ident::new(wire_name, sp),
                    ty: info.ty.clone(),
                    unpacked: false,
                    unpacked_ascending: false,
                    span: sp,
                }));
            }
            let fold_op = match reduction {
                SharedReduction::Or => BinOp::BitOr,
                SharedReduction::And => BinOp::BitAnd,
            };
            // Reduction in comb block: sig_next = _sig_in_0 <op> _sig_in_1 <op> ...
            let mut red_expr = Expr::new(ExprKind::Ident(format!("_{}_in_0", sig_name)), sp);
            for ti in 1..n_threads {
                red_expr = Expr::new(
                    ExprKind::Binary(
                        fold_op,
                        Box::new(red_expr),
                        Box::new(Expr::new(
                            ExprKind::Ident(format!("_{}_in_{}", sig_name, ti)),
                            sp,
                        )),
                    ),
                    sp,
                );
            }
            // Wire for reduction result
            let next_name = format!("_{}_next", sig_name);
            merged_body.push(ModuleBodyItem::LetBinding(LetBinding {
                name: Ident::new(next_name.clone(), sp),
                ty: Some(info.ty.clone()),
                value: red_expr,
                span: sp,
                destructure_fields: Vec::new(),
            }));
        }
    }

    // ── Per-thread state machines ──────────────────────────────────────
    let mut all_thread_comb: Vec<Stmt> = Vec::new();
    let mut all_thread_seq: Vec<Stmt> = Vec::new();
    // Release signals are registered one-cycle events. Clear every event at
    // the start of the merged sequential block; state-specific lock-exit
    // assignments appended below override this default on the firing edge.
    let mut release_resources: Vec<&String> = all_resources.iter().collect();
    release_resources.sort();
    for res_name in release_resources {
        for ti in 0..threads.len() {
            all_thread_seq.push(Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_release_{}", res_name, ti)), sp),
                value: Expr::new(ExprKind::Bool(false), sp),
                span: sp,
            }));
        }
    }
    let mut thread_map_threads: Vec<crate::thread_map::ThreadMapThread> = Vec::new();
    // Per-state `localparam` decls (one set per thread). Issue #247: make
    // thread-lowered FSMs debuggable by giving each state a descriptive
    // SV-level name (e.g. `_t0_S2_wait_until`) and emitting state
    // comparisons / assignments as `_t0_state == _t0_S2_wait_until`
    // instead of bare `_t0_state == 2`. Appended to the merged-threads
    // module's `params` list at construction time.
    let mut state_name_params: Vec<ParamDecl> = Vec::new();
    // Auto-emitted SVA spec-contract properties (gated by `opts.auto_asserts`).
    // Reset-guarded antecedent so they don't fire during reset.
    let mut auto_asserts: Vec<AssertDecl> = Vec::new();
    let rst_inactive: Option<Expr> = if opts.auto_asserts {
        let rst_id = Expr::new(ExprKind::Ident(rst_name.clone()), sp);
        Some(match _rst_level {
            // active-low: not_in_reset == rst
            ResetLevel::Low => rst_id,
            // active-high: not_in_reset == !rst
            ResetLevel::High => Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(rst_id)), sp),
        })
    } else {
        None
    };

    for (ti, (_tname, t)) in threads.iter().enumerate() {
        let cnt_width = infer_for_cnt_width(&t.body, &type_map);
        // A `thread` body with no `wait` / `wait until` / `do until`
        // (anywhere — directly or nested inside if/else/for/lock/fork)
        // collapses to a single FSM state and is structurally
        // indistinguishable from a `seq on clk` block. Surface this
        // loudly so users get the construct hint instead of the
        // silent single-state thread (which wastes a state-register
        // flop and obscures intent). The check is applied at the
        // top-level thread body — sub-body recursive calls into
        // `partition_thread_body` (e.g. if/else branches) are
        // permitted to lack waits as long as the outer body has one.
        if !contains_wait(&t.body) {
            errors.push(CompileError::general(
                "thread block must contain at least one `wait` or `do until` statement; \
                 use `seq on clk` for single-cycle logic (and `comb` at module scope \
                 for combinational outputs)",
                t.span,
            ));
            continue;
        }
        let mut loop_id_gen: u32 = 0;
        let mut raw_states =
            match partition_thread_body_with_loop_ids(&t.body, sp, cnt_width, &mut loop_id_gen) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
        let num_loop_counters = loop_id_gen as usize;

        // Rename per-thread: lock signals, counter regs
        // Counters: _cnt → _t{ti}_cnt, _loop_cnt_{id} → _t{ti}_loop_cnt_{id}
        // Each `for` instance in the thread gets a distinct counter
        // (issue #414) — rename all of them with the same per-thread prefix.
        let mut cnt_renames = vec![("_cnt".to_string(), format!("_t{}_cnt", ti))];
        for id in 0..num_loop_counters {
            cnt_renames.push((
                format!("_loop_cnt_{}", id),
                format!("_t{}_loop_cnt_{}", ti, id),
            ));
        }
        for (old, new) in &cnt_renames {
            for state in &mut raw_states {
                rename_ident_in_comb_stmts(&mut state.comb_stmts, old, new);
                rename_ident_in_stmts(&mut state.seq_stmts, old, new);
                if let Some(ref mut cond) = state.transition_cond {
                    rename_ident_in_expr(cond, old, new);
                }
                for (ref mut cond, _) in &mut state.multi_transitions {
                    rename_ident_in_expr(cond, old, new);
                }
                if let Some(LockReleaseInfo::ExitConditions(conditions)) =
                    &mut state.lock_release_info
                {
                    for cond in conditions {
                        rename_ident_in_expr(cond, old, new);
                    }
                }
            }
        }
        // Lock signals
        for res_name in &all_resources {
            let req_old = format!("_{}_req", res_name);
            let req_new = format!("_{}_req_{}", res_name, ti);
            let grant_old = format!("_{}_grant", res_name);
            let grant_new = format!("_{}_grant_{}", res_name, ti);
            for state in &mut raw_states {
                rename_ident_in_comb_stmts(&mut state.comb_stmts, &req_old, &req_new);
                rename_ident_in_comb_stmts(&mut state.comb_stmts, &grant_old, &grant_new);
                rename_ident_in_stmts(&mut state.seq_stmts, &req_old, &req_new);
                rename_ident_in_stmts(&mut state.seq_stmts, &grant_old, &grant_new);
                if let Some(ref mut cond) = state.transition_cond {
                    rename_ident_in_expr(cond, &grant_old, &grant_new);
                }
                for (ref mut cond, _) in &mut state.multi_transitions {
                    rename_ident_in_expr(cond, &grant_old, &grant_new);
                }
                if let Some(LockReleaseInfo::ExitConditions(conditions)) =
                    &mut state.lock_release_info
                {
                    for cond in conditions {
                        rename_ident_in_expr(cond, &grant_old, &grant_new);
                    }
                }
            }
        }
        // Rewrite seq assigns to shared(or)/shared(and) signals → comb assigns
        // to per-thread shadow wires, e.g. `r_ready <= 1` in thread 2 →
        // `_r_ready_in_2 = 1` (comb). The reduction fold happens later.
        if !shared_seq_names.is_empty() {
            for state in &mut raw_states {
                let mut moved_comb = Vec::new();
                let new_seq = rewrite_shared_or_seq_stmts(
                    &state.seq_stmts,
                    &shared_seq_names,
                    ti,
                    sp,
                    &mut moved_comb,
                );
                state.seq_stmts = new_seq;
                state.comb_stmts.extend(moved_comb);
            }
        }

        if raw_states.is_empty() {
            errors.push(CompileError::general(
                "thread must have at least one wait",
                sp,
            ));
            continue;
        }

        // Snapshot source-level transition intent before folded wait-exit
        // optimization mutates the runtime FSM table. The proof sidecar later
        // compacts these targets across folded states, giving the Lean source
        // model a channel that is not copied from the post-fold lowered table.
        let source_transition_intents = thread_source_transition_intents(&raw_states, ti, t.once);

        // Issue #306: fold register assignments from a sole-entry action state
        // that immediately follows a `wait until` state into the wait state's
        // cond-exit arm.  This makes `wait until cond; X <= Y;` fire X on
        // the same clock edge as the cond detection (one cycle earlier than
        // the unfolded two-state form).  The absorbed action state is marked
        // `is_folded` and skipped during codegen — it becomes unreachable.
        fold_wait_until_exit_assignments(&mut raw_states, t.once);

        // Lock release events: for each state marked as the last state of a
        // `lock` body, emit `_<res>_release_<ti> <= 1` sequentially when that
        // state's exit transition fires. The event is registered so the
        // release condition may depend on grant without feeding a
        // combinational release -> grant -> release loop (arch#709). The
        // arbiter consumes the event in its next ownership update and ignores
        // the old owner during the following comb phase, preserving immediate
        // uncontended reacquisition and post-edge handoff. Runs AFTER the
        // fold pass: a folded (absorbed) last state relocates its event into
        // the absorbing predecessor's cond-exit arm. Names are constructed
        // post-rename, so grant/cnt idents inside reused exit conditions are
        // already per-thread.
        #[derive(Clone)]
        enum LockReleaseFire {
            Never,
            Conditional(Expr),
            Unconditional,
        }
        for si in 0..raw_states.len() {
            let Some(res) = raw_states[si].lock_release.clone() else {
                continue;
            };
            // Use a span already inside the state's envelope (the exit
            // condition's, else an existing comb stmt's) so the injected
            // stmt doesn't widen the state's source band in the thread map.
            let rel_sp = raw_states[si]
                .transition_cond
                .as_ref()
                .map(|c| c.span)
                .or_else(|| {
                    raw_states[si].comb_stmts.iter().find_map(|s| match s {
                        Stmt::Assign(a) => Some(a.span),
                        Stmt::IfElse(ie) => Some(ie.span),
                        _ => None,
                    })
                })
                .unwrap_or(sp);
            let rel_assign = Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_release_{}", res, ti)), rel_sp),
                value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), rel_sp),
                span: rel_sp,
            });
            let (target_idx, fire): (usize, LockReleaseFire) = if raw_states[si].is_folded {
                // Absorbed into the preceding wait_until state's exit arm;
                // the fold fires on that state's transition condition.
                (
                    si - 1,
                    raw_states[si - 1]
                        .transition_cond
                        .clone()
                        .map_or(LockReleaseFire::Never, LockReleaseFire::Conditional),
                )
            } else if !raw_states[si].multi_transitions.is_empty() {
                // The lock lowering records whether these arms were created
                // by a construct inside the lock body or by a construct
                // outside it.  A lock used as an outer `for` body must
                // release on every arm (including the outer loop-back), while
                // a `for` nested inside the lock must release only on the
                // nested loop's exit arm.
                let release_conditions: Vec<Expr> = match raw_states[si].lock_release_info.as_ref()
                {
                    Some(LockReleaseInfo::AllTransitions) | None => raw_states[si]
                        .multi_transitions
                        .iter()
                        .map(|(cond, _)| cond.clone())
                        .collect(),
                    Some(LockReleaseInfo::ExitConditions(conditions)) => conditions.clone(),
                };
                let exit_disj = release_conditions.into_iter().reduce(|acc, cond| {
                    let cond_sp = cond.span;
                    Expr::new(
                        ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(cond)),
                        cond_sp,
                    )
                });
                (
                    si,
                    exit_disj.map_or(LockReleaseFire::Never, LockReleaseFire::Conditional),
                )
            } else if let Some(ref c) = raw_states[si].transition_cond {
                (si, LockReleaseFire::Conditional(c.clone()))
            } else if raw_states[si].wait_cycles.is_some() {
                // Counter-based wait exits when the per-thread counter hits 0.
                let cnt_id = Expr::new(ExprKind::Ident(format!("_t{}_cnt", ti)), sp);
                (
                    si,
                    LockReleaseFire::Conditional(Expr::new(
                        ExprKind::Binary(BinOp::Eq, Box::new(cnt_id), Box::new(make_zero_expr(sp))),
                        sp,
                    )),
                )
            } else {
                // Unconditional exit: released every cycle spent in this state.
                (si, LockReleaseFire::Unconditional)
            };
            let stmt = match fire.clone() {
                LockReleaseFire::Never => continue,
                LockReleaseFire::Conditional(cond) => {
                    // Anchor the wrapper (and the assign inside it) to the
                    // fire condition's span — for the folded case that span
                    // belongs to the absorbing predecessor state, keeping
                    // its thread-map source band unchanged.
                    let wrap_sp = cond.span;
                    let mut inner = rel_assign;
                    if let Stmt::Assign(ref mut a) = inner {
                        a.span = wrap_sp;
                        a.target.span = wrap_sp;
                        a.value.span = wrap_sp;
                    }
                    Stmt::IfElse(IfElse {
                        cond,
                        then_stmts: vec![inner],
                        else_stmts: Vec::new(),
                        unique: false,
                        span: wrap_sp,
                    })
                }
                LockReleaseFire::Unconditional => rel_assign,
            };
            raw_states[target_idx].seq_stmts.push(stmt);

            let pending_assign = Stmt::Assign(CombAssign {
                target: Expr::new(
                    ExprKind::Ident(format!("_{}_release_pending_{}", res, ti)),
                    rel_sp,
                ),
                value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), rel_sp),
                span: rel_sp,
            });
            let pending_stmt = match fire {
                LockReleaseFire::Never => continue,
                LockReleaseFire::Conditional(cond) => Stmt::IfElse(IfElse {
                    cond: cond.clone(),
                    then_stmts: vec![pending_assign],
                    else_stmts: Vec::new(),
                    unique: false,
                    span: cond.span,
                }),
                LockReleaseFire::Unconditional => pending_assign,
            };
            raw_states[target_idx].comb_stmts.push(pending_stmt);
        }

        let n_states = raw_states.len();
        let state_reg = format!("_t{}_state", ti);
        let state_bits = crate::width::index_width(n_states as u64) as u64;

        // Derive a descriptive name per state from structural shape (issue #247).
        // Role categories (checked in order, first match wins):
        //   - dispatch:    >1 multi_transitions  (fork/join product or if-dispatch)
        //   - wait_cycles: counter-driven stay-then-advance (`wait N cycle`)
        //   - wait_until:  transition_cond is Some (`wait until cond`)
        //   - entry:       state 0 with none of the above (clean entry state)
        //   - action:      everything else (unconditional advance with body work)
        // Per-thread prefix `_t{ti}_` keeps the names unique within the
        // merged-threads module across multiple threads; the state register
        // itself is also `_t{ti}_state`.
        let state_names: Vec<String> = (0..n_states)
            .map(|si| {
                let s = &raw_states[si];
                let role = thread_map_state_role(si, s);
                format!("_t{}_S{}_{}", ti, si, role)
            })
            .collect();

        // Emit one `localparam [W-1:0] _t{ti}_S{N}_<role> = N;` per state, so
        // SV waveform viewers and source readers can decode the state register
        // by name. The width matches the state register's UInt<W> type; W is
        // `state_bits.max(1)` (clog2-of-N with a floor of 1 for the
        // single-state edge case).
        let w_hi = if state_bits == 0 { 0 } else { state_bits - 1 };
        for si in 0..n_states {
            let hi_lit = Expr::new(ExprKind::Literal(LitKind::Dec(w_hi)), sp);
            let lo_lit = Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp);
            state_name_params.push(ParamDecl {
                name: Ident::new(state_names[si].clone(), sp),
                kind: ParamKind::WidthConst(hi_lit, lo_lit),
                default: Some(Expr::new(ExprKind::Literal(LitKind::Dec(si as u64)), sp)),
                constraint: None,
                is_local: true,
                span: sp,
                unpacked_size: None,
            });
        }

        // Helper: build an Expr that references the state-N localparam by name
        // instead of emitting a bare numeric literal. Replaces the previous
        // `ExprKind::Literal(LitKind::Dec(N))` pattern at every state-reference
        // site below — state == N comparisons, state <= N transition assigns,
        // and the SVA auto-assert state_lit closure. Bare literals would still
        // be correct SV (the localparam evaluates to the same N) but the
        // name-form is the whole point of #247.
        let state_name_expr =
            |id: usize| -> Expr { Expr::new(ExprKind::Ident(state_names[id].clone()), sp) };

        // State register
        merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
            name: Ident::new(state_reg.clone(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(state_bits.max(1))),
                sp,
            ))),
            init: Some(make_zero_expr(sp)),
            reset: RegReset::Inherit(Ident::new(rst_name.clone(), sp), make_zero_expr(sp)),
            guard: None,
            multicycle: None,
            span: sp,
        }));

        if opts.thread_map.is_some() {
            let mut states = Vec::new();
            for (si, raw) in raw_states.iter().enumerate() {
                let natural_next_state = if si + 1 < n_states {
                    si + 1
                } else if t.once {
                    si
                } else {
                    0
                };
                let next_state = raw.folded_exit_target.unwrap_or(natural_next_state);
                let source_next_state =
                    compact_thread_source_target(&raw_states, natural_next_state, t.once);
                let transitions = if !raw.multi_transitions.is_empty() {
                    raw.multi_transitions
                        .iter()
                        .map(|(cond, target)| {
                            let tgt = if *target >= n_states {
                                if t.once {
                                    n_states - 1
                                } else {
                                    0
                                }
                            } else {
                                *target
                            };
                            crate::thread_map::ThreadMapTransition {
                                condition: crate::thread_map::expr_label(cond),
                                condition_guard: Some(crate::thread_map::guard_expr(cond)),
                                target_index: tgt,
                                target_name: state_names[tgt].clone(),
                            }
                        })
                        .collect()
                } else if let Some(cond) = &raw.transition_cond {
                    vec![crate::thread_map::ThreadMapTransition {
                        condition: crate::thread_map::expr_label(cond),
                        condition_guard: Some(crate::thread_map::guard_expr(cond)),
                        target_index: next_state,
                        target_name: state_names[next_state].clone(),
                    }]
                } else if raw.wait_cycles.is_some() {
                    vec![crate::thread_map::ThreadMapTransition {
                        condition: format!("_t{}_cnt == 0", ti),
                        condition_guard: None,
                        target_index: next_state,
                        target_name: state_names[next_state].clone(),
                    }]
                } else {
                    vec![crate::thread_map::ThreadMapTransition {
                        condition: "always".to_string(),
                        condition_guard: Some(crate::thread_map::ThreadMapGuardExpr::True),
                        target_index: next_state,
                        target_name: state_names[next_state].clone(),
                    }]
                };
                states.push(crate::thread_map::ThreadMapState {
                    index: si,
                    state_name: state_names[si].clone(),
                    role: thread_map_state_role(si, raw).to_string(),
                    emitted: !raw.is_folded,
                    span: thread_fsm_state_span(raw, t.span),
                    labels: thread_map_state_labels(raw),
                    source_next_index: source_next_state,
                    source_next_name: state_names[source_next_state].clone(),
                    wait_cycles_count: raw.wait_cycles.as_ref().map(crate::thread_map::expr_label),
                    seq_updates: raw
                        .seq_stmts
                        .iter()
                        .map(crate::thread_map::stmt_label)
                        .collect(),
                    seq_assignments: crate::thread_map::stmt_assignments(&raw.seq_stmts),
                    folded_exit_updates: raw
                        .folded_exit_seq
                        .iter()
                        .map(crate::thread_map::stmt_label)
                        .collect(),
                    folded_exit_assignments: crate::thread_map::stmt_assignments(
                        &raw.folded_exit_seq,
                    ),
                    source_transitions: thread_source_map_transitions(
                        source_transition_intents
                            .get(si)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        &raw_states,
                        &state_names,
                        t.once,
                    ),
                    source_transition_origin: "pre_fold_snapshot".to_string(),
                    transitions,
                });
            }
            thread_map_threads.push(crate::thread_map::ThreadMapThread {
                name: _tname.clone(),
                index: ti,
                once: t.once,
                span: t.span,
                states,
                hazards: Vec::new(),
            });
        }

        // Pre-process: add counter loads on every transition edge into a
        // wait_cycles state. Older lowering only looked at the lexically
        // preceding state, which missed dispatch edges that jump into the
        // first state of a later branch.
        let cnt_name = format!("_t{}_cnt", ti);
        // Collect (state_idx, count_expr, transition_cond) tuples first to avoid borrow conflicts
        let mut counter_loads: Vec<(usize, Expr, Option<Expr>)> = Vec::new();
        for wait_idx in 0..raw_states.len() {
            let Some(count_expr) = raw_states[wait_idx].wait_cycles.clone() else {
                continue;
            };
            for si in 0..raw_states.len() {
                let natural_next = if si + 1 < raw_states.len() {
                    si + 1
                } else if t.once {
                    si
                } else {
                    0
                };
                if !raw_states[si].multi_transitions.is_empty() {
                    for (cond, target) in &raw_states[si].multi_transitions {
                        let resolved = if *target >= raw_states.len() {
                            if t.once {
                                raw_states.len() - 1
                            } else {
                                0
                            }
                        } else {
                            *target
                        };
                        if resolved == wait_idx {
                            counter_loads.push((si, count_expr.clone(), Some(cond.clone())));
                        }
                    }
                } else if raw_states[si].transition_cond.is_some() {
                    if natural_next == wait_idx {
                        counter_loads.push((
                            si,
                            count_expr.clone(),
                            raw_states[si].transition_cond.clone(),
                        ));
                    }
                } else if raw_states[si].wait_cycles.is_some() {
                    if natural_next == wait_idx {
                        let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                        let cnt_zero = Expr::new(
                            ExprKind::Binary(
                                BinOp::Eq,
                                Box::new(cnt_id),
                                Box::new(make_zero_expr(sp)),
                            ),
                            sp,
                        );
                        counter_loads.push((si, count_expr.clone(), Some(cnt_zero)));
                    }
                } else if natural_next == wait_idx {
                    counter_loads.push((si, count_expr.clone(), None));
                }
            }
        }
        for (si, count_expr, cond) in counter_loads {
            // cnt <= (count - 32'd1).trunc<32>()
            let count_span = count_expr.span;
            let sub = Expr::new(
                ExprKind::Binary(
                    BinOp::Sub,
                    Box::new(count_expr.clone()),
                    Box::new(Expr::new(
                        ExprKind::Literal(LitKind::Sized(32, 1)),
                        count_span,
                    )),
                ),
                count_span,
            );
            let load = Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(cnt_name.clone()), count_span),
                value: Expr::new(
                    ExprKind::MethodCall(
                        Box::new(sub),
                        Ident::new("trunc".to_string(), count_span),
                        vec![Expr::new(ExprKind::Literal(LitKind::Dec(32)), count_span)],
                    ),
                    count_span,
                ),
                span: count_span,
            });
            if let Some(guard) = cond {
                raw_states[si].seq_stmts.push(Stmt::IfElse(IfElse {
                    cond: guard,
                    then_stmts: vec![load],
                    else_stmts: Vec::new(),
                    unique: false,
                    span: count_span,
                }));
            } else {
                raw_states[si].seq_stmts.push(load);
            }
        }

        // A comb-overlapped successor is active during the predecessor's
        // transition cycle. Its sequential body must therefore run on that
        // same edge as well; otherwise a ready/valid successor advertises
        // acceptance without consuming the payload. Restrict overlap to a
        // simple conditional successor: dispatch and counter states need
        // entry-value forwarding before they can be collapsed safely.
        let overlap_targets: Vec<Option<usize>> = raw_states
            .iter()
            .enumerate()
            .map(|(si, raw)| {
                if !raw.multi_transitions.is_empty()
                    || raw.terminal_return.is_some()
                    || raw.transition_cond.is_none()
                {
                    return None;
                }
                let next_si = if let Some(folded_tgt) = raw.folded_exit_target {
                    folded_tgt
                } else if si + 1 < n_states {
                    si + 1
                } else if t.once {
                    si
                } else {
                    0
                };
                raw_states.get(next_si).and_then(|next| {
                    (next_si != si
                        && !next.is_folded
                        && !next.is_lock_body
                        && next.transition_cond.is_some()
                        && next.multi_transitions.is_empty()
                        && next.wait_cycles.is_none()
                        && next.terminal_return.is_none()
                        && !next.comb_stmts.is_empty())
                    .then_some(next_si)
                })
            })
            .collect();

        // State transition always_ff
        let mut seq_stmts: Vec<Stmt> = Vec::new();
        let mut seq_stmt_pos_by_state: Vec<Option<usize>> = vec![None; n_states];
        let mut state_bodies: Vec<Option<Vec<Stmt>>> = vec![None; n_states];
        for (si, raw) in raw_states.iter().enumerate() {
            // Issue #306: skip states that were absorbed into a preceding
            // wait_until exit arm.  They are unreachable at runtime.
            if raw.is_folded {
                continue;
            }

            // Only skip truly empty states that don't need state advancement
            let needs_transition = si + 1 < n_states || !t.once; // non-terminal states always need advancement
            if raw.seq_stmts.is_empty()
                && raw.transition_cond.is_none()
                && raw.wait_cycles.is_none()
                && raw.multi_transitions.is_empty()
                && raw.folded_exit_seq.is_empty()
                && !needs_transition
            {
                continue;
            }

            // Build transition + seq logic for this state
            let state_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Eq,
                    Box::new(Expr::new(ExprKind::Ident(state_reg.clone()), sp)),
                    Box::new(state_name_expr(si)),
                ),
                sp,
            );

            let mut body: Vec<Stmt> = Vec::new();

            // Seq assigns (fire on state entry)
            body.extend(raw.seq_stmts.clone());

            // State transitions
            // For thread_once: last state stays (terminal), otherwise wrap to 0.
            // Issue #306: when folded_exit_target is set, the wait_until state
            // transitions directly to that target (skipping the absorbed action
            // state si+1).  For all other transition kinds the natural next-state
            // computation below applies.
            let next_state = if let Some(folded_tgt) = raw.folded_exit_target {
                folded_tgt
            } else if si + 1 < n_states {
                si + 1
            } else if t.once {
                si // terminal: stay in last state
            } else {
                0 // repeating: wrap to first state
            };
            // Counter decrement is intrinsic to a wait_cycles state — it must
            // run regardless of how the transition target is decided. Hoisted
            // out of the wait_cycles transition branch below so that an
            // if/else-with-waits dispatch (which puts a (cnt==0, target)
            // entry in multi_transitions) doesn't accidentally suppress it.
            if raw.wait_cycles.is_some() {
                let cnt_name = format!("_t{}_cnt", ti);
                let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                let sub = Expr::new(
                    ExprKind::Binary(
                        BinOp::Sub,
                        Box::new(cnt_id),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(32, 1)), sp)),
                    ),
                    sp,
                );
                body.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(cnt_name.clone()), sp),
                    value: Expr::new(
                        ExprKind::MethodCall(
                            Box::new(sub),
                            Ident::new("trunc".to_string(), sp),
                            vec![Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp)],
                        ),
                        sp,
                    ),
                    span: sp,
                }));
            }

            if !raw.multi_transitions.is_empty() {
                for (cond, target) in &raw.multi_transitions {
                    let tgt = if *target >= n_states {
                        if t.once {
                            n_states - 1
                        } else {
                            0
                        }
                    } else {
                        *target
                    };
                    body.push(Stmt::IfElse(IfElse {
                        cond: cond.clone(),
                        then_stmts: vec![Stmt::Assign(RegAssign {
                            target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                            value: state_name_expr(tgt),
                            span: sp,
                        })],
                        else_stmts: Vec::new(),
                        unique: false,
                        span: sp,
                    }));
                }
            } else if let Some(ref cond) = raw.transition_cond {
                // Issue #306: if folded_exit_seq is non-empty, include those
                // seq assigns inside the cond-exit arm so they fire on the same
                // clock edge as the wait-exit detection (one cycle earlier than
                // the unfolded two-state form).
                let mut then_stmts: Vec<Stmt> = raw.folded_exit_seq.clone();
                then_stmts.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                    value: state_name_expr(next_state),
                    span: sp,
                }));
                body.push(Stmt::IfElse(IfElse {
                    cond: cond.clone(),
                    then_stmts,
                    else_stmts: Vec::new(),
                    unique: false,
                    span: sp,
                }));
            } else if raw.wait_cycles.is_some() {
                // Default wait_cycles transition: cnt==0 ⇒ next_state.
                let cnt_name = format!("_t{}_cnt", ti);
                let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                let cnt_zero = Expr::new(
                    ExprKind::Binary(BinOp::Eq, Box::new(cnt_id), Box::new(make_zero_expr(sp))),
                    sp,
                );
                body.push(Stmt::IfElse(IfElse {
                    cond: cnt_zero,
                    then_stmts: vec![Stmt::Assign(RegAssign {
                        target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                        value: state_name_expr(next_state),
                        span: sp,
                    })],
                    else_stmts: Vec::new(),
                    unique: false,
                    span: sp,
                }));
            } else {
                // Unconditional transition
                body.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                    value: state_name_expr(next_state),
                    span: sp,
                }));
            }

            // ── Auto-emit SVA spec-contract properties ─────────────────
            // Gated by `--auto-thread-asserts`. Guarded with `rst_inactive`
            // so they don't fire during reset. Skipped for terminal once
            // states (vacuous) and for threads with `default_when` (the
            // soft-reset escape can preempt any state).
            if opts.auto_asserts && t.default_when.is_none() && !(t.once && si + 1 >= n_states) {
                let mk_bin = |op: BinOp, a: Expr, b: Expr| -> Expr {
                    Expr::new(ExprKind::Binary(op, Box::new(a), Box::new(b)), sp)
                };
                let state_lit = |id: usize| state_name_expr(id);
                let state_id = || Expr::new(ExprKind::Ident(state_reg.clone()), sp);
                let state_eq = |id: usize| mk_bin(BinOp::Eq, state_id(), state_lit(id));
                let rst_g = rst_inactive.clone().unwrap();
                let in_state = mk_bin(BinOp::And, rst_g.clone(), state_eq(si));
                let push_assert = |name: String,
                                   antecedent: Expr,
                                   consequent: Expr,
                                   acc: &mut Vec<AssertDecl>| {
                    let prop = mk_bin(BinOp::ImpliesNext, antecedent, consequent);
                    acc.push(AssertDecl {
                        kind: AssertKind::Assert,
                        engine: crate::ast::AssertEngine::Solver,
                        name: Some(Ident::new(name, sp)),
                        expr: prop,
                        span: sp,
                    });
                };

                if !raw.multi_transitions.is_empty() {
                    // Each branch: when its cond fires, state goes to its target.
                    for (bi, (cond, target)) in raw.multi_transitions.iter().enumerate() {
                        let tgt = if *target >= n_states {
                            if t.once {
                                n_states - 1
                            } else {
                                0
                            }
                        } else {
                            *target
                        };
                        let antecedent = mk_bin(BinOp::And, in_state.clone(), cond.clone());
                        push_assert(
                            format!("_auto_thread_t{}_branch_s{}_b{}", ti, si, bi),
                            antecedent,
                            state_eq(tgt),
                            &mut auto_asserts,
                        );
                    }
                } else if let Some(ref cond) = raw.transition_cond {
                    // wait_until cond — guard fires ⇒ FSM advances next edge.
                    let antecedent = mk_bin(BinOp::And, in_state.clone(), cond.clone());
                    let mut consequent = state_eq(next_state);
                    if let Some(overlap_si) = overlap_targets[si] {
                        let overlap = &raw_states[overlap_si];
                        let overlap_next = if let Some(folded_tgt) = overlap.folded_exit_target {
                            folded_tgt
                        } else if overlap_si + 1 < n_states {
                            overlap_si + 1
                        } else if t.once {
                            overlap_si
                        } else {
                            0
                        };
                        if overlap_next != overlap_si {
                            consequent = mk_bin(BinOp::Or, consequent, state_eq(overlap_next));
                        }
                    }
                    push_assert(
                        format!("_auto_thread_t{}_wait_until_s{}", ti, si),
                        antecedent,
                        consequent,
                        &mut auto_asserts,
                    );
                } else if raw.wait_cycles.is_some() {
                    // wait N cycle — counter-driven stay-then-advance.
                    let cnt_name = format!("_t{}_cnt", ti);
                    let cnt_id = || Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                    let zero = || make_zero_expr(sp);
                    let cnt_eq_zero = mk_bin(BinOp::Eq, cnt_id(), zero());
                    let cnt_neq_zero = mk_bin(BinOp::Neq, cnt_id(), zero());
                    let stay_ant = mk_bin(BinOp::And, in_state.clone(), cnt_neq_zero);
                    let done_ant = mk_bin(BinOp::And, in_state.clone(), cnt_eq_zero);
                    push_assert(
                        format!("_auto_thread_t{}_wait_stay_s{}", ti, si),
                        stay_ant,
                        state_eq(si),
                        &mut auto_asserts,
                    );
                    push_assert(
                        format!("_auto_thread_t{}_wait_done_s{}", ti, si),
                        done_ant,
                        state_eq(next_state),
                        &mut auto_asserts,
                    );
                }
                // Unconditional transitions (no cond, no wait, no multi)
                // are not asserted: they're already trivially correct
                // ("|=> next") and add noise without catching anything new.
            }

            state_bodies[si] = Some(body.clone());
            seq_stmt_pos_by_state[si] = Some(seq_stmts.len());
            seq_stmts.push(Stmt::IfElse(IfElse {
                cond: state_cond,
                then_stmts: body,
                else_stmts: Vec::new(),
                unique: false,
                span: sp,
            }));
        }

        for (si, target) in overlap_targets.iter().enumerate() {
            let Some(target_si) = target else {
                continue;
            };
            let Some(source_pos) = seq_stmt_pos_by_state[si] else {
                continue;
            };
            let Some(target_body) = state_bodies[*target_si].clone() else {
                continue;
            };
            let Some(transition_cond) = raw_states[si].transition_cond.clone() else {
                continue;
            };
            let Stmt::IfElse(source_guard) = &mut seq_stmts[source_pos] else {
                unreachable!("thread state lowering always emits an if guard");
            };
            source_guard.then_stmts.push(Stmt::IfElse(IfElse {
                cond: transition_cond,
                then_stmts: target_body,
                else_stmts: Vec::new(),
                unique: false,
                span: sp,
            }));
        }

        // Wrap with `default when` if present: priority soft-reset
        // if (cond) { <assigns>; state <= 0; } else { <normal FSM states> }
        if let Some((dw_cond, dw_thread_stmts)) = &t.default_when {
            // Convert ThreadStmt::SeqAssign items to Stmt::Assign
            let mut dw_then: Vec<Stmt> = dw_thread_stmts
                .iter()
                .filter_map(|ts| {
                    if let ThreadStmt::SeqAssign(ra) = ts {
                        Some(Stmt::Assign(ra.clone()))
                    } else {
                        None // non-seq assigns in default when are silently ignored
                    }
                })
                .collect();
            // Reset state to 0 (the entry state — name-form for #247).
            dw_then.push(Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                value: state_name_expr(0),
                span: sp,
            }));
            all_thread_seq.push(Stmt::IfElse(IfElse {
                cond: dw_cond.clone(),
                then_stmts: dw_then,
                else_stmts: seq_stmts,
                unique: false,
                span: sp,
            }));
        } else {
            all_thread_seq.extend(seq_stmts);
        }

        // Collect comb outputs for this thread (merged into one block later)
        // For shared(or) signals, transform `sig = val` → `sig = sig | val`
        for (si, raw) in raw_states.iter().enumerate() {
            // Issue #306: folded states are unreachable; skip their comb outputs.
            if raw.is_folded {
                continue;
            }

            let state_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Eq,
                    Box::new(Expr::new(ExprKind::Ident(state_reg.clone()), sp)),
                    Box::new(state_name_expr(si)),
                ),
                sp,
            );

            // This state's own comb outputs
            if !raw.comb_stmts.is_empty() {
                let transformed_stmts =
                    transform_shared_or_assigns(&raw.comb_stmts, &shared_signals, sp);
                all_thread_comb.push(Stmt::IfElse(IfElse {
                    cond: state_cond.clone(),
                    then_stmts: transformed_stmts,
                    else_stmts: Vec::new(),
                    unique: false,
                    span: sp,
                }));
            }

            // Comb overlap: when this state's single conditional transition
            // fires, also drive the next state's comb outputs in the same
            // cycle. This lets back-to-back states pipeline without a dead
            // cycle. Restricted to next states that are NOT lock bodies
            // (issue #501): a lock-guarded state's outputs must never appear
            // before the grant cycle, so overlapping into a lock body would
            // leak critical-section outputs into the preceding state.
            // Multi-transition states are skipped — their successor is
            // condition-dependent, not the natural next state. The
            // terminal_return guard is defensive: today `terminal_return` is
            // only set by the TLM response-router partition path, whose
            // states never reach this loop, but the guard keeps overlap
            // correct if that ever changes.
            if let Some(next_si) = overlap_targets[si] {
                if let Some(ref trans_cond) = raw.transition_cond {
                    if let Some(next) = raw_states.get(next_si) {
                        let next_comb =
                            transform_shared_or_assigns(&next.comb_stmts, &shared_signals, sp);
                        let overlap_cond = Expr::new(
                            ExprKind::Binary(
                                BinOp::And,
                                Box::new(state_cond.clone()),
                                Box::new(trans_cond.clone()),
                            ),
                            sp,
                        );
                        all_thread_comb.push(Stmt::IfElse(IfElse {
                            cond: overlap_cond,
                            then_stmts: next_comb,
                            else_stmts: Vec::new(),
                            unique: false,
                            span: sp,
                        }));
                    }
                }
            }
        }
    }

    // Add shared(or)/shared(and) seq reduction: sig <= _sig_next
    for sig_name in shared_seq.keys() {
        all_thread_seq.push(Stmt::Assign(RegAssign {
            target: Expr::new(ExprKind::Ident(sig_name.clone()), sp),
            value: Expr::new(ExprKind::Ident(format!("_{}_next", sig_name)), sp),
            span: sp,
        }));
    }

    // Single merged always_ff for all threads (avoids multi-driver on shared regs)
    if !all_thread_seq.is_empty() {
        merged_body.push(ModuleBodyItem::RegBlock(RegBlock {
            clock: Ident::new(clk_name.clone(), sp),
            clock_edge: ClockEdge::Rising,
            stmts: all_thread_seq,
            span: sp,
        }));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    if let Some(map) = &opts.thread_map {
        map.borrow_mut()
            .modules
            .push(crate::thread_map::ThreadMapModule {
                module_name: m.name.name.clone(),
                generated_module_name: merged_name.clone(),
                span: m.span,
                threads: thread_map_threads,
            });
    }

    // ── Per-thread counter registers ─────────────────────────────────────
    for (ti, (_, t)) in threads.iter().enumerate() {
        let has_counter = thread_has_wait_cycles(&t.body);
        if has_counter {
            merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(format!("_t{}_cnt", ti), sp),
                ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp))),
                init: Some(make_zero_expr(sp)),
                reset: RegReset::None,
                guard: None,
                multicycle: None,
                span: sp,
            }));
        }
        // Per-`for`-instance loop counter regs. Each `for` (including nested
        // ones) gets its own `_t{ti}_loop_cnt_{id}` register so the inner
        // loop's increment doesn't clobber the outer loop's index
        // (issue #414). The id matches the allocation order used by
        // `lower_thread_for` via the shared `loop_id_gen`.
        let num_for_instances = count_for_instances(&t.body);
        if num_for_instances > 0 {
            let for_cnt_width = infer_for_cnt_width(&t.body, &type_map);
            for id in 0..num_for_instances {
                merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                    name: Ident::new(format!("_t{}_loop_cnt_{}", ti, id), sp),
                    ty: TypeExpr::UInt(Box::new(Expr::new(
                        ExprKind::Literal(LitKind::Dec(for_cnt_width as u64)),
                        sp,
                    ))),
                    init: Some(make_zero_expr(sp)),
                    reset: RegReset::None,
                    guard: None,
                    multicycle: None,
                    span: sp,
                }));
            }
        }
    }

    // ── Merged comb block: defaults + all per-thread comb stmts ──────
    let mut merged_comb: Vec<Stmt> = Vec::new();
    // Defaults: all comb outputs = 0
    //
    // Vec<T,N> ports need per-element zeros, not a bare `0` literal:
    //   - unpacked SV emission rejects scalar-to-unpacked-array assignment.
    //   - packed SV accepts `0` but the sim_codegen C++ path lowers the port
    //     to `uint64_t[N]`, which is not assignable as a whole array
    //     (`_foo = 0;` → "array type 'uint64_t[N]' is not assignable").
    // Per-lane assignment (`foo[i] = 0;`) is valid for both shapes on both
    // backends, so we apply it to any Vec output regardless of the
    // `unpacked` modifier.
    for p in &merged_ports {
        if p.direction == Direction::Out && p.default.is_some() {
            if let TypeExpr::Vec(_, size_expr) = &p.ty {
                if let Some(n) = try_eval_i64(size_expr, &HashMap::new()) {
                    for i in 0..(n as u64) {
                        merged_comb.push(Stmt::Assign(CombAssign {
                            target: Expr::new(
                                ExprKind::Index(
                                    Box::new(Expr::new(ExprKind::Ident(p.name.name.clone()), sp)),
                                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(i)), sp)),
                                ),
                                sp,
                            ),
                            value: make_zero_expr(sp),
                            span: sp,
                        }));
                    }
                    continue;
                }
                // Fall through (unknown shape) — let the codegen lint catch it.
            }
            merged_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(p.name.name.clone()), sp),
                value: p.default.as_ref().unwrap().clone(),
                span: sp,
            }));
        }
    }
    // Default lock req = 0. Release events are registered and are cleared by
    // the merged sequential block above before state-specific release events
    // override them on the same edge.
    for res_name in &all_resources {
        for ti in 0..threads.len() {
            merged_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp),
                value: Expr::new(ExprKind::Bool(false), sp),
                span: sp,
            }));
            merged_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(
                    ExprKind::Ident(format!("_{}_release_pending_{}", res_name, ti)),
                    sp,
                ),
                value: Expr::new(ExprKind::Bool(false), sp),
                span: sp,
            }));
        }
    }
    // Default shared(or)/shared(and) seq per-thread input wires = reduction identity.
    // A thread that hasn't reached its drive point contributes the identity
    // element so it doesn't skew the reduction: 0 for or, 1 for and.
    for (sig_name, reduction) in &shared_seq {
        let identity = match reduction {
            SharedReduction::Or => make_zero_expr(sp),
            SharedReduction::And => make_ones_expr(sp),
        };
        for ti in 0..n_threads {
            merged_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_in_{}", sig_name, ti)), sp),
                value: identity.clone(),
                span: sp,
            }));
        }
    }
    // Thread-level `default comb` assignments run unconditionally before
    // state-specific comb assignments. This preserves explicit protocol
    // defaults during compiler-inserted dead-skid states while still letting
    // the active state override them later in the same always_comb block.
    for (_, t) in &threads {
        merged_comb.extend(t.default_comb.iter().cloned());
    }
    // Per-thread state-guarded comb assigns
    merged_comb.extend(all_thread_comb);
    if !merged_comb.is_empty() {
        // arch#709: emit the lock request wires from their own always_comb so
        // the block's grant reads can't fabricate a grant -> req dependency
        // edge and lint as a combinational loop. Falls back to the single
        // merged block when the split isn't provably semantics-preserving.
        let req_names: HashSet<String> = all_resources
            .iter()
            .flat_map(|res| (0..n_threads).map(move |ti| format!("_{}_req_{}", res, ti)))
            .collect();
        match split_lock_req_comb(&merged_comb, &req_names) {
            Some((req_stmts, rest_stmts)) => {
                if !rest_stmts.is_empty() {
                    merged_body.insert(
                        0,
                        ModuleBodyItem::CombBlock(CombBlock {
                            stmts: rest_stmts,
                            span: sp,
                        }),
                    );
                }
                merged_body.insert(
                    0,
                    ModuleBodyItem::CombBlock(CombBlock {
                        stmts: req_stmts,
                        span: sp,
                    }),
                );
            }
            None => {
                merged_body.insert(
                    0,
                    ModuleBodyItem::CombBlock(CombBlock {
                        stmts: merged_comb,
                        span: sp,
                    }),
                );
            }
        }
    }

    // Prepend parent-module function clones so thread-body calls inside
    // `merged_body` (e.g. `MacRes(...)`) resolve when the submodule is
    // emitted as standalone SV. See note at `parent_functions`'s declaration.
    for f in parent_functions.into_iter().rev() {
        merged_body.insert(0, f);
    }

    // Auto-emitted SVA spec-contract properties from `--auto-thread-asserts`.
    // Flow through the existing module-level assert path
    // (codegen.rs `emit_asserts_for_construct` → `synopsys translate_off/on`).
    for a in auto_asserts {
        merged_body.push(ModuleBodyItem::Assert(a));
    }

    // Append per-thread state-name localparams (issue #247) AFTER parent
    // params so the inst-site parameter override order (which matches
    // parent's param list) is preserved. The state-name params are all
    // `is_local: true` → emit as SV `localparam`, not overridable from
    // the inst site, so they don't appear in the connection list anyway.
    let mut merged_params = parent_params;
    merged_params.extend(state_name_params);

    // Rewrite bus-port `FieldAccess(Ident(b), v)` → `Ident("b_v")` everywhere
    // in the synthesized sub-module body. Required because the sub-module
    // doesn't carry the bus ports themselves (only the flattened signal
    // outputs `b_v`, ...), so the original thread-body references to
    // `b.v` need to land on the flat names.
    rewrite_bus_targets_in_body(&mut merged_body, &bus_port_map, &vob_counts);

    let merged_module = ModuleDecl {
        name: Ident::new(merged_name.clone(), sp),
        params: merged_params,
        ports: merged_ports.clone(),
        body: merged_body,
        implements: None,
        hooks: Vec::new(),
        cdc_safe: false,
        rdc_safe: false,
        comb_loops_allowed: false,
        allow_dead_skid_feedback: false,
        span: sp,
        doc: None,
        inner_doc: None,
        is_interface: false,
    };

    // ── Create InstDecl in parent module ───────────────────────────────
    //
    // Parent wrapper exposes Vec-of-bus ports in PACKED form
    // (`ins_<sig> [N-1:0]` — see src/codegen). The threads sub-module
    // expects FLAT per-element signal names (`ins_<i>_<sig>`). Detect
    // when a sub-port name follows the `<base>_<i>_<sig>` pattern AND
    // `<base>` is a Vec-of-bus port on the wrapper, and emit the
    // signal as an Index into the packed wrapper port:
    //   port_name="ins_0_r_valid", signal=Index(Ident("ins_r_valid"), 0)
    // emit_expr_str renders this as `ins_r_valid[0]` — which IS a valid
    // SV expression slicing the packed wrapper port.
    use std::collections::HashMap as _HashMap;
    let parent_vob: _HashMap<String, (u32, Vec<String>)> = {
        let mut out = _HashMap::new();
        for p in &m.ports {
            let Some(bi) = p.bus_info.as_ref() else {
                continue;
            };
            let Some(count_expr) = bi.count.as_ref() else {
                continue;
            };
            // Build a tiny param-vals map from the module's param defaults so
            // expressions like `NUM_SLAVES` resolve at elaboration-time.
            let param_vals_local: HashMap<String, i64> = m
                .params
                .iter()
                .filter_map(|p| {
                    p.default.as_ref().and_then(|d| {
                        try_eval_i64(d, &HashMap::new()).map(|v| (p.name.name.clone(), v))
                    })
                })
                .collect();
            let n = try_eval_i64(count_expr, &param_vals_local).unwrap_or(0) as u32;
            if n == 0 {
                continue;
            }
            let Some(bus_decl) = bus_defs.get(&bi.bus_name.name) else {
                continue;
            };
            // BusDecl.signals lists unconditional signals; conditional ones
            // (under READ/WRITE flags) live in BusDecl.generates. Collect
            // names from BOTH so the `<base>_<i>_<sig>` pattern matches
            // every flattened sub-port name.
            let mut sigs: Vec<String> = bus_decl
                .signals
                .iter()
                .map(|s| s.name.name.clone())
                .collect();
            for g in &bus_decl.generates {
                for s in g.then_signals.iter().chain(g.else_signals.iter()) {
                    sigs.push(s.name.name.clone());
                }
            }
            out.insert(p.name.name.clone(), (n, sigs));
        }
        out
    };
    let mut connections: Vec<Connection> = Vec::new();
    for p in &merged_ports {
        let dir = match p.direction {
            Direction::In => ConnectDir::Input,
            Direction::Out => ConnectDir::Output,
        };
        // Try to decompose `<base>_<i>_<sig>` for Vec-of-bus parent ports.
        let mut signal = Expr::new(ExprKind::Ident(p.name.name.clone()), sp);
        for (base, (n, sigs)) in &parent_vob {
            let prefix = format!("{base}_");
            let Some(rest) = p.name.name.strip_prefix(&prefix) else {
                continue;
            };
            // rest looks like "<i>_<sig>" — split on first '_'.
            let Some(und) = rest.find('_') else {
                continue;
            };
            let idx_str = &rest[..und];
            let sig = &rest[und + 1..];
            let Ok(idx) = idx_str.parse::<u32>() else {
                continue;
            };
            if idx >= *n {
                continue;
            }
            if !sigs.iter().any(|s| s == sig) {
                continue;
            }
            // Match. Emit Index(Ident("<base>_<sig>"), idx).
            signal = Expr::new(
                ExprKind::Index(
                    Box::new(Expr::new(ExprKind::Ident(format!("{base}_{sig}")), sp)),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(idx as u64)), sp)),
                ),
                sp,
            );
            break;
        }
        connections.push(Connection {
            port_name: p.name.clone(),
            direction: dir,
            signal,
            reset_override: None,
            span: sp,
        });
    }
    let inst = InstDecl {
        name: Ident::new("_threads".to_string(), sp),
        module_name: Ident::new(merged_name, sp),
        auto_connect: None,
        param_assigns: m
            .params
            .iter()
            .filter(|p| !p.is_local)
            .map(|p| ParamAssign {
                name: p.name.clone(),
                value: Expr::new(ExprKind::Ident(p.name.name.clone()), p.span),
                ty: None,
            })
            .collect(),
        connections,
        for_loops: Vec::new(),
        span: sp,
    };
    new_body.push(ModuleBodyItem::Inst(inst));

    // Thread-driven regs live inside the synthesized threads module. Keep a
    // typed parent-side wire for each moved reg so ordinary parent logic can
    // still read the instance output with the original signedness/width.
    let thread_driven: HashSet<String> = all_seq_driven
        .iter()
        .chain(all_comb_driven.iter())
        .cloned()
        .collect();
    for item in &mut new_body {
        let ModuleBodyItem::RegDecl(r) = item else {
            continue;
        };
        if thread_driven.contains(&r.name.name) {
            *item = ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: r.name.clone(),
                ty: r.ty.clone(),
                unpacked: false,
                unpacked_ascending: false,
                span: r.span,
            });
        }
    }

    let new_module = ModuleDecl {
        body: new_body,
        ..m
    };
    let mut extras = synthesized_arbiters;
    extras.push(Item::Module(merged_module));
    Ok((new_module, extras))
}

/// Build the per-resource lock arbiter (one `ArbiterDecl` per `resource`,
/// instantiated inside the merged threads module).
///
/// Shape mirrors a standalone `arbiter` written by hand:
/// - `param NUM_REQ: const = <n_threads>;`
/// - `port clk: in Clock<...>; port rst: in Reset<...>;`
/// - `ports[NUM_REQ] request { valid: in Bool; ready: out Bool; }`
/// - `port grant_valid: out Bool; port grant_requester: out UInt<W>;`
/// - `policy <P>;` and optional `hook grant_select(...) = FnName(...);`
///
/// Reusing `ArbiterDecl` makes every policy supported by the standalone
/// arbiter — round_robin / priority / lru / weighted / custom — available
/// to `lock`-block arbitration without duplicating arbitration codegen.
pub(crate) fn synthesize_lock_arbiter(
    arb_module_name: &str,
    n_threads: usize,
    policy: ArbiterPolicy,
    hook: Option<ArbiterHookDecl>,
    clk_name: &str,
    rst_name: &str,
    rst_level: ResetLevel,
    sp: Span,
) -> ArbiterDecl {
    // Reset kind: synthesized arbiter inherits Async from the merged
    // module's reset (matches the merged module itself, which uses Async
    // for thread-driven resets).
    let rst_ty = TypeExpr::Reset(ResetKind::Async, rst_level);
    let clk_ty = TypeExpr::Clock(Ident::new("SysDomain".to_string(), sp));
    let n_threads_expr = Expr::new(ExprKind::Literal(LitKind::Dec(n_threads as u64)), sp);
    let gr_width = crate::width::index_width(n_threads as u64);

    // The arbiter is an internal synthesized module; its port names are
    // canonical (`clk` / `rst`) regardless of the parent's reset signal name.
    let _ = clk_name;
    let _ = rst_name;
    let scalar_ports = vec![
        PortDecl {
            name: Ident::new("clk".to_string(), sp),
            direction: Direction::In,
            ty: clk_ty,
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        },
        PortDecl {
            name: Ident::new("rst".to_string(), sp),
            direction: Direction::In,
            ty: rst_ty,
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        },
        PortDecl {
            name: Ident::new("grant_valid".to_string(), sp),
            direction: Direction::Out,
            ty: TypeExpr::Bool,
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        },
        PortDecl {
            name: Ident::new("grant_requester".to_string(), sp),
            direction: Direction::Out,
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(gr_width as u64)),
                sp,
            ))),
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            unpacked: false,
            unpacked_ascending: false,
            split: false,
            comb_deps: None,
            span: sp,
        },
    ];

    let request_array = PortArrayDecl {
        count_expr: Expr::new(ExprKind::Ident("NUM_REQ".to_string()), sp),
        name: Ident::new("request".to_string(), sp),
        signals: vec![
            PortDecl {
                name: Ident::new("valid".to_string(), sp),
                direction: Direction::In,
                ty: TypeExpr::Bool,
                default: None,
                reg_info: None,
                bus_info: None,
                shared: None,
                unpacked: false,
                unpacked_ascending: false,
                split: false,
                comb_deps: None,
                span: sp,
            },
            PortDecl {
                name: Ident::new("ready".to_string(), sp),
                direction: Direction::Out,
                ty: TypeExpr::Bool,
                default: None,
                reg_info: None,
                bus_info: None,
                shared: None,
                unpacked: false,
                unpacked_ascending: false,
                split: false,
                comb_deps: None,
                span: sp,
            },
            // Release pulse: asserted combinationally by the owner's last
            // lock-body state when its exit transition fires. Clears the
            // hold latch so back-to-back re-acquisition (request never
            // deasserting) still re-arbitrates at the boundary.
            PortDecl {
                name: Ident::new("release".to_string(), sp),
                direction: Direction::In,
                ty: TypeExpr::Bool,
                default: None,
                reg_info: None,
                bus_info: None,
                shared: None,
                unpacked: false,
                unpacked_ascending: false,
                split: false,
                comb_deps: None,
                span: sp,
            },
        ],
        span: sp,
    };

    ArbiterDecl {
        common: ConstructCommon {
            name: Ident::new(arb_module_name.to_string(), sp),
            params: vec![ParamDecl {
                name: Ident::new("NUM_REQ".to_string(), sp),
                kind: ParamKind::Const,
                default: Some(n_threads_expr),
                constraint: None,
                is_local: false,
                span: sp,
                unpacked_size: None,
            }],
            ports: scalar_ports,
            asserts: Vec::new(),
            span: sp,
            doc: None,
            inner_doc: None,
            is_interface: false,
        },
        port_arrays: vec![request_array],
        policy,
        hook,
        latency: 1,
        handshakes: Vec::new(),
        lock_hold: true,
    }
}

// Old multi-FSM approach removed. See git history for reference.

/// Like `expr_root_name` but for assignment targets: returns the
/// `<port>_<sig>` flat name when the target is a `FieldAccess` on a known
/// bus port. Falls back to the root name otherwise. Index-on-bus-port
/// (`chans[i].sig`) returns `<port>_<i>_<sig>` for literal `i`; non-literal
/// `i` (loop variable) returns just the root name and the caller is
/// responsible for the wildcard expansion against the Vec-of-bus count map.
fn expr_target_flat_name(e: &Expr, bus_port_map: &HashMap<String, String>) -> Option<String> {
    match &e.kind {
        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(base_name) = &base.kind {
                if bus_port_map.contains_key(base_name) {
                    return Some(format!("{}_{}", base_name, field.name));
                }
            }
            if let ExprKind::Index(arr, idx) = &base.kind {
                if let (ExprKind::Ident(arr_name), ExprKind::Literal(LitKind::Dec(i))) =
                    (&arr.kind, &idx.kind)
                {
                    if bus_port_map.contains_key(arr_name) {
                        return Some(format!("{}_{}_{}", arr_name, i, field.name));
                    }
                }
            }
            expr_root_name(base)
        }
        _ => expr_root_name(e),
    }
}

/// For a *variable*-index Vec<Bus> write target (`arr[idx].field` with a
/// non-literal `idx`), return every flattened per-lane signal name
/// (`arr_0_field` … `arr_{N-1}_field`). These all become driven outputs of
/// the synthesized thread sub-module because the write is lowered to a
/// per-lane demux (see `rewrite_bus_targets_in_body`). Returns `None` for
/// constant indices and non-bus / non-Vec<Bus> targets.
fn vob_write_lane_names(
    e: &Expr,
    bus_port_map: &HashMap<String, String>,
    vob_counts: &HashMap<String, u32>,
) -> Option<Vec<String>> {
    let ExprKind::FieldAccess(base, field) = &e.kind else {
        return None;
    };
    let ExprKind::Index(arr, idx) = &base.kind else {
        return None;
    };
    let ExprKind::Ident(arr_name) = &arr.kind else {
        return None;
    };
    if matches!(idx.kind, ExprKind::Literal(LitKind::Dec(_))) {
        return None;
    }
    if !bus_port_map.contains_key(arr_name) {
        return None;
    }
    vob_counts.get(arr_name).map(|&n| {
        (0..n)
            .map(|i| format!("{}_{}_{}", arr_name, i, field.name))
            .collect()
    })
}

/// Walk every `Stmt` in the synthesized thread sub-module body and
/// replace bus-port FieldAccess expressions (`b.v`) with the flat ident
/// (`b_v`) on both LHS and RHS. The sub-module exposes the flat signals
/// as ports — `b` itself was never carried over — so any reference to
/// the bus name must be rewritten before SV codegen.
fn rewrite_bus_targets_in_body(
    body: &mut Vec<ModuleBodyItem>,
    bus_port_map: &HashMap<String, String>,
    vob_counts: &HashMap<String, u32>,
) {
    // Build a runtime mux `(idx==0 ? arr_0_field : … : arr_{n-1}_field)`
    // selecting the right flattened lane for a *variable* index into a
    // Vec<Bus> read. Mirrors the SV/sim packed-array form `arr_field[idx]`
    // but over the sub-module's per-lane scalar input ports.
    fn build_lane_mux(arr: &str, field: &str, idx: &Expr, n: u32) -> ExprKind {
        let sp = idx.span;
        let mut acc = ExprKind::Ident(format!("{arr}_{}_{field}", n - 1));
        for i in (0..n - 1).rev() {
            let cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Eq,
                    Box::new(idx.clone()),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(i as u64)), sp)),
                ),
                sp,
            );
            let then_e = Expr::new(ExprKind::Ident(format!("{arr}_{i}_{field}")), sp);
            acc = ExprKind::Ternary(
                Box::new(cond),
                Box::new(then_e),
                Box::new(Expr::new(acc, sp)),
            );
        }
        acc
    }
    // `is_lhs` gates the variable-index mux: a mux is not an lvalue, so it
    // must only be substituted in read (RHS / condition) positions, never
    // as an assignment target. (Variable-index bus *writes* in threads are
    // a separate, currently-unsupported case — they are left unrewritten.)
    fn rw_expr(
        e: &mut Expr,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &HashMap<String, u32>,
        is_lhs: bool,
    ) {
        // Bottom-up: rewrite children first. Children are never themselves
        // assignment targets, so they recurse with is_lhs = false.
        match &mut e.kind {
            ExprKind::Binary(_, l, r) => {
                rw_expr(l, bus_port_map, vob_counts, false);
                rw_expr(r, bus_port_map, vob_counts, false);
            }
            ExprKind::Unary(_, x)
            | ExprKind::Cast(x, _)
            | ExprKind::LatencyAt(x, _)
            | ExprKind::SvaNext(_, x) => rw_expr(x, bus_port_map, vob_counts, false),
            ExprKind::Index(b, i) | ExprKind::BitSlice(b, i, _) => {
                rw_expr(b, bus_port_map, vob_counts, false);
                rw_expr(i, bus_port_map, vob_counts, false);
            }
            ExprKind::PartSelect(b, lo, hi, _) => {
                rw_expr(b, bus_port_map, vob_counts, false);
                rw_expr(lo, bus_port_map, vob_counts, false);
                rw_expr(hi, bus_port_map, vob_counts, false);
            }
            ExprKind::Ternary(c, t, e2) => {
                rw_expr(c, bus_port_map, vob_counts, false);
                rw_expr(t, bus_port_map, vob_counts, false);
                rw_expr(e2, bus_port_map, vob_counts, false);
            }
            ExprKind::Concat(parts) | ExprKind::FunctionCall(_, parts) => {
                for p in parts {
                    rw_expr(p, bus_port_map, vob_counts, false);
                }
            }
            ExprKind::MethodCall(b, _, args) => {
                rw_expr(b, bus_port_map, vob_counts, false);
                for a in args {
                    rw_expr(a, bus_port_map, vob_counts, false);
                }
            }
            ExprKind::FieldAccess(b, _) => rw_expr(b, bus_port_map, vob_counts, false),
            _ => {}
        }
        // Now check if THIS node is a bus-port FieldAccess to rewrite.
        let replacement = match &e.kind {
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if bus_port_map.contains_key(base_name) {
                        Some(ExprKind::Ident(format!("{}_{}", base_name, field.name)))
                    } else {
                        None
                    }
                } else if let ExprKind::Index(arr, idx) = &base.kind {
                    if let ExprKind::Ident(arr_name) = &arr.kind {
                        if let ExprKind::Literal(LitKind::Dec(i)) = &idx.kind {
                            if bus_port_map.contains_key(arr_name) {
                                Some(ExprKind::Ident(format!(
                                    "{}_{}_{}",
                                    arr_name, i, field.name
                                )))
                            } else {
                                None
                            }
                        } else if !is_lhs {
                            // Variable index in a read position → lane mux.
                            vob_counts
                                .get(arr_name)
                                .map(|&n| build_lane_mux(arr_name, &field.name, idx, n))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(new_kind) = replacement {
            e.kind = new_kind;
        }
    }
    // Detect an assignment target of the form `arr[idx].field` where `arr`
    // is a Vec<Bus> port and `idx` is *not* a compile-time literal. Returns
    // `(arr, field, idx, N)`. Such a write can't pick a static lane, so it
    // is expanded into a per-lane demux (see `rw_stmts`). Constant indices
    // are handled by the in-place `rw_expr` rewrite (single lane).
    fn var_index_bus_write_target<'a>(
        e: &'a Expr,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &'a HashMap<String, u32>,
    ) -> Option<(&'a str, &'a str, &'a Expr, u32)> {
        let ExprKind::FieldAccess(base, field) = &e.kind else {
            return None;
        };
        let ExprKind::Index(arr, idx) = &base.kind else {
            return None;
        };
        let ExprKind::Ident(arr_name) = &arr.kind else {
            return None;
        };
        if matches!(idx.kind, ExprKind::Literal(LitKind::Dec(_))) {
            return None;
        }
        if !bus_port_map.contains_key(arr_name) {
            return None;
        }
        vob_counts
            .get(arr_name)
            .map(|&n| (arr_name.as_str(), field.name.as_str(), idx.as_ref(), n))
    }
    // Rewrite a statement *list*, allowing one statement to expand into many.
    // A variable-index Vec<Bus> write `arr[idx].field <op> v` is expanded to
    // a per-lane demux — one guarded assign per lane:
    //   if (idx == 0) arr_0_field <op> v;
    //   …
    //   if (idx == N-1) arr_{N-1}_field <op> v;
    // For seq (`<=`) targets the unselected lanes simply hold; for comb the
    // surrounding default assignment covers them. This is the write-side
    // mirror of the read-side lane mux in `rw_expr`.
    fn rw_stmts(
        stmts: &mut Vec<Stmt>,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &HashMap<String, u32>,
    ) {
        let mut i = 0;
        while i < stmts.len() {
            let expansion: Option<Vec<Stmt>> = if let Stmt::Assign(a) = &stmts[i] {
                var_index_bus_write_target(&a.target, bus_port_map, vob_counts).map(
                    |(arr, field, idx, n)| {
                        let span = a.span;
                        // The RHS (and the index) are read positions — apply
                        // the normal rewrite (incl. read-side lane mux) once,
                        // then reuse for every lane.
                        let mut value = a.value.clone();
                        rw_expr(&mut value, bus_port_map, vob_counts, false);
                        let mut idx_e = idx.clone();
                        rw_expr(&mut idx_e, bus_port_map, vob_counts, false);
                        (0..n)
                            .map(|lane| {
                                let cond = Expr::new(
                                    ExprKind::Binary(
                                        BinOp::Eq,
                                        Box::new(idx_e.clone()),
                                        Box::new(Expr::new(
                                            ExprKind::Literal(LitKind::Dec(lane as u64)),
                                            span,
                                        )),
                                    ),
                                    span,
                                );
                                let assign = Stmt::Assign(Assign {
                                    target: Expr::new(
                                        ExprKind::Ident(format!("{arr}_{lane}_{field}")),
                                        span,
                                    ),
                                    value: value.clone(),
                                    span,
                                });
                                Stmt::IfElse(IfElseOf {
                                    cond,
                                    then_stmts: vec![assign],
                                    else_stmts: vec![],
                                    unique: false,
                                    span,
                                })
                            })
                            .collect()
                    },
                )
            } else {
                None
            };
            if let Some(expanded) = expansion {
                let cnt = expanded.len();
                stmts.splice(i..=i, expanded);
                i += cnt;
                continue;
            }
            rw_stmt(&mut stmts[i], bus_port_map, vob_counts);
            i += 1;
        }
    }
    fn rw_stmt(
        s: &mut Stmt,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &HashMap<String, u32>,
    ) {
        match s {
            Stmt::Assign(a) => {
                rw_expr(&mut a.target, bus_port_map, vob_counts, true);
                rw_expr(&mut a.value, bus_port_map, vob_counts, false);
            }
            Stmt::IfElse(ie) => {
                rw_expr(&mut ie.cond, bus_port_map, vob_counts, false);
                rw_stmts(&mut ie.then_stmts, bus_port_map, vob_counts);
                rw_stmts(&mut ie.else_stmts, bus_port_map, vob_counts);
            }
            Stmt::Match(m) => {
                rw_expr(&mut m.scrutinee, bus_port_map, vob_counts, false);
                for arm in &mut m.arms {
                    rw_stmts(&mut arm.body, bus_port_map, vob_counts);
                }
            }
            Stmt::For(f) => {
                rw_stmts(&mut f.body, bus_port_map, vob_counts);
            }
            Stmt::Init(ib) => {
                rw_stmts(&mut ib.body, bus_port_map, vob_counts);
            }
            Stmt::DoUntil { body, cond, .. } => {
                rw_expr(cond, bus_port_map, vob_counts, false);
                rw_stmts(body, bus_port_map, vob_counts);
            }
            Stmt::WaitUntil(e, _) => rw_expr(e, bus_port_map, vob_counts, false),
            Stmt::Log(l) => {
                for a in &mut l.args {
                    rw_expr(a, bus_port_map, vob_counts, false);
                }
            }
        }
    }
    for item in body.iter_mut() {
        match item {
            ModuleBodyItem::CombBlock(cb) => {
                rw_stmts(&mut cb.stmts, bus_port_map, vob_counts);
            }
            ModuleBodyItem::RegBlock(rb) => {
                rw_stmts(&mut rb.stmts, bus_port_map, vob_counts);
            }
            ModuleBodyItem::LatchBlock(lb) => {
                rw_stmts(&mut lb.stmts, bus_port_map, vob_counts);
            }
            ModuleBodyItem::LetBinding(lb) => {
                rw_expr(&mut lb.value, bus_port_map, vob_counts, false)
            }
            ModuleBodyItem::RegDecl(r) => {
                if let Some(init) = r.init.as_mut() {
                    rw_expr(init, bus_port_map, vob_counts, false);
                }
            }
            _ => {}
        }
    }
}

fn build_module_reg_map(m: &ModuleDecl) -> HashMap<String, RegDecl> {
    let mut map = HashMap::new();
    for item in &m.body {
        if let ModuleBodyItem::RegDecl(r) = item {
            map.insert(r.name.name.clone(), r.clone());
        }
    }
    map
}

// ── Signal analysis ─────────────────────────────────────────────────────────

fn collect_comb_stmt_signals_with_buses(
    stmts: &[Stmt],
    bus_port_map: &HashMap<String, String>,
) -> (HashSet<String>, HashSet<String>) {
    let (mut comb_driven, all_read) = collect_comb_stmt_signals(stmts);
    // Replace bus-port root names ("b") with their flattened equivalents
    // ("b_v"). The underlying expr_root_name walker can't distinguish them
    // since it doesn't know which signals are bus ports.
    comb_driven.retain(|n| !bus_port_map.contains_key(n));
    fn walk(
        stmts: &[Stmt],
        comb_driven: &mut HashSet<String>,
        bus_port_map: &HashMap<String, String>,
    ) {
        for s in stmts {
            match s {
                Stmt::Assign(a) => {
                    if let Some(name) = expr_target_flat_name(&a.target, bus_port_map) {
                        if !bus_port_map.contains_key(&name) {
                            comb_driven.insert(name);
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    walk(&ie.then_stmts, comb_driven, bus_port_map);
                    walk(&ie.else_stmts, comb_driven, bus_port_map);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        walk(&arm.body, comb_driven, bus_port_map);
                    }
                }
                Stmt::For(f) => walk(&f.body, comb_driven, bus_port_map),
                Stmt::Init(ib) => walk(&ib.body, comb_driven, bus_port_map),
                Stmt::DoUntil { body, .. } => walk(body, comb_driven, bus_port_map),
                _ => {}
            }
        }
    }
    walk(stmts, &mut comb_driven, bus_port_map);
    (comb_driven, all_read)
}

fn collect_thread_signals_with_buses(
    body: &[ThreadStmt],
    bus_port_map: &HashMap<String, String>,
    vob_counts: &HashMap<String, u32>,
) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
    let (mut comb_driven, mut seq_driven, all_read) = collect_thread_signals(body);
    comb_driven.retain(|n| !bus_port_map.contains_key(n));
    seq_driven.retain(|n| !bus_port_map.contains_key(n));
    fn record(
        target: &Expr,
        driven: &mut HashSet<String>,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &HashMap<String, u32>,
    ) {
        // A variable-index Vec<Bus> write lowers to a per-lane demux, so
        // every lane becomes a driven output. Constant / scalar targets keep
        // the single-name path.
        if let Some(lanes) = vob_write_lane_names(target, bus_port_map, vob_counts) {
            driven.extend(lanes);
        } else if let Some(name) = expr_target_flat_name(target, bus_port_map) {
            if !bus_port_map.contains_key(&name) {
                driven.insert(name);
            }
        }
    }
    fn walk(
        stmts: &[ThreadStmt],
        comb_driven: &mut HashSet<String>,
        seq_driven: &mut HashSet<String>,
        bus_port_map: &HashMap<String, String>,
        vob_counts: &HashMap<String, u32>,
    ) {
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(a) => {
                    record(&a.target, comb_driven, bus_port_map, vob_counts);
                }
                ThreadStmt::SeqAssign(a) | ThreadStmt::ForkTlmAssign(a) => {
                    record(&a.target, seq_driven, bus_port_map, vob_counts);
                }
                ThreadStmt::IfElse(ie) => {
                    walk(
                        &ie.then_stmts,
                        comb_driven,
                        seq_driven,
                        bus_port_map,
                        vob_counts,
                    );
                    walk(
                        &ie.else_stmts,
                        comb_driven,
                        seq_driven,
                        bus_port_map,
                        vob_counts,
                    );
                }
                ThreadStmt::For { body, .. }
                | ThreadStmt::Lock { body, .. }
                | ThreadStmt::DoUntil { body, .. } => {
                    walk(body, comb_driven, seq_driven, bus_port_map, vob_counts)
                }
                ThreadStmt::ForkJoin(branches, _) => {
                    for b in branches {
                        walk(b, comb_driven, seq_driven, bus_port_map, vob_counts);
                    }
                }
                _ => {}
            }
        }
    }
    walk(
        body,
        &mut comb_driven,
        &mut seq_driven,
        bus_port_map,
        vob_counts,
    );
    (comb_driven, seq_driven, all_read)
}

fn collect_comb_stmt_signals(stmts: &[Stmt]) -> (HashSet<String>, HashSet<String>) {
    let mut comb_driven = HashSet::new();
    let mut all_read = HashSet::new();

    fn walk(stmts: &[Stmt], comb_driven: &mut HashSet<String>, all_read: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    if let Some(name) = expr_root_name(&a.target) {
                        comb_driven.insert(name);
                    }
                    collect_expr_reads(&a.value, all_read);
                    collect_expr_index_reads(&a.target, all_read);
                }
                Stmt::IfElse(ie) => {
                    collect_expr_reads(&ie.cond, all_read);
                    walk(&ie.then_stmts, comb_driven, all_read);
                    walk(&ie.else_stmts, comb_driven, all_read);
                }
                Stmt::Match(m) => {
                    collect_expr_reads(&m.scrutinee, all_read);
                    for arm in &m.arms {
                        walk(&arm.body, comb_driven, all_read);
                    }
                }
                Stmt::Log(l) => {
                    for arg in &l.args {
                        collect_expr_reads(arg, all_read);
                    }
                }
                Stmt::For(f) => {
                    match &f.range {
                        ForRange::Range(start, end) => {
                            collect_expr_reads(start, all_read);
                            collect_expr_reads(end, all_read);
                        }
                        ForRange::ValueList(values) => {
                            for value in values {
                                collect_expr_reads(value, all_read);
                            }
                        }
                    }
                    walk(&f.body, comb_driven, all_read);
                }
                Stmt::Init(ib) => {
                    walk(&ib.body, comb_driven, all_read);
                }
                Stmt::WaitUntil(expr, _) => collect_expr_reads(expr, all_read),
                Stmt::DoUntil { body, cond, .. } => {
                    walk(body, comb_driven, all_read);
                    collect_expr_reads(cond, all_read);
                }
            }
        }
    }

    walk(stmts, &mut comb_driven, &mut all_read);
    (comb_driven, all_read)
}

/// arch#709: peel the `_<res>_req_<ti>` lock-request assignments out of the
/// merged thread `always_comb` into a block of their own.
///
/// The merged block writes both the lock request wires and ordinary
/// combinational outputs, and the latter are routinely gated on
/// `_<res>_grant_<ti>` (that is what a `lock` body *is*). Verilator models an
/// `always_comb` as a single dependency-graph vertex, so every read of the
/// block feeds every write of it — fabricating a `grant -> req` edge that
/// closes the arbiter's `req -> grant` path into a cycle and reports it as
/// `UNOPTFLAT: Circular combinational logic` on the grant wires. The bit-level
/// graph is acyclic (`req` is a function of the thread state registers alone),
/// which is why `arch check`'s per-signal comb-loop detector is silent, but the
/// emitted SV still lints as a combinational loop.
///
/// Splitting removes the false edge: the request block reads only state
/// registers, and the remaining block's grant reads no longer reach a request
/// wire. Where a request genuinely *is* derived from a grant, the guard travels
/// with the request assignment into the new block and the edge — a real one —
/// survives.
///
/// Returns `None` when the split is not known to be semantics-preserving, in
/// which case the caller keeps the single merged block (today's output):
///
///   - a moved statement's guard or right-hand side reads a signal that the
///     merged block itself drives. Within one block that read sees the value
///     assigned so far in statement order; across two blocks it would see the
///     other block's settled value instead.
///   - a request assignment sits inside a statement kind this pass does not
///     know how to duplicate structurally.
///
/// The partition is by assignment target, so the two blocks drive disjoint
/// signal sets (no multiple-driver hazard) and statement order — hence
/// last-write-wins — is preserved within each.
fn split_lock_req_comb(
    stmts: &[Stmt],
    req_names: &HashSet<String>,
) -> Option<(Vec<Stmt>, Vec<Stmt>)> {
    if req_names.is_empty() {
        return None;
    }
    let (driven, _) = collect_comb_stmt_signals(stmts);

    // An expression is safe to re-evaluate in the split-off block iff it reads
    // nothing the merged block drives.
    fn expr_safe(e: &Expr, driven: &HashSet<String>) -> bool {
        let mut reads = HashSet::new();
        collect_expr_reads(e, &mut reads);
        reads.is_disjoint(driven)
    }

    /// `Some((req_half, rest_half))`, or `None` to abandon the split.
    fn split_stmt(
        s: &Stmt,
        req_names: &HashSet<String>,
        driven: &HashSet<String>,
    ) -> Option<(Option<Stmt>, Option<Stmt>)> {
        match s {
            Stmt::Assign(a) => {
                let is_req = expr_root_name(&a.target)
                    .map(|n| req_names.contains(&n))
                    .unwrap_or(false);
                if !is_req {
                    return Some((None, Some(s.clone())));
                }
                // The target's own name is the signal being moved; only the
                // reads inside its index/select expressions matter here.
                let mut target_reads = HashSet::new();
                collect_expr_index_reads(&a.target, &mut target_reads);
                if !expr_safe(&a.value, driven) || !target_reads.is_disjoint(driven) {
                    return None;
                }
                Some((Some(s.clone()), None))
            }
            Stmt::IfElse(ie) => {
                let (then_req, then_rest) = split_list(&ie.then_stmts, req_names, driven)?;
                let (else_req, else_rest) = split_list(&ie.else_stmts, req_names, driven)?;
                let req_half = if then_req.is_empty() && else_req.is_empty() {
                    None
                } else {
                    if !expr_safe(&ie.cond, driven) {
                        return None;
                    }
                    Some(Stmt::IfElse(IfElse {
                        cond: ie.cond.clone(),
                        then_stmts: then_req,
                        else_stmts: else_req,
                        unique: ie.unique,
                        span: ie.span,
                    }))
                };
                let rest_half = if then_rest.is_empty() && else_rest.is_empty() {
                    None
                } else {
                    Some(Stmt::IfElse(IfElse {
                        cond: ie.cond.clone(),
                        then_stmts: then_rest,
                        else_stmts: else_rest,
                        unique: ie.unique,
                        span: ie.span,
                    }))
                };
                Some((req_half, rest_half))
            }
            Stmt::Match(m) => {
                let mut req_arms = Vec::new();
                let mut rest_arms = Vec::new();
                let mut any_req = false;
                let mut any_rest = false;
                for arm in &m.arms {
                    let (arm_req, arm_rest) = split_list(&arm.body, req_names, driven)?;
                    any_req |= !arm_req.is_empty();
                    any_rest |= !arm_rest.is_empty();
                    req_arms.push(MatchArm {
                        pattern: arm.pattern.clone(),
                        body: arm_req,
                    });
                    rest_arms.push(MatchArm {
                        pattern: arm.pattern.clone(),
                        body: arm_rest,
                    });
                }
                let req_half = if any_req {
                    if !expr_safe(&m.scrutinee, driven) {
                        return None;
                    }
                    Some(Stmt::Match(MatchStmt {
                        scrutinee: m.scrutinee.clone(),
                        arms: req_arms,
                        unique: m.unique,
                        span: m.span,
                    }))
                } else {
                    None
                };
                let rest_half = if any_rest {
                    Some(Stmt::Match(MatchStmt {
                        scrutinee: m.scrutinee.clone(),
                        arms: rest_arms,
                        unique: m.unique,
                        span: m.span,
                    }))
                } else {
                    None
                };
                Some((req_half, rest_half))
            }
            Stmt::For(f) => {
                let (body_req, body_rest) = split_list(&f.body, req_names, driven)?;
                let req_half = if body_req.is_empty() {
                    None
                } else {
                    let range_safe = match &f.range {
                        ForRange::Range(start, end) => {
                            expr_safe(start, driven) && expr_safe(end, driven)
                        }
                        ForRange::ValueList(values) => values.iter().all(|v| expr_safe(v, driven)),
                    };
                    if !range_safe {
                        return None;
                    }
                    Some(Stmt::For(ForLoop {
                        var: f.var.clone(),
                        range: f.range.clone(),
                        body: body_req,
                        span: f.span,
                    }))
                };
                let rest_half = if body_rest.is_empty() {
                    None
                } else {
                    Some(Stmt::For(ForLoop {
                        var: f.var.clone(),
                        range: f.range.clone(),
                        body: body_rest,
                        span: f.span,
                    }))
                };
                Some((req_half, rest_half))
            }
            // Statement kinds the merged thread comb block never uses for lock
            // requests. If one ever does contain a request assignment, abandon
            // the split rather than guess at its duplication semantics.
            other => {
                let (driven_here, _) = collect_comb_stmt_signals(std::slice::from_ref(other));
                if driven_here.is_disjoint(req_names) {
                    Some((None, Some(other.clone())))
                } else {
                    None
                }
            }
        }
    }

    fn split_list(
        stmts: &[Stmt],
        req_names: &HashSet<String>,
        driven: &HashSet<String>,
    ) -> Option<(Vec<Stmt>, Vec<Stmt>)> {
        let mut req = Vec::new();
        let mut rest = Vec::new();
        for s in stmts {
            let (r, o) = split_stmt(s, req_names, driven)?;
            if let Some(r) = r {
                req.push(r);
            }
            if let Some(o) = o {
                rest.push(o);
            }
        }
        Some((req, rest))
    }

    let (req_stmts, rest_stmts) = split_list(stmts, req_names, &driven)?;
    if req_stmts.is_empty() {
        return None;
    }
    Some((req_stmts, rest_stmts))
}

fn collect_thread_signals(
    body: &[ThreadStmt],
) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
    let mut comb_driven = HashSet::new();
    let mut seq_driven = HashSet::new();
    let mut all_read = HashSet::new();

    fn walk_stmts(
        stmts: &[ThreadStmt],
        comb_driven: &mut HashSet<String>,
        seq_driven: &mut HashSet<String>,
        all_read: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            match stmt {
                ThreadStmt::CombAssign(ca) => {
                    if let Some(name) = expr_root_name(&ca.target) {
                        comb_driven.insert(name);
                    }
                    collect_expr_reads(&ca.value, all_read);
                    // Also collect reads from indexed targets like buf[i]
                    collect_expr_index_reads(&ca.target, all_read);
                }
                ThreadStmt::SeqAssign(ra) | ThreadStmt::ForkTlmAssign(ra) => {
                    if let Some(name) = expr_root_name(&ra.target) {
                        seq_driven.insert(name);
                    }
                    collect_expr_reads(&ra.value, all_read);
                    collect_expr_index_reads(&ra.target, all_read);
                }
                ThreadStmt::WaitUntil(cond, _) => {
                    collect_expr_reads(cond, all_read);
                }
                ThreadStmt::WaitCycles(_, _) => {}
                ThreadStmt::JoinAll(_) => {}
                ThreadStmt::IfElse(ie) => {
                    collect_expr_reads(&ie.cond, all_read);
                    walk_stmts(&ie.then_stmts, comb_driven, seq_driven, all_read);
                    walk_stmts(&ie.else_stmts, comb_driven, seq_driven, all_read);
                }
                ThreadStmt::ForkJoin(branches, _) => {
                    for br in branches {
                        walk_stmts(br, comb_driven, seq_driven, all_read);
                    }
                }
                ThreadStmt::For {
                    var: _,
                    start,
                    end,
                    body,
                    ..
                } => {
                    collect_expr_reads(start, all_read);
                    collect_expr_reads(end, all_read);
                    walk_stmts(body, comb_driven, seq_driven, all_read);
                }
                ThreadStmt::Lock { body, .. } => {
                    walk_stmts(body, comb_driven, seq_driven, all_read);
                }
                ThreadStmt::DoUntil { body, cond, .. } => {
                    walk_stmts(body, comb_driven, seq_driven, all_read);
                    collect_expr_reads(cond, all_read);
                }
                ThreadStmt::Log(_) => {}
                ThreadStmt::Return(e, _) => {
                    collect_expr_reads(e, all_read);
                }
            }
        }
    }
    walk_stmts(body, &mut comb_driven, &mut seq_driven, &mut all_read);
    (comb_driven, seq_driven, all_read)
}

/// Extract the root identifier name from an expression (handles indexing, field access).
fn expr_root_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Index(base, _) => expr_root_name(base),
        ExprKind::BitSlice(base, _, _) => expr_root_name(base),
        ExprKind::FieldAccess(base, _) => expr_root_name(base),
        _ => None,
    }
}

/// Collect all identifier reads from an expression.
/// Walk an expression and add flat bus-signal names for any bus-port
/// FieldAccess it contains. `b.r` (a bus signal read) → adds `b_r` to
/// `out`. Used after the underlying `collect_expr_reads` to seed the
/// flattened input names into the sub-module's port list.
fn collect_expr_bus_reads(
    e: &Expr,
    bus_port_map: &HashMap<String, String>,
    vob_counts: &HashMap<String, u32>,
    out: &mut HashSet<String>,
) {
    if let ExprKind::FieldAccess(base, field) = &e.kind {
        if let ExprKind::Ident(base_name) = &base.kind {
            if bus_port_map.contains_key(base_name) {
                out.insert(format!("{}_{}", base_name, field.name));
            }
        }
        if let ExprKind::Index(arr, idx) = &base.kind {
            if let ExprKind::Ident(arr_name) = &arr.kind {
                if let ExprKind::Literal(LitKind::Dec(i)) = &idx.kind {
                    if bus_port_map.contains_key(arr_name) {
                        out.insert(format!("{}_{}_{}", arr_name, i, field.name));
                    }
                } else if let Some(&n) = vob_counts.get(arr_name) {
                    // Variable index into a Vec<Bus> port: the read is
                    // lowered to a runtime mux over all N lanes, so every
                    // per-lane signal becomes a sub-module input port.
                    for i in 0..n {
                        out.insert(format!("{}_{}_{}", arr_name, i, field.name));
                    }
                }
            }
        }
    }
    // Recurse into children.
    match &e.kind {
        ExprKind::Binary(_, l, r) => {
            collect_expr_bus_reads(l, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(r, bus_port_map, vob_counts, out);
        }
        ExprKind::Unary(_, x)
        | ExprKind::Cast(x, _)
        | ExprKind::LatencyAt(x, _)
        | ExprKind::SvaNext(_, x) => collect_expr_bus_reads(x, bus_port_map, vob_counts, out),
        ExprKind::Index(b, i) | ExprKind::BitSlice(b, i, _) => {
            collect_expr_bus_reads(b, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(i, bus_port_map, vob_counts, out);
        }
        ExprKind::PartSelect(b, lo, hi, _) => {
            collect_expr_bus_reads(b, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(lo, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(hi, bus_port_map, vob_counts, out);
        }
        ExprKind::Ternary(c, t, e2) => {
            collect_expr_bus_reads(c, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(t, bus_port_map, vob_counts, out);
            collect_expr_bus_reads(e2, bus_port_map, vob_counts, out);
        }
        ExprKind::Concat(parts) | ExprKind::FunctionCall(_, parts) => {
            for p in parts {
                collect_expr_bus_reads(p, bus_port_map, vob_counts, out);
            }
        }
        ExprKind::MethodCall(b, _, args) => {
            collect_expr_bus_reads(b, bus_port_map, vob_counts, out);
            for a in args {
                collect_expr_bus_reads(a, bus_port_map, vob_counts, out);
            }
        }
        ExprKind::FieldAccess(b, _) => collect_expr_bus_reads(b, bus_port_map, vob_counts, out),
        _ => {}
    }
}

/// Walk a thread body (recursively) and add flat bus-signal read names
/// to `out`. Companion to `collect_expr_bus_reads` for the statement
/// shape.
fn collect_thread_bus_reads(
    body: &[ThreadStmt],
    bus_port_map: &HashMap<String, String>,
    vob_counts: &HashMap<String, u32>,
    out: &mut HashSet<String>,
) {
    for s in body {
        match s {
            ThreadStmt::CombAssign(a) | ThreadStmt::SeqAssign(a) | ThreadStmt::ForkTlmAssign(a) => {
                collect_expr_bus_reads(&a.value, bus_port_map, vob_counts, out);
                collect_expr_bus_reads(&a.target, bus_port_map, vob_counts, out);
            }
            ThreadStmt::WaitUntil(c, _) => collect_expr_bus_reads(c, bus_port_map, vob_counts, out),
            ThreadStmt::IfElse(ie) => {
                collect_expr_bus_reads(&ie.cond, bus_port_map, vob_counts, out);
                collect_thread_bus_reads(&ie.then_stmts, bus_port_map, vob_counts, out);
                collect_thread_bus_reads(&ie.else_stmts, bus_port_map, vob_counts, out);
            }
            ThreadStmt::For { body, .. } | ThreadStmt::Lock { body, .. } => {
                collect_thread_bus_reads(body, bus_port_map, vob_counts, out)
            }
            ThreadStmt::DoUntil { body, cond, .. } => {
                collect_expr_bus_reads(cond, bus_port_map, vob_counts, out);
                collect_thread_bus_reads(body, bus_port_map, vob_counts, out);
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for b in branches {
                    collect_thread_bus_reads(b, bus_port_map, vob_counts, out);
                }
            }
            ThreadStmt::Return(e, _) => collect_expr_bus_reads(e, bus_port_map, vob_counts, out),
            _ => {}
        }
    }
}

fn collect_expr_reads(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(name) => {
            out.insert(name.clone());
        }
        ExprKind::Binary(_, l, r) => {
            collect_expr_reads(l, out);
            collect_expr_reads(r, out);
        }
        ExprKind::Unary(_, e) => collect_expr_reads(e, out),
        ExprKind::Index(base, idx) => {
            collect_expr_reads(base, out);
            collect_expr_reads(idx, out);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            collect_expr_reads(base, out);
            collect_expr_reads(hi, out);
            collect_expr_reads(lo, out);
        }
        ExprKind::PartSelect(base, start, width, _) => {
            collect_expr_reads(base, out);
            collect_expr_reads(start, out);
            collect_expr_reads(width, out);
        }
        ExprKind::FieldAccess(base, _) => collect_expr_reads(base, out),
        ExprKind::MethodCall(recv, _, args) => {
            collect_expr_reads(recv, out);
            for a in args {
                collect_expr_reads(a, out);
            }
        }
        ExprKind::Cast(e, _) => collect_expr_reads(e, out),
        ExprKind::Concat(parts) => {
            for p in parts {
                collect_expr_reads(p, out);
            }
        }
        ExprKind::Repeat(count, val) => {
            collect_expr_reads(count, out);
            collect_expr_reads(val, out);
        }
        ExprKind::Clog2(e) => collect_expr_reads(e, out),
        ExprKind::Signed(e) => collect_expr_reads(e, out),
        ExprKind::Unsigned(e) => collect_expr_reads(e, out),
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                collect_expr_reads(a, out);
            }
        }
        ExprKind::Ternary(c, t, f) => {
            collect_expr_reads(c, out);
            collect_expr_reads(t, out);
            collect_expr_reads(f, out);
        }
        ExprKind::Inside(e, members) => {
            collect_expr_reads(e, out);
            for m in members {
                match m {
                    InsideMember::Single(e) => collect_expr_reads(e, out),
                    InsideMember::Range(lo, hi) => {
                        collect_expr_reads(lo, out);
                        collect_expr_reads(hi, out);
                    }
                }
            }
        }
        ExprKind::Match(scrut, arms) => {
            collect_expr_reads(scrut, out);
            for arm in arms {
                for s in &arm.body {
                    if let Stmt::Assign(a) = s {
                        collect_expr_reads(&a.value, out);
                    }
                }
            }
        }
        ExprKind::ExprMatch(scrut, arms) => {
            collect_expr_reads(scrut, out);
            for arm in arms {
                collect_expr_reads(&arm.value, out);
            }
        }
        _ => {} // Literal, Bool, Todo, EnumVariant, StructLiteral
    }
}

/// Collect reads from index expressions in a target (e.g. `buf[i]` — `i` is a read).
fn collect_expr_index_reads(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        // Recurse into the base too: a 2D target `mem[i][j]` reads both `i`
        // and `j`, and a bus-element target `o[sel].field` carries the index
        // `sel` *under* the FieldAccess — without descending we'd miss it and
        // the synthesized thread sub-module would reference an undeclared
        // index signal.
        ExprKind::Index(base, idx) => {
            collect_expr_reads(idx, out);
            collect_expr_index_reads(base, out);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            collect_expr_reads(hi, out);
            collect_expr_reads(lo, out);
            collect_expr_index_reads(base, out);
        }
        ExprKind::FieldAccess(base, _) => collect_expr_index_reads(base, out),
        _ => {}
    }
}

// ── State partitioning ──────────────────────────────────────────────────────

/// A single FSM state derived from thread body partitioning.
///
/// `pub(crate)`: the type itself, plus six of its fields (each annotated
/// below), are read/written directly by TLM target-thread lowering in
/// `elaborate::mod` (`inline_lower_tlm_target_with_io`), which reuses this
/// thread-FSM partitioning shape for `thread port.method(...)` bodies rather
/// than duplicating it. Mechanical visibility bump only — the remaining
/// fields, untouched outside this module, stay private.
pub(crate) struct ThreadFsmState {
    /// Combinational assignments active in this state.
    pub(crate) comb_stmts: Vec<Stmt>,
    /// Sequential assignments that fire on the transition out of this state.
    pub(crate) seq_stmts: Vec<Stmt>,
    /// Transition condition (from `wait until`).  None = unconditional.
    pub(crate) transition_cond: Option<Expr>,
    /// Is this a counter-based wait state? If so, stores the count expression.
    pub(crate) wait_cycles: Option<Expr>,
    /// Multiple transitions (for fork/join product states).
    /// Each entry is (condition, target_state_offset_from_this_group).
    /// When non-empty, `transition_cond` is ignored.
    pub(crate) multi_transitions: Vec<(Expr, usize)>,
    /// Target-side TLM only: this state exits to a generated response state
    /// carrying the indexed return expression instead of falling through.
    pub(crate) terminal_return: Option<usize>,
    /// Issue #306: seq assigns folded from the immediately-following action
    /// state into this wait_until state's cond-exit arm.  Only populated
    /// when `transition_cond.is_some()` and the next state was a sole-entry
    /// pure-action state.  Emitted inside `if (cond) { folded_exit_seq; state <= next; }`.
    folded_exit_seq: Vec<Stmt>,
    /// When `folded_exit_seq` is non-empty, the transition target skips the
    /// absorbed action state and jumps directly to the state after it.
    /// `None` means use the natural `si + 1` computation.
    folded_exit_target: Option<usize>,
    /// True when this state was absorbed into a preceding wait_until exit arm
    /// (issue #306).  The state is unreachable; codegen skips it entirely.
    is_folded: bool,
    /// True when this state must NOT be folded into the preceding wait_until
    /// state's exit arm (issue #306).  Set when a `wait 1 cycle` was elided
    /// before this state — the natural S(wait)→S(action) transition provides
    /// the 1-cycle budget and folding would lose that cycle.
    no_fold_into_prev: bool,
    /// `Some(resource)` when this is the LAST state of a `lock` body for
    /// that resource. The merged-module generation emits a combinational
    /// release pulse (`_<res>_release_<ti>`) when this state's exit
    /// transition fires, so the lock arbiter's hold latch can distinguish
    /// "released and immediately re-requesting" (back-to-back lock in a
    /// loop — request stays asserted across the boundary) from "still
    /// holding". Without it, a re-locking thread would never rotate the
    /// grant to a waiting contender.
    lock_release: Option<String>,
    /// Transition provenance for the lock-release pulse. `AllTransitions`
    /// means every arm of a later multi-transition state leaves this lock
    /// body (for example, a lock used as the body of an outer `for`).
    /// `ExitConditions` records the arms that leave a multi-transition
    /// construct already nested inside the lock body.
    lock_release_info: Option<LockReleaseInfo>,
    /// True when this state belongs to a `lock` body (issue #501).  The comb
    /// overlap optimization must not drive a lock-guarded state's outputs
    /// during the preceding state's transition cycle: the first body state's
    /// outputs are grant-gated, and driving any lock-body outputs before the
    /// lock is held would leak them into states outside the critical section.
    is_lock_body: bool,
}

#[derive(Clone)]
enum LockReleaseInfo {
    AllTransitions,
    ExitConditions(Vec<Expr>),
}

/// Issue #306: fold `wait until` exit assignments.
///
/// Scans `states` for pairs (si, si+1) where:
///   - state si is a pure `wait until cond` state (has `transition_cond`,
///     no `wait_cycles`, no `multi_transitions`, no `folded_exit_seq` already)
///   - state si+1 is a pure action state (no `transition_cond`, no
///     `wait_cycles`, no `multi_transitions`, not already folded), AND
///   - state si+1 has at least one seq assign to fold, AND
///   - no other state targets si+1 via `multi_transitions` (sole-entry check)
///
/// When all conditions hold, `seq_stmts` from si+1 are moved into
/// si's `folded_exit_seq` field.  The transition target for si is updated to
/// `folded_exit_target = si+2` so the codegen skips si+1 directly.
/// State si+1 is marked `is_folded = true`; the codegen loop skips it.
///
/// This does NOT fold across `wait N cycle` states (those need the counter
/// states) or into/out of fork/join dispatch states.
fn fold_wait_until_exit_assignments(states: &mut Vec<ThreadFsmState>, t_once: bool) {
    let n = states.len();
    // Build the set of state indices targeted by any multi_transition from
    // any state.  A state in this set may be reachable from multiple
    // predecessors, so folding it into the single wait_until predecessor
    // would silently drop an execution path.
    let mut multi_targets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for state in states.iter() {
        for (_, target) in &state.multi_transitions {
            if *target < n {
                multi_targets.insert(*target);
            }
        }
    }

    // Walk forward; after folding si+1 into si we continue at si+1 (now
    // marked is_folded) — no index confusion since we only read/write
    // `states[si]` and `states[si+1]` in each iteration.
    for si in 0..n.saturating_sub(1) {
        let successor = si + 1;

        // State si: must be a pure wait_until (no wait_cycles, no multi,
        // no prior fold already applied, and empty folded_exit_seq).
        // Also require empty seq_stmts: if the fast_region mechanism has
        // already merged guarded assigns into si's seq_stmts (as it does for
        // `if not X; wait until X; end if` followed by actions), folding
        // the next action state would stack a second if-guard on top, producing
        // two separate `if (cond)` arms in the always_ff block.
        {
            let s = &states[si];
            if s.transition_cond.is_none()
                || s.wait_cycles.is_some()
                || !s.multi_transitions.is_empty()
                || !s.folded_exit_seq.is_empty()
                || !s.seq_stmts.is_empty()
                || s.is_folded
            {
                continue;
            }
        }

        // State si+1: must be a pure action state (no wait cond/cycles/multi,
        // not already folded), have at least one seq assign to fold, and be
        // a sole-entry state (not targeted by any multi_transition).
        // Also must not have the `no_fold_into_prev` flag set (which marks
        // states created after a `wait 1 cycle` elision — the natural
        // si→si+1 transition is the 1-cycle budget, folding would lose it).
        {
            let s1 = &states[successor];
            if s1.transition_cond.is_some()
                || s1.wait_cycles.is_some()
                || !s1.multi_transitions.is_empty()
                || s1.is_folded
                || s1.seq_stmts.is_empty()
                || s1.no_fold_into_prev
                || multi_targets.contains(&successor)
            {
                continue;
            }
        }

        // Compute the effective target after si+1: the state that si+1
        // would naturally advance to.
        let after_action = if successor + 1 < n {
            successor + 1
        } else if t_once {
            successor // terminal
        } else {
            0 // wrap
        };

        // Move seq_stmts from si+1 into si's folded_exit_seq.
        let folded = std::mem::take(&mut states[successor].seq_stmts);
        states[si].folded_exit_seq = folded;
        states[si].folded_exit_target = Some(after_action);
        states[successor].is_folded = true;
    }
}

fn thread_natural_next_state(si: usize, n_states: usize, t_once: bool) -> usize {
    if si + 1 < n_states {
        si + 1
    } else if t_once {
        si
    } else {
        0
    }
}

fn thread_resolve_target(target: usize, n_states: usize, t_once: bool) -> usize {
    if target >= n_states {
        if t_once {
            n_states.saturating_sub(1)
        } else {
            0
        }
    } else {
        target
    }
}

fn compact_thread_source_target(states: &[ThreadFsmState], target: usize, t_once: bool) -> usize {
    let n_states = states.len();
    let mut current = thread_resolve_target(target, n_states, t_once);
    for _ in 0..n_states {
        if !states[current].is_folded {
            return current;
        }
        let next = thread_natural_next_state(current, n_states, t_once);
        if next == current {
            return current;
        }
        current = next;
    }
    current
}

#[derive(Debug, Clone)]
struct ThreadSourceTransitionIntent {
    condition: String,
    condition_guard: Option<crate::thread_map::ThreadMapGuardExpr>,
    target: usize,
}

fn thread_source_transition_intents(
    states: &[ThreadFsmState],
    ti: usize,
    t_once: bool,
) -> Vec<Vec<ThreadSourceTransitionIntent>> {
    let n_states = states.len();
    states
        .iter()
        .enumerate()
        .map(|(si, state)| {
            let natural_next = thread_natural_next_state(si, n_states, t_once);
            if !state.multi_transitions.is_empty() {
                state
                    .multi_transitions
                    .iter()
                    .map(|(cond, target)| ThreadSourceTransitionIntent {
                        condition: crate::thread_map::expr_label(cond),
                        condition_guard: Some(crate::thread_map::guard_expr(cond)),
                        target: thread_resolve_target(*target, n_states, t_once),
                    })
                    .collect()
            } else if let Some(cond) = &state.transition_cond {
                vec![ThreadSourceTransitionIntent {
                    condition: crate::thread_map::expr_label(cond),
                    condition_guard: Some(crate::thread_map::guard_expr(cond)),
                    target: natural_next,
                }]
            } else if state.wait_cycles.is_some() {
                vec![ThreadSourceTransitionIntent {
                    condition: format!("_t{}_cnt == 0", ti),
                    condition_guard: None,
                    target: natural_next,
                }]
            } else {
                vec![ThreadSourceTransitionIntent {
                    condition: "always".to_string(),
                    condition_guard: Some(crate::thread_map::ThreadMapGuardExpr::True),
                    target: natural_next,
                }]
            }
        })
        .collect()
}

fn thread_source_map_transitions(
    intents: &[ThreadSourceTransitionIntent],
    states: &[ThreadFsmState],
    state_names: &[String],
    t_once: bool,
) -> Vec<crate::thread_map::ThreadMapTransition> {
    intents
        .iter()
        .map(|intent| {
            let compact_target = compact_thread_source_target(states, intent.target, t_once);
            crate::thread_map::ThreadMapTransition {
                condition: intent.condition.clone(),
                condition_guard: intent.condition_guard.clone(),
                target_index: compact_target,
                target_name: state_names[compact_target].clone(),
            }
        })
        .collect()
}

fn thread_map_state_role(si: usize, state: &ThreadFsmState) -> &'static str {
    if state.multi_transitions.len() > 1 {
        "dispatch"
    } else if state.wait_cycles.is_some() {
        "wait_cycles"
    } else if state.transition_cond.is_some() {
        "wait_until"
    } else if si == 0 {
        "entry"
    } else {
        "action"
    }
}

fn merge_span(acc: &mut Option<Span>, span: Span) {
    *acc = Some(match *acc {
        Some(existing) => existing.merge(span),
        None => span,
    });
}

fn thread_fsm_state_span(state: &ThreadFsmState, fallback: Span) -> Span {
    if let Some(count) = &state.wait_cycles {
        return count.span;
    }
    if let Some(cond) = &state.transition_cond {
        return cond.span;
    }

    let mut span = None;
    for stmt in &state.comb_stmts {
        merge_span(&mut span, crate::thread_map::stmt_span(stmt));
    }
    for stmt in &state.seq_stmts {
        merge_span(&mut span, crate::thread_map::stmt_span(stmt));
    }
    if let Some(span) = span {
        return span;
    }

    if !state.multi_transitions.is_empty() {
        let mut span = None;
        for (cond, _) in &state.multi_transitions {
            merge_span(&mut span, cond.span);
        }
        return span.unwrap_or(fallback);
    }

    fallback
}

fn thread_map_state_labels(state: &ThreadFsmState) -> Vec<String> {
    let mut labels = Vec::new();
    if !state.comb_stmts.is_empty() {
        labels.push(format!(
            "comb: {} stmt{}",
            state.comb_stmts.len(),
            plural_s(state.comb_stmts.len())
        ));
    }
    if !state.seq_stmts.is_empty() {
        labels.push(format!(
            "seq: {} stmt{}",
            state.seq_stmts.len(),
            plural_s(state.seq_stmts.len())
        ));
    }
    if let Some(cond) = &state.transition_cond {
        labels.push(format!(
            "wait until {}",
            crate::thread_map::expr_label(cond)
        ));
    }
    if let Some(count) = &state.wait_cycles {
        labels.push(format!(
            "wait {} cycle",
            crate::thread_map::expr_label(count)
        ));
    }
    if !state.multi_transitions.is_empty() {
        labels.push(format!("branches: {}", state.multi_transitions.len()));
    }
    labels
}

fn plural_s(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

const THREAD_TARGET_NEXT: usize = usize::MAX;
const THREAD_TARGET_RETURN_BASE: usize = usize::MAX / 2;

fn thread_return_target(idx: usize) -> usize {
    THREAD_TARGET_RETURN_BASE + idx
}

pub(crate) fn thread_target_return_idx(target: usize) -> Option<usize> {
    if (THREAD_TARGET_RETURN_BASE..usize::MAX).contains(&target) {
        Some(target - THREAD_TARGET_RETURN_BASE)
    } else {
        None
    }
}

fn thread_target_is_special(target: usize) -> bool {
    target == THREAD_TARGET_NEXT || thread_target_return_idx(target).is_some()
}

/// Extract the bit width of a UInt literal type expression (e.g. `UInt<8>` → 8).
fn type_expr_uint_width_literal(ty: &TypeExpr) -> Option<u32> {
    match ty {
        TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
            if let ExprKind::Literal(LitKind::Dec(n)) = &w.kind {
                Some(*n as u32)
            } else {
                None
            }
        }
        TypeExpr::Bool | TypeExpr::Bit => Some(1),
        _ => None,
    }
}

/// Infer the minimum UInt bit width needed for a `for` loop end expression.
/// Walks the expression tree with a simple heuristic:
///   - Ident → look up in type_map, extract UInt width
///   - Binary(Sub|Add, a, _) → width of a (subtract/add by small literals doesn't change range)
///   - MethodCall(inner, "trunc"|"zext"|"sext", [width_lit]) → use width literal
///   - Literal(Dec|Hex) → ceil(log2(v+1))
///   - Default → 16 (covers burst lengths up to 65535)
fn infer_expr_uint_width(expr: &Expr, type_map: &HashMap<String, SignalInfo>) -> u32 {
    match &expr.kind {
        ExprKind::Ident(name) => type_map
            .get(name)
            .and_then(|si| type_expr_uint_width_literal(&si.ty))
            .unwrap_or(16),
        ExprKind::Binary(BinOp::Sub | BinOp::Add | BinOp::BitAnd | BinOp::BitOr, a, _) => {
            infer_expr_uint_width(a, type_map)
        }
        ExprKind::MethodCall(inner, method, args) => {
            let method_name = method.name.as_str();
            if matches!(method_name, "trunc" | "zext" | "sext") {
                // First arg is the width literal
                if let Some(w_expr) = args.first() {
                    if let ExprKind::Literal(LitKind::Dec(n)) = &w_expr.kind {
                        return *n as u32;
                    }
                }
            }
            infer_expr_uint_width(inner, type_map)
        }
        ExprKind::Literal(LitKind::Dec(v)) => {
            if *v == 0 {
                1
            } else {
                (u64::BITS - v.leading_zeros()) as u32
            }
        }
        _ => 16,
    }
}

/// Find the minimum counter width across all `for` loops in a thread body.
/// Returns 16 if no for loops are found or width cannot be determined.
pub(crate) fn infer_for_cnt_width(
    stmts: &[ThreadStmt],
    type_map: &HashMap<String, SignalInfo>,
) -> u32 {
    let w = infer_for_cnt_width_inner(stmts, type_map);
    if w == 0 {
        16
    } else {
        w
    }
}

/// Inner helper: returns 0 when no for loops found (avoids poisoning max() with the default).
fn infer_for_cnt_width_inner(stmts: &[ThreadStmt], type_map: &HashMap<String, SignalInfo>) -> u32 {
    let mut max_width: u32 = 0;
    for stmt in stmts {
        match stmt {
            ThreadStmt::For { end, .. } => {
                // Only the end expression determines the counter width.
                // Do NOT recurse into the for-loop body — no nested for loops,
                // and recursing would find zero for-loops there, returning 0.
                let w = infer_expr_uint_width(end, type_map);

                max_width = max_width.max(w);
            }
            ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => {
                max_width = max_width.max(infer_for_cnt_width_inner(body, type_map));
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for br in branches {
                    max_width = max_width.max(infer_for_cnt_width_inner(br, type_map));
                }
            }
            ThreadStmt::IfElse(ie) => {
                max_width = max_width.max(infer_for_cnt_width_inner(&ie.then_stmts, type_map));
                max_width = max_width.max(infer_for_cnt_width_inner(&ie.else_stmts, type_map));
            }
            _ => {}
        }
    }
    max_width
}

/// Check if any ThreadStmt in a slice contains a wait (recursing into if/else).
fn thread_has_wait_cycles(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::WaitCycles(..) => true,
        ThreadStmt::IfElse(ie) => {
            thread_has_wait_cycles(&ie.then_stmts) || thread_has_wait_cycles(&ie.else_stmts)
        }
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| thread_has_wait_cycles(br)),
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => {
            thread_has_wait_cycles(body)
        }
        ThreadStmt::For { body, .. } => thread_has_wait_cycles(body),
        _ => false,
    })
}

/// Count every `for` instance in `stmts`, including those nested inside other
/// `for`/`lock`/`fork`/`if-else`/`do-until` bodies. Used to size the per-thread
/// loop-counter reg allocation: each `for` needs its own `_loop_cnt_{id}` so
/// nested loops don't clobber each other's running index (issue #414).
fn count_for_instances(stmts: &[ThreadStmt]) -> usize {
    let mut n = 0;
    for s in stmts {
        match s {
            ThreadStmt::For { body, .. } => {
                n += 1;
                n += count_for_instances(body);
            }
            ThreadStmt::IfElse(ie) => {
                n += count_for_instances(&ie.then_stmts);
                n += count_for_instances(&ie.else_stmts);
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for br in branches {
                    n += count_for_instances(br);
                }
            }
            ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => {
                n += count_for_instances(body);
            }
            _ => {}
        }
    }
    n
}

/// Redirect the natural fallthrough of `states[idx]` to `target`.
///
/// Used by the dispatch-and-rejoin lowering of `if/else` with internal waits
/// (see `doc/thread_lowering_proof.md` §II.10.2 step 5) to send each branch's
/// last state to the rejoin index instead of letting it fall through to the
/// other branch's first state.
///
/// Cases (mirroring the spec):
/// - `M = ∅, τ = ⊥, w = ⊥` (unconditional advance): replace with
///   `M = [(true, target)]`.
/// - `M = ∅, τ = c`: replace with `M = [(c, target)]`.
/// - `M = ∅, w = n` (wait_cycles): replace with `M = [(cnt == 0, target)]`.
///   The counter decrement is now hoisted out of the transition emitter
///   (see `lower_module_threads`'s seq-stmt construction), so this conversion
///   does not lose the decrement.
/// - `M ≠ ∅`: append `(true, target)` only if no existing entry already
///   targets `target`. (For-loop exits already target the resolved sentinel,
///   which equals `target` when the for-group is the last sub-state.)
fn redirect_fallthrough_to(states: &mut [ThreadFsmState], idx: usize, target: usize, span: Span) {
    let s = &mut states[idx];
    if s.terminal_return.is_some() {
        return;
    }
    if !s.multi_transitions.is_empty() {
        if !s.multi_transitions.iter().any(|(_, t)| *t == target) {
            s.multi_transitions
                .push((Expr::new(ExprKind::Bool(true), span), target));
        }
        return;
    }
    if let Some(cond) = s.transition_cond.take() {
        s.multi_transitions = vec![(cond, target)];
        return;
    }
    if s.wait_cycles.is_some() {
        let cnt_id = Expr::new(ExprKind::Ident("_cnt".to_string()), span);
        let cnt_zero = Expr::new(
            ExprKind::Binary(BinOp::Eq, Box::new(cnt_id), Box::new(make_zero_expr(span))),
            span,
        );
        s.multi_transitions = vec![(cnt_zero, target)];
        return;
    }
    s.multi_transitions = vec![(Expr::new(ExprKind::Bool(true), span), target)];
}

fn redirect_fallthrough_to_return(states: &mut Vec<ThreadFsmState>, return_idx: usize, span: Span) {
    let target = thread_return_target(return_idx);
    let Some(idx) = states.len().checked_sub(1) else {
        states.push(ThreadFsmState {
            comb_stmts: Vec::new(),
            seq_stmts: Vec::new(),
            transition_cond: None,
            wait_cycles: None,
            multi_transitions: Vec::new(),
            terminal_return: Some(return_idx),
            folded_exit_seq: Vec::new(),
            folded_exit_target: None,
            is_folded: false,
            no_fold_into_prev: false,
            lock_release: None,
            lock_release_info: None,
            is_lock_body: false,
        });
        return;
    };
    let next_idx = states.len();
    let s = &mut states[idx];
    if !s.multi_transitions.is_empty() {
        let mut rewrote = false;
        for (_, t) in &mut s.multi_transitions {
            if *t == next_idx || *t == THREAD_TARGET_NEXT {
                *t = target;
                rewrote = true;
            }
        }
        if !rewrote {
            s.multi_transitions
                .push((Expr::new(ExprKind::Bool(true), span), target));
        }
        return;
    }
    s.terminal_return = Some(return_idx);
}

fn contains_wait(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::WaitUntil(..) | ThreadStmt::WaitCycles(..) | ThreadStmt::DoUntil { .. } => true,
        ThreadStmt::IfElse(ie) => contains_wait(&ie.then_stmts) || contains_wait(&ie.else_stmts),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| contains_wait(br)),
        ThreadStmt::For { body, .. } => contains_wait(body),
        ThreadStmt::Lock { body, .. } => contains_wait(body),
        _ => false,
    })
}

pub(crate) fn contains_return(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::Return(..) => true,
        ThreadStmt::IfElse(ie) => {
            contains_return(&ie.then_stmts) || contains_return(&ie.else_stmts)
        }
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| contains_return(br)),
        ThreadStmt::For { body, .. } => contains_return(body),
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => contains_return(body),
        _ => false,
    })
}

pub(crate) fn thread_block_always_returns(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(thread_stmt_always_returns)
}

fn thread_stmt_always_returns(stmt: &ThreadStmt) -> bool {
    match stmt {
        ThreadStmt::Return(..) => true,
        ThreadStmt::IfElse(ie) => {
            thread_block_always_returns(&ie.then_stmts)
                && thread_block_always_returns(&ie.else_stmts)
        }
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => {
            thread_block_always_returns(body)
        }
        _ => false,
    }
}

fn expr_and(a: Expr, b: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Binary(BinOp::And, Box::new(a), Box::new(b)), span)
}

fn expr_not(e: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span)
}

struct HoistedThreadState {
    comb_stmts: Vec<Stmt>,
    seq_stmts: Vec<Stmt>,
}

fn try_hoist_initial_thread_state(states: &mut Vec<ThreadFsmState>) -> Option<HoistedThreadState> {
    let first = states.first()?;
    if !first.comb_stmts.is_empty()
        || first.seq_stmts.is_empty()
        || first.transition_cond.is_some()
        || first.wait_cycles.is_some()
        || !first.multi_transitions.is_empty()
    {
        return None;
    }

    // Only remove the first state if no later local transition targets it.
    // This keeps loop/fork products that intentionally branch to state 0
    // on the conservative path.
    if states
        .iter()
        .skip(1)
        .any(|s| s.multi_transitions.iter().any(|(_, target)| *target == 0))
    {
        return None;
    }

    let first = states.remove(0);
    for s in states {
        for (_, target) in &mut s.multi_transitions {
            if *target != usize::MAX {
                *target -= 1;
            }
        }
    }
    Some(HoistedThreadState {
        comb_stmts: Vec::new(),
        seq_stmts: first.seq_stmts,
    })
}

fn offset_thread_state_targets(states: &mut [ThreadFsmState], base: usize, len: usize) {
    for fs in states {
        for (_, target) in &mut fs.multi_transitions {
            if *target == usize::MAX {
                *target = base + len;
            } else {
                *target += base;
            }
        }
    }
}

fn flatten_thread_ifelse_chain<'a>(
    ie: &'a ThreadIfElse,
) -> (Vec<(Expr, &'a [ThreadStmt])>, &'a [ThreadStmt]) {
    let mut arms = vec![(ie.cond.clone(), ie.then_stmts.as_slice())];
    let mut else_stmts = ie.else_stmts.as_slice();
    while let [ThreadStmt::IfElse(nested)] = else_stmts {
        arms.push((nested.cond.clone(), nested.then_stmts.as_slice()));
        else_stmts = nested.else_stmts.as_slice();
    }
    (arms, else_stmts)
}

fn guarded_stmt(cond: Expr, stmts: Vec<Stmt>, span: Span) -> Option<Stmt> {
    if stmts.is_empty() {
        None
    } else {
        Some(Stmt::IfElse(IfElse {
            cond,
            then_stmts: stmts,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }))
    }
}

fn lit_same_shape(a: &LitKind, b: &LitKind) -> bool {
    match (a, b) {
        (LitKind::Dec(a), LitKind::Dec(b))
        | (LitKind::Hex(a), LitKind::Hex(b))
        | (LitKind::Bin(a), LitKind::Bin(b)) => a == b,
        (LitKind::Sized(aw, av), LitKind::Sized(bw, bv)) => aw == bw && av == bv,
        _ => false,
    }
}

fn inside_member_same_shape(a: &InsideMember, b: &InsideMember) -> bool {
    match (a, b) {
        (InsideMember::Single(a), InsideMember::Single(b)) => expr_same_shape(a, b),
        (InsideMember::Range(alo, ahi), InsideMember::Range(blo, bhi)) => {
            expr_same_shape(alo, blo) && expr_same_shape(ahi, bhi)
        }
        _ => false,
    }
}

fn expr_slice_same_shape(a: &[Expr], b: &[Expr]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(a_expr, b_expr)| expr_same_shape(a_expr, b_expr))
}

fn expr_same_shape(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Literal(a), ExprKind::Literal(b)) => lit_same_shape(a, b),
        (ExprKind::Ident(a), ExprKind::Ident(b)) => a == b,
        (ExprKind::SynthIdent(a, _), ExprKind::SynthIdent(b, _)) => a == b,
        (ExprKind::Binary(a_op, a_l, a_r), ExprKind::Binary(b_op, b_l, b_r)) => {
            a_op == b_op && expr_same_shape(a_l, b_l) && expr_same_shape(a_r, b_r)
        }
        (ExprKind::Unary(a_op, a_e), ExprKind::Unary(b_op, b_e)) => {
            a_op == b_op && expr_same_shape(a_e, b_e)
        }
        (ExprKind::FieldAccess(a_base, a_field), ExprKind::FieldAccess(b_base, b_field)) => {
            a_field.name == b_field.name && expr_same_shape(a_base, b_base)
        }
        (
            ExprKind::MethodCall(a_base, a_name, a_args),
            ExprKind::MethodCall(b_base, b_name, b_args),
        ) => {
            a_name.name == b_name.name
                && expr_same_shape(a_base, b_base)
                && expr_slice_same_shape(a_args, b_args)
        }
        (ExprKind::Index(a_base, a_idx), ExprKind::Index(b_base, b_idx)) => {
            expr_same_shape(a_base, b_base) && expr_same_shape(a_idx, b_idx)
        }
        (ExprKind::BitSlice(a_base, a_hi, a_lo), ExprKind::BitSlice(b_base, b_hi, b_lo)) => {
            expr_same_shape(a_base, b_base)
                && expr_same_shape(a_hi, b_hi)
                && expr_same_shape(a_lo, b_lo)
        }
        (
            ExprKind::PartSelect(a_base, a_start, a_width, a_dir),
            ExprKind::PartSelect(b_base, b_start, b_width, b_dir),
        ) => {
            a_dir == b_dir
                && expr_same_shape(a_base, b_base)
                && expr_same_shape(a_start, b_start)
                && expr_same_shape(a_width, b_width)
        }
        (ExprKind::EnumVariant(a_enum, a_var), ExprKind::EnumVariant(b_enum, b_var)) => {
            a_enum.name == b_enum.name && a_var.name == b_var.name
        }
        (ExprKind::Todo, ExprKind::Todo) => true,
        (ExprKind::Bool(a), ExprKind::Bool(b)) => a == b,
        (ExprKind::Concat(a), ExprKind::Concat(b)) => expr_slice_same_shape(a, b),
        (ExprKind::Repeat(a_count, a_expr), ExprKind::Repeat(b_count, b_expr)) => {
            expr_same_shape(a_count, b_count) && expr_same_shape(a_expr, b_expr)
        }
        (ExprKind::Clog2(a), ExprKind::Clog2(b))
        | (ExprKind::Onehot(a), ExprKind::Onehot(b))
        | (ExprKind::Signed(a), ExprKind::Signed(b))
        | (ExprKind::Unsigned(a), ExprKind::Unsigned(b)) => expr_same_shape(a, b),
        (ExprKind::LatencyAt(a, a_n), ExprKind::LatencyAt(b, b_n)) => {
            a_n == b_n && expr_same_shape(a, b)
        }
        (ExprKind::SvaNext(a_n, a), ExprKind::SvaNext(b_n, b)) => {
            a_n == b_n && expr_same_shape(a, b)
        }
        (ExprKind::FunctionCall(a_name, a_args), ExprKind::FunctionCall(b_name, b_args)) => {
            a_name == b_name && expr_slice_same_shape(a_args, b_args)
        }
        (ExprKind::Inside(a_expr, a_members), ExprKind::Inside(b_expr, b_members)) => {
            expr_same_shape(a_expr, b_expr)
                && a_members.len() == b_members.len()
                && a_members
                    .iter()
                    .zip(b_members.iter())
                    .all(|(a_member, b_member)| inside_member_same_shape(a_member, b_member))
        }
        (ExprKind::Ternary(a_c, a_t, a_f), ExprKind::Ternary(b_c, b_t, b_f)) => {
            expr_same_shape(a_c, b_c) && expr_same_shape(a_t, b_t) && expr_same_shape(a_f, b_f)
        }
        _ => false,
    }
}

fn fast_wait_if_condition(ie: &ThreadIfElse) -> Option<Expr> {
    if !ie.else_stmts.is_empty() || ie.then_stmts.len() != 1 {
        return None;
    }

    let ThreadStmt::WaitUntil(wait_cond, _) = &ie.then_stmts[0] else {
        return None;
    };
    let ExprKind::Unary(UnaryOp::Not, if_cond_inner) = &ie.cond.kind else {
        return None;
    };
    if expr_same_shape(if_cond_inner, wait_cond) {
        Some(wait_cond.clone())
    } else {
        None
    }
}

fn merge_fast_region_assigns(
    states: &mut [ThreadFsmState],
    fast_region: &mut Option<(usize, Expr)>,
    cur_comb: &mut Vec<Stmt>,
    cur_seq: &mut Vec<Stmt>,
    span: Span,
) -> bool {
    let Some((state_idx, guard)) = fast_region.take() else {
        return false;
    };
    if let Some(stmt) = guarded_stmt(guard.clone(), std::mem::take(cur_comb), span) {
        states[state_idx].comb_stmts.push(stmt);
    }
    if let Some(stmt) = guarded_stmt(guard, std::mem::take(cur_seq), span) {
        states[state_idx].seq_stmts.push(stmt);
    }
    true
}

fn flush_pending_thread_state(
    states: &mut Vec<ThreadFsmState>,
    fast_region: &mut Option<(usize, Expr)>,
    cur_comb: &mut Vec<Stmt>,
    cur_seq: &mut Vec<Stmt>,
    span: Span,
) -> bool {
    if cur_comb.is_empty() && cur_seq.is_empty() {
        return fast_region.take().is_some();
    }
    if merge_fast_region_assigns(states, fast_region, cur_comb, cur_seq, span) {
        return true;
    }
    states.push(ThreadFsmState {
        comb_stmts: std::mem::take(cur_comb),
        seq_stmts: std::mem::take(cur_seq),
        transition_cond: None,
        wait_cycles: None,
        multi_transitions: Vec::new(),
        terminal_return: None,
        folded_exit_seq: Vec::new(),
        folded_exit_target: None,
        is_folded: false,
        no_fold_into_prev: false,
        lock_release: None,
        lock_release_info: None,
        is_lock_body: false,
    });
    true
}

fn collect_single_state_thread_body(body: &[ThreadStmt]) -> (Vec<Stmt>, Vec<Stmt>) {
    let mut comb_stmts = Vec::new();
    let mut seq_stmts = Vec::new();
    for stmt in body {
        match stmt {
            ThreadStmt::CombAssign(ca) => comb_stmts.push(Stmt::Assign(ca.clone())),
            ThreadStmt::SeqAssign(ra) => seq_stmts.push(Stmt::Assign(ra.clone())),
            ThreadStmt::IfElse(ie) => {
                let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                if let Some(stmt) = comb_if {
                    comb_stmts.push(stmt);
                }
                if let Some(stmt) = seq_if {
                    seq_stmts.push(stmt);
                }
            }
            ThreadStmt::Log(l) => seq_stmts.push(Stmt::Log(l.clone())),
            _ => {
                unreachable!("single-state thread body contained an unexpected statement");
            }
        }
    }
    (comb_stmts, seq_stmts)
}

fn mealy_gate_first_lock_state(lock_states: &mut [ThreadFsmState], cond: &Expr, span: Span) {
    if let Some(first) = lock_states.first_mut() {
        let existing = std::mem::take(&mut first.comb_stmts);
        first.comb_stmts.push(Stmt::IfElse(IfElse {
            cond: cond.clone(),
            then_stmts: existing,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
        if let Some(existing_cond) = first.transition_cond.take() {
            first.transition_cond = Some(Expr::new(
                ExprKind::Binary(BinOp::And, Box::new(cond.clone()), Box::new(existing_cond)),
                span,
            ));
        }
    }
}

/// Optimize the common micro-architecture shape:
///
///   wait until req;
///   if op_a
///     first_cycle_a <= ...;
///     wait 1 cycle;
///   else
///     first_cycle_b <= ...;
///     wait 1 cycle;
///   end if
///
/// The conservative lowering emits `wait -> dispatch -> branch-prefix`.
/// A hand-written FSM usually folds the dispatch and first-cycle branch
/// work onto the edge that exits the wait state. This helper performs that
/// fusion when the immediately preceding state is a plain `wait until` and
/// the branch's first state is an unconditional seq-only action.
fn try_fuse_wait_ifelse(
    states: &mut Vec<ThreadFsmState>,
    ie: &ThreadIfElse,
    cnt_width: u32,
    loop_id_gen: &mut u32,
) -> Result<bool, CompileError> {
    let Some(wait_idx) = states.len().checked_sub(1) else {
        return Ok(false);
    };
    if states[wait_idx].transition_cond.is_none()
        || states[wait_idx].wait_cycles.is_some()
        || !states[wait_idx].multi_transitions.is_empty()
    {
        return Ok(false);
    }

    let wait_cond = states[wait_idx].transition_cond.clone().unwrap();

    let (arms, default_body) = flatten_thread_ifelse_chain(ie);
    let mut branch_meta: Vec<(usize, usize, Option<HoistedThreadState>)> = Vec::new();

    let mut branch_bodies: Vec<&[ThreadStmt]> = arms.iter().map(|(_, body)| *body).collect();
    branch_bodies.push(default_body);
    for body in branch_bodies {
        let mut branch_states = if body.is_empty() {
            Vec::new()
        } else {
            partition_thread_body_with_loop_ids(body, ie.span, cnt_width, loop_id_gen)?
        };
        let hoisted = try_hoist_initial_thread_state(&mut branch_states);
        let base = states.len();
        let len = branch_states.len();
        offset_thread_state_targets(&mut branch_states, base, len);
        states.extend(branch_states);
        branch_meta.push((base, len, hoisted));
    }

    let rejoin_idx = states.len();

    for (base, len, _) in &branch_meta {
        let end = *base + *len;
        if *len == 0 {
            continue;
        }
        if end != rejoin_idx {
            for s_idx in *base..end {
                for (_, t) in &mut states[s_idx].multi_transitions {
                    if *t == end {
                        *t = rejoin_idx;
                    }
                }
            }
        }
        redirect_fallthrough_to(&mut states[..], end - 1, rejoin_idx, ie.span);
    }

    let mut guards = Vec::new();
    let mut prior = wait_cond.clone();
    for (cond, _) in &arms {
        let guard = expr_and(prior.clone(), cond.clone(), ie.span);
        guards.push(guard);
        prior = expr_and(prior, expr_not(cond.clone(), ie.span), ie.span);
    }
    guards.push(prior);

    for (guard, (_, _, hoisted)) in guards.iter().cloned().zip(branch_meta.iter_mut()) {
        if let Some(h) = hoisted.take() {
            if let Some(stmt) = guarded_stmt(guard.clone(), h.comb_stmts, ie.span) {
                states[wait_idx].comb_stmts.push(stmt);
            }
            if let Some(stmt) = guarded_stmt(guard, h.seq_stmts, ie.span) {
                states[wait_idx].seq_stmts.push(stmt);
            }
        }
    }

    let transitions = guards
        .into_iter()
        .zip(branch_meta.iter())
        .map(|(guard, (base, len, _))| {
            let target = if *len == 0 { rejoin_idx } else { *base };
            (guard, target)
        })
        .collect();

    states[wait_idx].transition_cond = None;
    states[wait_idx].multi_transitions = transitions;

    Ok(true)
}

/// Partition thread body into FSM states, sharing a loop-counter id
/// generator with the caller. Each `for` instance encountered allocates a
/// fresh id from `loop_id_gen` and writes it back via the `&mut u32` so the
/// caller can declare the matching `_loop_cnt_{id}` regs. Nested `for`s
/// must each get a distinct counter — sharing one causes the inner loop
/// to clobber the outer's running index (issue #414).
fn partition_thread_body_with_loop_ids(
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    partition_thread_body_impl(body, span, cnt_width, None, loop_id_gen)
}

/// Validate the body of a `do … until cond;` statement.
///
/// `do … until` is a SINGLE-STATE hold construct: the body's comb/seq
/// assigns fire every cycle while the FSM is parked in this state, and
/// the FSM advances when `cond` becomes true. It is *not* a loop; for a
/// real loop use `for c in S..E { ... }` (which generates a `_loop_cnt`
/// register and proper back-edge transitions).
///
/// Bodies are restricted to `CombAssign`, `SeqAssign`, `IfElse`, and `Log`.
/// Any other `ThreadStmt` variant — `Lock`, `For`, `WaitUntil`, `WaitCycles`,
/// `ForkJoin`, nested `DoUntil`, `Return`, `ForkTlmAssign`, `JoinAll` — cannot be lowered as a hold-state and was
/// historically silently dropped, producing FSMs that miscompiled to an
/// infinite-loop (see issue #410). Reject those constructs with a precise
/// error pointing at the offending inner statement.
///
/// `IfElse` is allowed at the top level but its own then/else bodies are
/// recursively constrained the same way — a nested `wait` inside an `if`
/// inside a `do … until` would otherwise be silently dropped by
/// `thread_if_to_fsm_stmts`.
fn disallow_nested_control_in_do_until(
    body: &[ThreadStmt],
    do_span: Span,
) -> Result<(), CompileError> {
    for s in body {
        let bad = match s {
            ThreadStmt::CombAssign(_) | ThreadStmt::SeqAssign(_) | ThreadStmt::Log(_) => None,
            ThreadStmt::IfElse(ie) => {
                disallow_nested_control_in_do_until(&ie.then_stmts, do_span)?;
                disallow_nested_control_in_do_until(&ie.else_stmts, do_span)?;
                None
            }
            ThreadStmt::Lock { .. } => Some("`lock`"),
            ThreadStmt::For { .. } => Some("`for`"),
            ThreadStmt::WaitUntil(..) => Some("`wait until`"),
            ThreadStmt::WaitCycles(..) => Some("`wait N cycle`"),
            ThreadStmt::ForkJoin(..) => Some("`fork`/`join`"),
            ThreadStmt::DoUntil { .. } => Some("a nested `do ... until`"),
            ThreadStmt::Return(..) => Some("`return`"),
            ThreadStmt::ForkTlmAssign(_) => Some("a TLM `fork` call"),
            ThreadStmt::JoinAll(_) => Some("`join all`"),
        };
        if let Some(what) = bad {
            return Err(CompileError::general(
                &format!(
                    "{} is not allowed inside `do ... until` — that construct is a single-cycle-per-iteration hold state (drive comb + seq while waiting for the exit condition), not a loop. \
                    Use `for c in 0..N-1 ... end for` for a bounded iteration, or split the work into multiple `wait until` / `do ... until` statements at thread top level.",
                    what,
                ),
                thread_stmt_span(s).merge(do_span),
            ));
        }
    }
    Ok(())
}

/// TLM-target variant of [`partition_thread_body_with_loop_ids`] that also
/// collects early-return expressions. The number of loop counters allocated
/// is reflected through `loop_id_gen` so the caller can declare matching
/// `_loop_cnt_{id}` regs.
pub(crate) fn partition_tlm_target_thread_body_with_loop_ids(
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
    return_exprs: &mut Vec<Expr>,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    partition_thread_body_impl(body, span, cnt_width, Some(return_exprs), loop_id_gen)
}

fn partition_thread_body_impl(
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
    mut target_returns: Option<&mut Vec<Expr>>,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    let mut states: Vec<ThreadFsmState> = Vec::new();
    let mut cur_comb: Vec<Stmt> = Vec::new();
    let mut cur_seq: Vec<Stmt> = Vec::new();
    let mut fast_region: Option<(usize, Expr)> = None;
    let mut no_trailing_merge_from: Option<usize> = None;
    // Issue #306: set to true when a `wait 1 cycle` was elided using the
    // natural wait_until→action transition as the 1-cycle budget.  The NEXT
    // action state created must be marked `no_fold_into_prev` so the fold
    // pass does not absorb it back into the wait_until state (which would
    // lose the 1-cycle guarantee provided by the elision).
    let mut next_state_no_fold: bool = false;
    for (stmt_idx, stmt) in body.iter().enumerate() {
        match stmt {
            ThreadStmt::CombAssign(ca) => {
                cur_comb.push(Stmt::Assign(ca.clone()));
            }
            ThreadStmt::SeqAssign(ra) => {
                cur_seq.push(Stmt::Assign(ra.clone()));
            }
            ThreadStmt::Log(l) => {
                cur_seq.push(Stmt::Log(l.clone()));
            }
            ThreadStmt::WaitUntil(cond, sp) => {
                if let Some((state_idx, guard)) = fast_region.take() {
                    if let Some(stmt) = guarded_stmt(guard.clone(), cur_comb.clone(), *sp) {
                        states[state_idx].comb_stmts.push(stmt);
                    }
                    if let Some(stmt) = guarded_stmt(guard, std::mem::take(&mut cur_seq), *sp) {
                        states[state_idx].seq_stmts.push(stmt);
                    }
                }
                // Per spec §7a.2: only TRAILING seq assigns (after the last
                // wait in the body) may merge into the preceding state's
                // exit. Inter-yield seq assigns — assigns sitting BETWEEN
                // two yield statements — are not trailing, and must each
                // get a dead-skid state with unconditional advance.
                //
                // Comb assigns flow INTO the wait state so they hold while
                // waiting (`valid=1; wait until ready;` AXI intent). When
                // a dead-skid prefix state is needed (because seq assigns
                // were pending), comb assigns are duplicated into both the
                // prefix and the wait state so the protocol output stays
                // stable across the full inter-yield region — re-evaluating
                // the same comb expression in two consecutive states
                // produces the same per-cycle value.
                if !cur_seq.is_empty() {
                    // Issue #306: if next_state_no_fold is set (from a prior
                    // `wait 1 cycle` elision), apply it to the dead-skid
                    // prefix state and reset the flag.
                    let nfip = std::mem::take(&mut next_state_no_fold);
                    states.push(ThreadFsmState {
                        comb_stmts: cur_comb.clone(),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                        terminal_return: None,
                        folded_exit_seq: Vec::new(),
                        folded_exit_target: None,
                        is_folded: false,
                        no_fold_into_prev: nfip,
                        lock_release: None,
                        lock_release_info: None,
                        is_lock_body: false,
                    });
                } else {
                    // No dead-skid state created; reset next_state_no_fold
                    // since the wait_until state itself doesn't need it (the
                    // fold only targets action states, not wait states).
                    next_state_no_fold = false;
                }
                states.push(ThreadFsmState {
                    comb_stmts: std::mem::take(&mut cur_comb),
                    seq_stmts: Vec::new(),
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                    terminal_return: None,
                    folded_exit_seq: Vec::new(),
                    folded_exit_target: None,
                    is_folded: false,
                    no_fold_into_prev: false,
                    lock_release: None,
                    lock_release_info: None,
                    is_lock_body: false,
                });
                let _ = sp; // span retained for parity with the prior arm
            }
            ThreadStmt::WaitCycles(count, _) => {
                // Same: pure boundary, flush all pending assigns
                let merged_fast_idx = fast_region.as_ref().map(|(idx, _)| *idx);
                let had_flush = flush_pending_thread_state(
                    &mut states,
                    &mut fast_region,
                    &mut cur_comb,
                    &mut cur_seq,
                    span,
                );
                // `wait 1 cycle` between two seq-write boundaries is a no-op
                // structurally — the natural state transition from the
                // flushed prior state to whatever state comes next already
                // takes one clock edge. Emitting a dedicated wait_cycles
                // state for N=1 adds an extra cycle (load cnt=0, decrement,
                // check cnt==0, transition), so e.g.
                // `phase_q <= a; wait 1 cycle; phase_q <= b;` would put two
                // cycles between the two phase_q transitions instead of one.
                // Elide the wait state when (a) count is literal 1 AND
                // (b) a flush state was pushed in front (so the natural
                // transition out of that state provides the 1 cycle).
                // For standalone `wait 1 cycle` with no preceding flush
                // (e.g. an if/else branch whose only body is `wait 1
                // cycle;`), keep the wait state — eliding would leave the
                // branch with zero states and break dispatch-and-rejoin.
                let count_is_one = matches!(
                    &count.kind,
                    ExprKind::Literal(LitKind::Dec(1))
                        | ExprKind::Literal(LitKind::Hex(1))
                        | ExprKind::Literal(LitKind::Bin(1))
                        | ExprKind::Literal(LitKind::Sized(_, 1))
                );
                if !count_is_one || !had_flush {
                    // A real wait_cycles state is pushed; any prior
                    // `next_state_no_fold` from an earlier elision is no
                    // longer relevant (the boundary state absorbed it).
                    next_state_no_fold = false;
                    states.push(ThreadFsmState {
                        comb_stmts: Vec::new(),
                        seq_stmts: Vec::new(),
                        transition_cond: None,
                        wait_cycles: Some(count.clone()),
                        multi_transitions: Vec::new(),
                        terminal_return: None,
                        folded_exit_seq: Vec::new(),
                        folded_exit_target: None,
                        is_folded: false,
                        no_fold_into_prev: false,
                        lock_release: None,
                        lock_release_info: None,
                        is_lock_body: false,
                    });
                } else if let Some(idx) = merged_fast_idx {
                    no_trailing_merge_from = Some(idx);
                    // Issue #306: mark that the next action state (created after
                    // this elided wait) must not be folded into the preceding
                    // wait_until state — the natural transition provides the
                    // 1-cycle budget already consumed by the elision.
                    next_state_no_fold = true;
                }
            }
            ThreadStmt::IfElse(ie) => {
                if cur_comb.is_empty() && cur_seq.is_empty() {
                    if let Some(cond) = fast_wait_if_condition(ie) {
                        let fast_idx = states.len();
                        states.push(ThreadFsmState {
                            comb_stmts: Vec::new(),
                            seq_stmts: Vec::new(),
                            transition_cond: Some(cond.clone()),
                            wait_cycles: None,
                            multi_transitions: Vec::new(),
                            terminal_return: None,
                            folded_exit_seq: Vec::new(),
                            folded_exit_target: None,
                            is_folded: false,
                            no_fold_into_prev: false,
                            lock_release: None,
                            lock_release_info: None,
                            is_lock_body: false,
                        });
                        fast_region = Some((fast_idx, cond));
                        continue;
                    }
                }
                let then_has_wait = contains_wait(&ie.then_stmts);
                let else_has_wait = contains_wait(&ie.else_stmts);
                let then_has_return = contains_return(&ie.then_stmts);
                let else_has_return = contains_return(&ie.else_stmts);
                if then_has_wait || else_has_wait || then_has_return || else_has_return {
                    if cur_comb.is_empty()
                        && cur_seq.is_empty()
                        && !then_has_return
                        && !else_has_return
                        && try_fuse_wait_ifelse(&mut states, ie, cnt_width, loop_id_gen)?
                    {
                        fast_region.take();
                        continue;
                    }

                    // Dispatch-and-rejoin (see doc/thread_lowering_proof.md §II.10).
                    // Step 1: flush pending comb/seq into a predecessor state so
                    // `cond` reads post-flush register values.
                    flush_pending_thread_state(
                        &mut states,
                        &mut fast_region,
                        &mut cur_comb,
                        &mut cur_seq,
                        ie.span,
                    );
                    // Step 2: insert dispatch state placeholder; M filled below
                    // once branch base indices are known.
                    let dispatch_idx = states.len();
                    states.push(ThreadFsmState {
                        comb_stmts: Vec::new(),
                        seq_stmts: Vec::new(),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                        terminal_return: None,
                        folded_exit_seq: Vec::new(),
                        folded_exit_target: None,
                        is_folded: false,
                        no_fold_into_prev: false,
                        lock_release: None,
                        lock_release_info: None,
                        is_lock_body: false,
                    });
                    // Step 3: recursively partition `then_stmts` and append at then_base.
                    // Empty branches (§II.10.4) skip the recursive call —
                    // `partition_thread_body` rejects empty bodies, but the
                    // dispatch-and-rejoin lowering treats them as a direct jump
                    // to the rejoin index.
                    let then_base = states.len();
                    if !ie.then_stmts.is_empty() {
                        let mut then_states = if let Some(rets) = target_returns.as_deref_mut() {
                            partition_thread_body_impl(
                                &ie.then_stmts,
                                ie.span,
                                cnt_width,
                                Some(rets),
                                loop_id_gen,
                            )?
                        } else {
                            partition_thread_body_with_loop_ids(
                                &ie.then_stmts,
                                ie.span,
                                cnt_width,
                                loop_id_gen,
                            )?
                        };
                        let then_len = then_states.len();
                        for fs in &mut then_states {
                            for (_, target) in &mut fs.multi_transitions {
                                // Sentinel `usize::MAX` is the "next state after
                                // this for group" marker emitted by
                                // `lower_thread_for`. Inside a branch, that
                                // fallthrough should land at the rejoin index;
                                // the redirect step below rewrites it.
                                if *target == THREAD_TARGET_NEXT {
                                    *target = then_base + then_len;
                                } else if !thread_target_is_special(*target) {
                                    *target += then_base;
                                }
                            }
                        }
                        states.extend(then_states);
                    }
                    // Step 4: same for `else_stmts` at else_base.
                    let else_base = states.len();
                    if !ie.else_stmts.is_empty() {
                        let mut else_states = if let Some(rets) = target_returns.as_deref_mut() {
                            partition_thread_body_impl(
                                &ie.else_stmts,
                                ie.span,
                                cnt_width,
                                Some(rets),
                                loop_id_gen,
                            )?
                        } else {
                            partition_thread_body_with_loop_ids(
                                &ie.else_stmts,
                                ie.span,
                                cnt_width,
                                loop_id_gen,
                            )?
                        };
                        let else_len = else_states.len();
                        for fs in &mut else_states {
                            for (_, target) in &mut fs.multi_transitions {
                                if *target == THREAD_TARGET_NEXT {
                                    *target = else_base + else_len;
                                } else if !thread_target_is_special(*target) {
                                    *target += else_base;
                                }
                            }
                        }
                        states.extend(else_states);
                    }
                    let rejoin_idx = states.len();

                    // Fix for the for-loop-in-then-branch asymmetry (see
                    // doc/thread_lowering_proof.md §II.10.4).  In the
                    // then-branch, the natural "next state past this branch"
                    // is `else_base` (= `then_base + then_len`).  When a
                    // recursive `partition_thread_body` call resolves a
                    // `usize::MAX` sentinel (e.g. for-loop exit, nested
                    // if/else rejoin), the result after outer shifting is
                    // `else_base`, NOT `rejoin_idx`.  Walk the then-branch
                    // states and rewrite any such targets to `rejoin_idx`.
                    //
                    // The else-branch is symmetric and self-correcting:
                    // `else_base + else_len = rejoin_idx`, so its sentinels
                    // naturally land at `rejoin_idx`.  No rewrite needed.
                    //
                    // Without this rewrite, `redirect_fallthrough_to` case
                    // (A) appends `(true, rejoin_idx)` after the existing
                    // `(exit_cond, else_base)` arm, which under last-write-
                    // wins always fires and overrides the for-loop's
                    // loop-back arm — making the body execute exactly once.
                    if then_base < else_base {
                        for s_idx in then_base..else_base {
                            for (_, t) in &mut states[s_idx].multi_transitions {
                                if *t == else_base {
                                    *t = rejoin_idx;
                                }
                            }
                        }
                    }

                    // Step 5: redirect each branch's natural exit to rejoin_idx.
                    if then_base < else_base {
                        redirect_fallthrough_to(&mut states, else_base - 1, rejoin_idx, ie.span);
                    }
                    if else_base < rejoin_idx {
                        redirect_fallthrough_to(&mut states, rejoin_idx - 1, rejoin_idx, ie.span);
                    }
                    // Step 2 (deferred): fill dispatch state's M.
                    // Empty-branch handling (§II.10.4): if a branch is empty, its
                    // base equals the next position, and the dispatch jumps there.
                    let then_target = if then_base == else_base {
                        rejoin_idx
                    } else {
                        then_base
                    };
                    let else_target = if else_base == rejoin_idx {
                        rejoin_idx
                    } else {
                        else_base
                    };
                    let neg_cond = Expr::new(
                        ExprKind::Unary(UnaryOp::Not, Box::new(ie.cond.clone())),
                        ie.span,
                    );
                    states[dispatch_idx].multi_transitions =
                        vec![(ie.cond.clone(), then_target), (neg_cond, else_target)];
                } else {
                    // Same-state conditional: convert to IfElse / IfElse for comb and seq
                    let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                    if let Some(c) = comb_if {
                        cur_comb.push(c);
                    }
                    if let Some(s) = seq_if {
                        cur_seq.push(s);
                    }
                }
            }
            ThreadStmt::ForkJoin(branches, sp) => {
                // Flush pending statements into a state before fork
                flush_pending_thread_state(
                    &mut states,
                    &mut fast_region,
                    &mut cur_comb,
                    &mut cur_seq,
                    *sp,
                );
                // Lower fork/join via product-state expansion
                let mut fork_states = lower_fork_join(branches, *sp, cnt_width, loop_id_gen)?;
                // Adjust multi_transitions targets: product indices → global state indices
                let fork_base = states.len();
                for fs in &mut fork_states {
                    for (_, target) in &mut fs.multi_transitions {
                        if !thread_target_is_special(*target) {
                            *target += fork_base;
                        }
                    }
                }
                states.extend(fork_states);
            }
            ThreadStmt::For {
                var,
                start,
                end,
                body,
                span,
            } => {
                // Allocate this for-loop's unique counter id and name. Nested
                // for-loops inside `body` get their own ids via the recursive
                // partition inside `lower_thread_for`. Issue #414: without
                // per-instance ids, all `for`s in a thread shared one
                // `_loop_cnt`, so an inner loop clobbered the outer.
                let loop_id = *loop_id_gen;
                let cnt_name = format!("_loop_cnt_{}", loop_id);
                // Counter init: merge into the last existing state (if it has
                // unconditional advance) to avoid a dead cycle. Otherwise flush.
                let cnt_init = Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(cnt_name.clone()), *span),
                    value: start.clone(),
                    span: *span,
                });
                let merged = if cur_comb.is_empty() && cur_seq.is_empty() {
                    // No pending assigns — merge counter init into last state.
                    // The init fires on the same edge as the state's transition,
                    // so the counter is ready when the for-body starts.
                    if let Some((fast_idx, guard)) = fast_region.take() {
                        if let Some(stmt) = guarded_stmt(guard, vec![cnt_init.clone()], *span) {
                            states[fast_idx].seq_stmts.push(stmt);
                        }
                        true
                    } else if let Some(last_idx) = states.len().checked_sub(1) {
                        let last = &mut states[last_idx];
                        if last.multi_transitions.is_empty()
                            && no_trailing_merge_from != Some(last_idx)
                        {
                            last.seq_stmts.push(cnt_init.clone());
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !merged {
                    cur_seq.push(cnt_init.clone());
                    flush_pending_thread_state(
                        &mut states,
                        &mut fast_region,
                        &mut cur_comb,
                        &mut cur_seq,
                        *span,
                    );
                }
                let mut for_states =
                    lower_thread_for(var, start, end, body, *span, cnt_width, loop_id_gen)?;
                // Adjust multi_transitions targets (relative → absolute)
                let for_base = states.len();
                let for_len = for_states.len();
                for fs in &mut for_states {
                    for (_, target) in &mut fs.multi_transitions {
                        if *target == THREAD_TARGET_NEXT {
                            // Sentinel: "next state after this for group"
                            *target = for_base + for_len;
                        } else if !thread_target_is_special(*target) {
                            *target += for_base;
                        }
                    }
                }
                states.extend(for_states);
            }
            ThreadStmt::Lock {
                resource,
                body,
                span,
            } => {
                // Nested lock blocks would violate mutual exclusion:
                // once a thread is past the first body state (grant-gated), subsequent
                // states do not re-check grant, so a higher-priority thread can enter
                // the same critical section simultaneously.  Reject at compile time.
                let inner_resources = collect_locked_resources(body);
                if !inner_resources.is_empty() {
                    // Sort for a deterministic diagnostic — HashSet iteration
                    // order is not stable.
                    let mut names: Vec<&str> = inner_resources.iter().map(|s| s.as_str()).collect();
                    names.sort_unstable();
                    return Err(CompileError::general(
                        &format!(
                            "nested lock blocks are not supported (inner lock(s): {}); \
                             use sequential lock blocks instead",
                            names.join(", ")
                        ),
                        *span,
                    ));
                }
                // Flush pending statements
                if cur_comb.is_empty()
                    && cur_seq.is_empty()
                    && fast_region.is_some()
                    && matches!(body.first(), Some(ThreadStmt::DoUntil { .. }))
                {
                    let (fast_idx, wait_cond) = fast_region.take().unwrap();
                    if fast_idx + 1 == states.len() {
                        states.pop();
                    }
                    let mut lock_states =
                        lower_thread_lock(&resource.name, body, *span, cnt_width, loop_id_gen)?;
                    mealy_gate_first_lock_state(&mut lock_states, &wait_cond, *span);
                    states.extend(lock_states);
                    continue;
                }
                flush_pending_thread_state(
                    &mut states,
                    &mut fast_region,
                    &mut cur_comb,
                    &mut cur_seq,
                    *span,
                );
                let lock_states =
                    lower_thread_lock(&resource.name, body, *span, cnt_width, loop_id_gen)?;
                states.extend(lock_states);
            }
            ThreadStmt::DoUntil {
                body,
                cond,
                span: do_sp,
            } => {
                // `do { … } until cond;` is a SINGLE-STATE hold: the body's
                // comb/seq drives fire every cycle while waiting for `cond`.
                // It is NOT a loop construct. Nested control flow inside the
                // body (lock, for, wait, fork, do-until, return) cannot be
                // lowered as a hold-state and was historically silently
                // dropped — producing FSMs that looked plausible but ran
                // forever (issue #410). Reject those constructs up-front so
                // the user sees a precise error pointing at the offending
                // inner statement instead of an infinite-loop miscompile.
                disallow_nested_control_in_do_until(body, *do_sp)?;
                if cur_comb.is_empty() && cur_seq.is_empty() {
                    if let Some((fast_idx, wait_cond)) = fast_region.take() {
                        let (do_comb, do_seq) = collect_single_state_thread_body(body);
                        if let Some(stmt) = guarded_stmt(wait_cond.clone(), do_comb, *do_sp) {
                            states[fast_idx].comb_stmts.push(stmt);
                        }
                        if let Some(stmt) = guarded_stmt(wait_cond.clone(), do_seq, *do_sp) {
                            states[fast_idx].seq_stmts.push(stmt);
                        }
                        states[fast_idx].transition_cond =
                            Some(expr_and(wait_cond, cond.clone(), *do_sp));
                        continue;
                    }
                }
                // Flush pending assigns into a prior state
                flush_pending_thread_state(
                    &mut states,
                    &mut fast_region,
                    &mut cur_comb,
                    &mut cur_seq,
                    *do_sp,
                );
                // Collect the do-body's assigns: comb stays in-state, seq stays in-state
                let (do_comb, do_seq) = collect_single_state_thread_body(body);
                states.push(ThreadFsmState {
                    comb_stmts: do_comb,
                    seq_stmts: do_seq,
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                    terminal_return: None,
                    folded_exit_seq: Vec::new(),
                    folded_exit_target: None,
                    is_folded: false,
                    no_fold_into_prev: false,
                    lock_release: None,
                    lock_release_info: None,
                    is_lock_body: false,
                });
            }
            ThreadStmt::Return(e, ret_span) => {
                if let Some(rets) = target_returns.as_deref_mut() {
                    let return_idx = rets.len();
                    rets.push(e.clone());
                    if !cur_comb.is_empty() || !cur_seq.is_empty() {
                        let merged_fast_idx = fast_region.as_ref().map(|(idx, _)| *idx);
                        if merge_fast_region_assigns(
                            &mut states,
                            &mut fast_region,
                            &mut cur_comb,
                            &mut cur_seq,
                            *ret_span,
                        ) {
                            if let Some(idx) = merged_fast_idx {
                                states[idx].terminal_return = Some(return_idx);
                            }
                        } else {
                            states.push(ThreadFsmState {
                                comb_stmts: std::mem::take(&mut cur_comb),
                                seq_stmts: std::mem::take(&mut cur_seq),
                                transition_cond: None,
                                wait_cycles: None,
                                multi_transitions: Vec::new(),
                                terminal_return: Some(return_idx),
                                folded_exit_seq: Vec::new(),
                                folded_exit_target: None,
                                is_folded: false,
                                no_fold_into_prev: false,
                                lock_release: None,
                                lock_release_info: None,
                                is_lock_body: false,
                            });
                        }
                    } else {
                        redirect_fallthrough_to_return(&mut states, return_idx, *ret_span);
                    }
                    if stmt_idx + 1 != body.len() {
                        return Err(CompileError::general(
                            "statements after `return` are not supported in TLM target thread bodies",
                            *ret_span,
                        ));
                    }
                    break;
                }
                // `return expr;` is only valid inside a TLM method target
                // thread body, which has its own dedicated lowering pass
                // that rewrites Return into the rsp_valid/rsp_data drive
                // sequence before this pass runs. Reaching this arm means
                // a regular thread contained `return`, which is a user
                // error.
                return Err(CompileError::general(
                    "`return` is only valid inside a TLM method target thread (`thread port.method(args) ...`). Remove the return or wrap the body in a TLM target binding.",
                    *ret_span,
                ));
            }
            ThreadStmt::ForkTlmAssign(ra) => {
                return Err(CompileError::general(
                    "`target <= fork port.method(...);` is only valid for TLM initiator threads and must be paired with `join all;`",
                    ra.span,
                ));
            }
            ThreadStmt::JoinAll(span) => {
                return Err(CompileError::general(
                    "`join all;` is only valid after forked TLM calls (`target <= fork port.method(...);`)",
                    *span,
                ));
            }
        }
    }

    // Remaining statements after last wait become the final state.
    // For repeating threads, this state transitions back to S0.
    // For `thread once`, it becomes a terminal hold state.
    //
    // Optimization: if the last state has multi_transitions (e.g. for-loop
    // exit) and the remaining stmts are just seq assigns, merge them into
    // the exit transition's seq (guarded by exit condition) to avoid a
    // dead cycle.
    if fast_region.is_some() {
        flush_pending_thread_state(
            &mut states,
            &mut fast_region,
            &mut cur_comb,
            &mut cur_seq,
            span,
        );
    }
    if !cur_comb.is_empty() || !cur_seq.is_empty() {
        let merged_into_exit = if cur_comb.is_empty() && !cur_seq.is_empty() {
            if let Some(last_idx) = states.len().checked_sub(1) {
                let last = &mut states[last_idx];
                if no_trailing_merge_from == Some(last_idx) {
                    false
                } else if last.multi_transitions.len() == 2 {
                    // For-loop exit: guard trailing seq assigns by exit condition.
                    // Fires on the same clock edge as the for-loop's exit transition.
                    let exit_cond = last.multi_transitions[1].0.clone();
                    for s in cur_seq.iter().cloned() {
                        last.seq_stmts.push(Stmt::IfElse(IfElse {
                            cond: exit_cond.clone(),
                            then_stmts: vec![s],
                            else_stmts: Vec::new(),
                            unique: false,
                            span,
                        }));
                    }
                    // Issue #422: when the for-body's last statement is an
                    // if/else (or any multi-arm dispatch) with each arm
                    // independently falling off the end, the for-loop's
                    // "exit" arm sits not only in `last` but also in the
                    // sibling terminal states. Apply the same trailing-seq
                    // merge to every such state so all arms fire the
                    // outer-block trailing assigns (e.g. the outer
                    // counter's data update).
                    //
                    // The marker for "this transition leaves the body" is
                    // `target == states.len()` (i.e. the not-yet-existing
                    // index just past the for-group, which our
                    // unconditional-advance flush would land on). This is
                    // distinct from `THREAD_TARGET_NEXT`, which by this
                    // point has been resolved.
                    let exit_pos = states.len();
                    let n = states.len();
                    if n >= 2 {
                        for si in 0..n - 1 {
                            // Determine the OR of all conditions targeting exit_pos.
                            let mut exit_arm_conds: Vec<Expr> = Vec::new();
                            for (cond, target) in &states[si].multi_transitions {
                                if *target == exit_pos && !thread_target_is_special(*target) {
                                    exit_arm_conds.push(cond.clone());
                                }
                            }
                            if exit_arm_conds.is_empty() {
                                continue;
                            }
                            let arm_cond = if exit_arm_conds.len() == 1 {
                                exit_arm_conds.pop().unwrap()
                            } else {
                                let mut acc = exit_arm_conds.remove(0);
                                for c in exit_arm_conds {
                                    acc = Expr::new(
                                        ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(c)),
                                        span,
                                    );
                                }
                                acc
                            };
                            for s in cur_seq.iter().cloned() {
                                states[si].seq_stmts.push(Stmt::IfElse(IfElse {
                                    cond: arm_cond.clone(),
                                    then_stmts: vec![s],
                                    else_stmts: Vec::new(),
                                    unique: false,
                                    span,
                                }));
                            }
                        }
                    }
                    cur_seq.clear();
                    true
                } else if last.transition_cond.is_some() && last.multi_transitions.is_empty() {
                    // State with a conditional transition (e.g. do..until, wait until):
                    // guard trailing seq assigns by transition_cond so they fire on the
                    // same clock edge as the state exit — not every cycle while waiting.
                    let guard = last.transition_cond.clone().unwrap();
                    for s in cur_seq.drain(..) {
                        last.seq_stmts.push(Stmt::IfElse(IfElse {
                            cond: guard.clone(),
                            then_stmts: vec![s],
                            else_stmts: Vec::new(),
                            unique: false,
                            span,
                        }));
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !merged_into_exit {
            let nfip = std::mem::take(&mut next_state_no_fold);
            states.push(ThreadFsmState {
                comb_stmts: std::mem::take(&mut cur_comb),
                seq_stmts: std::mem::take(&mut cur_seq),
                transition_cond: None,
                wait_cycles: None,
                multi_transitions: Vec::new(),
                terminal_return: None,
                folded_exit_seq: Vec::new(),
                folded_exit_target: None,
                is_folded: false,
                no_fold_into_prev: nfip,
                lock_release: None,
                lock_release_info: None,
                is_lock_body: false,
            });
        }
    }

    if states.is_empty() {
        return Err(CompileError::general(
            "thread block must contain at least one `wait` statement; use `seq` for single-cycle logic",
            span,
        ));
    }

    Ok(states)
}

/// Lower a fork/join block into a sequence of FSM states using product-state expansion.
///
/// Each branch is partitioned into states. The product of all branch states is computed,
/// and each product-state combination becomes a flat FSM state. The final product state
/// (all branches done) transitions unconditionally to the next main-line state.
fn lower_fork_join(
    branches: &[Vec<ThreadStmt>],
    span: Span,
    cnt_width: u32,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    if branches.len() < 2 {
        return Err(CompileError::general(
            "fork/join requires at least 2 branches",
            span,
        ));
    }

    // Partition each branch, append a "done" hold state to each
    let mut branch_states: Vec<Vec<ThreadFsmState>> = Vec::new();
    for (i, br) in branches.iter().enumerate() {
        let mut states = partition_thread_body_with_loop_ids(br, span, cnt_width, loop_id_gen)
            .map_err(|e| CompileError::general(&format!("in fork branch {}: {}", i, e), span))?;
        if states.is_empty() {
            return Err(CompileError::general(
                &format!("fork branch {} has no wait", i),
                span,
            ));
        }
        states.push(ThreadFsmState {
            comb_stmts: Vec::new(),
            seq_stmts: Vec::new(),
            transition_cond: None,
            wait_cycles: None,
            multi_transitions: Vec::new(),
            terminal_return: None,
            folded_exit_seq: Vec::new(),
            folded_exit_target: None,
            is_folded: false,
            no_fold_into_prev: false,
            lock_release: None,
            lock_release_info: None,
            is_lock_body: false,
        });
        branch_states.push(states);
    }

    let branch_lens: Vec<usize> = branch_states.iter().map(|b| b.len()).collect();
    let total: usize = branch_lens.iter().product();
    if total > 64 {
        return Err(CompileError::general(
            &format!("fork/join product expansion too large ({} states)", total),
            span,
        ));
    }

    // Encode branch indices → flat product index
    let encode = |indices: &[usize]| -> usize {
        let (mut idx, mut m) = (0, 1);
        for (bi, &si) in indices.iter().enumerate() {
            idx += si * m;
            m *= branch_lens[bi];
        }
        idx
    };

    let mut result: Vec<ThreadFsmState> = Vec::new();

    for prod_idx in 0..total {
        // Decode
        let mut indices = Vec::new();
        let mut rem = prod_idx;
        for &len in &branch_lens {
            indices.push(rem % len);
            rem /= len;
        }

        let all_done = indices.iter().zip(&branch_lens).all(|(&i, &l)| i == l - 1);

        // Merge comb/seq from all branches' current states
        let mut comb = Vec::new();
        let mut seq = Vec::new();
        for (bi, &si) in indices.iter().enumerate() {
            let br = &branch_states[bi][si];
            comb.extend(br.comb_stmts.clone());
            if !br.seq_stmts.is_empty() {
                if let Some(ref c) = br.transition_cond {
                    seq.push(Stmt::IfElse(IfElse {
                        cond: c.clone(),
                        then_stmts: br.seq_stmts.clone(),
                        else_stmts: Vec::new(),
                        unique: false,
                        span,
                    }));
                } else {
                    seq.extend(br.seq_stmts.clone());
                }
            }
        }

        if all_done {
            // Skip the all-done product state. The done-marker per-branch
            // states are already empty (lines 2597-2600 push them with
            // empty comb/seq), so the merged comb/seq here are also
            // empty — this state is purely a 1-cycle pass-through.
            // Multi-transitions in non-all_done states encode their
            // destination as `total - 1` (= the would-be all_done
            // index) which, after `fork_base` adjustment in
            // `partition_thread_body`, points at the first post-fork
            // state. Eliding the all_done state removes one cycle of
            // FSM-state-cranking latency at every join.
            //
            // Sanity assert: comb + seq merged here must be empty
            // (otherwise we'd be losing user-driven assignments).
            debug_assert!(
                comb.is_empty() && seq.is_empty(),
                "fork all_done state non-empty — branch done-hold states have unexpected content"
            );
            continue;
        }

        // Build multi-transitions: enumerate subsets of active branches that can fire
        let active: Vec<(usize, Option<&Expr>)> = indices
            .iter()
            .enumerate()
            .filter(|&(bi, &si)| si < branch_lens[bi] - 1)
            .map(|(bi, _)| (bi, branch_states[bi][indices[bi]].transition_cond.as_ref()))
            .collect();

        // Unconditional branches (cond_opt=None) always fire — they must be set in every mask
        let unconditional_mask: u32 = active
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| c.is_none())
            .fold(0u32, |m, (bit, _)| m | (1 << bit));

        let n = active.len();
        let mut multi = Vec::new();

        for mask in (1..(1u32 << n)).rev() {
            // Skip masks that don't include all unconditional branches
            if mask & unconditional_mask != unconditional_mask {
                continue;
            }

            let mut next = indices.clone();
            let mut pos: Vec<Expr> = Vec::new();
            let mut neg: Vec<Expr> = Vec::new();
            for (bit, &(bi, cond_opt)) in active.iter().enumerate() {
                if (mask >> bit) & 1 == 1 {
                    next[bi] += 1;
                    if let Some(c) = cond_opt {
                        pos.push(c.clone());
                    }
                } else if let Some(c) = cond_opt {
                    neg.push(c.clone());
                }
            }
            let mut cond = if pos.is_empty() {
                Expr::new(ExprKind::Bool(true), span)
            } else {
                pos.into_iter()
                    .reduce(|a, b| {
                        Expr::new(ExprKind::Binary(BinOp::And, Box::new(a), Box::new(b)), span)
                    })
                    .unwrap()
            };
            for n in neg {
                cond = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(cond),
                        Box::new(Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(n)), span)),
                    ),
                    span,
                );
            }
            multi.push((cond, encode(&next)));
        }

        result.push(ThreadFsmState {
            comb_stmts: comb,
            seq_stmts: seq,
            transition_cond: None,
            wait_cycles: None,
            multi_transitions: multi,
            terminal_return: None,
            folded_exit_seq: Vec::new(),
            folded_exit_target: None,
            is_folded: false,
            no_fold_into_prev: false,
            lock_release: None,
            lock_release_info: None,
            is_lock_body: false,
        });
    }

    Ok(result)
}

/// Lower a `for` loop with waits into FSM states.
///
/// Generates: INIT state (set counter = start), body states, LOOP_BACK state
/// (increment counter, check if counter <= end → loop or exit).
///
/// Each `for` instance receives a distinct counter register named
/// `_loop_cnt_{id}` (id allocated from the shared `loop_id_gen`). This is
/// critical for nested for-loops: if the inner and outer loops shared a
/// counter, the inner loop's increment would clobber the outer loop's
/// running index, making the outer exit early (issue #414). The per-thread
/// rename pass later prefixes the name with `_t{ti}_`, producing the final
/// register name `_t{ti}_loop_cnt_{id}`.
fn lower_thread_for(
    var: &Ident,
    _start: &Expr,
    end: &Expr,
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    // Allocate a unique counter id for this for-loop instance.
    let loop_id = *loop_id_gen;
    *loop_id_gen += 1;
    let cnt_name = format!("_loop_cnt_{}", loop_id);

    // Replace loop variable with this counter in the body. Nested `for`
    // loops inside `body` will allocate their own ids during the
    // recursive partition below, so they each get distinct counter names.
    let rewritten_body: Vec<ThreadStmt> = body
        .iter()
        .map(|s| rewrite_loop_var(s, &var.name, &cnt_name))
        .collect();

    // Partition the rewritten body into states. Share `loop_id_gen` so
    // any nested `for` allocates a fresh id.
    let body_states =
        partition_thread_body_with_loop_ids(&rewritten_body, span, cnt_width, loop_id_gen)?;
    if body_states.is_empty() {
        return Err(CompileError::general(
            "for loop body must contain at least one wait statement",
            span,
        ));
    }

    let mut result: Vec<ThreadFsmState> = Vec::new();

    // Counter init (loop_cnt <= start) — merged into preceding state by caller,
    // or into a flush state if pending assigns exist.  No separate INIT state.

    // Body states (copied from partition)
    result.extend(body_states);

    // Merge loop counter logic into the LAST body state.
    // Instead of a separate LOOP_CHECK state, the last body state gets:
    //   - counter increment (seq, guarded by transition condition)
    //   - multi_transitions: (body_cond && cnt < end → loop back),
    //                        (body_cond && cnt >= end → exit)
    let cnt_ident = Expr::new(ExprKind::Ident(cnt_name.clone()), span);
    let cnt_inc = Stmt::Assign(RegAssign {
        target: cnt_ident.clone(),
        value: Expr::new(
            ExprKind::MethodCall(
                Box::new(Expr::new(
                    ExprKind::Binary(
                        BinOp::Add,
                        Box::new(cnt_ident.clone()),
                        Box::new(Expr::new(
                            ExprKind::Literal(LitKind::Sized(cnt_width, 1)),
                            span,
                        )),
                    ),
                    span,
                )),
                Ident::new("trunc".to_string(), span),
                vec![Expr::new(
                    ExprKind::Literal(LitKind::Dec(cnt_width as u64)),
                    span,
                )],
            ),
            span,
        ),
        span,
    });

    let loop_back_target = 0;

    // Match the end expression to cnt_width bits for the loop counter comparison.
    // Use `.resize<cnt_width>()` (direction-agnostic) rather than `.trunc<>()`
    // because:
    //   - End expressions like `burst_len_r - 1` widen above cnt_width
    //     (UInt<8> - UInt<1> → UInt<9>), where we need to truncate.
    //   - End expressions like literal `3` are already cnt_width bits
    //     (since `cnt_width` is computed from the end value's bit-width),
    //     where `.trunc<>()` would be flagged as a no-op by typecheck.
    // `resize` accepts both directions without complaint and lowers to the
    // same SV cast when widths match.
    let end_w = Expr::new(
        ExprKind::MethodCall(
            Box::new(end.clone()),
            Ident::new("resize".to_string(), span),
            vec![Expr::new(
                ExprKind::Literal(LitKind::Dec(cnt_width as u64)),
                span,
            )],
        ),
        span,
    );

    let result_len = result.len();
    if let Some(last) = result.last_mut() {
        if !last.multi_transitions.is_empty() {
            // Last body state already carries multi_transitions — typically
            // because the body's last statement is itself a `for` loop
            // (issue #414: nested-for case) whose own `lower_thread_for`
            // populated [(inner_back, 0), (inner_exit, NEXT_resolved)]
            // and the trailing-seq-merge optimization folded any
            // following seq assigns into this state. We must preserve
            // the inner loop-back and wrap only the inner-exit
            // transitions with our counter advance, so the outer
            // counter only ticks once per completed inner iteration.
            // The overwrite-and-rebuild strategy used in the "no
            // multi_transitions" branch below would destroy the inner
            // loop-back and increment the outer counter every cycle.
            //
            // An "inner-exit" transition is one whose target points
            // PAST the for-loop's own body (i.e. target >= result.len()
            // in this for's local index space — the inner For arm
            // resolved its NEXT sentinel to `for_base + for_len`,
            // which in this body's local frame equals `result.len()`).
            // Other transitions (loop-back to 0, jumps within the body)
            // are kept as-is.
            let prev = std::mem::take(&mut last.multi_transitions);
            let mut new_trans: Vec<(Expr, usize)> = Vec::with_capacity(prev.len() + 1);
            let mut inner_exit_conds: Vec<Expr> = Vec::new();
            for (cond, target) in prev {
                if target >= result_len && !thread_target_is_special(target) {
                    inner_exit_conds.push(cond);
                } else {
                    new_trans.push((cond, target));
                }
            }
            // Fold all inner-exit conditions into one (cond_a || cond_b || ...).
            let inner_exit = match inner_exit_conds.len() {
                0 => {
                    // No inner-exit transition was found — fall back to
                    // the unconditional-advance behavior. This shouldn't
                    // normally happen for a nested-for shape, but be
                    // robust if a future construct produces a non-empty
                    // multi_transitions with only intra-body targets.
                    last.seq_stmts.push(cnt_inc.clone());
                    last.multi_transitions = new_trans;
                    last.multi_transitions.push((
                        Expr::new(
                            ExprKind::Binary(
                                BinOp::Lt,
                                Box::new(cnt_ident.clone()),
                                Box::new(end_w.clone()),
                            ),
                            span,
                        ),
                        loop_back_target,
                    ));
                    last.multi_transitions.push((
                        Expr::new(
                            ExprKind::Binary(
                                BinOp::Gte,
                                Box::new(cnt_ident.clone()),
                                Box::new(end_w.clone()),
                            ),
                            span,
                        ),
                        usize::MAX,
                    ));
                    return Ok(result);
                }
                1 => inner_exit_conds.pop().unwrap(),
                _ => {
                    let mut acc = inner_exit_conds.remove(0);
                    for c in inner_exit_conds {
                        acc = Expr::new(
                            ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(c)),
                            span,
                        );
                    }
                    acc
                }
            };
            let inner_exit_for_loop = inner_exit.clone();
            let inner_exit_for_exit = inner_exit.clone();
            // Outer counter increment, guarded by inner-exit so it only
            // ticks once per completed inner iteration.
            last.seq_stmts.push(Stmt::IfElse(IfElse {
                cond: inner_exit,
                then_stmts: vec![cnt_inc.clone()],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
            // Outer loop-back: inner_exit && cnt < end → 0
            let outer_loop_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(inner_exit_for_loop),
                    Box::new(Expr::new(
                        ExprKind::Binary(
                            BinOp::Lt,
                            Box::new(cnt_ident.clone()),
                            Box::new(end_w.clone()),
                        ),
                        span,
                    )),
                ),
                span,
            );
            // Outer exit: inner_exit && cnt >= end → NEXT (sentinel)
            let outer_exit_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(inner_exit_for_exit),
                    Box::new(Expr::new(
                        ExprKind::Binary(
                            BinOp::Gte,
                            Box::new(cnt_ident.clone()),
                            Box::new(end_w.clone()),
                        ),
                        span,
                    )),
                ),
                span,
            );
            new_trans.push((outer_loop_cond, loop_back_target));
            new_trans.push((outer_exit_cond, usize::MAX));
            last.multi_transitions = new_trans;
        } else if let Some(body_cond) = last.transition_cond.take() {
            // Last body state had a transition_cond (e.g. do..until).
            // Replace with multi_transitions: loop-back and exit, both
            // guarded by the original body condition AND counter check.
            let body_cond_clone = body_cond.clone();
            let loop_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(body_cond.clone()),
                    Box::new(Expr::new(
                        ExprKind::Binary(
                            BinOp::Lt,
                            Box::new(cnt_ident.clone()),
                            Box::new(end_w.clone()),
                        ),
                        span,
                    )),
                ),
                span,
            );
            let exit_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(body_cond),
                    Box::new(Expr::new(
                        ExprKind::Binary(
                            BinOp::Gte,
                            Box::new(cnt_ident.clone()),
                            Box::new(end_w.clone()),
                        ),
                        span,
                    )),
                ),
                span,
            );

            // Counter increment guarded by the body condition —
            // only increment when a beat is actually accepted
            last.seq_stmts.push(Stmt::IfElse(IfElse {
                cond: body_cond_clone,
                then_stmts: vec![cnt_inc.clone()],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));

            last.multi_transitions = vec![
                (loop_cond, loop_back_target),
                (exit_cond, usize::MAX), // sentinel: next state after for group
            ];
        } else {
            // Last body state has no condition (unconditional advance) —
            // just add counter check as multi_transitions.
            let loop_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Lt,
                    Box::new(cnt_ident.clone()),
                    Box::new(end_w.clone()),
                ),
                span,
            );
            let exit_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Gte,
                    Box::new(cnt_ident.clone()),
                    Box::new(end_w.clone()),
                ),
                span,
            );
            last.seq_stmts.push(cnt_inc.clone());
            last.multi_transitions = vec![(loop_cond, loop_back_target), (exit_cond, usize::MAX)];
        }
    }

    // Issue #422: the body's last statement might be an if/else (or any
    // multi-arm dispatch) where each arm independently falls off the end of
    // the for body. The transformation above patches only `result.last_mut()`
    // (one terminal arm); other arms have their own "off-the-end" transitions
    // sitting in non-last states. Without patching them too, they jump
    // unconditionally past the for group on every iteration, skipping the
    // loop-continuation cascade (counter increment / loop-back / exit). Apply
    // the same cascade to every such state so all terminal arms participate.
    //
    // An "off-the-end" transition is one with `target >= result_len` and not
    // a special sentinel (THREAD_TARGET_NEXT, return). Skip the last state
    // (already handled above).
    let n = result.len();
    if n >= 2 {
        for si in 0..n - 1 {
            let prev = std::mem::take(&mut result[si].multi_transitions);
            if prev.is_empty() {
                continue;
            }
            let mut new_trans: Vec<(Expr, usize)> = Vec::with_capacity(prev.len() + 1);
            let mut off_end_conds: Vec<Expr> = Vec::new();
            for (cond, target) in prev {
                if target >= result_len && !thread_target_is_special(target) {
                    off_end_conds.push(cond);
                } else {
                    new_trans.push((cond, target));
                }
            }
            if off_end_conds.is_empty() {
                result[si].multi_transitions = new_trans;
                continue;
            }
            let off_end = if off_end_conds.len() == 1 {
                off_end_conds.pop().unwrap()
            } else {
                let mut acc = off_end_conds.remove(0);
                for c in off_end_conds {
                    acc = Expr::new(
                        ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(c)),
                        span,
                    );
                }
                acc
            };
            let off_end_for_inc = off_end.clone();
            let off_end_for_loop = off_end.clone();
            let off_end_for_exit = off_end;
            // Counter increment guarded by the off-the-end condition.
            result[si].seq_stmts.push(Stmt::IfElse(IfElse {
                cond: off_end_for_inc,
                then_stmts: vec![cnt_inc.clone()],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
            // Replace off-the-end transitions with loop-back + exit cascade.
            new_trans.push((
                Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(off_end_for_loop),
                        Box::new(Expr::new(
                            ExprKind::Binary(
                                BinOp::Lt,
                                Box::new(cnt_ident.clone()),
                                Box::new(end_w.clone()),
                            ),
                            span,
                        )),
                    ),
                    span,
                ),
                loop_back_target,
            ));
            new_trans.push((
                Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(off_end_for_exit),
                        Box::new(Expr::new(
                            ExprKind::Binary(
                                BinOp::Gte,
                                Box::new(cnt_ident.clone()),
                                Box::new(end_w.clone()),
                            ),
                            span,
                        )),
                    ),
                    span,
                ),
                usize::MAX,
            ));
            result[si].multi_transitions = new_trans;
        }
    }

    Ok(result)
}

/// Lower a `lock` block into FSM states.
///
/// Zero-cycle lock: if grant is free, the first body state executes immediately.
/// The req signal is asserted in all lock states; grant is ANDed into the
/// first body state's transition condition so it blocks only if contended.
fn lower_thread_lock(
    resource_name: &str,
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
    loop_id_gen: &mut u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    let req_signal = format!("_{}_req", resource_name);
    let grant_signal = format!("_{}_grant", resource_name);

    let make_grant = || Expr::new(ExprKind::Ident(grant_signal.clone()), span);
    let req_assign = Stmt::Assign(CombAssign {
        target: Expr::new(ExprKind::Ident(req_signal.clone()), span),
        value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), span),
        span,
    });

    let mut body_states = partition_thread_body_with_loop_ids(body, span, cnt_width, loop_id_gen)?;

    // Add req=1 to all body states, and tag them as lock-guarded so the comb
    // overlap optimization never drives their outputs from a preceding state
    // (issue #501).
    for bs in &mut body_states {
        bs.comb_stmts.insert(0, req_assign.clone());
        bs.is_lock_body = true;
    }

    // First body state: gate comb outputs AND transition on grant.
    // Without grant gating, all contending threads would drive outputs simultaneously.
    if let Some(first) = body_states.first_mut() {
        // Wrap ALL comb outputs (except req) in `if (grant) { ... }`
        let non_req_comb: Vec<Stmt> = first
            .comb_stmts
            .iter()
            .filter(|s| {
                if let Stmt::Assign(a) = s {
                    if let ExprKind::Ident(ref n) = a.target.kind {
                        return *n != req_signal;
                    }
                }
                true
            })
            .cloned()
            .collect();
        // Keep only req assign at top level
        first.comb_stmts.retain(|s| {
            if let Stmt::Assign(a) = s {
                if let ExprKind::Ident(ref n) = a.target.kind {
                    return *n == req_signal;
                }
            }
            false
        });
        // Add grant-gated outputs
        if !non_req_comb.is_empty() {
            first.comb_stmts.push(Stmt::IfElse(IfElse {
                cond: make_grant(),
                then_stmts: non_req_comb,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }

        // AND grant into transition condition
        if let Some(ref existing_cond) = first.transition_cond {
            first.transition_cond = Some(Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(make_grant()),
                    Box::new(existing_cond.clone()),
                ),
                span,
            ));
        } else if first.wait_cycles.is_none() && first.multi_transitions.is_empty() {
            first.transition_cond = Some(make_grant());
        }

        // Gate seq assigns in first state by grant.
        // Seq assigns merged from trailing statements (e.g. xfers_issued_r++) use
        // the pre-grant transition_cond as their guard, but without grant gating
        // they would fire even when this thread hasn't won the arbitration.
        // Wrap all first-state seq stmts in `if (grant) { ... }`.
        let first_seq = std::mem::take(&mut first.seq_stmts);
        if !first_seq.is_empty() {
            first.seq_stmts.push(Stmt::IfElse(IfElse {
                cond: make_grant(),
                then_stmts: first_seq,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }

    // Mark the last body state so merged-module generation can emit the
    // combinational release pulse when its exit transition fires (see the
    // `lock_release` field doc).
    let release_info = body_states.last().map(|last| {
        if last.multi_transitions.is_empty() {
            // No multi-transition was created inside the lock body. If an
            // enclosing construct (for example, a loop around this lock)
            // later expands this state into multiple arms, every arm exits
            // the lock body and must release the arbiter hold.
            LockReleaseInfo::AllTransitions
        } else {
            // Multi-transitions already present here were created inside the
            // lock body. Only arms that leave this local body-state vector
            // release the lock; internal loop-back arms keep the hold latch.
            let exit_conditions = last
                .multi_transitions
                .iter()
                .filter(|(_, target)| {
                    thread_target_is_special(*target) || *target >= body_states.len()
                })
                .map(|(cond, _)| cond.clone())
                .collect();
            LockReleaseInfo::ExitConditions(exit_conditions)
        }
    });
    if let Some(last) = body_states.last_mut() {
        last.lock_release_info = release_info;
        last.lock_release = Some(resource_name.to_string());
    }

    // If body is empty (shouldn't happen), add a grant-wait state
    if body_states.is_empty() {
        body_states.push(ThreadFsmState {
            comb_stmts: vec![req_assign],
            seq_stmts: Vec::new(),
            transition_cond: Some(make_grant()),
            wait_cycles: None,
            multi_transitions: Vec::new(),
            terminal_return: None,
            folded_exit_seq: Vec::new(),
            folded_exit_target: None,
            is_folded: false,
            no_fold_into_prev: false,
            lock_release: Some(resource_name.to_string()),
            lock_release_info: Some(LockReleaseInfo::AllTransitions),
            is_lock_body: true,
        });
    }

    Ok(body_states)
}

/// Collect resource names used in `lock` blocks within a thread body.
fn collect_locked_resources(stmts: &[ThreadStmt]) -> HashSet<String> {
    let mut resources = HashSet::new();
    for s in stmts {
        match s {
            ThreadStmt::Lock { resource, body, .. } => {
                resources.insert(resource.name.clone());
                resources.extend(collect_locked_resources(body));
            }
            ThreadStmt::IfElse(ie) => {
                resources.extend(collect_locked_resources(&ie.then_stmts));
                resources.extend(collect_locked_resources(&ie.else_stmts));
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for br in branches {
                    resources.extend(collect_locked_resources(br));
                }
            }
            ThreadStmt::For { body, .. } | ThreadStmt::DoUntil { body, .. } => {
                resources.extend(collect_locked_resources(body));
            }
            _ => {}
        }
    }
    resources
}

/// Rewrite loop variable references in a ThreadStmt tree.
pub(crate) fn rewrite_loop_var(stmt: &ThreadStmt, var: &str, replacement: &str) -> ThreadStmt {
    match stmt {
        ThreadStmt::CombAssign(ca) => ThreadStmt::CombAssign(CombAssign {
            target: rewrite_var_expr(ca.target.clone(), var, replacement),
            value: rewrite_var_expr(ca.value.clone(), var, replacement),
            span: ca.span,
        }),
        ThreadStmt::SeqAssign(ra) => ThreadStmt::SeqAssign(RegAssign {
            target: rewrite_var_expr(ra.target.clone(), var, replacement),
            value: rewrite_var_expr(ra.value.clone(), var, replacement),
            span: ra.span,
        }),
        ThreadStmt::ForkTlmAssign(ra) => ThreadStmt::ForkTlmAssign(RegAssign {
            target: rewrite_var_expr(ra.target.clone(), var, replacement),
            value: rewrite_var_expr(ra.value.clone(), var, replacement),
            span: ra.span,
        }),
        ThreadStmt::JoinAll(sp) => ThreadStmt::JoinAll(*sp),
        ThreadStmt::WaitUntil(cond, sp) => {
            ThreadStmt::WaitUntil(rewrite_var_expr(cond.clone(), var, replacement), *sp)
        }
        ThreadStmt::WaitCycles(n, sp) => {
            ThreadStmt::WaitCycles(rewrite_var_expr(n.clone(), var, replacement), *sp)
        }
        ThreadStmt::IfElse(ie) => ThreadStmt::IfElse(ThreadIfElse {
            cond: rewrite_var_expr(ie.cond.clone(), var, replacement),
            then_stmts: ie
                .then_stmts
                .iter()
                .map(|s| rewrite_loop_var(s, var, replacement))
                .collect(),
            else_stmts: ie
                .else_stmts
                .iter()
                .map(|s| rewrite_loop_var(s, var, replacement))
                .collect(),
            unique: ie.unique,
            span: ie.span,
        }),
        ThreadStmt::ForkJoin(branches, sp) => ThreadStmt::ForkJoin(
            branches
                .iter()
                .map(|br| {
                    br.iter()
                        .map(|s| rewrite_loop_var(s, var, replacement))
                        .collect()
                })
                .collect(),
            *sp,
        ),
        ThreadStmt::For {
            var: fv,
            start,
            end,
            body,
            span,
        } => ThreadStmt::For {
            var: fv.clone(),
            start: rewrite_var_expr(start.clone(), var, replacement),
            end: rewrite_var_expr(end.clone(), var, replacement),
            body: body
                .iter()
                .map(|s| rewrite_loop_var(s, var, replacement))
                .collect(),
            span: *span,
        },
        ThreadStmt::Lock {
            resource,
            body,
            span,
        } => ThreadStmt::Lock {
            resource: resource.clone(),
            body: body
                .iter()
                .map(|s| rewrite_loop_var(s, var, replacement))
                .collect(),
            span: *span,
        },
        ThreadStmt::DoUntil { body, cond, span } => ThreadStmt::DoUntil {
            body: body
                .iter()
                .map(|s| rewrite_loop_var(s, var, replacement))
                .collect(),
            cond: rewrite_var_expr(cond.clone(), var, replacement),
            span: *span,
        },
        ThreadStmt::Log(l) => ThreadStmt::Log(l.clone()),
        ThreadStmt::Return(e, span) => {
            ThreadStmt::Return(rewrite_var_expr(e.clone(), var, replacement), *span)
        }
    }
}

/// Replace ident `var` with `replacement` in an expression tree.
pub(crate) fn rewrite_var_expr(expr: Expr, var: &str, replacement: &str) -> Expr {
    // Recurse into every container variant — for-loop iteration vars can
    // appear inside Concat / BitSlice / function call args / method receiver
    // / field access / part-select indices / etc. Missing one of these
    // shapes silently leaves the iter-var ident in the lowered FSM body,
    // and SV emission then references an undefined `i`.
    let new_kind = match &expr.kind {
        ExprKind::Ident(name) if name == var => ExprKind::Ident(replacement.to_string()),
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(rewrite_var_expr(*l.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*r.clone(), var, replacement)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(
            *op,
            Box::new(rewrite_var_expr(*e.clone(), var, replacement)),
        ),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(rewrite_var_expr(*base.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*idx.clone(), var, replacement)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(rewrite_var_expr(*base.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*hi.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*lo.clone(), var, replacement)),
        ),
        ExprKind::PartSelect(base, start, width, up) => ExprKind::PartSelect(
            Box::new(rewrite_var_expr(*base.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*start.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*width.clone(), var, replacement)),
            *up,
        ),
        ExprKind::FieldAccess(base, f) => ExprKind::FieldAccess(
            Box::new(rewrite_var_expr(*base.clone(), var, replacement)),
            f.clone(),
        ),
        ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
            Box::new(rewrite_var_expr(*c.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*t.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*f.clone(), var, replacement)),
        ),
        ExprKind::Concat(parts) => ExprKind::Concat(
            parts
                .iter()
                .map(|p| rewrite_var_expr(p.clone(), var, replacement))
                .collect(),
        ),
        ExprKind::Repeat(count, inner) => ExprKind::Repeat(
            Box::new(rewrite_var_expr(*count.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*inner.clone(), var, replacement)),
        ),
        ExprKind::MethodCall(recv, name, args) => ExprKind::MethodCall(
            Box::new(rewrite_var_expr(*recv.clone(), var, replacement)),
            name.clone(),
            args.iter()
                .map(|a| rewrite_var_expr(a.clone(), var, replacement))
                .collect(),
        ),
        ExprKind::FunctionCall(name, args) => ExprKind::FunctionCall(
            name.clone(),
            args.iter()
                .map(|a| rewrite_var_expr(a.clone(), var, replacement))
                .collect(),
        ),
        ExprKind::Signed(inner) => {
            ExprKind::Signed(Box::new(rewrite_var_expr(*inner.clone(), var, replacement)))
        }
        ExprKind::Unsigned(inner) => {
            ExprKind::Unsigned(Box::new(rewrite_var_expr(*inner.clone(), var, replacement)))
        }
        // Leaf nodes / non-substitutable forms: Ident-not-matching, Literal,
        // Bool, EnumVariant, Todo, etc. Fall through unchanged.
        _ => return expr,
    };
    Expr {
        kind: new_kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

/// Convert a ThreadIfElse (no waits) into FSM comb and seq statements.
fn thread_if_to_fsm_stmts(ie: &ThreadIfElse) -> (Option<Stmt>, Option<Stmt>) {
    let mut then_comb = Vec::new();
    let mut then_seq = Vec::new();
    let mut else_comb = Vec::new();
    let mut else_seq = Vec::new();

    fn partition_stmts(stmts: &[ThreadStmt], comb: &mut Vec<Stmt>, seq: &mut Vec<Stmt>) {
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(ca) => comb.push(Stmt::Assign(ca.clone())),
                ThreadStmt::SeqAssign(ra) => seq.push(Stmt::Assign(ra.clone())),
                ThreadStmt::ForkTlmAssign(ra) => seq.push(Stmt::Assign(ra.clone())),
                ThreadStmt::Log(l) => seq.push(Stmt::Log(l.clone())),
                ThreadStmt::IfElse(nested) => {
                    let (c, s) = thread_if_to_fsm_stmts(nested);
                    if let Some(c) = c {
                        comb.push(c);
                    }
                    if let Some(s) = s {
                        seq.push(s);
                    }
                }
                _ => {} // wait already excluded by contains_wait check
            }
        }
    }

    partition_stmts(&ie.then_stmts, &mut then_comb, &mut then_seq);
    partition_stmts(&ie.else_stmts, &mut else_comb, &mut else_seq);

    let comb_if = if !then_comb.is_empty() || !else_comb.is_empty() {
        Some(Stmt::IfElse(IfElse {
            cond: ie.cond.clone(),
            then_stmts: then_comb,
            else_stmts: else_comb,
            unique: false,
            span: ie.span,
        }))
    } else {
        None
    };

    let seq_if = if !then_seq.is_empty() || !else_seq.is_empty() {
        Some(Stmt::IfElse(IfElse {
            cond: ie.cond.clone(),
            then_stmts: then_seq,
            else_stmts: else_seq,
            unique: false,
            span: ie.span,
        }))
    } else {
        None
    };

    (comb_if, seq_if)
}

/// Rewrite seq stmts: if a seq assign targets a shared(or) signal, convert it
/// to a comb assign targeting the per-thread shadow wire `_sig_in_ti`.
/// Returns the remaining (non-shared) seq stmts; appends converted comb stmts to `out_comb`.
fn rewrite_shared_or_seq_stmts(
    stmts: &[Stmt],
    shared_or_seq: &HashSet<String>,
    thread_idx: usize,
    sp: Span,
    out_comb: &mut Vec<Stmt>,
) -> Vec<Stmt> {
    let mut kept = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Assign(ra) => {
                if let Some(name) = expr_root_name(&ra.target) {
                    if shared_or_seq.contains(&name) {
                        let shadow = format!("_{}_in_{}", name, thread_idx);
                        out_comb.push(Stmt::Assign(CombAssign {
                            target: Expr::new(ExprKind::Ident(shadow), sp),
                            value: ra.value.clone(),
                            span: ra.span,
                        }));
                        continue;
                    }
                }
                kept.push(stmt.clone());
            }
            Stmt::IfElse(ie) => {
                let mut then_comb = Vec::new();
                let mut else_comb = Vec::new();
                let then_seq = rewrite_shared_or_seq_stmts(
                    &ie.then_stmts,
                    shared_or_seq,
                    thread_idx,
                    sp,
                    &mut then_comb,
                );
                let else_seq = rewrite_shared_or_seq_stmts(
                    &ie.else_stmts,
                    shared_or_seq,
                    thread_idx,
                    sp,
                    &mut else_comb,
                );
                // Push rewritten comb assigns under the same if guard
                if !then_comb.is_empty() || !else_comb.is_empty() {
                    out_comb.push(Stmt::IfElse(IfElse {
                        cond: ie.cond.clone(),
                        then_stmts: then_comb,
                        else_stmts: else_comb,
                        unique: ie.unique,
                        span: ie.span,
                    }));
                }
                if !then_seq.is_empty() || !else_seq.is_empty() {
                    kept.push(Stmt::IfElse(IfElse {
                        cond: ie.cond.clone(),
                        then_stmts: then_seq,
                        else_stmts: else_seq,
                        unique: ie.unique,
                        span: ie.span,
                    }));
                }
            }
            _ => kept.push(stmt.clone()),
        }
    }
    kept
}

/// Transform comb assigns for shared(or) signals: `sig = val` → `sig = sig | val`.
/// This ensures multiple threads OR-accumulate rather than last-writer-wins.
fn transform_shared_or_assigns(
    stmts: &[Stmt],
    shared: &HashMap<String, SharedReduction>,
    sp: Span,
) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|stmt| {
            match stmt {
                Stmt::Assign(a) => {
                    let target_name = match &a.target.kind {
                        ExprKind::Ident(n) => Some(n.clone()),
                        _ => None,
                    };
                    if let Some(ref name) = target_name {
                        if let Some(reduction) = shared.get(name) {
                            let fold_op = match reduction {
                                SharedReduction::Or => BinOp::BitOr,
                                SharedReduction::And => BinOp::BitAnd,
                            };
                            // sig = sig <op> val
                            return Stmt::Assign(CombAssign {
                                target: a.target.clone(),
                                value: Expr::new(
                                    ExprKind::Binary(
                                        fold_op,
                                        Box::new(Expr::new(ExprKind::Ident(name.clone()), sp)),
                                        Box::new(a.value.clone()),
                                    ),
                                    sp,
                                ),
                                span: a.span,
                            });
                        }
                    }
                    stmt.clone()
                }
                Stmt::IfElse(ie) => Stmt::IfElse(IfElse {
                    cond: ie.cond.clone(),
                    then_stmts: transform_shared_or_assigns(&ie.then_stmts, shared, sp),
                    else_stmts: transform_shared_or_assigns(&ie.else_stmts, shared, sp),
                    unique: ie.unique,
                    span: ie.span,
                }),
                _ => stmt.clone(),
            }
        })
        .collect()
}

/// Rename an identifier in an expression tree.
pub(crate) fn rename_ident_in_expr(expr: &mut Expr, old: &str, new: &str) {
    // Must recurse into every container variant that can hold sub-expressions
    // — counter renames (_loop_cnt → _t{N}_loop_cnt) walk the whole expression
    // tree, and missing a container leaves a bare `_loop_cnt` ident in the
    // lowered SV that references no real variable.
    match &mut expr.kind {
        ExprKind::Ident(ref mut name) if name == old => {
            *name = new.to_string();
        }
        ExprKind::Binary(_, l, r) => {
            rename_ident_in_expr(l, old, new);
            rename_ident_in_expr(r, old, new);
        }
        ExprKind::Unary(_, e) => rename_ident_in_expr(e, old, new),
        ExprKind::Index(b, i) => {
            rename_ident_in_expr(b, old, new);
            rename_ident_in_expr(i, old, new);
        }
        ExprKind::BitSlice(b, h, l) => {
            rename_ident_in_expr(b, old, new);
            rename_ident_in_expr(h, old, new);
            rename_ident_in_expr(l, old, new);
        }
        ExprKind::PartSelect(b, s, w, _) => {
            rename_ident_in_expr(b, old, new);
            rename_ident_in_expr(s, old, new);
            rename_ident_in_expr(w, old, new);
        }
        ExprKind::FieldAccess(b, _) => rename_ident_in_expr(b, old, new),
        ExprKind::MethodCall(recv, _, args) => {
            rename_ident_in_expr(recv, old, new);
            for a in args {
                rename_ident_in_expr(a, old, new);
            }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                rename_ident_in_expr(a, old, new);
            }
        }
        ExprKind::Ternary(c, t, f) => {
            rename_ident_in_expr(c, old, new);
            rename_ident_in_expr(t, old, new);
            rename_ident_in_expr(f, old, new);
        }
        ExprKind::Cast(e, _) => rename_ident_in_expr(e, old, new),
        ExprKind::Concat(parts) => {
            for p in parts {
                rename_ident_in_expr(p, old, new);
            }
        }
        ExprKind::Repeat(c, e) => {
            rename_ident_in_expr(c, old, new);
            rename_ident_in_expr(e, old, new);
        }
        ExprKind::Signed(e) | ExprKind::Unsigned(e) | ExprKind::Clog2(e) | ExprKind::Onehot(e) => {
            rename_ident_in_expr(e, old, new);
        }
        _ => {}
    }
}

pub(crate) fn rename_ident_in_stmts(stmts: &mut [Stmt], old: &str, new: &str) {
    for s in stmts {
        match s {
            Stmt::Assign(ra) => {
                rename_ident_in_expr(&mut ra.target, old, new);
                rename_ident_in_expr(&mut ra.value, old, new);
            }
            Stmt::IfElse(ie) => {
                rename_ident_in_expr(&mut ie.cond, old, new);
                rename_ident_in_stmts(&mut ie.then_stmts, old, new);
                rename_ident_in_stmts(&mut ie.else_stmts, old, new);
            }
            _ => {}
        }
    }
}

pub(crate) fn rename_ident_in_comb_stmts(stmts: &mut [Stmt], old: &str, new: &str) {
    for s in stmts {
        match s {
            Stmt::Assign(ca) => {
                rename_ident_in_expr(&mut ca.target, old, new);
                rename_ident_in_expr(&mut ca.value, old, new);
            }
            Stmt::IfElse(ie) => {
                rename_ident_in_expr(&mut ie.cond, old, new);
                rename_ident_in_comb_stmts(&mut ie.then_stmts, old, new);
                rename_ident_in_comb_stmts(&mut ie.else_stmts, old, new);
            }
            _ => {}
        }
    }
}

fn make_zero_expr(sp: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)
}

/// All-ones expression at the target signal's width — the identity element
/// for AND-reduction (`shared(and)`). Bitwise-complement of zero: widened
/// (zero-extended) to the target width by context, then inverted, so it
/// works for both 1-bit `Bool` and wider `logic[N:0]` shared signals.
fn make_ones_expr(sp: Span) -> Expr {
    Expr::new(
        ExprKind::Unary(UnaryOp::BitNot, Box::new(make_zero_expr(sp))),
        sp,
    )
}
