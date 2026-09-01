//! TLM (`tlm_method`) lowering — extracted from `elaborate.rs` / `elaborate/mod.rs`
//! (P4 phase 2c, move-only). This module owns the entire `tlm_method` lowering
//! family: bus-initiator/target `tlm_connect` sugar rewriting
//! (`lower_tlm_connects`), `thread port.method(...)` target-body lowering
//! (`lower_tlm_target_threads`), and the `d <= m.read(addr);` initiator-call
//! cohort/request-arbiter/response-router/tag-lane synthesis
//! (`lower_tlm_initiator_calls`, its ~4500-line helper cluster). Per
//! `src/main.rs`'s pipeline order, TLM lowering runs *before* generic
//! `lower_threads` (`elaborate::threads`, P4 phase 2b) and the two are
//! structurally disjoint passes — a module's threads are either TLM-bound
//! (routed through the passes here, which strip them before generic
//! `lower_threads` ever sees them) or plain (routed through
//! `elaborate::threads`) — so this was a clean seam, not an arbitrary cut.
//!
//! ## 2b/2c boundary: what stayed in `elaborate::mod`
//!
//! **Shared bus/const-eval infrastructure, used by *three* lowering
//! families** — `lower_tlm_connects` and TLM target/initiator lowering
//! (both in this module) plus `elaborate::threads` (generic thread
//! lowering) — stays in `elaborate::mod` rather than moving here:
//! `build_module_type_map`, `build_module_type_map_with_buses`,
//! `bus_effective_signals`, `tlm_effective_methods_for_bus`,
//! `tlm_method_effective_signals`, `gen_if_cond_truthy`,
//! `eval_const_expr_from_param_map_for_lower`, `eval_const_expr_for_lower`,
//! `subst_type_expr_for_lower`, and the `SignalInfo` type they traffic in.
//! `thread_stmt_span` joins this shared cluster for the same reason:
//! `elaborate::threads`'s `disallow_nested_control_in_do_until` calls it
//! directly (confirmed by grep — not just a doc-comment mention), so it
//! moved from its old file position (originally mid-file inside this
//! module's old range) to sit next to the rest of the shared cluster in
//! `elaborate::mod`, rather than being duplicated or bounced across two
//! module boundaries. This module reaches all of the above (plus
//! `try_eval_i64`/`try_eval_bool`/generate-expansion/`ParentShapeInfo`, per
//! the phase-2a precedent) via `use super::*;` below — the same
//! descendant-sees-ancestor privacy rule `elaborate::params` and
//! `elaborate::threads` both rely on.
//!
//! `check_fork_join_uniform_tlm_class` (rejects a `fork...join` TLM issue
//! group that mixes `blocking` and `out_of_order tags N` calls, PR #761)
//! moved here in this phase — it operates on `DirectTlmThread`/
//! `TlmMethodMeta` (TLM call-class metadata) with its only caller inside
//! `lower_tlm_initiator_calls`, so it belongs with the rest of the TLM
//! family now that TLM lowering has its own module.
//!
//! `fold_literal_bit_slices_thread_stmt`/`fold_literal_bit_slices_expr` also
//! moved here despite sitting, in the old file, in the middle of
//! `elaborate::mod`'s generate/`subst_*`-for-loop-unrolling family: their
//! only external caller (verified by grep) is `build_tlm_init_thread_plan`
//! in this module, not `expand_generate_for` or any other generate-expansion
//! function — a case where old file order didn't match actual ownership.
//!
//! Two unrelated constructs sit immediately after this module's old
//! location in file order and were never part of this family — `pipe_reg<T,
//! N>` port lowering (`lower_pipe_reg_ports`) and `credit_channel`
//! method dispatch (`lower_credit_channel_dispatch`) — both stay in
//! `elaborate::mod`, per the phase-2b precedent's note about the same two
//! constructs.
//!
//! ## Visibility
//!
//! `lower_tlm_target_threads` and `lower_tlm_initiator_calls` were already
//! `pub fn` (external callers: `main.rs`, `tests/integration_test.rs`,
//! `tests/param_where_constraints.rs`) and are re-exported via `pub use
//! tlm::{...};` in `mod.rs` so every existing
//! `crate::elaborate::lower_tlm_*` call site keeps resolving unchanged.
//! `lower_tlm_connects` has no caller outside `elaborate::mod`'s `elaborate()`
//! orchestrator, so it was bumped `fn` → `pub(super) fn` (not `pub(crate)`,
//! per minimal-visibility) and re-imported there via a plain `use
//! tlm::lower_tlm_connects;` so that bare call site is untouched too.
//! Every other item in this module (the ~4500-line initiator-call helper
//! cluster, the TLM connect-sugar helpers, `TlmCall`/`DirectTlmThread`/
//! `TlmInitThreadPlan`/etc.) has zero callers outside this module and stays
//! private.

use super::threads::{
    contains_return, infer_for_cnt_width, partition_tlm_target_thread_body_with_loop_ids,
    rename_ident_in_comb_stmts, rename_ident_in_expr, rename_ident_in_stmts, rewrite_loop_var,
    rewrite_var_expr, synthesize_lock_arbiter, thread_block_always_returns,
    thread_target_return_idx,
};
use super::*;

// ── TLM/bus connect sugar ───────────────────────────────────────────────────

pub(super) fn lower_tlm_connects(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    let module_defs: HashMap<String, (Vec<ParamDecl>, Vec<PortDecl>)> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Module(m) => Some((m.name.name.clone(), (m.params.clone(), m.ports.clone()))),
            Item::Fsm(f) => Some((f.name.name.clone(), (f.params.clone(), f.ports.clone()))),
            Item::Pipeline(p) => Some((p.name.name.clone(), (p.params.clone(), p.ports.clone()))),
            _ => None,
        })
        .collect();
    let bus_defs: HashMap<String, BusDecl> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Bus(b) => Some((b.name.name.clone(), b.clone())),
            _ => None,
        })
        .collect();
    let struct_defs: HashMap<String, StructDecl> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(s) => Some((s.name.name.clone(), s.clone())),
            _ => None,
        })
        .collect();

    let mut errors = Vec::new();
    let mut items = Vec::new();
    for item in ast.items {
        match item {
            Item::Module(m) => {
                match lower_tlm_connects_in_module(m, &module_defs, &bus_defs, &struct_defs) {
                    Ok(m) => items.push(Item::Module(m)),
                    Err(mut errs) => errors.append(&mut errs),
                }
            }
            other => items.push(other),
        }
    }

    if errors.is_empty() {
        Ok(SourceFile {
            items,
            inner_doc: ast.inner_doc,
            frontmatter: ast.frontmatter,
        })
    } else {
        Err(errors)
    }
}

fn lower_tlm_connects_in_module(
    mut m: ModuleDecl,
    module_defs: &HashMap<String, (Vec<ParamDecl>, Vec<PortDecl>)>,
    bus_defs: &HashMap<String, BusDecl>,
    struct_defs: &HashMap<String, StructDecl>,
) -> Result<ModuleDecl, Vec<CompileError>> {
    let connects: Vec<TlmConnectDecl> = m
        .body
        .iter()
        .filter_map(|item| match item {
            ModuleBodyItem::TlmConnect(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    if connects.is_empty() {
        return Ok(m);
    }

    let mut errors = Vec::new();
    let mut inst_modules: HashMap<String, String> = HashMap::new();
    let mut inst_params: HashMap<String, Vec<ParamAssign>> = HashMap::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for item in &m.body {
        match item {
            ModuleBodyItem::Inst(inst) => {
                inst_modules.insert(inst.name.name.clone(), inst.module_name.name.clone());
                inst_params.insert(inst.name.name.clone(), inst.param_assigns.clone());
                used_names.insert(inst.name.name.clone());
                for c in &inst.connections {
                    if c.port_name.name.starts_with("_tlm_conn_") {
                        used_names.insert(c.port_name.name.clone());
                    }
                }
            }
            ModuleBodyItem::RegDecl(r) => {
                used_names.insert(r.name.name.clone());
            }
            ModuleBodyItem::WireDecl(w) => {
                used_names.insert(w.name.name.clone());
            }
            ModuleBodyItem::LetBinding(l) => {
                used_names.insert(l.name.name.clone());
            }
            ModuleBodyItem::PipeRegDecl(p) => {
                used_names.insert(p.name.name.clone());
            }
            _ => {}
        }
    }
    for port in &m.ports {
        used_names.insert(port.name.name.clone());
    }

    let mut synthesized_wires = Vec::new();
    let mut synthesized_logic = Vec::new();
    let mut synthesized_conns: HashMap<String, Vec<Connection>> = HashMap::new();
    let mut connected_endpoints: HashMap<(String, String), Span> = HashMap::new();
    let connects = group_tlm_connects_by_initiator(connects, &inst_params, &mut errors);
    for conn in connects {
        let from = match tlm_connect_endpoint_bus(
            &conn.from_inst,
            &conn.from_port,
            &inst_modules,
            module_defs,
            &inst_params,
            bus_defs,
        ) {
            Ok(endpoint) => endpoint,
            Err(err) => {
                errors.push(err);
                continue;
            }
        };

        let mut target_infos = Vec::new();
        for target in &conn.targets {
            match tlm_connect_endpoint_bus(
                &target.to_inst,
                &target.to_port,
                &inst_modules,
                module_defs,
                &inst_params,
                bus_defs,
            ) {
                Ok(endpoint) => target_infos.push((target, endpoint)),
                Err(err) => {
                    errors.push(err);
                }
            }
        }
        if target_infos.len() != conn.targets.len() {
            continue;
        }

        let error_count_before_endpoint_checks = errors.len();
        let mut endpoints = vec![(&conn.from_inst, &conn.from_port)];
        for target in &conn.targets {
            endpoints.push((&target.to_inst, &target.to_port));
        }
        for endpoint in endpoints {
            let key = (endpoint.0.name.clone(), endpoint.1.name.clone());
            if let Some(first_span) = connected_endpoints.get(&key) {
                errors.push(CompileError::general(
                    &format!(
                        "TLM connect endpoint `{}.{}` is connected more than once",
                        endpoint.0.name, endpoint.1.name
                    ),
                    first_span.merge(conn.span),
                ));
            } else {
                connected_endpoints.insert(key, conn.span);
            }
        }
        if errors.len() != error_count_before_endpoint_checks {
            continue;
        }
        let mut bus_mismatch = false;
        for (target, to) in &target_infos {
            if from.bus_name != to.bus_name {
                errors.push(CompileError::general(
                    &format!(
                        "TLM connect bus mismatch: `{}.{}` is `{from_bus}`, but `{}.{}` is `{to_bus}`",
                        conn.from_inst.name,
                        conn.from_port.name,
                        target.to_inst.name,
                        target.to_port.name,
                        from_bus = from.bus_name,
                        to_bus = to.bus_name
                    ),
                    conn.span,
                ));
                bus_mismatch = true;
            }
        }
        if bus_mismatch {
            continue;
        }
        let mut bus_shape_mismatch = false;
        let decoded_connect = conn.decode_field.is_some();
        let bus_decl = bus_defs.get(&from.bus_name);
        for (target, to) in &target_infos {
            let compatible = if decoded_connect {
                bus_decl.map_or(false, |_| {
                    tlm_connect_decoded_shapes_compatible(&from.shape, &to.shape, &from.methods)
                })
            } else {
                from.shape == to.shape
            };
            if !compatible {
                errors.push(CompileError::general(
                    &format!(
                        "TLM connect bus-shape mismatch: `{}.{}` exposes {}, but `{}.{}` exposes {}",
                        conn.from_inst.name,
                        conn.from_port.name,
                        format_tlm_connect_shape(&from.shape),
                        target.to_inst.name,
                        target.to_port.name,
                        format_tlm_connect_shape(&to.shape)
                    ),
                    conn.span,
                ));
                bus_shape_mismatch = true;
            }
        }
        if bus_shape_mismatch {
            continue;
        }
        let mut direction_mismatch = false;
        for (target, to) in &target_infos {
            if from.perspective != BusPerspective::Initiator
                || to.perspective != BusPerspective::Target
            {
                errors.push(CompileError::general(
                    &format!(
                        "TLM connect requires `connect initiator_inst.initiator_port -> target_inst.target_port;` \
                         but `{}.{}` is {:?} and `{}.{}` is {:?}",
                        conn.from_inst.name,
                        conn.from_port.name,
                        from.perspective,
                        target.to_inst.name,
                        target.to_port.name,
                        to.perspective
                    ),
                    from.span.merge(to.span),
                ));
                direction_mismatch = true;
            }
        }
        if direction_mismatch {
            continue;
        }
        let error_count_before_duplicate_checks = errors.len();
        let mut endpoints = vec![(&conn.from_inst, &conn.from_port)];
        for target in &conn.targets {
            endpoints.push((&target.to_inst, &target.to_port));
        }
        for (inst_name, port_name) in endpoints {
            if let Some(existing) = m
                .body
                .iter()
                .find_map(|item| match item {
                    ModuleBodyItem::Inst(inst) if inst.name.name == inst_name.name => Some(inst),
                    _ => None,
                })
                .and_then(|inst| {
                    inst.connections
                        .iter()
                        .find(|c| c.port_name.name == port_name.name)
                })
            {
                errors.push(CompileError::general(
                    &format!(
                        "`connect {}.{}` duplicates an explicit connection on inst `{}` port `{}`",
                        inst_name.name, port_name.name, inst_name.name, port_name.name
                    ),
                    existing.span.merge(conn.span),
                ));
            }
        }
        if errors.len() != error_count_before_duplicate_checks {
            continue;
        }

        if conn.decode_field.is_none() {
            let target = &conn.targets[0];
            let wire_name = fresh_tlm_connect_name(
                &format!(
                    "_tlm_conn_{}_{}_{}_{}",
                    conn.from_inst.name,
                    conn.from_port.name,
                    target.to_inst.name,
                    target.to_port.name
                ),
                &mut used_names,
            );
            synthesized_wires.push(tlm_connect_bus_wire(
                &wire_name,
                &from.bus_name,
                &from.bus_params,
                conn.span,
            ));
            push_tlm_connect_connection(
                &mut synthesized_conns,
                &conn.from_inst,
                &conn.from_port,
                &wire_name,
                conn.span,
            );
            push_tlm_connect_connection(
                &mut synthesized_conns,
                &target.to_inst,
                &target.to_port,
                &wire_name,
                target.span,
            );
        } else {
            match lower_decoded_tlm_connect(
                &conn,
                &from,
                &target_infos
                    .iter()
                    .map(|(_, endpoint)| (*endpoint).clone())
                    .collect::<Vec<_>>(),
                bus_defs,
                struct_defs,
                &m,
                &mut used_names,
                &mut synthesized_conns,
            ) {
                Ok(mut lowered) => {
                    synthesized_wires.append(&mut lowered.wires);
                    synthesized_logic.append(&mut lowered.logic);
                }
                Err(mut errs) => errors.append(&mut errs),
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut lowered_body = Vec::new();
    lowered_body.extend(synthesized_wires);
    lowered_body.extend(synthesized_logic);
    for item in m.body {
        match item {
            ModuleBodyItem::Inst(mut inst) => {
                if let Some(mut conns) = synthesized_conns.remove(&inst.name.name) {
                    inst.connections.append(&mut conns);
                }
                lowered_body.push(ModuleBodyItem::Inst(inst));
            }
            ModuleBodyItem::TlmConnect(_) => {}
            other => lowered_body.push(other),
        }
    }
    m.body = lowered_body;
    Ok(m)
}

const TLM_CONNECT_SLAVE_START_PARAM: &str = "SLAVE_START_ADDR";
const TLM_CONNECT_SLAVE_END_PARAM: &str = "SLAVE_END_ADDR";
const TLM_CONNECT_DECODE_ARG: &str = "addr";

fn group_tlm_connects_by_initiator(
    connects: Vec<TlmConnectDecl>,
    inst_params: &HashMap<String, Vec<ParamAssign>>,
    errors: &mut Vec<CompileError>,
) -> Vec<TlmConnectDecl> {
    let mut groups: HashMap<(String, String), Vec<TlmConnectDecl>> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for conn in connects {
        let key = (conn.from_inst.name.clone(), conn.from_port.name.clone());
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(conn);
    }

    let mut out = Vec::new();
    for key in order {
        let Some(group) = groups.remove(&key) else {
            continue;
        };
        if group.len() == 1 || group.iter().any(|c| c.decode_field.is_some()) {
            out.extend(group);
            continue;
        }

        let mut targets = Vec::new();
        let mut unknown_target_inst = false;
        for conn in &group {
            let Some(target) = conn.targets.first() else {
                continue;
            };
            let Some(params) = inst_params.get(&target.to_inst.name) else {
                unknown_target_inst = true;
                break;
            };
            let Some(start) = tlm_connect_inst_param(params, TLM_CONNECT_SLAVE_START_PARAM) else {
                errors.push(CompileError::general(
                    &format!(
                        "one initiator TLM connect to multiple targets requires target inst `{}` to override `param {TLM_CONNECT_SLAVE_START_PARAM} = ...;`",
                        target.to_inst.name
                    ),
                    target.span,
                ));
                continue;
            };
            let Some(end) = tlm_connect_inst_param(params, TLM_CONNECT_SLAVE_END_PARAM) else {
                errors.push(CompileError::general(
                    &format!(
                        "one initiator TLM connect to multiple targets requires target inst `{}` to override `param {TLM_CONNECT_SLAVE_END_PARAM} = ...;`",
                        target.to_inst.name
                    ),
                    target.span,
                ));
                continue;
            };
            let mut target = target.clone();
            target.decode = Some(TlmConnectDecode::Range { lo: start, hi: end });
            targets.push(target);
        }
        if unknown_target_inst {
            out.extend(group);
            continue;
        }
        if targets.len() != group.len() {
            continue;
        }

        let first = &group[0];
        let span = group
            .iter()
            .fold(first.span, |acc, conn| acc.merge(conn.span));
        out.push(TlmConnectDecl {
            from_inst: first.from_inst.clone(),
            from_port: first.from_port.clone(),
            targets,
            decode_field: Some(Ident::new(TLM_CONNECT_DECODE_ARG.to_string(), first.span)),
            span,
        });
    }
    out
}

fn tlm_connect_inst_param(params: &[ParamAssign], name: &str) -> Option<Expr> {
    params
        .iter()
        .find(|p| p.name.name == name && p.ty.is_none())
        .map(|p| p.value.clone())
}

struct LoweredDecodedTlmConnect {
    wires: Vec<ModuleBodyItem>,
    logic: Vec<ModuleBodyItem>,
}

fn fresh_tlm_connect_name(base: &str, used_names: &mut HashSet<String>) -> String {
    let mut name = base.to_string();
    let mut suffix = 0usize;
    while used_names.contains(&name) {
        suffix += 1;
        name = format!("{base}_{suffix}");
    }
    used_names.insert(name.clone());
    name
}

fn tlm_connect_bus_wire(
    name: &str,
    bus_name: &str,
    bus_params: &[ParamAssign],
    span: Span,
) -> ModuleBodyItem {
    ModuleBodyItem::WireDecl(WireDecl {
        bus_params: bus_params.to_vec(),
        name: Ident::new(name.to_string(), span),
        ty: TypeExpr::Named(Ident::new(bus_name.to_string(), span)),
        unpacked: false,
        unpacked_ascending: false,
        span,
    })
}

fn push_tlm_connect_connection(
    conns: &mut HashMap<String, Vec<Connection>>,
    inst: &Ident,
    port: &Ident,
    wire_name: &str,
    span: Span,
) {
    conns
        .entry(inst.name.clone())
        .or_default()
        .push(Connection {
            port_name: port.clone(),
            direction: ConnectDir::Output,
            signal: Expr::new(ExprKind::Ident(wire_name.to_string()), span),
            reset_override: None,
            span,
        });
}

fn lower_decoded_tlm_connect(
    conn: &TlmConnectDecl,
    from: &TlmConnectEndpoint,
    target_endpoints: &[TlmConnectEndpoint],
    bus_defs: &HashMap<String, BusDecl>,
    struct_defs: &HashMap<String, StructDecl>,
    module: &ModuleDecl,
    used_names: &mut HashSet<String>,
    synthesized_conns: &mut HashMap<String, Vec<Connection>>,
) -> Result<LoweredDecodedTlmConnect, Vec<CompileError>> {
    let span = conn.span;
    let Some(decode_field) = conn.decode_field.as_ref() else {
        return Ok(LoweredDecodedTlmConnect {
            wires: Vec::new(),
            logic: Vec::new(),
        });
    };
    let bus_name = &from.bus_name;
    if !bus_defs.contains_key(bus_name) {
        return Err(vec![CompileError::general(
            &format!("one-to-many TLM connect references unknown bus `{bus_name}`"),
            span,
        )]);
    }
    if from.methods.is_empty() {
        return Err(vec![CompileError::general(
            "one-to-many TLM connect currently requires a bus with `tlm_method` declarations",
            span,
        )]);
    }

    let mut errors = Vec::new();
    let mut default_count = 0usize;
    for target in &conn.targets {
        match target.decode {
            Some(TlmConnectDecode::Default) => default_count += 1,
            Some(TlmConnectDecode::Range { .. }) => {}
            None => errors.push(CompileError::general(
                "one-to-many TLM connect target is missing an address range",
                target.span,
            )),
        }
    }
    if default_count > 1 {
        errors.push(CompileError::general(
            "one-to-many TLM connect allows at most one default target",
            span,
        ));
    }

    let (clk, rst) = match infer_single_clock_reset(module, span) {
        Ok(pair) => pair,
        Err(err) => {
            errors.push(err);
            (
                Ident::new("clk".to_string(), span),
                Ident::new("rst".to_string(), span),
            )
        }
    };

    let mut decode_width: Option<u32> = None;
    for method in &from.methods {
        if method.mode.name != "blocking" {
            errors.push(CompileError::general(
                &format!(
                    "one-to-many TLM connect currently supports only `blocking` TLM methods; `{}` is `{}`",
                    method.name.name, method.mode.name
                ),
                method.span,
            ));
            continue;
        }
        let Some((_, ty)) = method
            .args
            .iter()
            .find(|(arg, _)| arg.name == decode_field.name)
        else {
            errors.push(CompileError::general(
                &format!(
                    "one-to-many TLM connect routes on argument `{}` but method `{}` has no argument named `{}`",
                    decode_field.name, method.name.name, decode_field.name
                ),
                decode_field.span.merge(method.span),
            ));
            continue;
        };
        let Some(width) = uint_type_literal_width(ty) else {
            errors.push(CompileError::general(
                &format!(
                    "one-to-many TLM connect argument `{}` on method `{}` must be a literal-width UInt<N>",
                    decode_field.name, method.name.name
                ),
                decode_field.span.merge(method.span),
            ));
            continue;
        };
        if width > 64 {
            errors.push(CompileError::general(
                "one-to-many TLM connect ranges currently support decode widths up to 64 bits",
                decode_field.span.merge(method.span),
            ));
        }
        if let Some(prev) = decode_width {
            if prev != width {
                errors.push(CompileError::general(
                    &format!(
                        "one-to-many TLM connect argument `{}` has inconsistent widths across methods: {prev} and {width}",
                        decode_field.name
                    ),
                    decode_field.span.merge(method.span),
                ));
            }
        } else {
            decode_width = Some(width);
        }
    }
    if default_count == 0 {
        let width = decode_width.unwrap_or(0);
        if width == 0 || !tlm_connect_ranges_cover_full_space(&conn.targets, width) {
            errors.push(CompileError::general(
                "one-to-many TLM connect requires literal ranges that cover the full decode address space",
                span,
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let base = format!(
        "_tlm_conn_{}_{}_decode",
        conn.from_inst.name, conn.from_port.name
    );
    let up_wire = fresh_tlm_connect_name(&format!("{base}_up"), used_names);
    let mut target_wires = Vec::new();
    for (i, _) in conn.targets.iter().enumerate() {
        target_wires.push(fresh_tlm_connect_name(&format!("{base}_t{i}"), used_names));
    }

    let mut wires = Vec::new();
    wires.push(tlm_connect_bus_wire(
        &up_wire,
        bus_name,
        &from.bus_params,
        span,
    ));
    for (wire, endpoint) in target_wires.iter().zip(target_endpoints.iter()) {
        wires.push(tlm_connect_bus_wire(
            wire,
            bus_name,
            &endpoint.bus_params,
            span,
        ));
    }

    push_tlm_connect_connection(
        synthesized_conns,
        &conn.from_inst,
        &conn.from_port,
        &up_wire,
        span,
    );
    for (target, wire) in conn.targets.iter().zip(target_wires.iter()) {
        push_tlm_connect_connection(
            synthesized_conns,
            &target.to_inst,
            &target.to_port,
            wire,
            target.span,
        );
    }

    let route_w = clog2_width(conn.targets.len() as u64).max(1) as u32;
    let mut logic = Vec::new();
    let mut comb_stmts = Vec::new();
    let mut seq_stmts = Vec::new();

    for method in &from.methods {
        let target_supports_method: Vec<bool> = target_endpoints
            .iter()
            .map(|endpoint| tlm_connect_shape_has_method(&endpoint.shape, method))
            .collect();
        let any_missing_method = target_supports_method.iter().any(|supported| !*supported);
        let route_name =
            fresh_tlm_connect_name(&format!("{base}_{}_route", method.name.name), used_names);
        logic.push(ModuleBodyItem::RegDecl(RegDecl {
            name: Ident::new(route_name.clone(), span),
            ty: TypeExpr::UInt(Box::new(tlm_lit_dec(route_w as u64, span))),
            init: None,
            reset: RegReset::Inherit(rst.clone(), tlm_lit_dec(0, span)),
            guard: None,
            multicycle: None,
            span,
        }));
        let err_valid_name = if any_missing_method {
            let name = fresh_tlm_connect_name(
                &format!("{base}_{}_err_valid", method.name.name),
                used_names,
            );
            logic.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(name.clone(), span),
                ty: TypeExpr::Bool,
                init: None,
                reset: RegReset::Inherit(rst.clone(), tlm_bool(false, span)),
                guard: None,
                multicycle: None,
                span,
            }));
            Some(name)
        } else {
            None
        };

        let selectors = tlm_connect_effective_selectors(
            conn,
            &up_wire,
            &method.name.name,
            &decode_field.name,
            span,
        );

        for ((target_wire, selector), supports_method) in target_wires
            .iter()
            .zip(selectors.iter())
            .zip(target_supports_method.iter())
        {
            if !*supports_method {
                continue;
            }
            comb_stmts.push(tlm_assign(
                tlm_bus_field(
                    target_wire,
                    &format!("{}_req_valid", method.name.name),
                    span,
                ),
                tlm_and(
                    tlm_bus_field(&up_wire, &format!("{}_req_valid", method.name.name), span),
                    selector.clone(),
                    span,
                ),
                span,
            ));
            for (arg, _) in &method.args {
                comb_stmts.push(tlm_assign(
                    tlm_bus_field(
                        target_wire,
                        &format!("{}_{}", method.name.name, arg.name),
                        span,
                    ),
                    tlm_bus_field(
                        &up_wire,
                        &format!("{}_{}", method.name.name, arg.name),
                        span,
                    ),
                    span,
                ));
            }
        }

        let req_ready_terms: Vec<Expr> = target_wires
            .iter()
            .zip(target_supports_method.iter())
            .map(|(wire, supports_method)| {
                if *supports_method {
                    tlm_bus_field(wire, &format!("{}_req_ready", method.name.name), span)
                } else if let Some(err_valid_name) = &err_valid_name {
                    tlm_not(tlm_ident(err_valid_name, span), span)
                } else {
                    tlm_bool(false, span)
                }
            })
            .collect();
        let mut req_ready = tlm_mux_by_selectors(&selectors, &req_ready_terms, span);
        if let Some(err_valid_name) = &err_valid_name {
            req_ready = tlm_ternary(
                tlm_ident(err_valid_name, span),
                tlm_bool(false, span),
                req_ready,
                span,
            );
        }
        comb_stmts.push(tlm_assign(
            tlm_bus_field(&up_wire, &format!("{}_req_ready", method.name.name), span),
            req_ready,
            span,
        ));

        let route_expr = tlm_mux_index_by_selectors(&selectors, route_w, span);
        let missing_method_selectors: Vec<Expr> = selectors
            .iter()
            .zip(target_supports_method.iter())
            .filter_map(|(selector, supports_method)| (!*supports_method).then(|| selector.clone()))
            .collect();
        let missing_method_selected = tlm_or_chain(&missing_method_selectors, span);
        let req_fire = tlm_and(
            tlm_bus_field(&up_wire, &format!("{}_req_valid", method.name.name), span),
            tlm_bus_field(&up_wire, &format!("{}_req_ready", method.name.name), span),
            span,
        );
        let mut req_fire_stmts = vec![tlm_assign(tlm_ident(&route_name, span), route_expr, span)];
        if let Some(err_valid_name) = &err_valid_name {
            req_fire_stmts.push(Stmt::IfElse(IfElseOf {
                cond: missing_method_selected,
                then_stmts: vec![tlm_assign(
                    tlm_ident(err_valid_name, span),
                    tlm_bool(true, span),
                    span,
                )],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
        seq_stmts.push(Stmt::IfElse(IfElseOf {
            cond: req_fire,
            then_stmts: req_fire_stmts,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));

        let route_matches: Vec<Expr> = (0..target_wires.len())
            .map(|i| {
                tlm_bin(
                    BinOp::Eq,
                    tlm_ident(&route_name, span),
                    tlm_lit_sized(route_w, i as u64, span),
                    span,
                )
            })
            .collect();

        for ((target_wire, route_match), supports_method) in target_wires
            .iter()
            .zip(route_matches.iter())
            .zip(target_supports_method.iter())
        {
            if !*supports_method {
                continue;
            }
            comb_stmts.push(tlm_assign(
                tlm_bus_field(
                    target_wire,
                    &format!("{}_rsp_ready", method.name.name),
                    span,
                ),
                tlm_and(
                    tlm_bus_field(&up_wire, &format!("{}_rsp_ready", method.name.name), span),
                    route_match.clone(),
                    span,
                ),
                span,
            ));
        }

        let rsp_valid_terms: Vec<Expr> = target_wires
            .iter()
            .zip(route_matches.iter())
            .zip(target_supports_method.iter())
            .filter_map(|((wire, route_match), supports_method)| {
                (*supports_method).then(|| {
                    tlm_and(
                        route_match.clone(),
                        tlm_bus_field(wire, &format!("{}_rsp_valid", method.name.name), span),
                        span,
                    )
                })
            })
            .collect();
        let mut rsp_valid_terms = rsp_valid_terms;
        if let Some(err_valid_name) = &err_valid_name {
            rsp_valid_terms.push(tlm_ident(err_valid_name, span));
        }
        comb_stmts.push(tlm_assign(
            tlm_bus_field(&up_wire, &format!("{}_rsp_valid", method.name.name), span),
            tlm_or_chain(&rsp_valid_terms, span),
            span,
        ));

        if method.ret.is_some() {
            let supported_route_matches: Vec<Expr> = route_matches
                .iter()
                .zip(target_supports_method.iter())
                .filter_map(|(route_match, supports_method)| {
                    (*supports_method).then(|| route_match.clone())
                })
                .collect();
            let rsp_data_terms: Vec<Expr> = target_wires
                .iter()
                .zip(target_supports_method.iter())
                .filter_map(|(wire, supports_method)| {
                    (*supports_method).then(|| {
                        tlm_bus_field(wire, &format!("{}_rsp_data", method.name.name), span)
                    })
                })
                .collect();
            let mut rsp_data =
                tlm_mux_by_selectors(&supported_route_matches, &rsp_data_terms, span);
            if let (Some(err_valid_name), Some(ret_ty)) = (&err_valid_name, method.ret.as_ref()) {
                rsp_data = tlm_ternary(
                    tlm_ident(err_valid_name, span),
                    tlm_error_response_expr(ret_ty, struct_defs, span),
                    rsp_data,
                    span,
                );
            }
            comb_stmts.push(tlm_assign(
                tlm_bus_field(&up_wire, &format!("{}_rsp_data", method.name.name), span),
                rsp_data,
                span,
            ));
        }
        if let Some(err_valid_name) = &err_valid_name {
            seq_stmts.push(Stmt::IfElse(IfElseOf {
                cond: tlm_and(
                    tlm_ident(err_valid_name, span),
                    tlm_bus_field(&up_wire, &format!("{}_rsp_ready", method.name.name), span),
                    span,
                ),
                then_stmts: vec![tlm_assign(
                    tlm_ident(err_valid_name, span),
                    tlm_bool(false, span),
                    span,
                )],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }

    logic.push(ModuleBodyItem::CombBlock(CombBlock {
        stmts: comb_stmts,
        span,
    }));
    logic.push(ModuleBodyItem::RegBlock(RegBlock {
        clock: clk,
        clock_edge: ClockEdge::Rising,
        stmts: seq_stmts,
        span,
    }));

    Ok(LoweredDecodedTlmConnect { wires, logic })
}

fn infer_single_clock_reset(
    module: &ModuleDecl,
    span: Span,
) -> Result<(Ident, Ident), CompileError> {
    let clocks: Vec<Ident> = module
        .ports
        .iter()
        .filter(|p| matches!(p.ty, TypeExpr::Clock(_)))
        .map(|p| p.name.clone())
        .collect();
    let resets: Vec<Ident> = module
        .ports
        .iter()
        .filter(|p| matches!(p.ty, TypeExpr::Reset(_, _)))
        .map(|p| p.name.clone())
        .collect();
    match (clocks.as_slice(), resets.as_slice()) {
        ([clk], [rst]) => Ok((clk.clone(), rst.clone())),
        _ => Err(CompileError::general(
            "one-to-many TLM connect requires the enclosing module to have exactly one Clock port and one Reset port",
            span,
        )),
    }
}

fn uint_type_literal_width(ty: &TypeExpr) -> Option<u32> {
    let TypeExpr::UInt(width) = ty else {
        return None;
    };
    match &width.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as u32),
        _ => None,
    }
}

fn tlm_connect_ranges_cover_full_space(targets: &[TlmConnectTarget], width: u32) -> bool {
    if width == 0 || width > 64 {
        return false;
    }
    let max = if width == 64 {
        u64::MAX as u128
    } else {
        (1u128 << width) - 1
    };
    let mut ranges = Vec::new();
    for target in targets {
        let Some(TlmConnectDecode::Range { lo, hi }) = &target.decode else {
            continue;
        };
        let (Some(lo), Some(hi)) = (tlm_literal_u128(lo), tlm_literal_u128(hi)) else {
            return false;
        };
        if lo > hi || hi > max {
            return false;
        }
        ranges.push((lo, hi));
    }
    ranges.sort_by_key(|(lo, _)| *lo);
    let mut next = 0u128;
    for (lo, hi) in ranges {
        if lo > next {
            return false;
        }
        if hi >= next {
            next = hi.saturating_add(1);
        }
        if hi == max {
            return true;
        }
    }
    next > max
}

fn tlm_literal_u128(expr: &Expr) -> Option<u128> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as u128),
        _ => None,
    }
}

fn tlm_connect_effective_selectors(
    conn: &TlmConnectDecl,
    up_wire: &str,
    method: &str,
    decode_field: &str,
    span: Span,
) -> Vec<Expr> {
    let addr = tlm_bus_field(up_wire, &format!("{method}_{decode_field}"), span);
    let mut raw_ranges = Vec::new();
    for target in &conn.targets {
        if let Some(TlmConnectDecode::Range { lo, hi }) = &target.decode {
            raw_ranges.push(tlm_and(
                tlm_bin(BinOp::Gte, addr.clone(), lo.clone(), span),
                tlm_bin(BinOp::Lte, addr.clone(), hi.clone(), span),
                span,
            ));
        }
    }
    let any_previous = |raw_ranges: &[Expr], end: usize| -> Option<Expr> {
        if end == 0 {
            return None;
        }
        Some(tlm_or_chain(&raw_ranges[..end], span))
    };

    let mut selectors = Vec::new();
    let mut range_idx = 0usize;
    for target in &conn.targets {
        match &target.decode {
            Some(TlmConnectDecode::Range { .. }) => {
                let raw = raw_ranges[range_idx].clone();
                let effective = if let Some(prev) = any_previous(&raw_ranges, range_idx) {
                    tlm_and(raw, tlm_not(prev, span), span)
                } else {
                    raw
                };
                selectors.push(effective);
                range_idx += 1;
            }
            Some(TlmConnectDecode::Default) => {
                selectors.push(tlm_not(tlm_or_chain(&raw_ranges, span), span));
            }
            None => selectors.push(tlm_bool(false, span)),
        }
    }
    selectors
}

fn tlm_assign(target: Expr, value: Expr, span: Span) -> Stmt {
    Stmt::Assign(Assign {
        target,
        value,
        span,
    })
}

fn tlm_ident(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Ident(name.to_string()), span)
}

fn tlm_bus_field(bus: &str, field: &str, span: Span) -> Expr {
    Expr::new(
        ExprKind::FieldAccess(
            Box::new(tlm_ident(bus, span)),
            Ident::new(field.to_string(), span),
        ),
        span,
    )
}

/// Zero literal matching a synthesized reg's declared type: float types get
/// a typed +0.0 (all-zero bits in FP32/BF16/FP8E4M3/FP8E5M2), everything
/// else the integer 0.
fn tlm_zero_for_type(ty: &TypeExpr, span: Span) -> Expr {
    let fmt = match ty {
        TypeExpr::FP32 => Some(FloatLitFmt::Fp32),
        TypeExpr::BF16 => Some(FloatLitFmt::Bf16),
        TypeExpr::FP8E4M3 => Some(FloatLitFmt::E4m3),
        TypeExpr::FP8E5M2 => Some(FloatLitFmt::E5m2),
        _ => None,
    };
    match fmt {
        Some(f) => Expr::new(ExprKind::Literal(LitKind::TypedFloat(f, 0)), span),
        None => Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
    }
}

fn tlm_lit_dec(value: u64, span: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Dec(value)), span)
}

fn tlm_lit_sized(width: u32, value: u64, span: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Sized(width, value)), span)
}

fn tlm_bool(value: bool, span: Span) -> Expr {
    Expr::new(ExprKind::Bool(value), span)
}

fn tlm_error_response_expr(
    ty: &TypeExpr,
    struct_defs: &HashMap<String, StructDecl>,
    span: Span,
) -> Expr {
    match ty {
        TypeExpr::Bool => tlm_bool(false, span),
        TypeExpr::Named(name) => {
            if let Some(strukt) = struct_defs.get(&name.name) {
                let fields = strukt
                    .fields
                    .iter()
                    .map(|field| FieldInit {
                        name: field.name.clone(),
                        value: if field.name.name == "resp" {
                            tlm_lit_dec(1, span)
                        } else {
                            tlm_error_response_expr(&field.ty, struct_defs, span)
                        },
                    })
                    .collect();
                Expr::new(ExprKind::StructLiteral(name.clone(), fields), span)
            } else {
                tlm_lit_dec(0, span)
            }
        }
        _ => tlm_lit_dec(0, span),
    }
}

fn tlm_bin(op: BinOp, lhs: Expr, rhs: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span)
}

fn tlm_and(lhs: Expr, rhs: Expr, span: Span) -> Expr {
    tlm_bin(BinOp::And, lhs, rhs, span)
}

fn tlm_not(expr: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(expr)), span)
}

fn tlm_or_chain(exprs: &[Expr], span: Span) -> Expr {
    let mut iter = exprs.iter();
    let Some(first) = iter.next() else {
        return tlm_bool(false, span);
    };
    iter.fold(first.clone(), |acc, expr| {
        tlm_bin(BinOp::Or, acc, expr.clone(), span)
    })
}

fn tlm_ternary(cond: Expr, then_expr: Expr, else_expr: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Ternary(Box::new(cond), Box::new(then_expr), Box::new(else_expr)),
        span,
    )
}

fn tlm_mux_by_selectors(selectors: &[Expr], values: &[Expr], span: Span) -> Expr {
    let mut acc = values
        .last()
        .cloned()
        .unwrap_or_else(|| tlm_lit_dec(0, span));
    for (selector, value) in selectors.iter().zip(values.iter()).rev().skip(1) {
        acc = tlm_ternary(selector.clone(), value.clone(), acc, span);
    }
    acc
}

fn tlm_mux_index_by_selectors(selectors: &[Expr], width: u32, span: Span) -> Expr {
    let values: Vec<Expr> = selectors
        .iter()
        .enumerate()
        .map(|(i, _)| tlm_lit_sized(width, i as u64, span))
        .collect();
    tlm_mux_by_selectors(selectors, &values, span)
}

#[derive(Debug, Clone)]
struct TlmConnectEndpoint {
    bus_name: String,
    perspective: BusPerspective,
    span: Span,
    shape: TlmConnectShape,
    bus_params: Vec<ParamAssign>,
    methods: Vec<TlmMethodMeta>,
}

fn tlm_connect_endpoint_bus(
    inst: &Ident,
    port: &Ident,
    inst_modules: &HashMap<String, String>,
    module_defs: &HashMap<String, (Vec<ParamDecl>, Vec<PortDecl>)>,
    inst_params: &HashMap<String, Vec<ParamAssign>>,
    bus_defs: &HashMap<String, BusDecl>,
) -> Result<TlmConnectEndpoint, CompileError> {
    let Some(module_name) = inst_modules.get(&inst.name) else {
        return Err(CompileError::general(
            &format!("unknown TLM connect instance `{}`", inst.name),
            inst.span,
        ));
    };
    let Some((module_params, ports)) = module_defs.get(module_name) else {
        return Err(CompileError::general(
            &format!(
                "TLM connect instance `{}` has construct type `{}` whose ports are not supported by connect",
                inst.name, module_name
            ),
            inst.span,
        ));
    };
    let Some(p) = ports.iter().find(|p| p.name.name == port.name) else {
        return Err(CompileError::general(
            &format!(
                "module `{}` has no port `{}` for TLM connect endpoint `{}.{}`",
                module_name, port.name, inst.name, port.name
            ),
            port.span,
        ));
    };
    let Some(bi) = p.bus_info.as_ref() else {
        return Err(CompileError::general(
            &format!(
                "TLM connect endpoint `{}.{}` names non-bus port `{}` on module `{}`",
                inst.name, port.name, port.name, module_name
            ),
            p.span,
        ));
    };
    let Some(bus_decl) = bus_defs.get(&bi.bus_name.name) else {
        return Err(CompileError::general(
            &format!(
                "TLM connect endpoint `{}.{}` references unknown bus `{}`",
                inst.name, port.name, bi.bus_name.name
            ),
            p.span,
        ));
    };
    let shape = tlm_connect_shape_for_port(
        bus_decl,
        bi,
        module_params,
        inst_params
            .get(&inst.name)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    let methods = tlm_connect_methods_for_port(
        bus_decl,
        bi,
        module_params,
        inst_params
            .get(&inst.name)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    Ok(TlmConnectEndpoint {
        bus_name: bi.bus_name.name.clone(),
        perspective: bi.perspective,
        span: p.span,
        shape,
        bus_params: bi.params.clone(),
        methods,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TlmConnectShape {
    count: Option<u64>,
    signals: Vec<(String, String)>,
}

fn tlm_connect_shape_for_port(
    bus_decl: &BusDecl,
    bi: &BusPortInfo,
    module_params: &[ParamDecl],
    inst_param_assigns: &[ParamAssign],
) -> TlmConnectShape {
    let mut module_param_map: HashMap<String, &Expr> = module_params
        .iter()
        .filter_map(|p| p.default.as_ref().map(|d| (p.name.name.clone(), d)))
        .collect();
    for pa in inst_param_assigns {
        module_param_map.insert(pa.name.name.clone(), &pa.value);
    }

    let mut bus_param_map = module_param_map.clone();
    for p in &bus_decl.params {
        if let Some(default) = p.default.as_ref() {
            bus_param_map.insert(p.name.name.clone(), default);
        }
    }
    for pa in &bi.params {
        bus_param_map.insert(pa.name.name.clone(), &pa.value);
    }

    let count = bi
        .count
        .as_ref()
        .and_then(|e| eval_const_expr_from_param_map_for_lower(e, &module_param_map));
    let mut signals: Vec<(String, String)> = bus_effective_signals(bus_decl, &bus_param_map)
        .into_iter()
        .map(|(name, _dir, ty)| {
            let subst = subst_type_expr_for_lower(&ty, &bus_param_map);
            (name, format!("{subst:?}"))
        })
        .collect();
    signals.sort();
    TlmConnectShape { count, signals }
}

fn tlm_connect_methods_for_port(
    bus_decl: &BusDecl,
    bi: &BusPortInfo,
    module_params: &[ParamDecl],
    inst_param_assigns: &[ParamAssign],
) -> Vec<TlmMethodMeta> {
    let mut module_param_map: HashMap<String, &Expr> = module_params
        .iter()
        .filter_map(|p| p.default.as_ref().map(|d| (p.name.name.clone(), d)))
        .collect();
    for pa in inst_param_assigns {
        module_param_map.insert(pa.name.name.clone(), &pa.value);
    }

    let mut bus_param_map = module_param_map;
    for p in &bus_decl.params {
        if let Some(default) = p.default.as_ref() {
            bus_param_map.insert(p.name.name.clone(), default);
        }
    }
    for pa in &bi.params {
        bus_param_map.insert(pa.name.name.clone(), &pa.value);
    }

    tlm_effective_methods_for_bus(bus_decl, &bus_param_map)
        .iter()
        .map(|method| specialize_tlm_method(method, &bus_param_map))
        .collect()
}

fn tlm_connect_decoded_shapes_compatible(
    from_shape: &TlmConnectShape,
    to_shape: &TlmConnectShape,
    methods: &[TlmMethodMeta],
) -> bool {
    if from_shape.count != to_shape.count {
        return false;
    }

    let from: HashMap<&str, &str> = from_shape
        .signals
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.as_str()))
        .collect();
    let to: HashMap<&str, &str> = to_shape
        .signals
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.as_str()))
        .collect();

    for (name, ty) in &to {
        if from.get(name).copied() != Some(*ty) {
            return false;
        }
    }

    let method_fields: HashSet<String> = methods.iter().flat_map(tlm_method_signal_names).collect();
    for (name, _) in &from_shape.signals {
        if to.contains_key(name.as_str()) {
            continue;
        }
        if !method_fields.contains(name) {
            return false;
        }
    }

    methods.iter().all(|method| {
        let fields = tlm_method_signal_names(method);
        let present = fields
            .iter()
            .filter(|field| to.contains_key(field.as_str()))
            .count();
        present == 0 || present == fields.len()
    })
}

fn tlm_connect_shape_has_method(shape: &TlmConnectShape, method: &TlmMethodMeta) -> bool {
    let names: HashSet<&str> = shape
        .signals
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    tlm_method_signal_names(method)
        .iter()
        .all(|field| names.contains(field.as_str()))
}

fn tlm_method_signal_names(method: &TlmMethodMeta) -> Vec<String> {
    let mut fields = vec![
        format!("{}_req_valid", method.name.name),
        format!("{}_req_ready", method.name.name),
        format!("{}_rsp_valid", method.name.name),
        format!("{}_rsp_ready", method.name.name),
    ];
    if method.out_of_order_tags.is_some() {
        fields.push(format!("{}_req_tag", method.name.name));
        fields.push(format!("{}_rsp_tag", method.name.name));
    }
    for (arg, _) in &method.args {
        fields.push(format!("{}_{}", method.name.name, arg.name));
    }
    if method.ret.is_some() {
        fields.push(format!("{}_rsp_data", method.name.name));
    }
    fields
}

fn format_tlm_connect_shape(shape: &TlmConnectShape) -> String {
    let count = shape
        .count
        .map(|n| format!("count={n}, "))
        .unwrap_or_default();
    let sigs = shape
        .signals
        .iter()
        .map(|(name, ty)| format!("{name}:{ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{count}signals [{sigs}]")
}

fn fold_literal_bit_slices_thread_stmt(stmt: ThreadStmt) -> ThreadStmt {
    match stmt {
        ThreadStmt::CombAssign(ca) => ThreadStmt::CombAssign(CombAssign {
            target: fold_literal_bit_slices_expr(ca.target),
            value: fold_literal_bit_slices_expr(ca.value),
            span: ca.span,
        }),
        ThreadStmt::SeqAssign(ra) => ThreadStmt::SeqAssign(RegAssign {
            target: fold_literal_bit_slices_expr(ra.target),
            value: fold_literal_bit_slices_expr(ra.value),
            span: ra.span,
        }),
        ThreadStmt::ForkTlmAssign(ra) => ThreadStmt::ForkTlmAssign(RegAssign {
            target: fold_literal_bit_slices_expr(ra.target),
            value: fold_literal_bit_slices_expr(ra.value),
            span: ra.span,
        }),
        ThreadStmt::WaitUntil(cond, sp) => {
            ThreadStmt::WaitUntil(fold_literal_bit_slices_expr(cond), sp)
        }
        ThreadStmt::WaitCycles(n, sp) => {
            ThreadStmt::WaitCycles(fold_literal_bit_slices_expr(n), sp)
        }
        ThreadStmt::IfElse(mut ie) => {
            ie.cond = fold_literal_bit_slices_expr(ie.cond);
            ie.then_stmts = ie
                .then_stmts
                .into_iter()
                .map(fold_literal_bit_slices_thread_stmt)
                .collect();
            ie.else_stmts = ie
                .else_stmts
                .into_iter()
                .map(fold_literal_bit_slices_thread_stmt)
                .collect();
            ThreadStmt::IfElse(ie)
        }
        ThreadStmt::ForkJoin(branches, sp) => ThreadStmt::ForkJoin(
            branches
                .into_iter()
                .map(|br| {
                    br.into_iter()
                        .map(fold_literal_bit_slices_thread_stmt)
                        .collect()
                })
                .collect(),
            sp,
        ),
        ThreadStmt::For {
            var,
            start,
            end,
            body,
            span,
        } => ThreadStmt::For {
            var,
            start: fold_literal_bit_slices_expr(start),
            end: fold_literal_bit_slices_expr(end),
            body: body
                .into_iter()
                .map(fold_literal_bit_slices_thread_stmt)
                .collect(),
            span,
        },
        ThreadStmt::Lock {
            resource,
            body,
            span,
        } => ThreadStmt::Lock {
            resource,
            body: body
                .into_iter()
                .map(fold_literal_bit_slices_thread_stmt)
                .collect(),
            span,
        },
        ThreadStmt::DoUntil { body, cond, span } => ThreadStmt::DoUntil {
            body: body
                .into_iter()
                .map(fold_literal_bit_slices_thread_stmt)
                .collect(),
            cond: fold_literal_bit_slices_expr(cond),
            span,
        },
        ThreadStmt::Return(e, sp) => ThreadStmt::Return(fold_literal_bit_slices_expr(e), sp),
        other => other,
    }
}

fn fold_literal_bit_slices_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let parenthesized = expr.parenthesized;
    let kind = match expr.kind {
        ExprKind::Index(base, idx) => {
            let base = fold_literal_bit_slices_expr(*base);
            let idx = fold_literal_bit_slices_expr(*idx);
            if let (Some(v), Some(idx_v)) = (literal_expr_u64(&base), literal_expr_u64(&idx)) {
                if idx_v < 64 {
                    ExprKind::Literal(LitKind::Sized(1, (v >> idx_v) & 1))
                } else {
                    ExprKind::Index(Box::new(base), Box::new(idx))
                }
            } else {
                ExprKind::Index(Box::new(base), Box::new(idx))
            }
        }
        ExprKind::BitSlice(base, hi, lo) => {
            let base = fold_literal_bit_slices_expr(*base);
            let hi = fold_literal_bit_slices_expr(*hi);
            let lo = fold_literal_bit_slices_expr(*lo);
            if let (Some(v), Some(hi_v), Some(lo_v)) = (
                literal_expr_u64(&base),
                literal_expr_u64(&hi),
                literal_expr_u64(&lo),
            ) {
                if hi_v >= lo_v && hi_v < 64 {
                    let width = (hi_v - lo_v + 1) as u32;
                    let mask = if width >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << width) - 1
                    };
                    ExprKind::Literal(LitKind::Sized(width, (v >> lo_v) & mask))
                } else {
                    ExprKind::BitSlice(Box::new(base), Box::new(hi), Box::new(lo))
                }
            } else {
                ExprKind::BitSlice(Box::new(base), Box::new(hi), Box::new(lo))
            }
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            op,
            Box::new(fold_literal_bit_slices_expr(*l)),
            Box::new(fold_literal_bit_slices_expr(*r)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(op, Box::new(fold_literal_bit_slices_expr(*e))),
        ExprKind::FieldAccess(e, f) => {
            ExprKind::FieldAccess(Box::new(fold_literal_bit_slices_expr(*e)), f)
        }
        ExprKind::MethodCall(e, m, args) => ExprKind::MethodCall(
            Box::new(fold_literal_bit_slices_expr(*e)),
            m,
            args.into_iter().map(fold_literal_bit_slices_expr).collect(),
        ),
        ExprKind::Cast(e, ty) => ExprKind::Cast(Box::new(fold_literal_bit_slices_expr(*e)), ty),
        ExprKind::Concat(exprs) => ExprKind::Concat(
            exprs
                .into_iter()
                .map(fold_literal_bit_slices_expr)
                .collect(),
        ),
        ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
            Box::new(fold_literal_bit_slices_expr(*c)),
            Box::new(fold_literal_bit_slices_expr(*t)),
            Box::new(fold_literal_bit_slices_expr(*f)),
        ),
        other => other,
    };
    Expr {
        kind,
        span,
        parenthesized,
    }
}

// ── TLM target thread lowering ──────────────────────────────────────────────
//
// Transforms each `thread port.method(args) ... end` body into a regular
// thread that:
//  1. Waits for `<port>_<method>_req_valid`, driving
//     `<port>_<method>_req_ready = 1` while waiting (accept-on-transition).
//  2. Latches each declared arg from the request bus into a synthesized
//     reg `__tlm_<port>_<method>_<arg>_latched` (SeqAssign fires on
//     transition, i.e. the cycle the request is accepted).
//  3. Executes the user body with arg ident references rewritten to the
//     latched reg names.
//  4. Rewrites each `return expr;` into the response drive sequence:
//     `rsp_valid = 1; rsp_data = expr; wait until rsp_ready;`.
//  5. Loops back to state 0 via the normal non-`once` thread semantics.
//
// Runs before lower_threads. Synthesized latch regs are injected as
// RegDecls at the start of the module body.

pub fn lower_tlm_target_threads(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    use std::collections::HashMap;
    // Build {bus_name -> Vec<TlmMethodMeta>}.
    let mut bus_methods: HashMap<String, Vec<TlmMethodMeta>> = HashMap::new();
    let mut bus_params: HashMap<String, Vec<ParamDecl>> = HashMap::new();
    let mut bus_generates: HashMap<String, Vec<BusGenerateIf>> = HashMap::new();
    for it in &ast.items {
        match it {
            Item::Bus(b) => {
                if bus_has_tlm_methods(b) {
                    bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                    bus_params.insert(b.name.name.clone(), b.params.clone());
                    bus_generates.insert(b.name.name.clone(), b.generates.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if bus_has_tlm_methods(b) {
                        bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                        bus_params.insert(b.name.name.clone(), b.params.clone());
                        bus_generates.insert(b.name.name.clone(), b.generates.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_methods.is_empty() {
        return Ok(ast);
    }

    let mut out_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    for it in ast.items {
        match it {
            Item::Module(mut m) => {
                // Build port → bus_name map for this module.
                let (port_buses, port_methods) = specialize_tlm_methods_for_module_ports(
                    &m,
                    &bus_methods,
                    &bus_params,
                    &bus_generates,
                );
                // Detect multi-implementer target cases. Indexed target
                // lanes (`thread s.read[t](...)`) are handled below by
                // generating private lane endpoints plus one shared mux.
                // Non-indexed multi-targets still produce multiple drivers.
                {
                    let mut counts: HashMap<(String, String), (usize, usize, Span)> =
                        HashMap::new();
                    for item in &m.body {
                        if let ModuleBodyItem::Thread(t) = item {
                            let key = if let Some(tb) = &t.tlm_target {
                                Some((
                                    tb.port.name.clone(),
                                    tb.method.name.clone(),
                                    tb.tag_lane.is_some(),
                                ))
                            } else if let Some(ib) = &t.implement {
                                if ib.kind == TlmImplementKind::Target {
                                    Some((ib.port.name.clone(), ib.method.name.clone(), false))
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if let Some((port, method, indexed)) = key {
                                let e = counts.entry((port, method)).or_insert((0, 0, t.span));
                                e.0 += 1;
                                if indexed {
                                    e.1 += 1;
                                }
                            }
                        }
                    }
                    for ((port, method), (n, indexed, span)) in &counts {
                        if *n > 1 && *indexed != *n {
                            errors.push(CompileError::general(
                                &format!(
                                    "multi-implementer target for `{port}.{method}` requires every target thread to use indexed tag-lane syntax, e.g. `thread {port}.{method}[t](...)`; {n} threads bind to this method but only {indexed} are indexed.",
                                ),
                                *span,
                            ));
                        }
                    }
                }

                // Collect TLM target threads + their method metadata.
                let mut new_body: Vec<ModuleBodyItem> = Vec::new();
                let resource_decls: HashMap<String, ResourceDecl> = m
                    .body
                    .iter()
                    .filter_map(|item| match item {
                        ModuleBodyItem::Resource(r) => Some((r.name.name.clone(), r.clone())),
                        _ => None,
                    })
                    .collect();
                let mut indexed_target_groups: HashMap<
                    (String, String),
                    Vec<(ThreadBlock, TlmTargetBinding, TlmMethodMeta)>,
                > = HashMap::new();
                for item in std::mem::take(&mut m.body) {
                    if let ModuleBodyItem::Thread(t) = &item {
                        // v1 dotted-name form populates `tlm_target`; v2
                        // `implement target port.method(args)` populates
                        // `implement`. Normalize to a single TlmTargetBinding.
                        let effective_target: Option<TlmTargetBinding> =
                            t.tlm_target.clone().or_else(|| {
                                t.implement
                                    .as_ref()
                                    .filter(|b| b.kind == TlmImplementKind::Target)
                                    .map(|b| TlmTargetBinding {
                                        port: b.port.clone(),
                                        method: b.method.clone(),
                                        tag_lane: None,
                                        args: b.args.clone(),
                                    })
                            });
                        if let Some(binding) = effective_target {
                            let bus_name = match port_buses.get(&binding.port.name) {
                                Some(b) => b.clone(),
                                None => {
                                    errors.push(CompileError::general(
                                        &format!(
                                            "thread `{}.{}(...)` references port `{}` which is not a bus port on module `{}`",
                                            binding.port.name, binding.method.name, binding.port.name, m.name.name,
                                        ),
                                        binding.port.span,
                                    ));
                                    new_body.push(item);
                                    continue;
                                }
                            };
                            let method = match port_methods.get(&binding.port.name).and_then(|v| {
                                v.iter().find(|mm| mm.name.name == binding.method.name)
                            }) {
                                Some(m) => m.clone(),
                                None => {
                                    errors.push(CompileError::general(
                                        &format!(
                                            "bus `{}` has no `tlm_method {}` matching `thread {}.{}(...)`",
                                            bus_name, binding.method.name, binding.port.name, binding.method.name,
                                        ),
                                        binding.method.span,
                                    ));
                                    new_body.push(item);
                                    continue;
                                }
                            };
                            // Arg count / name check.
                            if binding.args.len() != method.args.len() {
                                errors.push(CompileError::general(
                                    &format!(
                                        "`thread {}.{}(...)` takes {} args but `tlm_method {}` declares {}",
                                        binding.port.name, binding.method.name, binding.args.len(),
                                        method.name.name, method.args.len(),
                                    ),
                                    binding.method.span,
                                ));
                                new_body.push(item);
                                continue;
                            }
                            let t_moved = if let ModuleBodyItem::Thread(t) = item {
                                t
                            } else {
                                unreachable!()
                            };
                            if binding.tag_lane.is_some() {
                                indexed_target_groups
                                    .entry((binding.port.name.clone(), binding.method.name.clone()))
                                    .or_default()
                                    .push((t_moved, binding, method));
                            } else {
                                match inline_lower_tlm_target(t_moved, &binding, &method) {
                                    Ok(items) => new_body.extend(items),
                                    Err(e) => errors.push(e),
                                }
                            }
                        } else {
                            new_body.push(item);
                        }
                    } else {
                        new_body.push(item);
                    }
                }
                let mut extra_items: Vec<Item> = Vec::new();
                for ((_port, _method), group) in indexed_target_groups {
                    match lower_indexed_tlm_target_group(&m.name.name, group, &resource_decls) {
                        Ok((items, mut extras)) => {
                            new_body.extend(items);
                            extra_items.append(&mut extras);
                        }
                        Err(e) => errors.push(e),
                    }
                }
                // Inline lowering emits its own RegDecl / RegBlock /
                // CombBlock items directly into new_body; no additional
                // accumulation needed.
                if !new_body
                    .iter()
                    .any(|item| matches!(item, ModuleBodyItem::Thread(_)))
                {
                    new_body.retain(|item| !matches!(item, ModuleBodyItem::Resource(_)));
                }
                m.body = new_body;
                out_items.extend(extra_items);
                out_items.push(Item::Module(m));
            }
            other => out_items.push(other),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(SourceFile {
        items: out_items,
        inner_doc: None,
        frontmatter: None,
    })
}

fn bus_has_tlm_methods(bus: &BusDecl) -> bool {
    !bus.tlm_methods.is_empty()
        || bus
            .generates
            .iter()
            .any(|gi| !gi.then_tlm_methods.is_empty() || !gi.else_tlm_methods.is_empty())
}

fn specialize_tlm_methods_for_module_ports(
    m: &ModuleDecl,
    bus_methods: &HashMap<String, Vec<TlmMethodMeta>>,
    bus_params: &HashMap<String, Vec<ParamDecl>>,
    bus_generates: &HashMap<String, Vec<BusGenerateIf>>,
) -> (HashMap<String, String>, HashMap<String, Vec<TlmMethodMeta>>) {
    let mut port_buses: HashMap<String, String> = HashMap::new();
    let mut port_methods: HashMap<String, Vec<TlmMethodMeta>> = HashMap::new();
    for p in &m.ports {
        let Some(bi) = &p.bus_info else {
            continue;
        };
        let bus_name = bi.bus_name.name.clone();
        port_buses.insert(p.name.name.clone(), bus_name.clone());
        let Some(methods) = bus_methods.get(&bus_name) else {
            continue;
        };
        let mut param_map: HashMap<String, &Expr> = HashMap::new();
        if let Some(params) = bus_params.get(&bus_name) {
            for pd in params {
                if let Some(default) = &pd.default {
                    param_map.insert(pd.name.name.clone(), default);
                }
            }
        }
        for pa in &bi.params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let mut effective_methods = methods.clone();
        if let Some(generates) = bus_generates.get(&bus_name) {
            for gi in generates {
                let cond = gen_if_cond_truthy(&gi.cond, &param_map);
                let branch = if cond {
                    &gi.then_tlm_methods
                } else {
                    &gi.else_tlm_methods
                };
                effective_methods.extend(branch.clone());
            }
        }
        port_methods.insert(
            p.name.name.clone(),
            effective_methods
                .iter()
                .map(|method| specialize_tlm_method(method, &param_map))
                .collect(),
        );
    }
    (port_buses, port_methods)
}

fn specialize_tlm_method(
    method: &TlmMethodMeta,
    param_map: &HashMap<String, &Expr>,
) -> TlmMethodMeta {
    TlmMethodMeta {
        name: method.name.clone(),
        args: method
            .args
            .iter()
            .map(|(name, ty)| (name.clone(), subst_type_expr_params(ty, param_map)))
            .collect(),
        ret: method
            .ret
            .as_ref()
            .map(|ty| subst_type_expr_params(ty, param_map)),
        mode: method.mode.clone(),
        out_of_order_tags: method
            .out_of_order_tags
            .as_ref()
            .map(|expr| subst_expr_params(expr, param_map)),
        span: method.span,
    }
}

fn subst_type_expr_params(ty: &TypeExpr, param_map: &HashMap<String, &Expr>) -> TypeExpr {
    match ty {
        TypeExpr::UInt(e) => TypeExpr::UInt(Box::new(subst_expr_params(e, param_map))),
        TypeExpr::SInt(e) => TypeExpr::SInt(Box::new(subst_expr_params(e, param_map))),
        TypeExpr::Vec(inner, size) => TypeExpr::Vec(
            Box::new(subst_type_expr_params(inner, param_map)),
            Box::new(subst_expr_params(size, param_map)),
        ),
        _ => ty.clone(),
    }
}

fn subst_expr_params(expr: &Expr, param_map: &HashMap<String, &Expr>) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(replacement) = param_map.get(name.as_str()) {
                return (*replacement).clone();
            }
            ExprKind::Ident(name.clone())
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(subst_expr_params(l, param_map)),
            Box::new(subst_expr_params(r, param_map)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(subst_expr_params(e, param_map))),
        ExprKind::Ternary(c, t, e) => ExprKind::Ternary(
            Box::new(subst_expr_params(c, param_map)),
            Box::new(subst_expr_params(t, param_map)),
            Box::new(subst_expr_params(e, param_map)),
        ),
        ExprKind::Clog2(e) => ExprKind::Clog2(Box::new(subst_expr_params(e, param_map))),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(subst_expr_params(base, param_map)),
            Box::new(subst_expr_params(idx, param_map)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(subst_expr_params(base, param_map)),
            Box::new(subst_expr_params(hi, param_map)),
            Box::new(subst_expr_params(lo, param_map)),
        ),
        ExprKind::MethodCall(base, method, args) => ExprKind::MethodCall(
            Box::new(subst_expr_params(base, param_map)),
            method.clone(),
            args.iter()
                .map(|arg| subst_expr_params(arg, param_map))
                .collect(),
        ),
        ExprKind::Concat(parts) => ExprKind::Concat(
            parts
                .iter()
                .map(|part| subst_expr_params(part, param_map))
                .collect(),
        ),
        ExprKind::Literal(LitKind::ParamSized(name, value)) => {
            // Resolve the param-width identifier to a concrete width.
            if let Some(width) = eval_const_expr_from_param_map_for_lower(
                &Expr {
                    kind: ExprKind::Ident(name.clone()),
                    span: expr.span,
                    parenthesized: false,
                },
                param_map,
            ) {
                return Expr {
                    kind: ExprKind::Literal(LitKind::Sized(width as u32, *value)),
                    span: expr.span,
                    parenthesized: expr.parenthesized,
                };
            }
            // If the param can't be resolved yet, keep it as ParamSized.
            // The typechecker will catch the unresolved reference.
            ExprKind::Literal(LitKind::ParamSized(name.clone(), *value))
        }
        _ => return expr.clone(),
    };
    Expr {
        kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

// ── TLM initiator call-site lowering ────────────────────────────────────────
//
// Recognizes `target_reg <= port.method(args);` as a TLM call site inside a
// thread body and expands it into the synthesizable request/response protocol
// described in doc/ARCH_HDL_Specification.md §18d/§22 and
// doc/archive/plan_tlm_method.md. Call sites outside this shape are rejected with a
// targeted message.

pub fn lower_tlm_initiator_calls(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    use std::collections::HashMap;
    let mut bus_methods: HashMap<String, Vec<TlmMethodMeta>> = HashMap::new();
    let mut bus_params: HashMap<String, Vec<ParamDecl>> = HashMap::new();
    let mut bus_generates: HashMap<String, Vec<BusGenerateIf>> = HashMap::new();
    for it in &ast.items {
        match it {
            Item::Bus(b) => {
                if bus_has_tlm_methods(b) {
                    bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                    bus_params.insert(b.name.name.clone(), b.params.clone());
                    bus_generates.insert(b.name.name.clone(), b.generates.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if bus_has_tlm_methods(b) {
                        bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                        bus_params.insert(b.name.name.clone(), b.params.clone());
                        bus_generates.insert(b.name.name.clone(), b.generates.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_methods.is_empty() {
        return Ok(ast);
    }

    let mut errors: Vec<CompileError> = Vec::new();
    let mut out_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    for it in ast.items {
        match it {
            Item::Module(mut m) => {
                let (_port_bus_names, port_methods) = specialize_tlm_methods_for_module_ports(
                    &m,
                    &bus_methods,
                    &bus_params,
                    &bus_generates,
                );
                let port_buses: HashMap<String, String> = port_methods
                    .keys()
                    .map(|port| (port.clone(), port.clone()))
                    .collect();
                let resource_decls: HashMap<String, ResourceDecl> = m
                    .body
                    .iter()
                    .filter_map(|item| match item {
                        ModuleBodyItem::Resource(r) => Some((r.name.name.clone(), r.clone())),
                        _ => None,
                    })
                    .collect();

                let mut direct_groups: HashMap<(String, String), Vec<DirectTlmThread>> =
                    HashMap::new();
                for item in &m.body {
                    if let ModuleBodyItem::Thread(t) = item {
                        if t.tlm_target.is_some() || t.implement.is_some() {
                            continue;
                        }
                        let dts = direct_tlm_threads(t, &port_buses, &port_methods);
                        // A `fork ... and ... join` direct-call group is
                        // recognized here when `dts.len() > 1` (see
                        // `direct_tlm_threads`). Grouping downstream keys
                        // purely by `(port, method)`, so a group that mixes
                        // `blocking` and `out_of_order` calls across
                        // different methods can partially satisfy the
                        // same-method cohort size check for whichever class
                        // happens to repeat, while the minority-class
                        // branch(es) are silently dropped instead of being
                        // lowered. Reject mixed-class groups up front with a
                        // clean diagnostic — see #500 Gap 1 and
                        // doc/ARCH_HDL_Specification.md §22.2.2.
                        if dts.len() > 1 {
                            if let Some(e) = check_fork_join_uniform_tlm_class(&dts) {
                                errors.push(e);
                                continue;
                            }
                        }
                        for dt in dts {
                            direct_groups
                                .entry((dt.call.port.clone(), dt.call.method.clone()))
                                .or_default()
                                .push(dt);
                        }
                    }
                }
                let cohort_groups: std::collections::HashSet<(String, String)> = direct_groups
                    .iter()
                    .filter_map(|(k, v)| if v.len() > 1 { Some(k.clone()) } else { None })
                    .collect();
                let cohort_thread_spans: std::collections::HashSet<(usize, usize)> = direct_groups
                    .values()
                    .filter(|v| v.len() > 1)
                    .flat_map(|v| {
                        v.iter()
                            .map(|dt| (dt.thread.span.start, dt.thread.span.end))
                    })
                    .collect();

                // Detect unlocked multi-thread sharing of a (port, method)
                // pair. ARCH's existing `lock RESOURCE` construct serializes
                // bus-channel drives across threads; wrapping each TLM call
                // in a lock makes the resource mutex handle request-side
                // arbitration uniformly with other shared-channel idioms
                // (AXI AR/AW in ThreadMm2s, etc.). Without lock, multiple
                // threads drive `<port>_<method>_req_valid` simultaneously
                // and the later single-driver check fires a confusing
                // multi-driver error. Emit a targeted diagnostic here
                // pointing at the lock/resource idiom.
                {
                    let mut bare_uses: HashMap<(String, String), Vec<Span>> = HashMap::new();
                    for item in &m.body {
                        if let ModuleBodyItem::Thread(t) = item {
                            if t.tlm_target.is_some() {
                                continue;
                            }
                            // Threads carrying `implement` are the opt-in
                            // mechanism for multi-thread TLM — skip the
                            // lock-idiom diagnostic on them; the TLM
                            // lowering pass groups/cohorts them below.
                            if t.implement.is_some() {
                                continue;
                            }
                            collect_bare_tlm_calls(
                                &t.body,
                                t.span,
                                &port_buses,
                                &port_methods,
                                &mut bare_uses,
                            );
                        }
                    }
                    for ((port, method), spans) in &bare_uses {
                        if cohort_groups.contains(&(port.clone(), method.clone())) {
                            continue;
                        }
                        let mut sorted_offsets: Vec<(usize, usize)> =
                            spans.iter().map(|s| (s.start, s.end)).collect();
                        sorted_offsets.sort();
                        sorted_offsets.dedup();
                        if sorted_offsets.len() > 1 {
                            errors.push(CompileError::general(
                                &format!(
                                    "multi-thread sharing of `{port}.{method}` without a `lock` — {n} threads issue calls on this method outside any lock block. Wrap each call in `lock <res> ... end lock <res>` and declare `resource <res>: mutex<round_robin>;` at module scope. Lock serializes request-side drive across threads (same idiom as AXI AR/AW sharing). Concurrent-in-flight pipelining ships with `out_of_order` mode (v2b).",
                                    n = sorted_offsets.len(),
                                ),
                                *spans.first().unwrap(),
                            ));
                        }
                    }
                }
                if !errors.is_empty() {
                    out_items.push(Item::Module(m));
                    continue;
                }

                // Identify threads that contain TLM calls and inline them.
                let mut inline_tlm_threads: Vec<ThreadBlock> = Vec::new();
                let mut inline_tlm_thread_spans: std::collections::HashSet<(usize, usize)> =
                    std::collections::HashSet::new();
                for item in &m.body {
                    if let ModuleBodyItem::Thread(t) = item {
                        let t_key = (t.span.start, t.span.end);
                        if t.tlm_target.is_some()
                            || cohort_thread_spans.contains(&t_key)
                            || thread_has_fork_tlm_assign(&t.body)
                        {
                            continue;
                        }
                        if thread_body_has_tlm_call(&t.body, &port_buses, &port_methods) {
                            inline_tlm_thread_spans.insert(t_key);
                            inline_tlm_threads.push(t.clone());
                        }
                    }
                }

                let mut new_body: Vec<ModuleBodyItem> = Vec::new();
                let mut emitted_cohorts: std::collections::HashSet<(String, String)> =
                    std::collections::HashSet::new();
                let mut emitted_inline_tlm_group = false;
                for item in std::mem::take(&mut m.body) {
                    if let ModuleBodyItem::Thread(t) = &item {
                        let t_key = (t.span.start, t.span.end);
                        if cohort_thread_spans.contains(&t_key) {
                            if let Some(dt) = direct_tlm_threads(t, &port_buses, &port_methods)
                                .into_iter()
                                .next()
                            {
                                let key = (dt.call.port.clone(), dt.call.method.clone());
                                if emitted_cohorts.insert(key.clone()) {
                                    if let Some(group) = direct_groups.get(&key) {
                                        match lower_tlm_initiator_cohort(group, m.span) {
                                            Ok(items) => new_body.extend(items),
                                            Err(e) => errors.push(e),
                                        }
                                    }
                                }
                            }
                            continue;
                        }
                        if inline_tlm_thread_spans.contains(&t_key) {
                            if !emitted_inline_tlm_group {
                                match inline_lower_tlm_initiator_group(
                                    inline_tlm_threads.clone(),
                                    &port_buses,
                                    &port_methods,
                                    &resource_decls,
                                ) {
                                    Ok(items) => new_body.extend(items),
                                    Err(e) => errors.push(e),
                                }
                                emitted_inline_tlm_group = true;
                            }
                            continue;
                        }
                        if t.tlm_target.is_some() {
                            new_body.push(item);
                            continue;
                        }
                        if thread_has_fork_tlm_assign(&t.body) {
                            match inline_lower_tlm_fork_join_all(
                                t.clone(),
                                &port_buses,
                                &port_methods,
                            ) {
                                Ok(items) => new_body.extend(items),
                                Err(e) => {
                                    errors.push(e);
                                    new_body.push(item);
                                }
                            }
                            continue;
                        }
                        // Target-side `implement` is handled by
                        // lower_tlm_target_threads before this pass runs,
                        // so anything reaching here is initiator (if set).
                        if let Some(ib) = &t.implement {
                            if ib.kind == TlmImplementKind::Initiator {
                                // Initiator-side `implement m.method()` is an
                                // annotation over ordinary call-site/cohort
                                // lowering; fall through to the same path as
                                // non-`implement` TLM worker threads.
                            } else {
                                // Target kind here is unexpected (should've
                                // been consumed earlier). Leave for the
                                // lower_threads defensive error.
                                new_body.push(item);
                                continue;
                            }
                        }
                        if thread_body_has_tlm_call(&t.body, &port_buses, &port_methods) {
                            let t_moved = if let ModuleBodyItem::Thread(t) = item {
                                t
                            } else {
                                unreachable!()
                            };
                            match inline_lower_tlm_initiator(t_moved, &port_buses, &port_methods) {
                                Ok(items) => new_body.extend(items),
                                Err(e) => errors.push(e),
                            }
                            continue;
                        }
                    }
                    new_body.push(item);
                }
                // If every TLM-bearing thread in this module was consumed
                // by the in-place initiator lowering, any resource
                // declarations that only existed for those locks must not
                // leak to codegen. Regular thread lowering consumes
                // resources only when Thread items remain.
                if !new_body
                    .iter()
                    .any(|item| matches!(item, ModuleBodyItem::Thread(_)))
                {
                    new_body.retain(|item| !matches!(item, ModuleBodyItem::Resource(_)));
                }
                m.body = new_body;
                out_items.push(Item::Module(m));
            }
            other => out_items.push(other),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(SourceFile {
        items: out_items,
        inner_doc: None,
        frontmatter: None,
    })
}

#[derive(Clone)]
struct DirectTlmThread {
    thread: ThreadBlock,
    target: Expr,
    call: TlmCall,
}

fn direct_single_tlm_thread(
    t: &ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Option<DirectTlmThread> {
    let ThreadStmt::SeqAssign(ra) = &t.body[0] else {
        return None;
    };
    direct_tlm_assign_thread(t, ra, port_buses, bus_methods)
}

fn direct_tlm_threads(
    t: &ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Vec<DirectTlmThread> {
    if t.default_when.is_some() || !t.default_comb.is_empty() || t.once || t.body.len() != 1 {
        return Vec::new();
    }
    match &t.body[0] {
        ThreadStmt::SeqAssign(_) => direct_single_tlm_thread(t, port_buses, bus_methods)
            .into_iter()
            .collect(),
        ThreadStmt::ForkJoin(branches, _) => {
            let mut out = Vec::new();
            for branch in branches {
                if branch.len() != 1 {
                    return Vec::new();
                }
                let ThreadStmt::SeqAssign(ra) = &branch[0] else {
                    return Vec::new();
                };
                let Some(dt) = direct_tlm_assign_thread(t, ra, port_buses, bus_methods) else {
                    return Vec::new();
                };
                out.push(dt);
            }
            if out.len() > 1 {
                out
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Human-readable class label for a TLM method's concurrency mode, used in
/// the mixed-class fork-group diagnostic below. `blocking` methods print as
/// `blocking`; `out_of_order tags N` methods print with the literal tag
/// count when it resolves, otherwise just `out_of_order`.
fn tlm_mode_label(meta: &TlmMethodMeta) -> String {
    match &meta.out_of_order_tags {
        Some(tags) => match literal_expr_u64(tags) {
            Some(n) => format!("out_of_order tags {n}"),
            None => "out_of_order".to_string(),
        },
        None => meta.mode.name.clone(),
    }
}

/// Reject a direct-call `fork ... and ... join` TLM issue group (see
/// `direct_tlm_threads`) that mixes `blocking` and `out_of_order tags N`
/// calls. See doc/ARCH_HDL_Specification.md §22.2.2 and arch-com #500 Gap 1:
/// the downstream `(port, method)` cohort grouping in
/// `lower_tlm_initiator_calls` only recognizes a same-method cohort when at
/// least two branches share one `(port, method)` key, so a mixed-class group
/// can partially satisfy that check for whichever class happens to repeat
/// while silently dropping the minority-class branch(es) instead of
/// lowering them. Uniform-class groups (the only supported shape) return
/// `None`.
fn check_fork_join_uniform_tlm_class(dts: &[DirectTlmThread]) -> Option<CompileError> {
    let first = dts.first()?;
    let first_is_ooo = first.call.method_meta.out_of_order_tags.is_some();
    for dt in &dts[1..] {
        let is_ooo = dt.call.method_meta.out_of_order_tags.is_some();
        if is_ooo != first_is_ooo {
            return Some(CompileError::general(
                &format!(
                    "fork issue group mixes blocking and out_of_order calls (`{}.{}` is {}, `{}.{}` is {}); split into separate fork groups or make the classes uniform",
                    first.call.port,
                    first.call.method,
                    tlm_mode_label(&first.call.method_meta),
                    dt.call.port,
                    dt.call.method,
                    tlm_mode_label(&dt.call.method_meta),
                ),
                dt.thread.span,
            ));
        }
    }
    None
}

fn direct_tlm_assign_thread(
    t: &ThreadBlock,
    ra: &RegAssign,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Option<DirectTlmThread> {
    let call = match_tlm_call(&ra.value, port_buses, bus_methods)?;
    if contains_tlm_call(&ra.target, port_buses, bus_methods) {
        return None;
    }
    if call.args.len() != call.method_meta.args.len() {
        return None;
    }
    Some(DirectTlmThread {
        thread: t.clone(),
        target: ra.target.clone(),
        call,
    })
}

fn lower_tlm_initiator_cohort(
    group: &[DirectTlmThread],
    module_span: Span,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    if group.len() < 2 {
        return Err(CompileError::general(
            "internal error: TLM cohort lowering requires at least two threads",
            module_span,
        ));
    }
    let first = &group[0];
    let port = first.call.port.clone();
    let method = first.call.method.clone();
    let method_meta = first.call.method_meta.clone();
    let span = first.thread.span;
    let tag_width = if let Some(e) = &method_meta.out_of_order_tags {
        Some(literal_expr_u64(e).ok_or_else(|| {
            CompileError::general(
                "`out_of_order tags` must be a literal width in the first implementation",
                span,
            )
        })? as u32)
    } else {
        None
    };
    let clk = first.thread.clock.clone();
    let rst = first.thread.reset.clone();
    let clock_edge = first.thread.clock_edge;
    let reset_level = first.thread.reset_level;

    for dt in group {
        if dt.thread.clock.name != clk.name
            || dt.thread.reset.name != rst.name
            || dt.thread.clock_edge != clock_edge
            || dt.thread.reset_level != reset_level
        {
            return Err(CompileError::general(
                "TLM generated-thread cohort must use one clock/reset domain in the first implementation",
                dt.thread.span,
            ));
        }
        if dt.call.args.len() != method_meta.args.len() {
            return Err(CompileError::general(
                &format!(
                    "TLM call `{port}.{method}` takes {} args but `tlm_method {}` declares {}",
                    dt.call.args.len(),
                    method,
                    method_meta.args.len()
                ),
                dt.thread.span,
            ));
        }
    }

    let n = group.len();
    if let Some(tag_w) = tag_width {
        let tag_slots = if tag_w >= 64 {
            u128::MAX
        } else {
            1u128 << tag_w
        };
        if tag_slots < n as u128 {
            return Err(CompileError::general(
                &format!(
                    "`{port}.{method}` has {n} workers but only {tag_slots} out-of-order tags; increase `tags` width"
                ),
                span,
            ));
        }
    }
    let idx_w = clog2_width(n as u64);
    let occ_w = clog2_width((n + 1) as u64);
    let prefix = format!("_tlm_pool_{}_{}", port, method);

    let ident = |name: String| Ident { name, span };
    let id = |name: String| Expr::new(ExprKind::Ident(name), span);
    let dec = |v: u64| Expr::new(ExprKind::Literal(LitKind::Dec(v)), span);
    let sized = |w: u32, v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(w, v)), span);
    let zero = || Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
    let bool_lit = |b: bool| Expr::new(ExprKind::Bool(b), span);
    let bin = |op: BinOp, l: Expr, r: Expr| {
        Expr::new(ExprKind::Binary(op, Box::new(l), Box::new(r)), span)
    };
    let not = |e: Expr| Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span);
    let tern = |c: Expr, t: Expr, e: Expr| {
        Expr::new(
            ExprKind::Ternary(Box::new(c), Box::new(t), Box::new(e)),
            span,
        )
    };
    let index =
        |base: Expr, idx: Expr| Expr::new(ExprKind::Index(Box::new(base), Box::new(idx)), span);
    let trunc = |e: Expr, w: u32| {
        Expr::new(
            ExprKind::MethodCall(Box::new(e), ident("trunc".to_string()), vec![dec(w as u64)]),
            span,
        )
    };
    let port_member = |member: String| {
        Expr::new(
            ExprKind::FieldAccess(Box::new(id(port.clone())), ident(member)),
            span,
        )
    };
    let state_name = |i: usize| format!("{prefix}_t{i}_state");
    let fifo_name = format!("{prefix}_fifo");
    let head_name = format!("{prefix}_head");
    let tail_name = format!("{prefix}_tail");
    let occ_name = format!("{prefix}_occ");

    let state_ty = TypeExpr::UInt(Box::new(dec(1)));
    let idx_ty = TypeExpr::UInt(Box::new(dec(idx_w as u64)));
    let occ_ty = TypeExpr::UInt(Box::new(dec(occ_w as u64)));
    let fifo_ty = TypeExpr::Vec(Box::new(idx_ty.clone()), Box::new(dec(n as u64)));
    let mut items: Vec<ModuleBodyItem> = Vec::new();

    for i in 0..n {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(state_name(i)),
            ty: state_ty.clone(),
            init: None,
            reset: RegReset::Inherit(rst.clone(), zero()),
            guard: None,
            multicycle: None,
            span,
        }));
    }
    items.push(ModuleBodyItem::RegDecl(RegDecl {
        name: ident(fifo_name.clone()),
        ty: fifo_ty,
        init: None,
        reset: RegReset::Inherit(rst.clone(), zero()),
        guard: None,
        multicycle: None,
        span,
    }));
    for ptr in [&head_name, &tail_name] {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(ptr.clone()),
            ty: idx_ty.clone(),
            init: None,
            reset: RegReset::Inherit(rst.clone(), zero()),
            guard: None,
            multicycle: None,
            span,
        }));
    }
    items.push(ModuleBodyItem::RegDecl(RegDecl {
        name: ident(occ_name.clone()),
        ty: occ_ty,
        init: None,
        reset: RegReset::Inherit(rst.clone(), zero()),
        guard: None,
        multicycle: None,
        span,
    }));

    let occ_nonzero = bin(BinOp::Gt, id(occ_name.clone()), sized(occ_w, 0));
    let occ_not_full = bin(BinOp::Lt, id(occ_name.clone()), sized(occ_w, n as u64));
    let rsp_pop = bin(
        BinOp::And,
        port_member(format!("{method}_rsp_valid")),
        occ_nonzero.clone(),
    );
    let fifo_head = index(id(fifo_name.clone()), id(head_name.clone()));

    let mut grants: Vec<Expr> = Vec::new();
    let mut wants: Vec<Expr> = Vec::new();
    for i in 0..n {
        let want_i = bin(BinOp::Eq, id(state_name(i)), sized(1, 0));
        let mut grant_i = bin(BinOp::And, want_i.clone(), occ_not_full.clone());
        for prev in &wants {
            grant_i = bin(BinOp::And, grant_i, not(prev.clone()));
        }
        wants.push(want_i);
        grants.push(grant_i);
    }
    let or_expr = |xs: &[Expr]| -> Expr {
        let mut acc = xs.first().cloned().unwrap_or_else(|| bool_lit(false));
        for x in &xs[1..] {
            acc = bin(BinOp::Or, acc, x.clone());
        }
        acc
    };
    let req_valid = or_expr(&grants);
    let req_fire = bin(
        BinOp::And,
        req_valid.clone(),
        port_member(format!("{method}_req_ready")),
    );
    let ptr_inc = |ptr: &str, width: u32| -> Expr {
        tern(
            bin(BinOp::Eq, id(ptr.to_string()), sized(width, (n - 1) as u64)),
            sized(width, 0),
            trunc(bin(BinOp::Add, id(ptr.to_string()), sized(width, 1)), width),
        )
    };

    let mut comb_stmts: Vec<Stmt> = Vec::new();
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method}_req_valid")),
        value: req_valid.clone(),
        span,
    }));
    for (arg_i, (arg_ident, _)) in method_meta.args.iter().enumerate() {
        let mut value = zero();
        for (i, dt) in group.iter().enumerate().rev() {
            value = tern(grants[i].clone(), dt.call.args[arg_i].clone(), value);
        }
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(format!("{}_{}", method, arg_ident.name)),
            value,
            span,
        }));
    }
    if let Some(tag_w) = tag_width {
        let mut value = sized(tag_w, 0);
        for i in (0..n).rev() {
            value = tern(grants[i].clone(), sized(tag_w, i as u64), value);
        }
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(format!("{method}_req_tag")),
            value,
            span,
        }));
    }
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method}_rsp_ready")),
        value: occ_nonzero.clone(),
        span,
    }));

    let mut seq_body: Vec<Stmt> = Vec::new();
    for (i, dt) in group.iter().enumerate() {
        let push_i = bin(
            BinOp::And,
            grants[i].clone(),
            port_member(format!("{method}_req_ready")),
        );
        seq_body.push(Stmt::IfElse(IfElse {
            cond: push_i.clone(),
            then_stmts: vec![
                Stmt::Assign(RegAssign {
                    target: index(id(fifo_name.clone()), id(tail_name.clone())),
                    value: sized(idx_w, i as u64),
                    span,
                }),
                Stmt::Assign(RegAssign {
                    target: id(state_name(i)),
                    value: sized(1, 1),
                    span,
                }),
            ],
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));

        let rsp_i = if let Some(tag_w) = tag_width {
            bin(
                BinOp::And,
                bin(
                    BinOp::And,
                    rsp_pop.clone(),
                    bin(BinOp::Eq, id(state_name(i)), sized(1, 1)),
                ),
                bin(
                    BinOp::Eq,
                    port_member(format!("{method}_rsp_tag")),
                    sized(tag_w, i as u64),
                ),
            )
        } else {
            bin(
                BinOp::And,
                rsp_pop.clone(),
                bin(BinOp::Eq, fifo_head.clone(), sized(idx_w, i as u64)),
            )
        };
        let mut rsp_then: Vec<Stmt> = Vec::new();
        if method_meta.ret.is_some() {
            rsp_then.push(Stmt::Assign(RegAssign {
                target: dt.target.clone(),
                value: port_member(format!("{method}_rsp_data")),
                span,
            }));
        }
        rsp_then.push(Stmt::Assign(RegAssign {
            target: id(state_name(i)),
            value: sized(1, 0),
            span,
        }));
        seq_body.push(Stmt::IfElse(IfElse {
            cond: rsp_i,
            then_stmts: rsp_then,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    seq_body.push(Stmt::IfElse(IfElse {
        cond: req_fire.clone(),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(tail_name.clone()),
            value: ptr_inc(&tail_name, idx_w),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: rsp_pop.clone(),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(head_name.clone()),
            value: ptr_inc(&head_name, idx_w),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: bin(BinOp::And, req_fire.clone(), not(rsp_pop.clone())),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(occ_name.clone()),
            value: trunc(
                bin(BinOp::Add, id(occ_name.clone()), sized(occ_w, 1)),
                occ_w,
            ),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: bin(BinOp::And, rsp_pop.clone(), not(req_fire)),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(occ_name.clone()),
            value: trunc(
                bin(BinOp::Sub, id(occ_name.clone()), sized(occ_w, 1)),
                occ_w,
            ),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));

    items.push(ModuleBodyItem::RegBlock(RegBlock {
        clock: clk,
        clock_edge,
        stmts: seq_body,
        span,
    }));
    items.push(ModuleBodyItem::CombBlock(CombBlock {
        stmts: comb_stmts,
        span,
    }));
    Ok(items)
}

/// Walk a thread body and record spans of any TLM call that is NOT
/// inside a `lock RESOURCE ... end lock` block. Used by the multi-
/// thread sharing diagnostic in `lower_tlm_initiator_calls` — calls
/// wrapped in a lock are considered safely serialized by the existing
/// resource-mutex machinery, so we skip them.
fn collect_bare_tlm_calls(
    stmts: &[ThreadStmt],
    owner_span: Span,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
    out: &mut std::collections::HashMap<(String, String), Vec<Span>>,
) {
    for s in stmts {
        match s {
            ThreadStmt::SeqAssign(ra) => {
                if let Some(call) = match_tlm_call(&ra.value, port_buses, bus_methods) {
                    out.entry((call.port.clone(), call.method.clone()))
                        .or_default()
                        .push(owner_span);
                }
            }
            ThreadStmt::ForkTlmAssign(ra) => {
                if let Some(call) = match_tlm_call(&ra.value, port_buses, bus_methods) {
                    out.entry((call.port.clone(), call.method.clone()))
                        .or_default()
                        .push(owner_span);
                }
            }
            ThreadStmt::Lock { .. } => {
                // TLM calls inside a lock are serialized by the resource
                // mutex — not a multi-driver hazard. Skip.
            }
            ThreadStmt::IfElse(ie) => {
                collect_bare_tlm_calls(&ie.then_stmts, owner_span, port_buses, bus_methods, out);
                collect_bare_tlm_calls(&ie.else_stmts, owner_span, port_buses, bus_methods, out);
            }
            ThreadStmt::ForkJoin(branches, _) => {
                for branch in branches {
                    let branch_span = branch.first().map(thread_stmt_span).unwrap_or(owner_span);
                    collect_bare_tlm_calls(branch, branch_span, port_buses, bus_methods, out);
                }
            }
            ThreadStmt::For { body, .. } => {
                collect_bare_tlm_calls(body, owner_span, port_buses, bus_methods, out);
            }
            ThreadStmt::DoUntil { body, .. } => {
                collect_bare_tlm_calls(body, owner_span, port_buses, bus_methods, out);
            }
            _ => {}
        }
    }
}

fn thread_body_has_tlm_call(
    stmts: &[ThreadStmt],
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::SeqAssign(ra) => {
            contains_tlm_call(&ra.value, port_buses, bus_methods)
                || contains_tlm_call(&ra.target, port_buses, bus_methods)
        }
        ThreadStmt::ForkTlmAssign(ra) => {
            contains_tlm_call(&ra.value, port_buses, bus_methods)
                || contains_tlm_call(&ra.target, port_buses, bus_methods)
        }
        ThreadStmt::CombAssign(ca) => {
            contains_tlm_call(&ca.value, port_buses, bus_methods)
                || contains_tlm_call(&ca.target, port_buses, bus_methods)
        }
        ThreadStmt::WaitUntil(e, _) => contains_tlm_call(e, port_buses, bus_methods),
        ThreadStmt::IfElse(ie) => {
            contains_tlm_call(&ie.cond, port_buses, bus_methods)
                || thread_body_has_tlm_call(&ie.then_stmts, port_buses, bus_methods)
                || thread_body_has_tlm_call(&ie.else_stmts, port_buses, bus_methods)
        }
        ThreadStmt::ForkJoin(branches, _) => branches
            .iter()
            .any(|branch| thread_body_has_tlm_call(branch, port_buses, bus_methods)),
        ThreadStmt::For { body, .. }
        | ThreadStmt::Lock { body, .. }
        | ThreadStmt::DoUntil { body, .. } => {
            thread_body_has_tlm_call(body, port_buses, bus_methods)
        }
        _ => false,
    })
}

fn thread_has_fork_tlm_assign(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::ForkTlmAssign(_) => true,
        ThreadStmt::IfElse(ie) => {
            thread_has_fork_tlm_assign(&ie.then_stmts) || thread_has_fork_tlm_assign(&ie.else_stmts)
        }
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|b| thread_has_fork_tlm_assign(b)),
        ThreadStmt::For { body, .. }
        | ThreadStmt::Lock { body, .. }
        | ThreadStmt::DoUntil { body, .. } => thread_has_fork_tlm_assign(body),
        _ => false,
    })
}

enum TlmInitGroupStateKind {
    Compute {
        seq_on_exit: Vec<Stmt>,
    },
    TlmIssue {
        port: String,
        method: String,
        args: Vec<Expr>,
        method_meta: TlmMethodMeta,
        lock_resource: Option<String>,
    },
    TlmWait {
        port: String,
        method: String,
        method_meta: TlmMethodMeta,
        dest: Option<Expr>,
    },
}

enum TlmInitGroupNext {
    Fallthrough,
    Goto {
        target: usize,
        span: Span,
    },
    Branch {
        cond: Expr,
        then_start: usize,
        else_start: usize,
        span: Span,
    },
    LoopBack {
        counter: String,
        end: Expr,
        body_start: usize,
        span: Span,
    },
}

struct TlmInitGroupState {
    kind: TlmInitGroupStateKind,
    next: TlmInitGroupNext,
}

struct TlmInitThreadPlan {
    thread: ThreadBlock,
    tag: String,
    states: Vec<TlmInitGroupState>,
    loop_counters: Vec<String>,
}

fn build_tlm_init_thread_plan(
    t: ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<TlmInitThreadPlan, CompileError> {
    fn lower_stmts(
        stmts: Vec<ThreadStmt>,
        states: &mut Vec<TlmInitGroupState>,
        pending_seq: &mut Vec<Stmt>,
        loop_counters: &mut Vec<String>,
        tag: &str,
        port_buses: &std::collections::HashMap<String, String>,
        bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
        span: Span,
        current_lock: Option<&str>,
    ) -> Result<(), CompileError> {
        fn push_state(states: &mut Vec<TlmInitGroupState>, kind: TlmInitGroupStateKind) {
            states.push(TlmInitGroupState {
                kind,
                next: TlmInitGroupNext::Fallthrough,
            });
        }

        fn compute_only_thread_stmts_to_seq(
            stmts: Vec<ThreadStmt>,
            port_buses: &std::collections::HashMap<String, String>,
            bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
            span: Span,
        ) -> Result<Vec<Stmt>, CompileError> {
            let mut out = Vec::new();
            for stmt in stmts {
                match stmt {
                    ThreadStmt::SeqAssign(ra) => {
                        if contains_tlm_call(&ra.value, port_buses, bus_methods)
                            || contains_tlm_call(&ra.target, port_buses, bus_methods)
                        {
                            return Err(CompileError::general(
                                "TLM method calls inside `if` branches are not supported in v1 initiator lowering",
                                ra.span,
                            ));
                        }
                        out.push(Stmt::Assign(ra));
                    }
                    ThreadStmt::IfElse(ie) => {
                        let then_stmts = compute_only_thread_stmts_to_seq(
                            ie.then_stmts,
                            port_buses,
                            bus_methods,
                            ie.span,
                        )?;
                        let else_stmts = compute_only_thread_stmts_to_seq(
                            ie.else_stmts,
                            port_buses,
                            bus_methods,
                            ie.span,
                        )?;
                        out.push(Stmt::IfElse(IfElseOf {
                            cond: ie.cond,
                            then_stmts,
                            else_stmts,
                            unique: false,
                            span: ie.span,
                        }));
                    }
                    other => {
                        return Err(CompileError::general(
                            &format!(
                                "v1 TLM initiator compute-only `if` branches only support SeqAssign statements and nested compute-only `if` blocks (found {:?}).",
                                std::mem::discriminant(&other),
                            ),
                            span,
                        ));
                    }
                }
            }
            Ok(out)
        }

        for stmt in stmts {
            match stmt {
                ThreadStmt::SeqAssign(ra) => {
                    if match_tlm_call(&ra.value, port_buses, bus_methods).is_none()
                        && contains_tlm_call(&ra.value, port_buses, bus_methods)
                    {
                        return Err(CompileError::general(
                            "TLM method call must be the direct right-hand side of `<=` in a thread body — nested or composed uses are not supported in v1",
                            ra.span,
                        ));
                    }
                    if contains_tlm_call(&ra.target, port_buses, bus_methods) {
                        return Err(CompileError::general(
                            "TLM method calls cannot appear on the LHS of an assignment",
                            ra.span,
                        ));
                    }
                    if let Some(call) = match_tlm_call(&ra.value, port_buses, bus_methods) {
                        if !pending_seq.is_empty() {
                            push_state(
                                states,
                                TlmInitGroupStateKind::Compute {
                                    seq_on_exit: std::mem::take(pending_seq),
                                },
                            );
                        }
                        let has_ret = call.method_meta.ret.is_some();
                        push_state(
                            states,
                            TlmInitGroupStateKind::TlmIssue {
                                port: call.port.clone(),
                                method: call.method.clone(),
                                args: call.args.clone(),
                                method_meta: call.method_meta.clone(),
                                lock_resource: current_lock.map(|s| s.to_string()),
                            },
                        );
                        push_state(
                            states,
                            TlmInitGroupStateKind::TlmWait {
                                port: call.port,
                                method: call.method,
                                method_meta: call.method_meta.clone(),
                                dest: if has_ret { Some(ra.target) } else { None },
                            },
                        );
                    } else {
                        pending_seq.push(Stmt::Assign(ra));
                    }
                }
                ThreadStmt::Lock { resource, body, .. } => {
                    lower_stmts(
                        body,
                        states,
                        pending_seq,
                        loop_counters,
                        tag,
                        port_buses,
                        bus_methods,
                        span,
                        Some(&resource.name),
                    )?;
                }
                ThreadStmt::IfElse(ie) => {
                    let then_has_tlm =
                        thread_body_has_tlm_call(&ie.then_stmts, port_buses, bus_methods);
                    let else_has_tlm =
                        thread_body_has_tlm_call(&ie.else_stmts, port_buses, bus_methods);
                    if !then_has_tlm && !else_has_tlm {
                        pending_seq.push(Stmt::IfElse(IfElseOf {
                            cond: ie.cond,
                            then_stmts: compute_only_thread_stmts_to_seq(
                                ie.then_stmts,
                                port_buses,
                                bus_methods,
                                ie.span,
                            )?,
                            else_stmts: compute_only_thread_stmts_to_seq(
                                ie.else_stmts,
                                port_buses,
                                bus_methods,
                                ie.span,
                            )?,
                            unique: false,
                            span: ie.span,
                        }));
                        continue;
                    }

                    if !pending_seq.is_empty() {
                        push_state(
                            states,
                            TlmInitGroupStateKind::Compute {
                                seq_on_exit: std::mem::take(pending_seq),
                            },
                        );
                    }

                    let branch_idx = states.len();
                    push_state(
                        states,
                        TlmInitGroupStateKind::Compute {
                            seq_on_exit: Vec::new(),
                        },
                    );

                    let then_start = states.len();
                    let mut then_pending = Vec::new();
                    lower_stmts(
                        ie.then_stmts,
                        states,
                        &mut then_pending,
                        loop_counters,
                        tag,
                        port_buses,
                        bus_methods,
                        span,
                        current_lock,
                    )?;
                    if !then_pending.is_empty() {
                        push_state(
                            states,
                            TlmInitGroupStateKind::Compute {
                                seq_on_exit: std::mem::take(&mut then_pending),
                            },
                        );
                    }
                    let then_end = states.len();

                    let else_start = states.len();
                    let mut else_pending = Vec::new();
                    lower_stmts(
                        ie.else_stmts,
                        states,
                        &mut else_pending,
                        loop_counters,
                        tag,
                        port_buses,
                        bus_methods,
                        span,
                        current_lock,
                    )?;
                    if !else_pending.is_empty() {
                        push_state(
                            states,
                            TlmInitGroupStateKind::Compute {
                                seq_on_exit: std::mem::take(&mut else_pending),
                            },
                        );
                    }
                    let else_end = states.len();

                    let join_idx = states.len();
                    push_state(
                        states,
                        TlmInitGroupStateKind::Compute {
                            seq_on_exit: Vec::new(),
                        },
                    );

                    if let Some(branch_state) = states.get_mut(branch_idx) {
                        branch_state.next = TlmInitGroupNext::Branch {
                            cond: ie.cond,
                            then_start: if then_end > then_start {
                                then_start
                            } else {
                                join_idx
                            },
                            else_start: if else_end > else_start {
                                else_start
                            } else {
                                join_idx
                            },
                            span: ie.span,
                        };
                    }
                    if then_end > then_start {
                        if let Some(last_then) = states.get_mut(then_end - 1) {
                            last_then.next = TlmInitGroupNext::Goto {
                                target: join_idx,
                                span: ie.span,
                            };
                        }
                    }
                    if else_end > else_start {
                        if let Some(last_else) = states.get_mut(else_end - 1) {
                            last_else.next = TlmInitGroupNext::Goto {
                                target: join_idx,
                                span: ie.span,
                            };
                        }
                    }
                }
                ThreadStmt::For {
                    var,
                    start,
                    end,
                    body,
                    span: for_span,
                } => match (literal_expr_u64(&start), literal_expr_u64(&end)) {
                    (Some(start_v), Some(end_v)) => {
                        if end_v < start_v {
                            return Err(CompileError::general(
                                "TLM initiator `for` loop end must be >= start",
                                for_span,
                            ));
                        }
                        for i in start_v..=end_v {
                            let expanded: Vec<ThreadStmt> = body
                                .iter()
                                .map(|s| subst_thread_stmt(s, &var.name, i as i64))
                                .map(fold_literal_bit_slices_thread_stmt)
                                .collect();
                            lower_stmts(
                                expanded,
                                states,
                                pending_seq,
                                loop_counters,
                                tag,
                                port_buses,
                                bus_methods,
                                span,
                                current_lock,
                            )?;
                        }
                    }
                    _ => {
                        if !pending_seq.is_empty() {
                            push_state(
                                states,
                                TlmInitGroupStateKind::Compute {
                                    seq_on_exit: std::mem::take(pending_seq),
                                },
                            );
                        }

                        let counter = format!("_tlm_init_{}_loop_cnt_{}", tag, loop_counters.len());
                        loop_counters.push(counter.clone());
                        push_state(
                            states,
                            TlmInitGroupStateKind::Compute {
                                seq_on_exit: vec![Stmt::Assign(RegAssign {
                                    target: Expr::new(ExprKind::Ident(counter.clone()), for_span),
                                    value: start.clone(),
                                    span: for_span,
                                })],
                            },
                        );

                        let body_start = states.len();
                        let rewritten: Vec<ThreadStmt> = body
                            .iter()
                            .map(|s| rewrite_loop_var(s, &var.name, &counter))
                            .collect();
                        let mut body_pending = Vec::new();
                        lower_stmts(
                            rewritten,
                            states,
                            &mut body_pending,
                            loop_counters,
                            tag,
                            port_buses,
                            bus_methods,
                            span,
                            current_lock,
                        )?;
                        if !body_pending.is_empty() {
                            push_state(
                                states,
                                TlmInitGroupStateKind::Compute {
                                    seq_on_exit: std::mem::take(&mut body_pending),
                                },
                            );
                        }
                        if states.len() == body_start {
                            return Err(CompileError::general(
                                    "TLM initiator runtime `for` loop body must lower to at least one state",
                                    for_span,
                                ));
                        }
                        if let Some(last) = states.last_mut() {
                            last.next = TlmInitGroupNext::LoopBack {
                                counter,
                                end: end.clone(),
                                body_start,
                                span: for_span,
                            };
                        }
                    }
                },
                other => {
                    return Err(CompileError::general(
                        &format!(
                            "v1 TLM initiator thread body only supports SeqAssign statements, serialized `for` loops, compute-only `if` branches, and `lock` blocks around them (found {:?}). Refactor more complex control flow into a `thread` without TLM calls.",
                            std::mem::discriminant(&other),
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(())
    }

    let span = t.span;
    let tag = t
        .name
        .as_ref()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "tlm_init".to_string());
    let mut states = Vec::new();
    let mut pending_seq = Vec::new();
    let mut loop_counters = Vec::new();
    lower_stmts(
        t.body.clone(),
        &mut states,
        &mut pending_seq,
        &mut loop_counters,
        &tag,
        port_buses,
        bus_methods,
        span,
        None,
    )?;
    if !pending_seq.is_empty() {
        states.push(TlmInitGroupState {
            kind: TlmInitGroupStateKind::Compute {
                seq_on_exit: std::mem::take(&mut pending_seq),
            },
            next: TlmInitGroupNext::Fallthrough,
        });
    }
    Ok(TlmInitThreadPlan {
        thread: t,
        tag,
        states,
        loop_counters,
    })
}

fn inline_lower_tlm_initiator_group(
    threads: Vec<ThreadBlock>,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
    resource_decls: &std::collections::HashMap<String, ResourceDecl>,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let mut plans = Vec::new();
    for t in threads {
        plans.push(build_tlm_init_thread_plan(t, port_buses, bus_methods)?);
    }
    plans.retain(|p| !p.states.is_empty());
    if plans.is_empty() {
        return Ok(Vec::new());
    }

    let span = plans[0].thread.span;
    let clk = plans[0].thread.clock.clone();
    let rst = plans[0].thread.reset.clone();
    let clock_edge = plans[0].thread.clock_edge;
    let reset_level = plans[0].thread.reset_level;
    for p in &plans {
        if p.thread.clock.name != clk.name
            || p.thread.reset.name != rst.name
            || p.thread.clock_edge != clock_edge
            || p.thread.reset_level != reset_level
        {
            return Err(CompileError::general(
                "TLM initiator thread group must use one clock/reset domain",
                p.thread.span,
            ));
        }
    }

    let mk_ident = |name: String| Ident { name, span };
    let id = |name: String| Expr::new(ExprKind::Ident(name), span);
    let dec = |v: u64| Expr::new(ExprKind::Literal(LitKind::Dec(v)), span);
    let sized = |w: u32, v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(w, v)), span);
    let bin = |op: BinOp, l: Expr, r: Expr| {
        Expr::new(ExprKind::Binary(op, Box::new(l), Box::new(r)), span)
    };
    let tern = |c: Expr, t: Expr, e: Expr| {
        Expr::new(
            ExprKind::Ternary(Box::new(c), Box::new(t), Box::new(e)),
            span,
        )
    };
    let port_member = |port: &str, member: String| {
        Expr::new(
            ExprKind::FieldAccess(Box::new(id(port.to_string())), mk_ident(member)),
            span,
        )
    };

    struct GroupAgg {
        port: String,
        method: String,
        arg_decls: Vec<(Ident, TypeExpr)>,
        tag_width: Option<Expr>,
        issues: Vec<(Expr, Vec<Expr>, Expr, Expr, Option<String>)>,
        waits: Vec<Expr>,
    }

    let mut items = Vec::new();
    let mut seq_body = Vec::new();
    let mut aggs: std::collections::BTreeMap<String, GroupAgg> = std::collections::BTreeMap::new();

    for plan in &plans {
        let total_states = plan.states.len();
        let state_width = clog2_width(total_states as u64);
        let state_reg_name = format!("_tlm_init_{}_state", plan.tag);
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: mk_ident(state_reg_name.clone()),
            ty: TypeExpr::UInt(Box::new(dec(state_width as u64))),
            init: None,
            reset: RegReset::Inherit(rst.clone(), dec(0)),
            guard: None,
            multicycle: None,
            span,
        }));
        for counter in &plan.loop_counters {
            items.push(ModuleBodyItem::RegDecl(RegDecl {
                name: mk_ident(counter.clone()),
                ty: TypeExpr::UInt(Box::new(dec(32))),
                init: Some(dec(0)),
                reset: RegReset::None,
                guard: None,
                multicycle: None,
                span,
            }));
        }
        let state_expr = id(state_reg_name.clone());
        let state_lit = |v: u64| sized(state_width, v);
        let state_eq = |v: u64| bin(BinOp::Eq, state_expr.clone(), state_lit(v));
        let loop_transition_stmts =
            |next: &TlmInitGroupNext, normal_next_idx: u64, state_expr: Expr| -> Vec<Stmt> {
                match next {
                    TlmInitGroupNext::Fallthrough => vec![Stmt::Assign(RegAssign {
                        target: state_expr,
                        value: state_lit(normal_next_idx),
                        span,
                    })],
                    TlmInitGroupNext::Goto {
                        target,
                        span: goto_span,
                    } => vec![Stmt::Assign(RegAssign {
                        target: state_expr,
                        value: state_lit(*target as u64),
                        span: *goto_span,
                    })],
                    TlmInitGroupNext::Branch {
                        cond,
                        then_start,
                        else_start,
                        span: branch_span,
                    } => {
                        vec![Stmt::IfElse(IfElseOf {
                            cond: cond.clone(),
                            then_stmts: vec![Stmt::Assign(RegAssign {
                                target: state_expr.clone(),
                                value: state_lit(*then_start as u64),
                                span: *branch_span,
                            })],
                            else_stmts: vec![Stmt::Assign(RegAssign {
                                target: state_expr,
                                value: state_lit(*else_start as u64),
                                span: *branch_span,
                            })],
                            unique: false,
                            span: *branch_span,
                        })]
                    }
                    TlmInitGroupNext::LoopBack {
                        counter,
                        end,
                        body_start,
                        span: loop_span,
                    } => {
                        let counter_expr = id(counter.clone());
                        let inc_expr = bin(BinOp::AddWrap, counter_expr.clone(), sized(32, 1));
                        let end_w = Expr::new(
                            ExprKind::MethodCall(
                                Box::new(end.clone()),
                                mk_ident("resize".to_string()),
                                vec![dec(32)],
                            ),
                            *loop_span,
                        );
                        let loop_cond = bin(BinOp::Lt, counter_expr.clone(), end_w);
                        let inc_stmt = || {
                            Stmt::Assign(RegAssign {
                                target: counter_expr.clone(),
                                value: inc_expr.clone(),
                                span: *loop_span,
                            })
                        };
                        vec![Stmt::IfElse(IfElseOf {
                            cond: loop_cond,
                            then_stmts: vec![
                                inc_stmt(),
                                Stmt::Assign(RegAssign {
                                    target: state_expr.clone(),
                                    value: state_lit(*body_start as u64),
                                    span: *loop_span,
                                }),
                            ],
                            else_stmts: vec![
                                inc_stmt(),
                                Stmt::Assign(RegAssign {
                                    target: state_expr,
                                    value: state_lit(normal_next_idx),
                                    span: *loop_span,
                                }),
                            ],
                            unique: false,
                            span: *loop_span,
                        })]
                    }
                }
            };

        for (i, state) in plan.states.iter().enumerate() {
            let cur_idx = i as u64;
            let next_idx = ((i + 1) % total_states) as u64;
            match &state.kind {
                TlmInitGroupStateKind::Compute { seq_on_exit } => {
                    let mut then_stmts = seq_on_exit.clone();
                    then_stmts.extend(loop_transition_stmts(
                        &state.next,
                        next_idx,
                        state_expr.clone(),
                    ));
                    seq_body.push(Stmt::IfElse(IfElseOf {
                        cond: state_eq(cur_idx),
                        then_stmts,
                        else_stmts: Vec::new(),
                        unique: false,
                        span,
                    }));
                }
                TlmInitGroupStateKind::TlmIssue {
                    port,
                    method,
                    args,
                    method_meta,
                    lock_resource,
                } => {
                    aggs.entry(format!("{port}.{method}"))
                        .or_insert_with(|| GroupAgg {
                            port: port.clone(),
                            method: method.clone(),
                            arg_decls: method_meta.args.clone(),
                            tag_width: method_meta.out_of_order_tags.clone(),
                            issues: Vec::new(),
                            waits: Vec::new(),
                        })
                        .issues
                        .push((
                            state_eq(cur_idx),
                            args.clone(),
                            state_expr.clone(),
                            state_lit(next_idx),
                            lock_resource.clone(),
                        ));
                }
                TlmInitGroupStateKind::TlmWait {
                    port,
                    method,
                    method_meta,
                    dest,
                } => {
                    aggs.entry(format!("{port}.{method}"))
                        .or_insert_with(|| GroupAgg {
                            port: port.clone(),
                            method: method.clone(),
                            arg_decls: method_meta.args.clone(),
                            tag_width: method_meta.out_of_order_tags.clone(),
                            issues: Vec::new(),
                            waits: Vec::new(),
                        })
                        .waits
                        .push(state_eq(cur_idx));
                    let mut then_stmts = Vec::new();
                    if let Some(dest_expr) = dest {
                        then_stmts.push(Stmt::Assign(RegAssign {
                            target: dest_expr.clone(),
                            value: port_member(port, format!("{method}_rsp_data")),
                            span,
                        }));
                    }
                    then_stmts.extend(loop_transition_stmts(
                        &state.next,
                        next_idx,
                        state_expr.clone(),
                    ));
                    let mut advance_rhs = port_member(port, format!("{method}_rsp_valid"));
                    if let Some(tag_w_expr) = &method_meta.out_of_order_tags {
                        let tag_w = literal_expr_u64(tag_w_expr)
                            .ok_or_else(|| {
                                CompileError::general(
                                    "`out_of_order tags` must be a literal width in the first implementation",
                                    tag_w_expr.span,
                                )
                            })? as u32;
                        advance_rhs = bin(
                            BinOp::And,
                            advance_rhs,
                            bin(
                                BinOp::Eq,
                                port_member(port, format!("{method}_rsp_tag")),
                                sized(tag_w, 0),
                            ),
                        );
                    }
                    seq_body.push(Stmt::IfElse(IfElseOf {
                        cond: bin(BinOp::And, state_eq(cur_idx), advance_rhs),
                        then_stmts,
                        else_stmts: Vec::new(),
                        unique: false,
                        span,
                    }));
                }
            }
        }
    }

    let or_expr = |exprs: &[Expr]| -> Expr {
        let mut acc = exprs
            .first()
            .cloned()
            .unwrap_or_else(|| Expr::new(ExprKind::Bool(false), span));
        for e in &exprs[1..] {
            acc = Expr::new(
                ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(e.clone())),
                span,
            );
        }
        acc
    };
    let emit_bool_or_tree = |prefix: &str,
                             exprs: &[Expr],
                             items: &mut Vec<ModuleBodyItem>,
                             comb_stmts: &mut Vec<Stmt>|
     -> Expr {
        const CHUNK: usize = 8;
        if exprs.is_empty() {
            return Expr::new(ExprKind::Bool(false), span);
        }
        let mut level = 0usize;
        let mut cur = exprs.to_vec();
        while cur.len() > CHUNK {
            let mut next = Vec::new();
            for (chunk_i, chunk) in cur.chunks(CHUNK).enumerate() {
                let wire_name = format!("{prefix}_or_l{level}_{chunk_i}");
                items.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: mk_ident(wire_name.clone()),
                    ty: TypeExpr::Bool,
                    unpacked: false,
                    unpacked_ascending: false,
                    span,
                }));
                comb_stmts.push(Stmt::Assign(CombAssign {
                    target: id(wire_name.clone()),
                    value: or_expr(chunk),
                    span,
                }));
                next.push(id(wire_name));
            }
            cur = next;
            level += 1;
        }
        or_expr(&cur)
    };
    let emit_bool_and_tree = |prefix: &str,
                              exprs: &[Expr],
                              items: &mut Vec<ModuleBodyItem>,
                              comb_stmts: &mut Vec<Stmt>|
     -> Expr {
        const CHUNK: usize = 8;
        if exprs.is_empty() {
            return Expr::new(ExprKind::Bool(true), span);
        }
        let mut level = 0usize;
        let mut cur = exprs.to_vec();
        while cur.len() > CHUNK {
            let mut next = Vec::new();
            for (chunk_i, chunk) in cur.chunks(CHUNK).enumerate() {
                let wire_name = format!("{prefix}_and_l{level}_{chunk_i}");
                items.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: mk_ident(wire_name.clone()),
                    ty: TypeExpr::Bool,
                    unpacked: false,
                    unpacked_ascending: false,
                    span,
                }));
                let mut value = chunk
                    .first()
                    .cloned()
                    .unwrap_or_else(|| Expr::new(ExprKind::Bool(true), span));
                for e in &chunk[1..] {
                    value = bin(BinOp::And, value, e.clone());
                }
                comb_stmts.push(Stmt::Assign(CombAssign {
                    target: id(wire_name.clone()),
                    value,
                    span,
                }));
                next.push(id(wire_name));
            }
            cur = next;
            level += 1;
        }
        let mut value = cur
            .first()
            .cloned()
            .unwrap_or_else(|| Expr::new(ExprKind::Bool(true), span));
        for e in &cur[1..] {
            value = bin(BinOp::And, value, e.clone());
        }
        value
    };
    let emit_mux_tree = |prefix: &str,
                         ty: &TypeExpr,
                         pairs: &[(Expr, Expr)],
                         default: Expr,
                         items: &mut Vec<ModuleBodyItem>,
                         comb_stmts: &mut Vec<Stmt>|
     -> Expr {
        const CHUNK: usize = 8;
        if pairs.is_empty() {
            return default;
        }
        let mut level = 0usize;
        let mut cur = pairs.to_vec();
        while cur.len() > CHUNK {
            let mut next = Vec::new();
            for (chunk_i, chunk) in cur.chunks(CHUNK).enumerate() {
                let valid_name = format!("{prefix}_mux_valid_l{level}_{chunk_i}");
                let data_name = format!("{prefix}_mux_data_l{level}_{chunk_i}");
                items.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: mk_ident(valid_name.clone()),
                    ty: TypeExpr::Bool,
                    unpacked: false,
                    unpacked_ascending: false,
                    span,
                }));
                items.push(ModuleBodyItem::WireDecl(WireDecl {
                    bus_params: Vec::new(),
                    name: mk_ident(data_name.clone()),
                    ty: ty.clone(),
                    unpacked: false,
                    unpacked_ascending: false,
                    span,
                }));
                let valid_inputs: Vec<Expr> = chunk.iter().map(|(sel, _)| sel.clone()).collect();
                comb_stmts.push(Stmt::Assign(CombAssign {
                    target: id(valid_name.clone()),
                    value: or_expr(&valid_inputs),
                    span,
                }));
                let mut value = default.clone();
                for (sel, data) in chunk.iter().rev() {
                    value = tern(sel.clone(), data.clone(), value);
                }
                comb_stmts.push(Stmt::Assign(CombAssign {
                    target: id(data_name.clone()),
                    value,
                    span,
                }));
                next.push((id(valid_name), id(data_name)));
            }
            cur = next;
            level += 1;
        }
        let mut value = default;
        for (sel, data) in cur.iter().rev() {
            value = tern(sel.clone(), data.clone(), value);
        }
        value
    };

    let mut comb_stmts = Vec::new();
    for (_, agg) in &aggs {
        let issue_conds: Vec<Expr> = agg.issues.iter().map(|(c, _, _, _, _)| c.clone()).collect();
        let mut want_refs: Vec<Expr> = Vec::new();
        for (i, cond) in issue_conds.iter().enumerate() {
            let want_name = format!("_tlm_init_{}_{}_want_{}", agg.port, agg.method, i);
            items.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: mk_ident(want_name.clone()),
                ty: TypeExpr::Bool,
                unpacked: false,
                unpacked_ascending: false,
                span,
            }));
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: id(want_name.clone()),
                value: cond.clone(),
                span,
            }));
            want_refs.push(id(want_name));
        }
        let rr_resource = agg
            .issues
            .iter()
            .filter_map(|(_, _, _, _, r)| r.as_ref())
            .find(|name| {
                resource_decls
                    .get(*name)
                    .map(|rd| matches!(rd.policy, ArbiterPolicy::RoundRobin))
                    .unwrap_or(false)
            });
        let use_round_robin = rr_resource.is_some() && agg.issues.len() > 1;
        let grant_exprs: Vec<Expr> = if use_round_robin {
            let n = agg.issues.len();
            let ptr_w = clog2_width(n as u64);
            let rr_name = format!("_tlm_init_{}_{}_rr_ptr", agg.port, agg.method);
            items.push(ModuleBodyItem::RegDecl(RegDecl {
                name: mk_ident(rr_name.clone()),
                ty: TypeExpr::UInt(Box::new(dec(ptr_w as u64))),
                init: None,
                reset: RegReset::Inherit(rst.clone(), dec(0)),
                guard: None,
                multicycle: None,
                span,
            }));
            let rr_id = id(rr_name.clone());
            let mut grant_terms_by_i: Vec<Vec<Expr>> = vec![Vec::new(); n];
            for start in 0..n {
                let rr_eq_start = bin(BinOp::Eq, rr_id.clone(), sized(ptr_w, start as u64));
                for offset in 0..n {
                    let i = (start + offset) % n;
                    let mut term_inputs = vec![rr_eq_start.clone(), want_refs[i].clone()];
                    let mut j = start;
                    while j != i {
                        term_inputs.push(Expr::new(
                            ExprKind::Unary(UnaryOp::Not, Box::new(want_refs[j].clone())),
                            span,
                        ));
                        j = (j + 1) % n;
                    }
                    let term = emit_bool_and_tree(
                        &format!("_tlm_init_{}_{}_rr_s{}_g{}", agg.port, agg.method, start, i),
                        &term_inputs,
                        &mut items,
                        &mut comb_stmts,
                    );
                    grant_terms_by_i[i].push(term);
                }
            }
            grant_terms_by_i
                .into_iter()
                .enumerate()
                .map(|(i, terms)| {
                    emit_bool_or_tree(
                        &format!("_tlm_init_{}_{}_rr_grant_{}", agg.port, agg.method, i),
                        &terms,
                        &mut items,
                        &mut comb_stmts,
                    )
                })
                .collect()
        } else {
            let mut grants = Vec::new();
            let mut prev_taken = Expr::new(ExprKind::Bool(false), span);
            for (i, want) in want_refs.iter().enumerate() {
                let grant = bin(
                    BinOp::And,
                    want.clone(),
                    Expr::new(
                        ExprKind::Unary(UnaryOp::Not, Box::new(prev_taken.clone())),
                        span,
                    ),
                );
                grants.push(grant);
                if i + 1 < want_refs.len() {
                    let taken_name = format!("_tlm_init_{}_{}_taken_{}", agg.port, agg.method, i);
                    items.push(ModuleBodyItem::WireDecl(WireDecl {
                        bus_params: Vec::new(),
                        name: mk_ident(taken_name.clone()),
                        ty: TypeExpr::Bool,
                        unpacked: false,
                        unpacked_ascending: false,
                        span,
                    }));
                    comb_stmts.push(Stmt::Assign(CombAssign {
                        target: id(taken_name.clone()),
                        value: bin(BinOp::Or, prev_taken, want.clone()),
                        span,
                    }));
                    prev_taken = id(taken_name);
                }
            }
            grants
        };
        let mut grants: Vec<Expr> = Vec::new();
        for (i, grant_expr) in grant_exprs.iter().enumerate() {
            let grant_name = format!("_tlm_init_{}_{}_grant_{}", agg.port, agg.method, i);
            items.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: mk_ident(grant_name.clone()),
                ty: TypeExpr::Bool,
                unpacked: false,
                unpacked_ascending: false,
                span,
            }));
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: id(grant_name.clone()),
                value: grant_expr.clone(),
                span,
            }));
            grants.push(id(grant_name));
        }
        if use_round_robin {
            let n = agg.issues.len();
            let ptr_w = clog2_width(n as u64);
            let rr_name = format!("_tlm_init_{}_{}_rr_ptr", agg.port, agg.method);
            let rr_id = id(rr_name);
            let req_fire = bin(
                BinOp::And,
                or_expr(&grants),
                port_member(&agg.port, format!("{}_req_ready", agg.method)),
            );
            let mut rr_then = Vec::new();
            for (i, grant) in grants.iter().enumerate() {
                let next = if i + 1 == n { 0 } else { i + 1 };
                rr_then.push(Stmt::IfElse(IfElseOf {
                    cond: grant.clone(),
                    then_stmts: vec![Stmt::Assign(RegAssign {
                        target: rr_id.clone(),
                        value: sized(ptr_w, next as u64),
                        span,
                    })],
                    else_stmts: Vec::new(),
                    unique: false,
                    span,
                }));
            }
            seq_body.push(Stmt::IfElse(IfElseOf {
                cond: req_fire,
                then_stmts: rr_then,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
        for (grant, (_, _, state_expr, next_state, _)) in grants.iter().zip(agg.issues.iter()) {
            let advance_cond = bin(
                BinOp::And,
                grant.clone(),
                port_member(&agg.port, format!("{}_req_ready", agg.method)),
            );
            seq_body.push(Stmt::IfElse(IfElseOf {
                cond: advance_cond,
                then_stmts: vec![Stmt::Assign(RegAssign {
                    target: state_expr.clone(),
                    value: next_state.clone(),
                    span,
                })],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
        let req_valid_value = emit_bool_or_tree(
            &format!("_tlm_init_{}_{}_req_valid", agg.port, agg.method),
            &grants,
            &mut items,
            &mut comb_stmts,
        );
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(&agg.port, format!("{}_req_valid", agg.method)),
            value: req_valid_value,
            span,
        }));
        for (arg_i, (arg_ident, arg_ty)) in agg.arg_decls.iter().enumerate() {
            let pairs: Vec<(Expr, Expr)> = grants
                .iter()
                .zip(agg.issues.iter())
                .filter_map(|(grant, (_, args, _, _, _))| {
                    args.get(arg_i).map(|arg| (grant.clone(), arg.clone()))
                })
                .collect();
            let arg_value = emit_mux_tree(
                &format!("_tlm_init_{}_{}_{}", agg.port, agg.method, arg_ident.name),
                arg_ty,
                &pairs,
                dec(0),
                &mut items,
                &mut comb_stmts,
            );
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: port_member(&agg.port, format!("{}_{}", agg.method, arg_ident.name)),
                value: arg_value,
                span,
            }));
        }
        if let Some(tag_w_expr) = &agg.tag_width {
            let tag_w = literal_expr_u64(tag_w_expr).unwrap_or(1) as u32;
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: port_member(&agg.port, format!("{}_req_tag", agg.method)),
                value: sized(tag_w, 0),
                span,
            }));
        }
        let rsp_ready_value = emit_bool_or_tree(
            &format!("_tlm_init_{}_{}_rsp_ready", agg.port, agg.method),
            &agg.waits,
            &mut items,
            &mut comb_stmts,
        );
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(&agg.port, format!("{}_rsp_ready", agg.method)),
            value: rsp_ready_value,
            span,
        }));
    }

    items.push(ModuleBodyItem::RegBlock(RegBlock {
        clock: clk,
        clock_edge,
        stmts: seq_body,
        span,
    }));
    items.push(ModuleBodyItem::CombBlock(CombBlock {
        stmts: comb_stmts,
        span,
    }));
    Ok(items)
}

#[derive(Clone)]
struct ForkedTlmIssue {
    delay: u64,
    target: Expr,
    call: TlmCall,
    span: Span,
}

struct ForkJoinAllPlan {
    issues: Vec<ForkedTlmIssue>,
    tail_stmts: Vec<Stmt>,
}

fn fork_join_tail_to_seq_stmt(
    stmt: &ThreadStmt,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<Stmt, CompileError> {
    match stmt {
        ThreadStmt::SeqAssign(ra) => {
            if contains_tlm_call(&ra.target, port_buses, bus_methods)
                || contains_tlm_call(&ra.value, port_buses, bus_methods)
            {
                return Err(CompileError::general(
                    "RHS-fork TLM tail after `join all;` cannot contain TLM method calls",
                    ra.span,
                ));
            }
            Ok(Stmt::Assign(ra.clone()))
        }
        ThreadStmt::IfElse(ie) => {
            if contains_tlm_call(&ie.cond, port_buses, bus_methods) {
                return Err(CompileError::general(
                    "RHS-fork TLM tail condition after `join all;` cannot contain TLM method calls",
                    ie.span,
                ));
            }
            let then_stmts = ie.then_stmts.iter()
                .map(|s| fork_join_tail_to_seq_stmt(s, port_buses, bus_methods))
                .collect::<Result<Vec<_>, _>>()?;
            let else_stmts = ie.else_stmts.iter()
                .map(|s| fork_join_tail_to_seq_stmt(s, port_buses, bus_methods))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Stmt::IfElse(IfElse {
                cond: ie.cond.clone(),
                then_stmts,
                else_stmts,
                unique: ie.unique,
                span: ie.span,
            }))
        }
        ThreadStmt::ForkTlmAssign(ra) => Err(CompileError::general(
            "`target <= fork port.method(...);` cannot appear after `join all;` in an RHS-fork TLM group",
            ra.span,
        )),
        other => Err(CompileError::general(
            &format!(
                "RHS-fork TLM tail after `join all;` supports only sequential assignments and compute-only `if` branches (found {:?})",
                std::mem::discriminant(other),
            ),
            thread_stmt_span(other),
        )),
    }
}

fn collect_fork_join_all_plan(
    t: &ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<ForkJoinAllPlan, CompileError> {
    let mut delay = 0u64;
    let mut issues = Vec::new();
    let mut tail_stmts = Vec::new();
    let mut saw_join = false;
    for stmt in &t.body {
        match stmt {
            ThreadStmt::ForkTlmAssign(ra) => {
                if saw_join {
                    return Err(CompileError::general(
                        "`target <= fork port.method(...);` cannot appear after `join all;` in v1",
                        ra.span,
                    ));
                }
                let Some(call) = match_tlm_call(&ra.value, port_buses, bus_methods) else {
                    return Err(CompileError::general(
                        "`fork` on the RHS of `<=` is only supported for direct TLM method calls, e.g. `dst <= fork bus.read(addr);`",
                        ra.span,
                    ));
                };
                if contains_tlm_call(&ra.target, port_buses, bus_methods) {
                    return Err(CompileError::general(
                        "TLM method calls cannot appear on the LHS of a forked TLM assignment",
                        ra.span,
                    ));
                }
                issues.push(ForkedTlmIssue {
                    delay,
                    target: ra.target.clone(),
                    call,
                    span: ra.span,
                });
            }
            ThreadStmt::WaitCycles(n, sp) => {
                if saw_join {
                    return Err(CompileError::general(
                        "RHS-fork TLM tail after `join all;` is compute-only; `wait N cycle;` is not supported there",
                        *sp,
                    ));
                }
                let Some(v) = literal_expr_u64(n) else {
                    return Err(CompileError::general(
                        "v1 forked TLM issue offsets require a literal `wait N cycle;` count",
                        n.span,
                    ));
                };
                delay = delay.saturating_add(v);
            }
            ThreadStmt::JoinAll(sp) => {
                if saw_join {
                    return Err(CompileError::general(
                        "duplicate `join all;` in forked TLM group",
                        *sp,
                    ));
                }
                saw_join = true;
            }
            other => {
                if saw_join {
                    tail_stmts.push(fork_join_tail_to_seq_stmt(other, port_buses, bus_methods)?);
                    continue;
                }
                return Err(CompileError::general(
                    &format!(
                        "v1 RHS-fork TLM groups only support `target <= fork port.method(...);`, literal `wait N cycle;`, `join all;`, and an optional compute-only tail (found {:?})",
                        std::mem::discriminant(other),
                    ),
                    thread_stmt_span(other),
                ));
            }
        }
    }
    if issues.is_empty() {
        return Err(CompileError::general(
            "`join all;` has no preceding forked TLM calls",
            t.span,
        ));
    }
    if !saw_join {
        return Err(CompileError::general(
            "forked TLM calls require an explicit `join all;` barrier",
            t.span,
        ));
    }
    Ok(ForkJoinAllPlan { issues, tail_stmts })
}

fn inline_lower_tlm_fork_join_all(
    t: ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let plan = collect_fork_join_all_plan(&t, port_buses, bus_methods)?;
    let issues = plan.issues;
    let tail_stmts = plan.tail_stmts;
    let first = &issues[0];
    let port = first.call.port.clone();
    let method = first.call.method.clone();
    let method_meta = first.call.method_meta.clone();
    let span = t.span;
    for issue in &issues {
        if issue.call.port != port || issue.call.method != method {
            return Err(CompileError::general(
                "v1 forked TLM groups must target one method; split different methods into separate threads",
                issue.span,
            ));
        }
        if issue.call.args.len() != method_meta.args.len() {
            return Err(CompileError::general(
                &format!(
                    "TLM call `{port}.{method}` takes {} args but `tlm_method {}` declares {}",
                    issue.call.args.len(),
                    method,
                    method_meta.args.len()
                ),
                issue.span,
            ));
        }
    }
    let n = issues.len();
    let tag_width = if let Some(e) = &method_meta.out_of_order_tags {
        Some(literal_expr_u64(e).ok_or_else(|| {
            CompileError::general(
                "`out_of_order tags` must be a literal width in the first implementation",
                e.span,
            )
        })? as u32)
    } else {
        None
    };
    if let Some(tag_w) = tag_width {
        let tag_slots = if tag_w >= 64 {
            u128::MAX
        } else {
            1u128 << tag_w
        };
        if tag_slots < n as u128 {
            return Err(CompileError::general(
                &format!("`{port}.{method}` has {n} forked calls but only {tag_slots} out-of-order tags; increase `tags` width"),
                span,
            ));
        }
    }

    let max_delay = issues.iter().map(|i| i.delay).max().unwrap_or(0);
    let idx_w = clog2_width(n as u64);
    let occ_w = clog2_width((n + 1) as u64);
    let age_w = clog2_width((max_delay + 2).max(2));
    let tag = t
        .name
        .as_ref()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "anon".to_string());
    let prefix = format!("_tlm_fork_{}_{}_{}", tag, port, method);

    let ident = |name: String| Ident { name, span };
    let id = |name: String| Expr::new(ExprKind::Ident(name), span);
    let dec = |v: u64| Expr::new(ExprKind::Literal(LitKind::Dec(v)), span);
    let sized = |w: u32, v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(w, v)), span);
    let zero = || Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
    let bool_lit = |b: bool| Expr::new(ExprKind::Bool(b), span);
    let bin = |op: BinOp, l: Expr, r: Expr| {
        Expr::new(ExprKind::Binary(op, Box::new(l), Box::new(r)), span)
    };
    let not = |e: Expr| Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span);
    let tern = |c: Expr, t: Expr, e: Expr| {
        Expr::new(
            ExprKind::Ternary(Box::new(c), Box::new(t), Box::new(e)),
            span,
        )
    };
    let index =
        |base: Expr, idx: Expr| Expr::new(ExprKind::Index(Box::new(base), Box::new(idx)), span);
    let trunc = |e: Expr, w: u32| {
        Expr::new(
            ExprKind::MethodCall(Box::new(e), ident("trunc".to_string()), vec![dec(w as u64)]),
            span,
        )
    };
    let port_member = |member: String| {
        Expr::new(
            ExprKind::FieldAccess(Box::new(id(port.clone())), ident(member)),
            span,
        )
    };
    let state_name = |i: usize| format!("{prefix}_t{i}_state");
    let fifo_name = format!("{prefix}_fifo");
    let head_name = format!("{prefix}_head");
    let tail_name = format!("{prefix}_tail");
    let occ_name = format!("{prefix}_occ");
    let age_name = format!("{prefix}_age");
    let tail_done_name = format!("{prefix}_tail_done");

    let idx_ty = TypeExpr::UInt(Box::new(dec(idx_w as u64)));
    let occ_ty = TypeExpr::UInt(Box::new(dec(occ_w as u64)));
    let age_ty = TypeExpr::UInt(Box::new(dec(age_w as u64)));
    let state_ty = TypeExpr::UInt(Box::new(dec(2)));
    let fifo_ty = TypeExpr::Vec(Box::new(idx_ty.clone()), Box::new(dec(n as u64)));
    let mut items = Vec::new();
    for i in 0..n {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(state_name(i)),
            ty: state_ty.clone(),
            init: None,
            reset: RegReset::Inherit(t.reset.clone(), zero()),
            guard: None,
            multicycle: None,
            span,
        }));
    }
    for (name, ty) in [
        (fifo_name.clone(), fifo_ty),
        (head_name.clone(), idx_ty.clone()),
        (tail_name.clone(), idx_ty),
        (occ_name.clone(), occ_ty.clone()),
        (age_name.clone(), age_ty.clone()),
    ] {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(name),
            ty,
            init: None,
            reset: RegReset::Inherit(t.reset.clone(), zero()),
            guard: None,
            multicycle: None,
            span,
        }));
    }
    if !tail_stmts.is_empty() {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(tail_done_name.clone()),
            ty: TypeExpr::Bool,
            init: None,
            reset: RegReset::Inherit(t.reset.clone(), bool_lit(false)),
            guard: None,
            multicycle: None,
            span,
        }));
    }

    let occ_nonzero = bin(BinOp::Gt, id(occ_name.clone()), sized(occ_w, 0));
    let occ_not_full = bin(BinOp::Lt, id(occ_name.clone()), sized(occ_w, n as u64));
    let rsp_pop = bin(
        BinOp::And,
        port_member(format!("{method}_rsp_valid")),
        occ_nonzero.clone(),
    );
    let fifo_head = index(id(fifo_name.clone()), id(head_name.clone()));
    let all_done = {
        let mut acc = bin(BinOp::Eq, id(state_name(0)), sized(2, 2));
        for i in 1..n {
            acc = bin(
                BinOp::And,
                acc,
                bin(BinOp::Eq, id(state_name(i)), sized(2, 2)),
            );
        }
        acc
    };
    let mut wants: Vec<Expr> = Vec::new();
    let mut grants: Vec<Expr> = Vec::new();
    for (i, issue) in issues.iter().enumerate() {
        let pending = bin(BinOp::Eq, id(state_name(i)), sized(2, 0));
        let aged = if issue.delay == 0 {
            bool_lit(true)
        } else {
            bin(BinOp::Gte, id(age_name.clone()), sized(age_w, issue.delay))
        };
        let want_i = bin(
            BinOp::And,
            bin(BinOp::And, pending, aged),
            occ_not_full.clone(),
        );
        let mut grant_i = want_i.clone();
        for prev in &wants {
            grant_i = bin(BinOp::And, grant_i, not(prev.clone()));
        }
        wants.push(want_i);
        grants.push(grant_i);
    }
    let or_expr = |xs: &[Expr]| -> Expr {
        let mut acc = xs.first().cloned().unwrap_or_else(|| bool_lit(false));
        for x in &xs[1..] {
            acc = bin(BinOp::Or, acc, x.clone());
        }
        acc
    };
    let req_valid = or_expr(&grants);
    let req_fire = bin(
        BinOp::And,
        req_valid.clone(),
        port_member(format!("{method}_req_ready")),
    );
    let ptr_inc = |ptr: &str, width: u32| -> Expr {
        tern(
            bin(BinOp::Eq, id(ptr.to_string()), sized(width, (n - 1) as u64)),
            sized(width, 0),
            trunc(bin(BinOp::Add, id(ptr.to_string()), sized(width, 1)), width),
        )
    };

    let mut comb_stmts = Vec::new();
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method}_req_valid")),
        value: req_valid.clone(),
        span,
    }));
    for (arg_i, (arg_ident, _)) in method_meta.args.iter().enumerate() {
        let mut value = zero();
        for (i, issue) in issues.iter().enumerate().rev() {
            value = tern(grants[i].clone(), issue.call.args[arg_i].clone(), value);
        }
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(format!("{}_{}", method, arg_ident.name)),
            value,
            span,
        }));
    }
    if let Some(tag_w) = tag_width {
        let mut value = sized(tag_w, 0);
        for i in (0..n).rev() {
            value = tern(grants[i].clone(), sized(tag_w, i as u64), value);
        }
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(format!("{method}_req_tag")),
            value,
            span,
        }));
    }
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method}_rsp_ready")),
        value: occ_nonzero.clone(),
        span,
    }));

    let mut reset_group_stmts: Vec<Stmt> = (0..n)
        .map(|i| {
            Stmt::Assign(RegAssign {
                target: id(state_name(i)),
                value: sized(2, 0),
                span,
            })
        })
        .chain(std::iter::once(Stmt::Assign(RegAssign {
            target: id(age_name.clone()),
            value: sized(age_w, 0),
            span,
        })))
        .collect();
    if !tail_stmts.is_empty() {
        reset_group_stmts.push(Stmt::Assign(RegAssign {
            target: id(tail_done_name.clone()),
            value: bool_lit(false),
            span,
        }));
    }
    let age_progress_stmts = if max_delay > 0 {
        vec![Stmt::IfElse(IfElse {
            cond: bin(BinOp::Lt, id(age_name.clone()), sized(age_w, max_delay)),
            then_stmts: vec![Stmt::Assign(RegAssign {
                target: id(age_name.clone()),
                value: trunc(
                    bin(BinOp::Add, id(age_name.clone()), sized(age_w, 1)),
                    age_w,
                ),
                span,
            })],
            else_stmts: Vec::new(),
            unique: false,
            span,
        })]
    } else {
        Vec::new()
    };

    let mut seq_body: Vec<Stmt> = Vec::new();
    if tail_stmts.is_empty() {
        seq_body.push(Stmt::IfElse(IfElse {
            cond: all_done.clone(),
            then_stmts: reset_group_stmts,
            else_stmts: age_progress_stmts,
            unique: false,
            span,
        }));
    } else {
        let tail_pending = bin(
            BinOp::And,
            all_done.clone(),
            not(id(tail_done_name.clone())),
        );
        let mut run_tail_stmts = tail_stmts.clone();
        run_tail_stmts.push(Stmt::Assign(RegAssign {
            target: id(tail_done_name.clone()),
            value: bool_lit(true),
            span,
        }));
        seq_body.push(Stmt::IfElse(IfElse {
            cond: all_done.clone(),
            then_stmts: vec![Stmt::IfElse(IfElse {
                cond: tail_pending,
                then_stmts: run_tail_stmts,
                else_stmts: reset_group_stmts,
                unique: false,
                span,
            })],
            else_stmts: age_progress_stmts,
            unique: false,
            span,
        }));
    }
    for i in 0..n {
        let push_i = bin(
            BinOp::And,
            grants[i].clone(),
            port_member(format!("{method}_req_ready")),
        );
        seq_body.push(Stmt::IfElse(IfElse {
            cond: push_i,
            then_stmts: vec![
                Stmt::Assign(RegAssign {
                    target: index(id(fifo_name.clone()), id(tail_name.clone())),
                    value: sized(idx_w, i as u64),
                    span,
                }),
                Stmt::Assign(RegAssign {
                    target: id(state_name(i)),
                    value: sized(2, 1),
                    span,
                }),
            ],
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
        let rsp_i = if let Some(tag_w) = tag_width {
            bin(
                BinOp::And,
                bin(
                    BinOp::And,
                    rsp_pop.clone(),
                    bin(BinOp::Eq, id(state_name(i)), sized(2, 1)),
                ),
                bin(
                    BinOp::Eq,
                    port_member(format!("{method}_rsp_tag")),
                    sized(tag_w, i as u64),
                ),
            )
        } else {
            bin(
                BinOp::And,
                bin(
                    BinOp::And,
                    rsp_pop.clone(),
                    bin(BinOp::Eq, id(state_name(i)), sized(2, 1)),
                ),
                bin(BinOp::Eq, fifo_head.clone(), sized(idx_w, i as u64)),
            )
        };
        let mut rsp_then = Vec::new();
        if method_meta.ret.is_some() {
            rsp_then.push(Stmt::Assign(RegAssign {
                target: issues[i].target.clone(),
                value: port_member(format!("{method}_rsp_data")),
                span,
            }));
        }
        rsp_then.push(Stmt::Assign(RegAssign {
            target: id(state_name(i)),
            value: sized(2, 2),
            span,
        }));
        seq_body.push(Stmt::IfElse(IfElse {
            cond: rsp_i,
            then_stmts: rsp_then,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    seq_body.push(Stmt::IfElse(IfElse {
        cond: req_fire.clone(),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(tail_name.clone()),
            value: ptr_inc(&tail_name, idx_w),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: rsp_pop.clone(),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(head_name.clone()),
            value: ptr_inc(&head_name, idx_w),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: bin(BinOp::And, req_fire.clone(), not(rsp_pop.clone())),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(occ_name.clone()),
            value: trunc(
                bin(BinOp::Add, id(occ_name.clone()), sized(occ_w, 1)),
                occ_w,
            ),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    seq_body.push(Stmt::IfElse(IfElse {
        cond: bin(BinOp::And, rsp_pop.clone(), not(req_fire)),
        then_stmts: vec![Stmt::Assign(RegAssign {
            target: id(occ_name.clone()),
            value: trunc(
                bin(BinOp::Sub, id(occ_name.clone()), sized(occ_w, 1)),
                occ_w,
            ),
            span,
        })],
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));

    items.push(ModuleBodyItem::RegBlock(RegBlock {
        clock: t.clock,
        clock_edge: t.clock_edge,
        stmts: seq_body,
        span,
    }));
    items.push(ModuleBodyItem::CombBlock(CombBlock {
        stmts: comb_stmts,
        span,
    }));
    Ok(items)
}

/// In-place lowering of a thread containing TLM initiator calls. Emits
/// RegDecl + RegBlock + CombBlock items directly into the parent module
/// body. v1 accepts a linear body of SeqAssigns only; any other stmt kind
/// produces a targeted error.
fn inline_lower_tlm_initiator(
    t: ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let span = t.span;
    let mk_ident = |name: String| Ident { name, span };

    // Thread name for state-reg naming; anonymous threads get a counter
    // elsewhere, but at this point it should have a name from the parser.
    let tag = t
        .name
        .as_ref()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "tlm_init".to_string());

    // Each state is either ComputeOnly (fire seq then advance) or
    // IssueThenWait (drive req / wait for req_ready; drive rsp_ready /
    // capture on rsp_valid). We build a flat list of state kinds.
    enum StateKind {
        Compute {
            seq_on_exit: Vec<Stmt>,
        },
        TlmIssue {
            port: String,
            method: String,
            args: Vec<Expr>,
            method_meta: TlmMethodMeta,
        },
        TlmWait {
            port: String,
            method: String,
            method_meta: TlmMethodMeta,
            dest: Option<Expr>,
        },
    }
    let mut states: Vec<StateKind> = Vec::new();
    let mut pending_seq: Vec<Stmt> = Vec::new();

    fn lower_initiator_stmts(
        stmts: Vec<ThreadStmt>,
        states: &mut Vec<StateKind>,
        pending_seq: &mut Vec<Stmt>,
        port_buses: &std::collections::HashMap<String, String>,
        bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
        span: Span,
    ) -> Result<(), CompileError> {
        for stmt in stmts {
            match stmt {
                ThreadStmt::SeqAssign(ra) => {
                    // Reject nested TLM calls in either side (composed RHS
                    // like `d <= m.read(a) + 1;` or LHS ref).
                    if match_tlm_call(&ra.value, port_buses, bus_methods).is_none()
                        && contains_tlm_call(&ra.value, port_buses, bus_methods)
                    {
                        return Err(CompileError::general(
                        "TLM method call must be the direct right-hand side of `<=` in a thread body — nested or composed uses are not supported in v1",
                        ra.span,
                    ));
                    }
                    if contains_tlm_call(&ra.target, port_buses, bus_methods) {
                        return Err(CompileError::general(
                            "TLM method calls cannot appear on the LHS of an assignment",
                            ra.span,
                        ));
                    }
                    if let Some(call) = match_tlm_call(&ra.value, port_buses, bus_methods) {
                        // Flush any pending non-TLM seq assigns as a Compute state.
                        if !pending_seq.is_empty() {
                            states.push(StateKind::Compute {
                                seq_on_exit: std::mem::take(pending_seq),
                            });
                        }
                        let has_ret = call.method_meta.ret.is_some();
                        states.push(StateKind::TlmIssue {
                            port: call.port.clone(),
                            method: call.method.clone(),
                            args: call.args.clone(),
                            method_meta: call.method_meta.clone(),
                        });
                        states.push(StateKind::TlmWait {
                            port: call.port,
                            method: call.method,
                            method_meta: call.method_meta.clone(),
                            dest: if has_ret { Some(ra.target) } else { None },
                        });
                    } else {
                        pending_seq.push(Stmt::Assign(ra));
                    }
                }
                ThreadStmt::Lock { body, .. } => {
                    // TLM initiator lowering is responsible for consuming any
                    // TLM call before the generic thread pass sees it. Recurse
                    // through `lock` so the lock idiom recommended by the
                    // multi-driver diagnostic is accepted for TLM methods. The
                    // actual TLM method drives are still emitted once per lowered
                    // thread by the method aggregator below, so the call site
                    // stays on the TLM path instead of falling into ordinary
                    // thread lowering.
                    lower_initiator_stmts(
                        body,
                        states,
                        pending_seq,
                        port_buses,
                        bus_methods,
                        span,
                    )?;
                }
                other => {
                    return Err(CompileError::general(
                    &format!(
                        "v1 TLM initiator thread body only supports SeqAssign statements and `lock` blocks around them (found {:?}). Refactor more complex control flow into a `thread` without TLM calls.",
                        std::mem::discriminant(&other),
                    ),
                    span,
                ));
                }
            }
        }
        Ok(())
    }

    lower_initiator_stmts(
        t.body,
        &mut states,
        &mut pending_seq,
        port_buses,
        bus_methods,
        span,
    )?;
    // Trailing pending seq becomes a Compute state too.
    if !pending_seq.is_empty() {
        states.push(StateKind::Compute {
            seq_on_exit: std::mem::take(&mut pending_seq),
        });
    }
    // Empty body is fine — nothing to lower.
    if states.is_empty() {
        return Ok(Vec::new());
    }

    let total_states = states.len();
    let state_width = clog2_width(total_states as u64);
    let state_reg_name = format!("_tlm_init_{}_state", tag);
    let state_expr = Expr::new(ExprKind::Ident(state_reg_name.clone()), span);
    let mk_state_lit = |v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(state_width, v)), span);
    let state_eq = |v: u64| {
        Expr::new(
            ExprKind::Binary(
                BinOp::Eq,
                Box::new(state_expr.clone()),
                Box::new(mk_state_lit(v)),
            ),
            span,
        )
    };
    let state_reg_decl = RegDecl {
        name: mk_ident(state_reg_name.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(state_width as u64)),
            span,
        ))),
        init: None,
        reset: RegReset::Inherit(
            t.reset.clone(),
            Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
        ),
        guard: None,
        multicycle: None,
        span,
    };

    let mk_port_member = |port: &str, member: String| {
        Expr::new(
            ExprKind::FieldAccess(
                Box::new(Expr::new(ExprKind::Ident(port.to_string()), span)),
                mk_ident(member),
            ),
            span,
        )
    };

    let mut seq_body: Vec<Stmt> = Vec::new();
    // Per-method aggregators for unconditional drives — keyed by
    // "<port>.<method>". Each entry collects issue-state indices
    // (for req_valid + arg muxes) and wait-state indices (for
    // rsp_ready). Emitting the drives as unconditional CombAssigns
    // whose RHS is a state-OR/mux avoids the comb-block no-latch
    // check tripping over state-guarded drives.
    struct MethodAgg {
        port: String,
        method: String,
        ret_ty: Option<TypeExpr>,
        arg_decls: Vec<(Ident, TypeExpr)>,
        tag_width: Option<Expr>,
        issues: Vec<(u64, Vec<Expr>)>, // (state_idx, args at that call site)
        waits: Vec<u64>,               // state_idx
    }
    let mut aggs: std::collections::BTreeMap<String, MethodAgg> = std::collections::BTreeMap::new();

    for (i, sk) in states.iter().enumerate() {
        let next_idx = ((i + 1) % total_states) as u64;
        let cur_idx = i as u64;
        match sk {
            StateKind::Compute { seq_on_exit } => {
                let mut then_stmts = seq_on_exit.clone();
                then_stmts.push(Stmt::Assign(RegAssign {
                    target: state_expr.clone(),
                    value: mk_state_lit(next_idx),
                    span,
                }));
                seq_body.push(Stmt::IfElse(IfElseOf {
                    cond: state_eq(cur_idx),
                    then_stmts,
                    else_stmts: Vec::new(),
                    unique: false,
                    span,
                }));
            }
            StateKind::TlmIssue {
                port,
                method,
                args,
                method_meta,
            } => {
                let key = format!("{port}.{method}");
                aggs.entry(key)
                    .or_insert_with(|| MethodAgg {
                        port: port.clone(),
                        method: method.clone(),
                        ret_ty: method_meta.ret.clone(),
                        arg_decls: method_meta.args.clone(),
                        tag_width: method_meta.out_of_order_tags.clone(),
                        issues: Vec::new(),
                        waits: Vec::new(),
                    })
                    .issues
                    .push((cur_idx, args.clone()));
                // Seq: advance on req_ready.
                let advance_cond = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(state_eq(cur_idx)),
                        Box::new(mk_port_member(port, format!("{method}_req_ready"))),
                    ),
                    span,
                );
                seq_body.push(Stmt::IfElse(IfElseOf {
                    cond: advance_cond,
                    then_stmts: vec![Stmt::Assign(RegAssign {
                        target: state_expr.clone(),
                        value: mk_state_lit(next_idx),
                        span,
                    })],
                    else_stmts: Vec::new(),
                    unique: false,
                    span,
                }));
            }
            StateKind::TlmWait {
                port,
                method,
                method_meta,
                dest,
            } => {
                let key = format!("{port}.{method}");
                aggs.entry(key)
                    .or_insert_with(|| MethodAgg {
                        port: port.clone(),
                        method: method.clone(),
                        ret_ty: method_meta.ret.clone(),
                        arg_decls: method_meta.args.clone(),
                        tag_width: method_meta.out_of_order_tags.clone(),
                        issues: Vec::new(),
                        waits: Vec::new(),
                    })
                    .waits
                    .push(cur_idx);
                let mut then_stmts: Vec<Stmt> = Vec::new();
                if let Some(dest_expr) = dest {
                    then_stmts.push(Stmt::Assign(RegAssign {
                        target: dest_expr.clone(),
                        value: mk_port_member(port, format!("{method}_rsp_data")),
                        span,
                    }));
                }
                then_stmts.push(Stmt::Assign(RegAssign {
                    target: state_expr.clone(),
                    value: mk_state_lit(next_idx),
                    span,
                }));
                let mut advance_rhs = mk_port_member(port, format!("{method}_rsp_valid"));
                if let Some(tag_w_expr) = &method_meta.out_of_order_tags {
                    let tag_w = literal_expr_u64(tag_w_expr)
                        .ok_or_else(|| CompileError::general(
                            "`out_of_order tags` must be a literal width in the first implementation",
                            tag_w_expr.span,
                        ))? as u32;
                    advance_rhs = Expr::new(
                        ExprKind::Binary(
                            BinOp::And,
                            Box::new(advance_rhs),
                            Box::new(Expr::new(
                                ExprKind::Binary(
                                    BinOp::Eq,
                                    Box::new(mk_port_member(port, format!("{method}_rsp_tag"))),
                                    Box::new(Expr::new(
                                        ExprKind::Literal(LitKind::Sized(tag_w, 0)),
                                        span,
                                    )),
                                ),
                                span,
                            )),
                        ),
                        span,
                    );
                }
                let advance_cond = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(state_eq(cur_idx)),
                        Box::new(advance_rhs),
                    ),
                    span,
                );
                seq_body.push(Stmt::IfElse(IfElseOf {
                    cond: advance_cond,
                    then_stmts,
                    else_stmts: Vec::new(),
                    unique: false,
                    span,
                }));
            }
        }
    }

    // Build comb drives: one unconditional CombAssign per wire, with
    // state-dependent RHS. OR-of-state-eq for booleans; ternary-mux for
    // argument values (default 0 when not in an issue state).
    let mut comb_stmts: Vec<Stmt> = Vec::new();
    let or_of_states = |indices: &[u64]| -> Expr {
        if indices.is_empty() {
            return Expr::new(ExprKind::Literal(LitKind::Sized(1, 0)), span);
        }
        let mut acc = state_eq(indices[0]);
        for idx in &indices[1..] {
            acc = Expr::new(
                ExprKind::Binary(BinOp::Or, Box::new(acc), Box::new(state_eq(*idx))),
                span,
            );
        }
        acc
    };
    for (_, agg) in &aggs {
        // req_valid = OR of issue states
        let issue_idxs: Vec<u64> = agg.issues.iter().map(|(i, _)| *i).collect();
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: mk_port_member(&agg.port, format!("{}_req_valid", agg.method)),
            value: or_of_states(&issue_idxs),
            span,
        }));
        // Each arg: ternary chain over issue states; default 0.
        for (arg_i, (arg_ident, _arg_ty)) in agg.arg_decls.iter().enumerate() {
            let mut value_expr = Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
            for (state_idx, args) in agg.issues.iter().rev() {
                if let Some(a) = args.get(arg_i) {
                    value_expr = Expr::new(
                        ExprKind::Ternary(
                            Box::new(state_eq(*state_idx)),
                            Box::new(a.clone()),
                            Box::new(value_expr),
                        ),
                        span,
                    );
                }
            }
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: mk_port_member(&agg.port, format!("{}_{}", agg.method, arg_ident.name)),
                value: value_expr,
                span,
            }));
            let _ = agg.ret_ty;
        }
        if let Some(tag_w_expr) = &agg.tag_width {
            let tag_w = literal_expr_u64(tag_w_expr).unwrap_or(1) as u32;
            comb_stmts.push(Stmt::Assign(CombAssign {
                target: mk_port_member(&agg.port, format!("{}_req_tag", agg.method)),
                value: Expr::new(ExprKind::Literal(LitKind::Sized(tag_w, 0)), span),
                span,
            }));
        }
        // rsp_ready = OR of wait states
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: mk_port_member(&agg.port, format!("{}_rsp_ready", agg.method)),
            value: or_of_states(&agg.waits),
            span,
        }));
    }

    let reg_block = RegBlock {
        clock: t.clock.clone(),
        clock_edge: t.clock_edge,
        stmts: seq_body,
        span,
    };
    let comb_block = CombBlock {
        stmts: comb_stmts,
        span,
    };

    Ok(vec![
        ModuleBodyItem::RegDecl(state_reg_decl),
        ModuleBodyItem::RegBlock(reg_block),
        ModuleBodyItem::CombBlock(comb_block),
    ])
}

#[derive(Clone)]
struct TlmCall {
    port: String,
    method: String,
    args: Vec<Expr>,
    method_meta: TlmMethodMeta,
}

fn match_tlm_call(
    e: &Expr,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Option<TlmCall> {
    if let ExprKind::MethodCall(recv, method, args) = &e.kind {
        if let ExprKind::Ident(port_name) = &recv.kind {
            let bus = port_buses.get(port_name)?;
            let methods = bus_methods.get(bus)?;
            let meta = methods.iter().find(|m| m.name.name == method.name)?;
            return Some(TlmCall {
                port: port_name.clone(),
                method: method.name.clone(),
                args: args.clone(),
                method_meta: meta.clone(),
            });
        }
    }
    None
}

fn contains_tlm_call(
    e: &Expr,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> bool {
    if match_tlm_call(e, port_buses, bus_methods).is_some() {
        return true;
    }
    match &e.kind {
        ExprKind::Binary(_, l, r) => {
            contains_tlm_call(l, port_buses, bus_methods)
                || contains_tlm_call(r, port_buses, bus_methods)
        }
        ExprKind::Unary(_, x)
        | ExprKind::Cast(x, _)
        | ExprKind::Clog2(x)
        | ExprKind::Onehot(x)
        | ExprKind::Signed(x)
        | ExprKind::Unsigned(x)
        | ExprKind::LatencyAt(x, _)
        | ExprKind::SvaNext(_, x) => contains_tlm_call(x, port_buses, bus_methods),
        ExprKind::Index(b, i) => {
            contains_tlm_call(b, port_buses, bus_methods)
                || contains_tlm_call(i, port_buses, bus_methods)
        }
        ExprKind::FieldAccess(b, _) => contains_tlm_call(b, port_buses, bus_methods),
        ExprKind::MethodCall(recv, _, args) => {
            contains_tlm_call(recv, port_buses, bus_methods)
                || args
                    .iter()
                    .any(|a| contains_tlm_call(a, port_buses, bus_methods))
        }
        ExprKind::Ternary(c, t, el) => {
            contains_tlm_call(c, port_buses, bus_methods)
                || contains_tlm_call(t, port_buses, bus_methods)
                || contains_tlm_call(el, port_buses, bus_methods)
        }
        ExprKind::Concat(xs) | ExprKind::FunctionCall(_, xs) => xs
            .iter()
            .any(|x| contains_tlm_call(x, port_buses, bus_methods)),
        ExprKind::Repeat(n, x) => {
            contains_tlm_call(n, port_buses, bus_methods)
                || contains_tlm_call(x, port_buses, bus_methods)
        }
        _ => false,
    }
}

// ── TLM target in-place lowering ────────────────────────────────────────────
//
// Replaces the previous "transform into regular thread" approach with
// direct emission of RegDecl + RegBlock + CombBlock items into the
// parent module body. This bypasses lower_threads entirely for TLM
// target threads and avoids the sub-module bus-flattening bridging that
// the thread-extraction path doesn't handle for FieldAccess(bus_port,
// member) drives.
//
// Supported target bodies reuse ordinary thread lowering before generated
// response states, including assignments, waits, if, counted for, fork/join,
// lock, and branch-local returns. Statements after a return in the same block
// are rejected with a targeted error.

fn lower_indexed_tlm_target_group(
    module_name: &str,
    mut group: Vec<(ThreadBlock, TlmTargetBinding, TlmMethodMeta)>,
    resource_decls: &HashMap<String, ResourceDecl>,
) -> Result<(Vec<ModuleBodyItem>, Vec<Item>), CompileError> {
    if group.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let span = group[0].0.span;
    let group_clk = group[0].0.clock.name.clone();
    let group_clock_edge = group[0].0.clock_edge;
    let group_reset_ident = group[0].0.reset.clone();
    let group_rst = group[0].0.reset.name.clone();
    let group_rst_level = group[0].0.reset_level;
    let port = group[0].1.port.name.clone();
    let method_name = group[0].1.method.name.clone();
    let method = group[0].2.clone();
    let tag_w_expr = method.out_of_order_tags.clone().ok_or_else(|| {
        CompileError::general(
            &format!("indexed target method `{port}.{method_name}[...]` requires `tlm_method {method_name}(...): out_of_order tags N`"),
            span,
        )
    })?;
    let tag_w = literal_expr_u64(&tag_w_expr).ok_or_else(|| {
        CompileError::general(
            "`out_of_order tags` must be a literal width for indexed target lowering",
            tag_w_expr.span,
        )
    })? as u32;
    let tag_slots = if tag_w >= 64 {
        u128::MAX
    } else {
        1u128 << tag_w
    };

    let mut seen = std::collections::HashSet::new();
    let mut lanes: Vec<(u64, ThreadBlock, TlmTargetBinding, TlmMethodMeta)> = Vec::new();
    for (t, binding, method_meta) in group.drain(..) {
        let lane_expr = binding.tag_lane.as_ref().ok_or_else(|| {
            CompileError::general(
                "internal error: indexed TLM target group contains an unindexed target",
                t.span,
            )
        })?;
        let lane = literal_expr_u64(lane_expr).ok_or_else(|| {
            CompileError::general(
                "indexed TLM target lane must be compile-time literal after generate_for expansion",
                lane_expr.span,
            )
        })?;
        if lane as u128 >= tag_slots {
            return Err(CompileError::general(
                &format!("indexed TLM target lane {lane} exceeds `{port}.{method_name}` tag capacity {tag_slots}"),
                lane_expr.span,
            ));
        }
        if !seen.insert(lane) {
            return Err(CompileError::general(
                &format!("duplicate indexed TLM target lane {lane} for `{port}.{method_name}`"),
                lane_expr.span,
            ));
        }
        lanes.push((lane, t, binding, method_meta));
    }
    lanes.sort_by_key(|(lane, _, _, _)| *lane);

    let mk_ident = |name: String| Ident { name, span };
    let ident_expr = |name: String| Expr::new(ExprKind::Ident(name), span);
    let port_member = |member: String| {
        Expr::new(
            ExprKind::FieldAccess(
                Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
                mk_ident(member),
            ),
            span,
        )
    };
    let lit0 = Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
    let lit1 = Expr::new(ExprKind::Literal(LitKind::Sized(1, 1)), span);
    let tag_lit = |lane: u64| Expr::new(ExprKind::Literal(LitKind::Sized(tag_w, lane)), span);
    let tag_eq = |lane: u64| {
        Expr::new(
            ExprKind::Binary(
                BinOp::Eq,
                Box::new(port_member(format!("{method_name}_req_tag"))),
                Box::new(tag_lit(lane)),
            ),
            span,
        )
    };

    let mut out = Vec::new();
    let mut extra_items = Vec::new();
    let mut lane_infos = Vec::new();
    let mut response_resource: Option<String> = None;
    let mut response_resource_lanes = 0usize;
    for (lane, t, binding, method_meta) in lanes {
        let mut t = t;
        let lane_response_resource = strip_tlm_response_lock(&mut t)?;
        if let Some(res) = lane_response_resource {
            response_resource_lanes += 1;
            match &response_resource {
                Some(existing) if existing != &res => {
                    return Err(CompileError::general(
                        &format!(
                            "indexed TLM target lanes for `{port}.{method_name}` use different response-channel resources (`{existing}` and `{res}`); use one shared `resource` for the method response channel",
                        ),
                        t.span,
                    ));
                }
                None => response_resource = Some(res),
                _ => {}
            }
        }
        let prefix = format!("_tlm_{port}_{method_name}_tag{lane}");
        let req_ready = format!("{prefix}_req_ready");
        let rsp_valid = format!("{prefix}_rsp_valid");
        let rsp_ready = format!("{prefix}_rsp_ready");
        let rsp_tag = format!("{prefix}_rsp_tag");
        let rsp_data = format!("{prefix}_rsp_data");

        out.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: mk_ident(req_ready.clone()),
            ty: TypeExpr::Bool,
            unpacked: false,
            unpacked_ascending: false,
            span,
        }));
        out.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: mk_ident(rsp_valid.clone()),
            ty: TypeExpr::Bool,
            unpacked: false,
            unpacked_ascending: false,
            span,
        }));
        out.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: mk_ident(rsp_ready.clone()),
            ty: TypeExpr::Bool,
            unpacked: false,
            unpacked_ascending: false,
            span,
        }));
        out.push(ModuleBodyItem::WireDecl(WireDecl {
            bus_params: Vec::new(),
            name: mk_ident(rsp_tag.clone()),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(tag_w as u64)),
                span,
            ))),
            unpacked: false,
            unpacked_ascending: false,
            span,
        }));
        if let Some(ret_ty) = &method.ret {
            out.push(ModuleBodyItem::WireDecl(WireDecl {
                bus_params: Vec::new(),
                name: mk_ident(rsp_data.clone()),
                ty: ret_ty.clone(),
                unpacked: false,
                unpacked_ascending: false,
                span,
            }));
        }

        let req_valid = Expr::new(
            ExprKind::Binary(
                BinOp::And,
                Box::new(port_member(format!("{method_name}_req_valid"))),
                Box::new(tag_eq(lane)),
            ),
            span,
        );
        let io = TlmTargetIo {
            suffix: format!("_tag{lane}"),
            req_valid,
            rsp_ready: ident_expr(rsp_ready.clone()),
            req_ready_target: ident_expr(req_ready.clone()),
            rsp_valid_target: ident_expr(rsp_valid.clone()),
            rsp_data_target: method.ret.as_ref().map(|_| ident_expr(rsp_data.clone())),
            rsp_tag_target: Some(ident_expr(rsp_tag.clone())),
        };
        out.extend(inline_lower_tlm_target_with_io(
            t,
            &binding,
            &method_meta,
            io,
        )?);
        lane_infos.push((lane, req_ready, rsp_valid, rsp_ready, rsp_data, rsp_tag));
    }
    if response_resource.is_some() && response_resource_lanes != lane_infos.len() {
        return Err(CompileError::general(
            &format!(
                "indexed TLM target lanes for `{port}.{method_name}` must all wrap `return` in the same response-channel `lock` when any lane names a response resource",
            ),
            span,
        ));
    }

    let arb_res_name = response_resource
        .clone()
        .unwrap_or_else(|| format!("_tlm_{port}_{method_name}_rsp_ch"));
    let arb_prefix = format!("_tlm_{port}_{method_name}_rsp_arb");
    let req_packed = format!("{arb_prefix}_req_packed");
    let grant_packed = format!("{arb_prefix}_grant_packed");
    let grant_valid = format!("{arb_prefix}_grant_valid");
    let grant_requester = format!("{arb_prefix}_grant_requester");
    let hold_valid = format!("{arb_prefix}_hold_valid_r");
    let hold_idx = format!("{arb_prefix}_hold_idx_r");
    let lane_count = lane_infos.len();
    let lane_count_expr = Expr::new(ExprKind::Literal(LitKind::Dec(lane_count as u64)), span);
    let grant_width = crate::width::index_width(lane_count as u64);

    out.push(ModuleBodyItem::WireDecl(WireDecl {
        bus_params: Vec::new(),
        name: mk_ident(req_packed.clone()),
        ty: TypeExpr::UInt(Box::new(lane_count_expr.clone())),
        unpacked: false,
        unpacked_ascending: false,
        span,
    }));
    out.push(ModuleBodyItem::WireDecl(WireDecl {
        bus_params: Vec::new(),
        name: mk_ident(grant_packed.clone()),
        ty: TypeExpr::UInt(Box::new(lane_count_expr.clone())),
        unpacked: false,
        unpacked_ascending: false,
        span,
    }));
    out.push(ModuleBodyItem::WireDecl(WireDecl {
        bus_params: Vec::new(),
        name: mk_ident(grant_valid.clone()),
        ty: TypeExpr::Bool,
        unpacked: false,
        unpacked_ascending: false,
        span,
    }));
    out.push(ModuleBodyItem::WireDecl(WireDecl {
        bus_params: Vec::new(),
        name: mk_ident(grant_requester.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(grant_width as u64)),
            span,
        ))),
        unpacked: false,
        unpacked_ascending: false,
        span,
    }));
    out.push(ModuleBodyItem::RegDecl(RegDecl {
        name: mk_ident(hold_valid.clone()),
        ty: TypeExpr::Bool,
        init: None,
        reset: RegReset::Inherit(
            group_reset_ident.clone(),
            Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
        ),
        guard: None,
        multicycle: None,
        span,
    }));
    out.push(ModuleBodyItem::RegDecl(RegDecl {
        name: mk_ident(hold_idx.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(grant_width as u64)),
            span,
        ))),
        init: None,
        reset: RegReset::Inherit(
            group_reset_ident.clone(),
            Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
        ),
        guard: None,
        multicycle: None,
        span,
    }));

    let arb_module_name = format!("_arb_{module_name}_{arb_res_name}");
    let (policy, hook) = match response_resource
        .as_ref()
        .and_then(|res| resource_decls.get(res))
    {
        Some(rd) => (rd.policy.clone(), rd.hook.clone()),
        None => (ArbiterPolicy::Priority, None),
    };
    extra_items.push(Item::Arbiter(synthesize_lock_arbiter(
        &arb_module_name,
        lane_count,
        policy,
        hook,
        &group_clk,
        &group_rst,
        group_rst_level,
        span,
    )));
    out.push(ModuleBodyItem::Inst(InstDecl {
        name: mk_ident(format!("{arb_prefix}_inst")),
        module_name: mk_ident(arb_module_name),
        param_assigns: Vec::new(),
        auto_connect: None,
        connections: vec![
            Connection {
                port_name: mk_ident("clk".to_string()),
                direction: ConnectDir::Input,
                signal: Expr::new(ExprKind::Ident(group_clk.clone()), span),
                reset_override: None,
                span,
            },
            Connection {
                port_name: mk_ident("rst".to_string()),
                direction: ConnectDir::Input,
                signal: Expr::new(ExprKind::Ident(group_rst.clone()), span),
                reset_override: None,
                span,
            },
            Connection {
                port_name: mk_ident("request_valid".to_string()),
                direction: ConnectDir::Input,
                signal: Expr::new(ExprKind::Ident(req_packed.clone()), span),
                reset_override: None,
                span,
            },
            Connection {
                port_name: mk_ident("request_ready".to_string()),
                direction: ConnectDir::Output,
                signal: Expr::new(ExprKind::Ident(grant_packed.clone()), span),
                reset_override: None,
                span,
            },
            Connection {
                port_name: mk_ident("grant_valid".to_string()),
                direction: ConnectDir::Output,
                signal: Expr::new(ExprKind::Ident(grant_valid.clone()), span),
                reset_override: None,
                span,
            },
            Connection {
                port_name: mk_ident("grant_requester".to_string()),
                direction: ConnectDir::Output,
                signal: Expr::new(ExprKind::Ident(grant_requester.clone()), span),
                reset_override: None,
                span,
            },
        ],
        for_loops: Vec::new(),
        span,
    }));

    let mut comb_stmts = Vec::new();
    for (idx, (_lane, _req_ready, rsp_valid, _rsp_ready, _rsp_data, _rsp_tag)) in
        lane_infos.iter().enumerate()
    {
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: Expr::new(
                ExprKind::Index(
                    Box::new(Expr::new(ExprKind::Ident(req_packed.clone()), span)),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(idx as u64)), span)),
                ),
                span,
            ),
            value: Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(Expr::new(
                        ExprKind::Unary(
                            UnaryOp::Not,
                            Box::new(Expr::new(ExprKind::Ident(hold_valid.clone()), span)),
                        ),
                        span,
                    )),
                    Box::new(ident_expr(rsp_valid.clone())),
                ),
                span,
            ),
            span,
        }));
    }
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method_name}_req_ready")),
        value: lit0.clone(),
        span,
    }));
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method_name}_rsp_valid")),
        value: lit0.clone(),
        span,
    }));
    if method.ret.is_some() {
        let default_rsp_data = lane_infos
            .first()
            .map(|(_, _, _, _, rsp_data, _)| ident_expr(rsp_data.clone()))
            .unwrap_or_else(|| lit0.clone());
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: port_member(format!("{method_name}_rsp_data")),
            value: default_rsp_data,
            span,
        }));
    }
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: port_member(format!("{method_name}_rsp_tag")),
        value: lit0.clone(),
        span,
    }));
    for (_lane, _req_ready, _rsp_valid, rsp_ready, _rsp_data, _rsp_tag) in &lane_infos {
        comb_stmts.push(Stmt::Assign(CombAssign {
            target: ident_expr(rsp_ready.clone()),
            value: lit0.clone(),
            span,
        }));
    }
    for (lane, req_ready, _rsp_valid, _rsp_ready, _rsp_data, _rsp_tag) in &lane_infos {
        comb_stmts.push(Stmt::IfElse(IfElse {
            cond: tag_eq(*lane),
            then_stmts: vec![Stmt::Assign(CombAssign {
                target: port_member(format!("{method_name}_req_ready")),
                value: ident_expr(req_ready.clone()),
                span,
            })],
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    for (idx, (_lane, _req_ready, rsp_valid, rsp_ready, rsp_data, rsp_tag)) in
        lane_infos.iter().enumerate()
    {
        let lane_grant = Expr::new(
            ExprKind::Index(
                Box::new(Expr::new(ExprKind::Ident(grant_packed.clone()), span)),
                Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(idx as u64)), span)),
            ),
            span,
        );
        let held_lane = Expr::new(
            ExprKind::Binary(
                BinOp::And,
                Box::new(Expr::new(ExprKind::Ident(hold_valid.clone()), span)),
                Box::new(Expr::new(
                    ExprKind::Binary(
                        BinOp::Eq,
                        Box::new(Expr::new(ExprKind::Ident(hold_idx.clone()), span)),
                        Box::new(Expr::new(
                            ExprKind::Literal(LitKind::Sized(grant_width, idx as u64)),
                            span,
                        )),
                    ),
                    span,
                )),
            ),
            span,
        );
        let selected_lane = Expr::new(
            ExprKind::Binary(BinOp::Or, Box::new(held_lane), Box::new(lane_grant)),
            span,
        );
        let mut then_stmts = vec![
            Stmt::Assign(CombAssign {
                target: port_member(format!("{method_name}_rsp_valid")),
                value: lit1.clone(),
                span,
            }),
            Stmt::Assign(CombAssign {
                target: port_member(format!("{method_name}_rsp_tag")),
                value: ident_expr(rsp_tag.clone()),
                span,
            }),
            Stmt::Assign(CombAssign {
                target: ident_expr(rsp_ready.clone()),
                value: port_member(format!("{method_name}_rsp_ready")),
                span,
            }),
        ];
        if method.ret.is_some() {
            then_stmts.push(Stmt::Assign(CombAssign {
                target: port_member(format!("{method_name}_rsp_data")),
                value: ident_expr(rsp_data.clone()),
                span,
            }));
        }
        comb_stmts.push(Stmt::IfElse(IfElse {
            cond: Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(selected_lane),
                    Box::new(ident_expr(rsp_valid.clone())),
                ),
                span,
            ),
            then_stmts,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    out.push(ModuleBodyItem::CombBlock(CombBlock {
        stmts: comb_stmts,
        span,
    }));
    let rsp_ready_member = port_member(format!("{method_name}_rsp_ready"));
    out.push(ModuleBodyItem::RegBlock(RegBlock {
        clock: Ident::new(group_clk.clone(), span),
        clock_edge: group_clock_edge,
        stmts: vec![
            Stmt::IfElse(IfElseOf {
                cond: Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(Expr::new(ExprKind::Ident(hold_valid.clone()), span)),
                        Box::new(rsp_ready_member.clone()),
                    ),
                    span,
                ),
                then_stmts: vec![Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(hold_valid.clone()), span),
                    value: lit0.clone(),
                    span,
                })],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }),
            Stmt::IfElse(IfElseOf {
                cond: Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(Expr::new(
                            ExprKind::Unary(
                                UnaryOp::Not,
                                Box::new(Expr::new(ExprKind::Ident(hold_valid.clone()), span)),
                            ),
                            span,
                        )),
                        Box::new(Expr::new(
                            ExprKind::Binary(
                                BinOp::And,
                                Box::new(Expr::new(ExprKind::Ident(grant_valid.clone()), span)),
                                Box::new(Expr::new(
                                    ExprKind::Unary(UnaryOp::Not, Box::new(rsp_ready_member)),
                                    span,
                                )),
                            ),
                            span,
                        )),
                    ),
                    span,
                ),
                then_stmts: vec![
                    Stmt::Assign(RegAssign {
                        target: Expr::new(ExprKind::Ident(hold_valid.clone()), span),
                        value: lit1.clone(),
                        span,
                    }),
                    Stmt::Assign(RegAssign {
                        target: Expr::new(ExprKind::Ident(hold_idx.clone()), span),
                        value: Expr::new(ExprKind::Ident(grant_requester.clone()), span),
                        span,
                    }),
                ],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }),
        ],
        span,
    }));
    Ok((out, extra_items))
}

fn strip_tlm_response_lock(t: &mut ThreadBlock) -> Result<Option<String>, CompileError> {
    fn rewrite_stmts(
        stmts: Vec<ThreadStmt>,
        found: &mut Option<String>,
    ) -> Result<Vec<ThreadStmt>, CompileError> {
        let mut out = Vec::new();
        for stmt in stmts {
            match stmt {
                ThreadStmt::Lock {
                    resource,
                    body,
                    span,
                } if contains_return(&body) => {
                    if let Some(existing) = found.as_ref() {
                        if existing != &resource.name {
                            return Err(CompileError::general(
                                &format!(
                                    "TLM target response return is guarded by multiple resources (`{existing}` and `{}`); use one response-channel resource",
                                    resource.name
                                ),
                                span,
                            ));
                        }
                    } else {
                        *found = Some(resource.name.clone());
                    }
                    out.extend(rewrite_stmts(body, found)?);
                }
                ThreadStmt::IfElse(mut ie) => {
                    ie.then_stmts = rewrite_stmts(ie.then_stmts, found)?;
                    ie.else_stmts = rewrite_stmts(ie.else_stmts, found)?;
                    out.push(ThreadStmt::IfElse(ie));
                }
                ThreadStmt::For {
                    var,
                    start,
                    end,
                    body,
                    span,
                } => {
                    out.push(ThreadStmt::For {
                        var,
                        start,
                        end,
                        body: rewrite_stmts(body, found)?,
                        span,
                    });
                }
                ThreadStmt::ForkJoin(branches, span) => {
                    let mut new_branches = Vec::new();
                    for branch in branches {
                        new_branches.push(rewrite_stmts(branch, found)?);
                    }
                    out.push(ThreadStmt::ForkJoin(new_branches, span));
                }
                ThreadStmt::DoUntil { body, cond, span } => {
                    out.push(ThreadStmt::DoUntil {
                        body: rewrite_stmts(body, found)?,
                        cond,
                        span,
                    });
                }
                other => out.push(other),
            }
        }
        Ok(out)
    }

    let mut found = None;
    t.body = rewrite_stmts(std::mem::take(&mut t.body), &mut found)?;
    Ok(found)
}

#[derive(Clone)]
struct TlmTargetIo {
    suffix: String,
    req_valid: Expr,
    rsp_ready: Expr,
    req_ready_target: Expr,
    rsp_valid_target: Expr,
    rsp_data_target: Option<Expr>,
    rsp_tag_target: Option<Expr>,
}

fn inline_lower_tlm_target(
    t: ThreadBlock,
    binding: &TlmTargetBinding,
    method: &TlmMethodMeta,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let port = &binding.port.name;
    let method_name = &binding.method.name;
    let span = t.span;
    let mk_ident = |name: String| Ident { name, span };
    let mk_port_member = |member: String| {
        Expr::new(
            ExprKind::FieldAccess(
                Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
                mk_ident(member),
            ),
            span,
        )
    };
    let io = TlmTargetIo {
        suffix: String::new(),
        req_valid: mk_port_member(format!("{method_name}_req_valid")),
        rsp_ready: mk_port_member(format!("{method_name}_rsp_ready")),
        req_ready_target: mk_port_member(format!("{method_name}_req_ready")),
        rsp_valid_target: mk_port_member(format!("{method_name}_rsp_valid")),
        rsp_data_target: method
            .ret
            .as_ref()
            .map(|_| mk_port_member(format!("{method_name}_rsp_data"))),
        rsp_tag_target: method
            .out_of_order_tags
            .as_ref()
            .map(|_| mk_port_member(format!("{method_name}_rsp_tag"))),
    };
    inline_lower_tlm_target_with_io(t, binding, method, io)
}

fn inline_lower_tlm_target_with_io(
    t: ThreadBlock,
    binding: &TlmTargetBinding,
    method: &TlmMethodMeta,
    io: TlmTargetIo,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let port = &binding.port.name;
    let method_name = &binding.method.name;
    let span = t.span;
    let mk_ident = |name: String| Ident { name, span };
    let mk_port_member = |member: String| {
        Expr::new(
            ExprKind::FieldAccess(
                Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
                mk_ident(member),
            ),
            span,
        )
    };
    // Arg renames: user-bound arg name → latched reg name.
    let mut arg_renames: Vec<(String, String)> = Vec::new();
    let mut latch_regs: Vec<RegDecl> = Vec::new();
    let tag_latch_name = method.out_of_order_tags.as_ref().map(|tag_w| {
        let latch_name = format!("_tlm_{port}_{method_name}{}_tag_latched", io.suffix);
        latch_regs.push(RegDecl {
            name: mk_ident(latch_name.clone()),
            ty: TypeExpr::UInt(Box::new(tag_w.clone())),
            init: None,
            reset: RegReset::Inherit(
                t.reset.clone(),
                Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
            ),
            guard: None,
            multicycle: None,
            span,
        });
        latch_name
    });
    for (user_arg, method_arg) in binding.args.iter().zip(method.args.iter()) {
        let latch_name = format!(
            "_tlm_{port}_{method_name}{}_{}_latched",
            io.suffix, method_arg.0.name
        );
        latch_regs.push(RegDecl {
            name: mk_ident(latch_name.clone()),
            ty: method_arg.1.clone(),
            init: None,
            // Type-aware zero: float-typed method args need a float-kind
            // reset literal (+0.0 encodes as all-zero bits in every format)
            // or the integer-literal-in-float-slot guard rejects the
            // synthesized reg.
            reset: RegReset::Inherit(t.reset.clone(), tlm_zero_for_type(&method_arg.1, span)),
            guard: None,
            multicycle: None,
            span,
        });
        arg_renames.push((user_arg.name.clone(), latch_name));
    }

    let mut body_before_return = Vec::new();
    let mut return_expr: Option<Expr> = None;
    let mut saw_return = false;
    for stmt in t.body.into_iter() {
        match stmt {
            ThreadStmt::Return(e, ret_span) => {
                if saw_return {
                    return Err(CompileError::general(
                        "`return` may appear only once in a TLM target thread body",
                        ret_span,
                    ));
                }
                saw_return = true;
                let mut renamed = e;
                for (from, to) in &arg_renames {
                    renamed = rewrite_var_expr(renamed, from, to);
                }
                return_expr = Some(renamed);
            }
            other if saw_return => {
                return Err(CompileError::general(
                    "statements after `return` are not supported in TLM target thread bodies",
                    thread_stmt_span(&other),
                ));
            }
            other => {
                let mut renamed = other;
                for (from, to) in &arg_renames {
                    renamed = rewrite_loop_var(&renamed, from, to);
                }
                body_before_return.push(renamed);
            }
        }
    }
    if return_expr.is_none()
        && method.ret.is_some()
        && !thread_block_always_returns(&body_before_return)
    {
        return Err(CompileError::general(
            &format!(
                "`thread {}.{}(...)` must end with `return <expr>;` or all control-flow paths must return (method declares return type {:?})",
                port, method_name, method.ret,
            ),
            span,
        ));
    }

    let cnt_width = infer_for_cnt_width(&body_before_return, &HashMap::new()).max(32);
    let loop_cnt_name_base = format!("_tlm_{port}_{method_name}{}_loop_cnt", io.suffix);
    let mut early_return_exprs: Vec<Expr> = Vec::new();
    let mut loop_id_gen: u32 = 0;
    let mut body_states = if body_before_return.is_empty() {
        Vec::new()
    } else {
        partition_tlm_target_thread_body_with_loop_ids(
            &body_before_return,
            span,
            cnt_width,
            &mut early_return_exprs,
            &mut loop_id_gen,
        )?
    };
    let num_loop_counters = loop_id_gen as usize;
    // Each `for` instance in the TLM target body got its own
    // `_loop_cnt_{id}` placeholder; rename each to a unique
    // `<base>_{id}` so nested loops don't share a counter (issue #414).
    let loop_renames: Vec<(String, String)> = (0..num_loop_counters)
        .map(|id| {
            (
                format!("_loop_cnt_{}", id),
                format!("{}_{}", loop_cnt_name_base, id),
            )
        })
        .collect();
    for (old, new) in &loop_renames {
        for state in &mut body_states {
            rename_ident_in_comb_stmts(&mut state.comb_stmts, old, new);
            rename_ident_in_stmts(&mut state.seq_stmts, old, new);
            if let Some(cond) = &mut state.transition_cond {
                rename_ident_in_expr(cond, old, new);
            }
            for (cond, _) in &mut state.multi_transitions {
                rename_ident_in_expr(cond, old, new);
            }
        }
        for expr in &mut early_return_exprs {
            rename_ident_in_expr(expr, old, new);
        }
    }

    let mut response_exprs = early_return_exprs;
    let terminal_response_slot = return_expr.as_ref().map(|_| {
        let slot = response_exprs.len();
        response_exprs.push(return_expr.clone().unwrap());
        slot
    });
    let response_count = response_exprs.len().max(1);

    // Total states: ENTRY (0) + body states + one response state per return.
    let total_states = 1 + body_states.len() + response_count;
    let state_width = clog2_width(total_states as u64);
    let entry_idx = 0u64;
    let response_base = 1 + body_states.len();
    let fallback_response_idx = (response_base + terminal_response_slot.unwrap_or(0)) as u64;

    let state_reg_name = format!("_tlm_{port}_{method_name}{}_state", io.suffix);
    let state_ident = Expr::new(ExprKind::Ident(state_reg_name.clone()), span);
    let mk_state_lit = |v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(state_width, v)), span);
    let state_eq = |v: u64| {
        Expr::new(
            ExprKind::Binary(
                BinOp::Eq,
                Box::new(state_ident.clone()),
                Box::new(mk_state_lit(v)),
            ),
            span,
        )
    };

    // ── State register declaration ───────────────────────────────────────
    let state_reg = RegDecl {
        name: mk_ident(state_reg_name.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(state_width as u64)),
            span,
        ))),
        init: None,
        reset: RegReset::Inherit(
            t.reset.clone(),
            Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
        ),
        guard: None,
        multicycle: None,
        span,
    };
    let has_wait_cycles = body_states.iter().any(|s| s.wait_cycles.is_some());
    let wait_cnt_name = format!("_tlm_{port}_{method_name}{}_wait_cnt", io.suffix);
    let wait_cnt_ident = Expr::new(ExprKind::Ident(wait_cnt_name.clone()), span);
    let wait_count_init = |count: &Expr| -> Expr {
        if let Some(v) = literal_expr_u64(count) {
            Expr::new(
                ExprKind::Literal(LitKind::Sized(32, v.saturating_sub(1))),
                span,
            )
        } else {
            count.clone()
        }
    };
    let wait_cnt_zero = Expr::new(
        ExprKind::Binary(
            BinOp::Eq,
            Box::new(wait_cnt_ident.clone()),
            Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(32, 0)), span)),
        ),
        span,
    );
    let wait_cnt_dec = Expr::new(
        ExprKind::MethodCall(
            Box::new(Expr::new(
                ExprKind::Binary(
                    BinOp::Sub,
                    Box::new(wait_cnt_ident.clone()),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(32, 1)), span)),
                ),
                span,
            )),
            mk_ident("trunc".to_string()),
            vec![Expr::new(ExprKind::Literal(LitKind::Dec(32)), span)],
        ),
        span,
    );

    // ── Seq block: state transitions + arg latches + user seq assigns ──
    // Build nested if/elsif over state_reg.
    let mut seq_body: Vec<Stmt> = Vec::new();
    // State 0: ENTRY — if req_valid, latch args and advance to 1.
    let mut entry_then: Vec<Stmt> = Vec::new();
    for (user_arg, method_arg) in binding.args.iter().zip(method.args.iter()) {
        let latch_name = format!(
            "_tlm_{port}_{method_name}{}_{}_latched",
            io.suffix, method_arg.0.name
        );
        entry_then.push(Stmt::Assign(RegAssign {
            target: Expr::new(ExprKind::Ident(latch_name), span),
            value: mk_port_member(format!("{method_name}_{}", method_arg.0.name)),
            span,
        }));
        let _ = user_arg;
    }
    if let Some(latch_name) = &tag_latch_name {
        entry_then.push(Stmt::Assign(RegAssign {
            target: Expr::new(ExprKind::Ident(latch_name.clone()), span),
            value: mk_port_member(format!("{method_name}_req_tag")),
            span,
        }));
    }
    let transition_to_body_or_respond = |target: usize, state_ident: Expr| -> Vec<Stmt> {
        let mut stmts = Vec::new();
        if thread_target_return_idx(target).is_none() {
            if let Some(wait) = body_states.get(target).and_then(|s| s.wait_cycles.as_ref()) {
                stmts.push(Stmt::Assign(RegAssign {
                    target: wait_cnt_ident.clone(),
                    value: wait_count_init(wait),
                    span,
                }));
            }
        }
        let target_state = if let Some(return_idx) = thread_target_return_idx(target) {
            (response_base + return_idx) as u64
        } else if target < body_states.len() {
            (target + 1) as u64
        } else {
            fallback_response_idx
        };
        stmts.push(Stmt::Assign(RegAssign {
            target: state_ident,
            value: mk_state_lit(target_state),
            span,
        }));
        stmts
    };

    let transition_to_state_response = |response_slot: usize, state_ident: Expr| -> Vec<Stmt> {
        vec![Stmt::Assign(RegAssign {
            target: state_ident,
            value: mk_state_lit((response_base + response_slot) as u64),
            span,
        })]
    };

    if let Some(first_wait) = body_states.first().and_then(|s| s.wait_cycles.as_ref()) {
        entry_then.push(Stmt::Assign(RegAssign {
            target: wait_cnt_ident.clone(),
            value: wait_count_init(first_wait),
            span,
        }));
    }
    entry_then.push(Stmt::Assign(RegAssign {
        target: state_ident.clone(),
        value: if body_states.is_empty() {
            mk_state_lit(fallback_response_idx)
        } else {
            mk_state_lit(1)
        },
        span,
    }));
    let entry_branch_cond = Expr::new(
        ExprKind::Binary(
            BinOp::And,
            Box::new(state_eq(entry_idx)),
            Box::new(io.req_valid.clone()),
        ),
        span,
    );
    seq_body.push(Stmt::IfElse(IfElseOf {
        cond: entry_branch_cond,
        then_stmts: entry_then,
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));
    // User body states 1..N. Their relative targets come from the ordinary
    // thread partitioner; falling off the body enters the terminal response
    // state, while early returns enter their own response states.
    for (i, us) in body_states.iter().enumerate() {
        let state_idx = (i + 1) as u64;
        if us.wait_cycles.is_some() {
            seq_body.push(Stmt::IfElse(IfElseOf {
                cond: Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(state_eq(state_idx)),
                        Box::new(Expr::new(
                            ExprKind::Unary(UnaryOp::Not, Box::new(wait_cnt_zero.clone())),
                            span,
                        )),
                    ),
                    span,
                ),
                then_stmts: vec![Stmt::Assign(RegAssign {
                    target: wait_cnt_ident.clone(),
                    value: wait_cnt_dec.clone(),
                    span,
                })],
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }

        if !us.multi_transitions.is_empty() {
            for (cond, target) in &us.multi_transitions {
                let target_idx = *target;
                let mut then_stmts = us.seq_stmts.clone();
                then_stmts.extend(transition_to_body_or_respond(
                    target_idx,
                    state_ident.clone(),
                ));
                let mut branch_cond = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(state_eq(state_idx)),
                        Box::new(cond.clone()),
                    ),
                    span,
                );
                if us.wait_cycles.is_some() {
                    branch_cond = Expr::new(
                        ExprKind::Binary(
                            BinOp::And,
                            Box::new(branch_cond),
                            Box::new(wait_cnt_zero.clone()),
                        ),
                        span,
                    );
                }
                seq_body.push(Stmt::IfElse(IfElseOf {
                    cond: branch_cond,
                    then_stmts,
                    else_stmts: Vec::new(),
                    unique: false,
                    span,
                }));
            }
        } else {
            let mut then_stmts = us.seq_stmts.clone();
            if let Some(return_idx) = us.terminal_return {
                then_stmts.extend(transition_to_state_response(
                    return_idx,
                    state_ident.clone(),
                ));
            } else {
                then_stmts.extend(transition_to_body_or_respond(i + 1, state_ident.clone()));
            }
            let transition_cond = us
                .transition_cond
                .clone()
                .unwrap_or_else(|| Expr::new(ExprKind::Bool(true), span));
            let mut branch_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::And,
                    Box::new(state_eq(state_idx)),
                    Box::new(transition_cond),
                ),
                span,
            );
            if us.wait_cycles.is_some() {
                branch_cond = Expr::new(
                    ExprKind::Binary(
                        BinOp::And,
                        Box::new(branch_cond),
                        Box::new(wait_cnt_zero.clone()),
                    ),
                    span,
                );
            }
            seq_body.push(Stmt::IfElse(IfElseOf {
                cond: branch_cond,
                then_stmts,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }
    // Response states → entry (loop back) when rsp_ready.
    for slot in 0..response_count {
        let response_idx = (response_base + slot) as u64;
        let respond_branch_cond = Expr::new(
            ExprKind::Binary(
                BinOp::And,
                Box::new(state_eq(response_idx)),
                Box::new(io.rsp_ready.clone()),
            ),
            span,
        );
        seq_body.push(Stmt::IfElse(IfElseOf {
            cond: respond_branch_cond,
            then_stmts: vec![Stmt::Assign(RegAssign {
                target: state_ident.clone(),
                value: mk_state_lit(entry_idx),
                span,
            })],
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }

    let reg_block = RegBlock {
        clock: t.clock.clone(),
        clock_edge: t.clock_edge,
        stmts: seq_body,
        span,
    };

    // ── Comb block: drive req_ready / rsp_valid / rsp_data ──────────────
    let mut comb_stmts: Vec<Stmt> = Vec::new();
    // req_ready = (state == 0)
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: io.req_ready_target.clone(),
        value: state_eq(entry_idx),
        span,
    }));
    // rsp_valid = state in any generated response state.
    let mut rsp_valid_expr = state_eq(response_base as u64);
    for slot in 1..response_count {
        rsp_valid_expr = Expr::new(
            ExprKind::Binary(
                BinOp::Or,
                Box::new(rsp_valid_expr),
                Box::new(state_eq((response_base + slot) as u64)),
            ),
            span,
        );
    }
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: io.rsp_valid_target.clone(),
        value: rsp_valid_expr,
        span,
    }));
    // rsp_data = selected return expression (only observed when rsp_valid).
    if method.ret.is_some() {
        if let Some(target) = io.rsp_data_target.clone() {
            if let Some(first) = response_exprs.first() {
                comb_stmts.push(Stmt::Assign(CombAssign {
                    target: target.clone(),
                    value: first.clone(),
                    span,
                }));
                for (slot, expr) in response_exprs.iter().enumerate() {
                    comb_stmts.push(Stmt::IfElse(IfElse {
                        cond: state_eq((response_base + slot) as u64),
                        then_stmts: vec![Stmt::Assign(CombAssign {
                            target: target.clone(),
                            value: expr.clone(),
                            span,
                        })],
                        else_stmts: Vec::new(),
                        unique: false,
                        span,
                    }));
                }
            }
        }
    }
    if let Some(latch_name) = &tag_latch_name {
        if let Some(target) = io.rsp_tag_target.clone() {
            comb_stmts.push(Stmt::Assign(CombAssign {
                target,
                value: Expr::new(ExprKind::Ident(latch_name.clone()), span),
                span,
            }));
        }
    }
    // User-written CombAssigns from the body — per-state guarded.
    for (i, us) in body_states.iter().enumerate() {
        let state_idx = (i + 1) as u64;
        if !us.comb_stmts.is_empty() {
            comb_stmts.push(Stmt::IfElse(IfElse {
                cond: state_eq(state_idx),
                then_stmts: us.comb_stmts.clone(),
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }

    let comb_block = CombBlock {
        stmts: comb_stmts,
        span,
    };

    // ── Assemble output items ────────────────────────────────────────────
    let mut items: Vec<ModuleBodyItem> = Vec::new();
    items.push(ModuleBodyItem::RegDecl(state_reg));
    if has_wait_cycles {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: mk_ident(wait_cnt_name),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(32)),
                span,
            ))),
            init: None,
            reset: RegReset::Inherit(
                t.reset.clone(),
                Expr::new(ExprKind::Literal(LitKind::Dec(0)), span),
            ),
            guard: None,
            multicycle: None,
            span,
        }));
    }
    // One loop-counter reg per `for` instance in the TLM target body
    // (matches the unique names assigned by `lower_thread_for` via the
    // shared `loop_id_gen`).
    for id in 0..num_loop_counters {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: mk_ident(format!("{}_{}", loop_cnt_name_base, id)),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(cnt_width as u64)),
                span,
            ))),
            init: Some(Expr::new(ExprKind::Literal(LitKind::Dec(0)), span)),
            reset: RegReset::None,
            guard: None,
            multicycle: None,
            span,
        }));
    }
    for r in latch_regs {
        items.push(ModuleBodyItem::RegDecl(r));
    }
    items.push(ModuleBodyItem::RegBlock(reg_block));
    items.push(ModuleBodyItem::CombBlock(comb_block));
    Ok(items)
}

/// Width-of-state helper. Compatibility shim — delegates to [`crate::width::index_width`].
fn clog2_width(n: u64) -> u32 {
    crate::width::index_width(n)
}

fn literal_expr_u64(expr: &Expr) -> Option<u64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
        _ => None,
    }
}
