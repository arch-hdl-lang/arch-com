//! Elaboration pass: expands `generate for`/`if` blocks and monomorphizes
//! modules that are instantiated with different param combinations.
//!
//! Algorithm
//! ---------
//! 1. Compute default const-param values for every module.
//! 2. Collect raw param overrides from every `inst` block in the file
//!    (including inst blocks nested inside generate items).
//! 3. For each module, derive the set of *distinct effective-param maps*
//!    that appear across all inst sites (defaults + per-site overrides).
//!    If there is only one distinct map, the module keeps its original name.
//!    If there are multiple, every variant is emitted as a separate SV module
//!    named `ModName_PARAM1_VAL1_PARAM2_VAL2` (only params that differ across
//!    variants appear in the suffix; params are sorted alphabetically).
//! 4. Elaborate each variant: expand generate blocks using that variant's
//!    param map, rewrite inner inst module-names to point at the correct
//!    variant of the instantiated module, and rename the module itself.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::Span;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn elaborate(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    // Build enum variant → value map for resolving enum-typed params
    let enum_values: HashMap<String, Vec<(String, u64)>> = ast.items.iter()
        .filter_map(|item| {
            let e = match item {
                Item::Enum(e) => Some(e),
                Item::Package(p) => p.enums.first(),  // simplification: first enum in pkg
                _ => None,
            }?;
            let entries: Vec<(String, u64)> = e.variants.iter().enumerate().map(|(i, v)| {
                let val = e.values.get(i)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|expr| match &expr.kind {
                        ExprKind::Literal(LitKind::Dec(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Hex(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Bin(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
                        _ => None,
                    })
                    .unwrap_or(i as u64);
                (v.name.clone(), val)
            }).collect();
            Some((e.name.name.clone(), entries))
        })
        .collect();

    // Step 1 — default params (resolve enum variant defaults to integers first)
    let module_defaults: HashMap<String, HashMap<String, i64>> = ast
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Module(m) = item {
                Some((m.name.name.clone(), compute_defaults_with_enums(&m.params, &enum_values)))
            } else {
                None
            }
        })
        .collect();

    // Step 2 — raw overrides from every inst site in the file
    let mut inst_raw: HashMap<String, Vec<HashMap<String, i64>>> = HashMap::new();
    for item in &ast.items {
        if let Item::Module(m) = item {
            collect_raw_overrides_from_body(&m.body, &mut inst_raw);
        }
    }

    // Step 3 — distinct effective-param sets and variant names per module
    let module_variants = compute_all_variants(&ast.items, &module_defaults, &inst_raw);

    // Step 4 — elaborate and emit
    let mut new_items: Vec<Item> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    for item in ast.items {
        match item {
            Item::Module(m) => {
                let variants = module_variants.get(&m.name.name).cloned().unwrap_or_else(|| {
                    let d = module_defaults.get(&m.name.name).cloned().unwrap_or_default();
                    vec![(d, m.name.name.clone())]
                });
                for (param_vals, variant_name) in variants {
                    match elaborate_module_variant(
                        m.clone(),
                        param_vals,
                        variant_name,
                        &module_variants,
                        &module_defaults,
                    ) {
                        Ok(elaborated) => new_items.push(Item::Module(elaborated)),
                        Err(mut errs) => errors.append(&mut errs),
                    }
                }
            }
            Item::Pipeline(p) => new_items.push(Item::Pipeline(p)),
            Item::Package(p) => new_items.push(Item::Package(p)),
            Item::Use(u) => new_items.push(Item::Use(u)),
            other => new_items.push(other),
        }
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(SourceFile { items: new_items })
    }
}

// ── Step 2: collect raw inst overrides ───────────────────────────────────────

fn collect_raw_overrides_from_body(
    body: &[ModuleBodyItem],
    out: &mut HashMap<String, Vec<HashMap<String, i64>>>,
) {
    for item in body {
        match item {
            ModuleBodyItem::Inst(inst) => record_inst(inst, out),
            ModuleBodyItem::Generate(gen) => {
                let all_items: Vec<&GenItem> = match gen {
                    GenerateDecl::For(gf) => gf.items.iter().collect(),
                    GenerateDecl::If(gi) => gi.then_items.iter()
                        .chain(gi.else_items.iter()).collect(),
                };
                for item in all_items {
                    if let GenItem::Inst(inst) = item {
                        record_inst(inst, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn record_inst(inst: &InstDecl, out: &mut HashMap<String, Vec<HashMap<String, i64>>>) {
    let mut overrides = HashMap::new();
    for pa in &inst.param_assigns {
        if let Some(v) = try_eval_i64(&pa.value, &HashMap::new()) {
            overrides.insert(pa.name.name.clone(), v);
        }
    }
    // Encode reset-type overrides as synthetic params so the variant system tracks them.
    // A connection of the form `rst <- signal as Reset<Async, Low>` is parsed as an
    // `ExprKind::As(signal, TypeExpr::Reset(...))` expression. Extract those here.
    // Key format: "__ro__<port_name>__kind" (0=Sync,1=Async) and "__ro__<port_name>__level" (0=High,1=Low)
    for conn in &inst.connections {
        if let ExprKind::Cast(_, ty) = &conn.signal.kind {
            if let TypeExpr::Reset(kind, level) = ty.as_ref() {
                let pname = &conn.port_name.name;
                overrides.insert(format!("__ro__{pname}__kind"),  if kind == &ResetKind::Async { 1 } else { 0 });
                overrides.insert(format!("__ro__{pname}__level"), if level == &ResetLevel::Low { 1 } else { 0 });
            }
        }
    }
    out.entry(inst.module_name.name.clone()).or_default().push(overrides);
}

// ── Step 3: compute variants ──────────────────────────────────────────────────

/// Returns `module_name → Vec<(effective_params, variant_name)>`.
fn compute_all_variants(
    items: &[Item],
    module_defaults: &HashMap<String, HashMap<String, i64>>,
    inst_raw: &HashMap<String, Vec<HashMap<String, i64>>>,
) -> HashMap<String, Vec<(HashMap<String, i64>, String)>> {
    let mut result = HashMap::new();

    for item in items {
        if let Item::Module(m) = item {
            let defaults = module_defaults.get(&m.name.name).cloned().unwrap_or_default();

            // Compute effective params for each inst site (deduped)
            let mut effective_sets: Vec<HashMap<String, i64>> = Vec::new();

            if let Some(raw_list) = inst_raw.get(&m.name.name) {
                for raw in raw_list {
                    let mut effective = defaults.clone();
                    effective.extend(raw.iter().map(|(k, v)| (k.clone(), *v)));
                    if !effective_sets.contains(&effective) {
                        effective_sets.push(effective);
                    }
                }
            }

            // Module never instantiated — use defaults as the sole variant
            if effective_sets.is_empty() {
                effective_sets.push(defaults);
            }

            let variants = if effective_sets.len() == 1 {
                // Only one combination → keep original name
                vec![(effective_sets.into_iter().next().unwrap(), m.name.name.clone())]
            } else {
                // Multiple combinations → mangle names
                let varying = find_varying_params(&effective_sets);
                effective_sets
                    .into_iter()
                    .map(|params| {
                        let name = make_variant_name(&m.name.name, &params, &varying);
                        (params, name)
                    })
                    .collect()
            };

            result.insert(m.name.name.clone(), variants);
        }
    }

    result
}

fn find_varying_params(param_sets: &[HashMap<String, i64>]) -> Vec<String> {
    let all_keys: std::collections::HashSet<String> =
        param_sets.iter().flat_map(|m| m.keys().cloned()).collect();

    let mut varying: Vec<String> = all_keys
        .into_iter()
        .filter(|k| {
            let first = param_sets[0].get(k);
            param_sets[1..].iter().any(|m| m.get(k) != first)
        })
        .collect();

    varying.sort(); // deterministic order
    varying
}

fn make_variant_name(base: &str, params: &HashMap<String, i64>, varying: &[String]) -> String {
    // Regular param suffixes (skip __ro__* synthetic reset-override keys)
    let regular: Vec<String> = varying
        .iter()
        .filter(|k| !k.starts_with("__ro__"))
        .map(|k| format!("{}_{}", k, params.get(k).copied().unwrap_or(0)))
        .collect();

    // Reset-override suffixes: group by port name for a clean suffix like rst_Async_Low
    let mut ro_ports: Vec<String> = varying
        .iter()
        .filter(|k| k.starts_with("__ro__") && k.ends_with("__kind"))
        .map(|k| {
            // Extract port name: "__ro__PORT__kind" → "PORT"
            let port = &k["__ro__".len()..k.len() - "__kind".len()];
            let kind_val = params.get(k.as_str()).copied().unwrap_or(0);
            let level_key = format!("__ro__{port}__level");
            let level_val = params.get(&level_key).copied().unwrap_or(0);
            let kind_str  = if kind_val  == 1 { "Async" } else { "Sync"  };
            let level_str = if level_val == 1 { "Low"   } else { "High"  };
            format!("{port}_{kind_str}_{level_str}")
        })
        .collect();
    ro_ports.sort();

    let mut parts = regular;
    parts.extend(ro_ports);

    if parts.is_empty() {
        base.to_string()
    } else {
        format!("{}__{}", base, parts.join("_"))
    }
}

// ── Step 4: elaborate a single module variant ─────────────────────────────────

fn elaborate_module_variant(
    m: ModuleDecl,
    param_vals: HashMap<String, i64>,
    variant_name: String,
    module_variants: &HashMap<String, Vec<(HashMap<String, i64>, String)>>,
    module_defaults: &HashMap<String, HashMap<String, i64>>,
) -> Result<ModuleDecl, Vec<CompileError>> {
    // Expand generate blocks
    let mut extra_ports: Vec<PortDecl> = Vec::new();
    let mut pre_rewrite: Vec<ModuleBodyItem> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    for item in m.body {
        match item {
            ModuleBodyItem::Generate(gen) => match expand_generate(gen, &param_vals) {
                Ok((ports, items)) => {
                    extra_ports.extend(ports);
                    pre_rewrite.extend(items);
                }
                Err(mut errs) => errors.append(&mut errs),
            },
            other => pre_rewrite.push(other),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Rewrite inst module-names → variant names
    let new_body = pre_rewrite
        .into_iter()
        .map(|item| match item {
            ModuleBodyItem::Inst(inst) => {
                ModuleBodyItem::Inst(rewrite_inst(inst, module_variants, module_defaults))
            }
            other => other,
        })
        .collect();

    let mut new_name = m.name.clone();
    new_name.name = variant_name;

    let mut all_ports = m.ports;
    all_ports.extend(extra_ports);

    // Apply reset-type overrides from inst-site `as Reset<...>` annotations.
    // Synthetic keys: "__ro__<port>__kind" (0=Sync,1=Async), "__ro__<port>__level" (0=High,1=Low)
    for port in &mut all_ports {
        if let TypeExpr::Reset(_, _) = &port.ty {
            let kind_key  = format!("__ro__{}__kind",  port.name.name);
            let level_key = format!("__ro__{}__level", port.name.name);
            if let Some(&k) = param_vals.get(&kind_key) {
                let l = param_vals.get(&level_key).copied().unwrap_or(0);
                let new_kind  = if k == 1 { ResetKind::Async } else { ResetKind::Sync  };
                let new_level = if l == 1 { ResetLevel::Low   } else { ResetLevel::High };
                port.ty = TypeExpr::Reset(new_kind, new_level);
            }
        }
    }

    // Update param defaults to match the monomorphized values so
    // the SV declaration is consistent with the expanded body.
    // - Enum-typed params: preserve the EnumVariant expression for clean SV output.
    // - Derived params (default expr references other params): preserve the original
    //   expression so SV emits e.g. `parameter int NBW_MULT = DATA_WIDTH + COEFF_WIDTH`
    //   instead of a hardcoded literal. This allows derived params to update correctly
    //   when a parent param is overridden at instantiation.
    // - Literal-only params: replace with the evaluated literal.
    let param_names: std::collections::HashSet<&str> = param_vals.keys().map(|s| s.as_str()).collect();
    let new_params: Vec<ParamDecl> = m.params.into_iter().map(|mut p| {
        if let Some(&val) = param_vals.get(&p.name.name) {
            if matches!(p.kind, ParamKind::EnumConst(_)) {
                // Preserve the EnumVariant expression for clean SV output
            } else if p.default.as_ref().map_or(false, |d| expr_references_params(d, &param_names)) {
                // Preserve original expression for derived params
            } else {
                p.default = Some(Expr::new(
                    ExprKind::Literal(LitKind::Dec(val as u64)),
                    p.name.span,
                ));
            }
        }
        p
    }).collect();

    Ok(ModuleDecl { name: new_name, params: new_params, ports: all_ports, body: new_body, implements: m.implements, hooks: m.hooks, cdc_safe: m.cdc_safe, span: m.span })
}

/// Rewrite an inst's `module_name` to the correct variant name.
fn rewrite_inst(
    inst: InstDecl,
    module_variants: &HashMap<String, Vec<(HashMap<String, i64>, String)>>,
    module_defaults: &HashMap<String, HashMap<String, i64>>,
) -> InstDecl {
    let variants = match module_variants.get(&inst.module_name.name) {
        Some(v) if v.len() > 1 => v,
        _ => return inst, // single variant → name unchanged
    };

    // Compute effective params for this inst (regular + reset-override synthetic params)
    let defaults = module_defaults
        .get(&inst.module_name.name)
        .cloned()
        .unwrap_or_default();
    let mut effective = defaults;
    for pa in &inst.param_assigns {
        if let Some(v) = try_eval_i64(&pa.value, &HashMap::new()) {
            effective.insert(pa.name.name.clone(), v);
        }
    }
    for conn in &inst.connections {
        if let ExprKind::Cast(_, ty) = &conn.signal.kind {
            if let TypeExpr::Reset(kind, level) = ty.as_ref() {
                let pname = &conn.port_name.name;
                effective.insert(format!("__ro__{pname}__kind"),  if kind == &ResetKind::Async { 1 } else { 0 });
                effective.insert(format!("__ro__{pname}__level"), if level == &ResetLevel::Low { 1 } else { 0 });
            }
        }
    }

    // Find matching variant
    for (params, variant_name) in variants {
        if *params == effective {
            let mut new_inst = inst;
            new_inst.module_name.name = variant_name.clone();
            return new_inst;
        }
    }

    inst // no match (shouldn't happen) — leave unchanged
}

// ── Generate expansion ────────────────────────────────────────────────────────

fn expand_generate(
    gen: GenerateDecl,
    param_vals: &HashMap<String, i64>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    match gen {
        GenerateDecl::For(gf) => expand_generate_for(gf, param_vals),
        GenerateDecl::If(gi) => expand_generate_if(gi, param_vals),
    }
}

/// Check whether an expression references any identifier that is a param name.
fn expr_references_param(expr: &Expr, param_names: &[String]) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => param_names.contains(name),
        ExprKind::Binary(_, l, r) => {
            expr_references_param(l, param_names)
                || expr_references_param(r, param_names)
        }
        ExprKind::Unary(_, e) => expr_references_param(e, param_names),
        ExprKind::Clog2(e)
        | ExprKind::Signed(e)
        | ExprKind::Unsigned(e) => expr_references_param(e, param_names),
        _ => false,
    }
}

fn expand_generate_for(
    gf: GenerateFor,
    param_vals: &HashMap<String, i64>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    // Collect param names from param_vals
    let param_names: Vec<String> = param_vals.keys().cloned().collect();

    let has_port_items = gf.items.iter().any(|item| matches!(item, GenItem::Port(_)));
    let has_thread_items = gf.items.iter().any(|item| matches!(item, GenItem::Thread(_)));
    let range_depends_on_param = expr_references_param(&gf.start, &param_names)
        || expr_references_param(&gf.end, &param_names);

    // Try to evaluate the range bounds
    let start_val = try_eval_i64(&gf.start, param_vals);
    let end_val = try_eval_i64(&gf.end, param_vals);

    // If the range references a param and there are no port or thread items,
    // preserve the generate block as-is so codegen emits SV generate for.
    // This allows the SV to be parameterized (e.g. NUM_MODULES can be overridden).
    // Threads must always be expanded (they need concrete lowering to FSMs).
    if range_depends_on_param && !has_port_items && !has_thread_items {
        return Ok((
            Vec::new(),
            vec![ModuleBodyItem::Generate(GenerateDecl::For(gf))],
        ));
    }

    let start = start_val.ok_or_else(|| {
        vec![CompileError::general(
            "generate for: start expression must be a compile-time constant",
            gf.start.span,
        )]
    })?;
    let end = end_val.ok_or_else(|| {
        vec![CompileError::general(
            "generate for: end expression must be a compile-time constant",
            gf.end.span,
        )]
    })?;

    if end < start {
        return Ok((Vec::new(), Vec::new()));
    }

    let var = &gf.var.name;
    let mut ports = Vec::new();
    let mut body = Vec::new();

    for i in start..=end {
        for item in &gf.items {
            match item {
                GenItem::Port(p) => ports.push(subst_port(p, var, i)),
                GenItem::Inst(inst) => body.push(ModuleBodyItem::Inst(subst_inst(inst, var, i))),
                GenItem::Thread(t) => body.push(ModuleBodyItem::Thread(subst_thread(t, var, i))),
            }
        }
    }

    Ok((ports, body))
}

fn expand_generate_if(
    gi: GenerateIf,
    param_vals: &HashMap<String, i64>,
) -> Result<(Vec<PortDecl>, Vec<ModuleBodyItem>), Vec<CompileError>> {
    let cond = try_eval_bool(&gi.cond, param_vals).ok_or_else(|| {
        vec![CompileError::general(
            "generate if: condition must be a compile-time constant boolean \
             (literal, param name, or comparison of params/literals)",
            gi.cond.span,
        )]
    })?;

    let active_items = if cond { gi.then_items } else { gi.else_items };

    let mut ports = Vec::new();
    let mut body = Vec::new();
    for item in active_items {
        match item {
            GenItem::Port(p) => ports.push(p),
            GenItem::Inst(inst) => body.push(ModuleBodyItem::Inst(inst)),
            GenItem::Thread(t) => body.push(ModuleBodyItem::Thread(t)),
        }
    }
    Ok((ports, body))
}

// ── Substitution helpers ──────────────────────────────────────────────────────

fn subst_port(p: &PortDecl, var: &str, val: i64) -> PortDecl {
    PortDecl {
        name: subst_ident(&p.name, var, val),
        direction: p.direction,
        ty: subst_type_expr(&p.ty, var, val),
        default: p.default.as_ref().map(|e| subst_expr(e.clone(), var, val)),
        reg_info: p.reg_info.clone(),
        bus_info: p.bus_info.clone(),
        shared: p.shared,
        span: p.span,
    }
}

fn subst_inst(inst: &InstDecl, var: &str, val: i64) -> InstDecl {
    InstDecl {
        name: subst_ident(&inst.name, var, val),
        module_name: inst.module_name.clone(),
        param_assigns: inst
            .param_assigns
            .iter()
            .map(|pa| ParamAssign {
                name: pa.name.clone(),
                value: subst_expr(pa.value.clone(), var, val),
            })
            .collect(),
        connections: inst
            .connections
            .iter()
            .map(|c| Connection {
                port_name: subst_ident(&c.port_name, var, val),
                direction: c.direction,
                signal: subst_expr(c.signal.clone(), var, val),
                reset_override: c.reset_override,
                span: c.span,
            })
            .collect(),
        span: inst.span,
    }
}

fn subst_thread(t: &ThreadBlock, var: &str, val: i64) -> ThreadBlock {
    ThreadBlock {
        name: t.name.as_ref().map(|n| subst_ident(n, var, val)),
        clock: t.clock.clone(),
        clock_edge: t.clock_edge,
        reset: t.reset.clone(),
        reset_level: t.reset_level,
        once: t.once,
        body: t.body.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
        span: t.span,
    }
}

fn subst_thread_stmt(stmt: &ThreadStmt, var: &str, val: i64) -> ThreadStmt {
    match stmt {
        ThreadStmt::CombAssign(ca) => ThreadStmt::CombAssign(CombAssign {
            target: subst_expr_names(ca.target.clone(), var, val),
            value: subst_expr_names(ca.value.clone(), var, val),
            span: ca.span,
        }),
        ThreadStmt::SeqAssign(ra) => ThreadStmt::SeqAssign(RegAssign {
            target: subst_expr_names(ra.target.clone(), var, val),
            value: subst_expr_names(ra.value.clone(), var, val),
            span: ra.span,
        }),
        ThreadStmt::WaitUntil(cond, sp) => {
            ThreadStmt::WaitUntil(subst_expr_names(cond.clone(), var, val), *sp)
        }
        ThreadStmt::WaitCycles(n, sp) => {
            ThreadStmt::WaitCycles(subst_expr_names(n.clone(), var, val), *sp)
        }
        ThreadStmt::IfElse(ie) => ThreadStmt::IfElse(ThreadIfElse {
            cond: subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie.then_stmts.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
            else_stmts: ie.else_stmts.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
            span: ie.span,
        }),
        ThreadStmt::ForkJoin(branches, sp) => ThreadStmt::ForkJoin(
            branches.iter().map(|br| br.iter().map(|s| subst_thread_stmt(s, var, val)).collect()).collect(),
            *sp,
        ),
        ThreadStmt::For { var: fvar, start: fstart, end: fend, body, span } => ThreadStmt::For {
            var: subst_ident(fvar, var, val),
            start: subst_expr_names(fstart.clone(), var, val),
            end: subst_expr_names(fend.clone(), var, val),
            body: body.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
            span: *span,
        },
        ThreadStmt::Lock { resource, body, span } => ThreadStmt::Lock {
            resource: resource.clone(),
            body: body.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
            span: *span,
        },
        ThreadStmt::DoUntil { body, cond, span } => ThreadStmt::DoUntil {
            body: body.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
            cond: subst_expr_names(cond.clone(), var, val),
            span: *span,
        },
    }
}

/// Like `subst_expr` but also applies `subst_name` to all identifiers (for thread
/// signal name substitution: `valid_i` → `valid_0`).
fn subst_expr_names(expr: Expr, var: &str, val: i64) -> Expr {
    let new_kind = match expr.kind {
        ExprKind::Ident(ref name) => {
            // Exact match: bare loop variable → replace with literal
            if name == var {
                ExprKind::Literal(LitKind::Dec(val as u64))
            } else {
                // Suffix match: signal_i → signal_0 (name substitution)
                let new_name = subst_name(name, var, val);
                if new_name != *name {
                    ExprKind::Ident(new_name)
                } else {
                    return expr;
                }
            }
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            op,
            Box::new(subst_expr_names(*l, var, val)),
            Box::new(subst_expr_names(*r, var, val)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(op, Box::new(subst_expr_names(*e, var, val))),
        ExprKind::FieldAccess(e, f) => ExprKind::FieldAccess(
            Box::new(subst_expr_names(*e, var, val)),
            subst_ident(&f, var, val),
        ),
        ExprKind::MethodCall(e, m, args) => ExprKind::MethodCall(
            Box::new(subst_expr_names(*e, var, val)),
            m,
            args.into_iter().map(|a| subst_expr_names(a, var, val)).collect(),
        ),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(subst_expr_names(*base, var, val)),
            Box::new(subst_expr_names(*idx, var, val)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(subst_expr_names(*base, var, val)),
            Box::new(subst_expr_names(*hi, var, val)),
            Box::new(subst_expr_names(*lo, var, val)),
        ),
        ExprKind::Cast(e, ty) => ExprKind::Cast(
            Box::new(subst_expr_names(*e, var, val)),
            ty,
        ),
        ExprKind::Concat(exprs) => {
            ExprKind::Concat(exprs.into_iter().map(|e| subst_expr_names(e, var, val)).collect())
        }
        ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
            Box::new(subst_expr_names(*c, var, val)),
            Box::new(subst_expr_names(*t, var, val)),
            Box::new(subst_expr_names(*f, var, val)),
        ),
        other => other,
    };
    Expr { kind: new_kind, span: expr.span, parenthesized: expr.parenthesized }
}

fn subst_ident(ident: &Ident, var: &str, val: i64) -> Ident {
    Ident { name: subst_name(&ident.name, var, val), span: ident.span }
}

fn subst_name(name: &str, var: &str, val: i64) -> String {
    let suffix = format!("_{}", var);
    if name.ends_with(&suffix) {
        let base = &name[..name.len() - suffix.len()];
        format!("{}_{}", base, val)
    } else if name == var {
        format!("g{}", val)
    } else {
        name.to_string()
    }
}

fn subst_type_expr(ty: &TypeExpr, var: &str, val: i64) -> TypeExpr {
    match ty {
        TypeExpr::UInt(e) => TypeExpr::UInt(Box::new(subst_expr(*e.clone(), var, val))),
        TypeExpr::SInt(e) => TypeExpr::SInt(Box::new(subst_expr(*e.clone(), var, val))),
        TypeExpr::Vec(inner, size) => TypeExpr::Vec(
            Box::new(subst_type_expr(inner, var, val)),
            Box::new(subst_expr(*size.clone(), var, val)),
        ),
        other => other.clone(),
    }
}

fn subst_expr(expr: Expr, var: &str, val: i64) -> Expr {
    let new_kind = match expr.kind {
        ExprKind::Ident(ref name) if name == var => ExprKind::Literal(LitKind::Dec(val as u64)),
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            op,
            Box::new(subst_expr(*l, var, val)),
            Box::new(subst_expr(*r, var, val)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(op, Box::new(subst_expr(*e, var, val))),
        ExprKind::FieldAccess(e, f) => ExprKind::FieldAccess(Box::new(subst_expr(*e, var, val)), f),
        ExprKind::MethodCall(e, m, args) => ExprKind::MethodCall(
            Box::new(subst_expr(*e, var, val)),
            m,
            args.into_iter().map(|a| subst_expr(a, var, val)).collect(),
        ),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(subst_expr(*base, var, val)),
            Box::new(subst_expr(*idx, var, val)),
        ),
        ExprKind::BitSlice(base, hi, lo) => ExprKind::BitSlice(
            Box::new(subst_expr(*base, var, val)),
            Box::new(subst_expr(*hi, var, val)),
            Box::new(subst_expr(*lo, var, val)),
        ),
        ExprKind::Cast(e, ty) => ExprKind::Cast(Box::new(subst_expr(*e, var, val)), ty),
        ExprKind::Concat(exprs) => {
            ExprKind::Concat(exprs.into_iter().map(|e| subst_expr(e, var, val)).collect())
        }
        other => other,
    };
    Expr { kind: new_kind, span: expr.span, parenthesized: false }
}

/// Returns true if the expression references any identifier in `param_names`.
fn expr_references_params(expr: &Expr, param_names: &std::collections::HashSet<&str>) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => param_names.contains(name.as_str()),
        ExprKind::Binary(_, l, r) => {
            expr_references_params(l, param_names) || expr_references_params(r, param_names)
        }
        ExprKind::Unary(_, e) => expr_references_params(e, param_names),
        ExprKind::Clog2(e) => expr_references_params(e, param_names),
        ExprKind::FieldAccess(e, _) => expr_references_params(e, param_names),
        ExprKind::Index(e, i) => {
            expr_references_params(e, param_names) || expr_references_params(i, param_names)
        }
        ExprKind::Ternary(c, t, f) => {
            expr_references_params(c, param_names)
                || expr_references_params(t, param_names)
                || expr_references_params(f, param_names)
        }
        _ => false,
    }
}

// ── Const evaluation ──────────────────────────────────────────────────────────

/// Compute default values for all `const` params (used in Step 1).
fn compute_defaults_with_enums(
    params: &[ParamDecl],
    enum_values: &HashMap<String, Vec<(String, u64)>>,
) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    for p in params {
        match &p.kind {
            ParamKind::Const | ParamKind::WidthConst(..) => {
                if let Some(default) = &p.default {
                    if let Some(v) = try_eval_i64(default, &map) {
                        map.insert(p.name.name.clone(), v);
                    }
                }
            }
            ParamKind::EnumConst(enum_name) => {
                if let Some(default) = &p.default {
                    // Resolve EnumVariant expr to its integer value
                    let val = if let ExprKind::EnumVariant(_, variant) = &default.kind {
                        enum_values.get(enum_name)
                            .and_then(|entries| entries.iter().find(|(n, _)| *n == variant.name))
                            .map(|(_, v)| *v as i64)
                    } else {
                        try_eval_i64(default, &map)
                    };
                    if let Some(v) = val {
                        map.insert(p.name.name.clone(), v);
                    }
                }
            }
            _ => {}
        }
    }
    map
}

/// Evaluate an expression to an i64 using `param_vals` for identifier lookups.
pub fn try_eval_i64(expr: &Expr, param_vals: &HashMap<String, i64>) -> Option<i64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v)) => Some(*v as i64),
        ExprKind::Literal(LitKind::Hex(v)) => Some(*v as i64),
        ExprKind::Literal(LitKind::Bin(v)) => Some(*v as i64),
        ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as i64),
        ExprKind::Ident(name) => param_vals.get(name.as_str()).copied(),
        ExprKind::Binary(BinOp::Add, l, r) => {
            Some(try_eval_i64(l, param_vals)? + try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Sub, l, r) => {
            Some(try_eval_i64(l, param_vals)? - try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Mul, l, r) => {
            Some(try_eval_i64(l, param_vals)? * try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Div, l, r) => {
            let rv = try_eval_i64(r, param_vals)?;
            if rv == 0 { None } else { Some(try_eval_i64(l, param_vals)? / rv) }
        }
        ExprKind::Binary(BinOp::Mod, l, r) => {
            let rv = try_eval_i64(r, param_vals)?;
            if rv == 0 { None } else { Some(try_eval_i64(l, param_vals)? % rv) }
        }
        ExprKind::Unary(UnaryOp::Neg, e) => Some(-try_eval_i64(e, param_vals)?),
        ExprKind::Unary(UnaryOp::Not, e) => {
            Some(if try_eval_i64(e, param_vals)? != 0 { 0 } else { 1 })
        }
        // Comparison operators → 0 or 1
        ExprKind::Binary(BinOp::Eq, l, r) => {
            Some(if try_eval_i64(l, param_vals)? == try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Neq, l, r) => {
            Some(if try_eval_i64(l, param_vals)? != try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Lt, l, r) => {
            Some(if try_eval_i64(l, param_vals)? < try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Gt, l, r) => {
            Some(if try_eval_i64(l, param_vals)? > try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Lte, l, r) => {
            Some(if try_eval_i64(l, param_vals)? <= try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Gte, l, r) => {
            Some(if try_eval_i64(l, param_vals)? >= try_eval_i64(r, param_vals)? { 1 } else { 0 })
        }
        // Logical operators
        ExprKind::Binary(BinOp::And, l, r) => {
            Some(if try_eval_i64(l, param_vals)? != 0 && try_eval_i64(r, param_vals)? != 0 { 1 } else { 0 })
        }
        ExprKind::Binary(BinOp::Or, l, r) => {
            Some(if try_eval_i64(l, param_vals)? != 0 || try_eval_i64(r, param_vals)? != 0 { 1 } else { 0 })
        }
        // Bitwise operators
        ExprKind::Binary(BinOp::BitAnd, l, r) => {
            Some(try_eval_i64(l, param_vals)? & try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::BitOr, l, r) => {
            Some(try_eval_i64(l, param_vals)? | try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::BitXor, l, r) => {
            Some(try_eval_i64(l, param_vals)? ^ try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Shl, l, r) => {
            Some(try_eval_i64(l, param_vals)? << try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Shr, l, r) => {
            Some(try_eval_i64(l, param_vals)? >> try_eval_i64(r, param_vals)?)
        }
        // Ternary: cond ? then : else
        ExprKind::Ternary(cond, then_expr, else_expr) => {
            let c = try_eval_i64(cond, param_vals)?;
            if c != 0 {
                try_eval_i64(then_expr, param_vals)
            } else {
                try_eval_i64(else_expr, param_vals)
            }
        }
        // Bool literals
        ExprKind::Bool(b) => Some(if *b { 1 } else { 0 }),
        ExprKind::Clog2(arg) => {
            let v = try_eval_i64(arg, param_vals)? as u64;
            if v <= 1 { Some(1) } else { Some(64 - (v - 1).leading_zeros() as i64) }
        }
        _ => None,
    }
}

fn try_eval_bool(expr: &Expr, param_vals: &HashMap<String, i64>) -> Option<bool> {
    match &expr.kind {
        ExprKind::Bool(b) => Some(*b),
        ExprKind::Literal(LitKind::Dec(0)) => Some(false),
        ExprKind::Literal(LitKind::Dec(v)) if *v != 0 => Some(true),
        ExprKind::Ident(name) => param_vals.get(name.as_str()).map(|&v| v != 0),
        ExprKind::Binary(BinOp::Eq, l, r) => {
            Some(try_eval_i64(l, param_vals)? == try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Neq, l, r) => {
            Some(try_eval_i64(l, param_vals)? != try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Gt, l, r) => {
            Some(try_eval_i64(l, param_vals)? > try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Gte, l, r) => {
            Some(try_eval_i64(l, param_vals)? >= try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Lt, l, r) => {
            Some(try_eval_i64(l, param_vals)? < try_eval_i64(r, param_vals)?)
        }
        ExprKind::Binary(BinOp::Lte, l, r) => {
            Some(try_eval_i64(l, param_vals)? <= try_eval_i64(r, param_vals)?)
        }
        ExprKind::Unary(UnaryOp::Not, e) => Some(!try_eval_bool(e, param_vals)?),
        _ => None,
    }
}

fn _dummy_span() -> Span {
    Span::new(0, 0)
}

// ── Thread → FSM lowering ───────────────────────────────────────────────────

/// Lower all `thread` blocks in modules to FSM + inst.
///
/// For each module containing ThreadBlock items, this pass:
/// 1. Analyzes signals read/written by the thread
/// 2. Creates a top-level FsmDecl with auto-generated states
/// 3. Replaces the ThreadBlock with an InstDecl wiring up the FSM
pub fn lower_threads(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    let mut new_items: Vec<Item> = Vec::new();
    let mut extra_fsms: Vec<Item> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    for item in ast.items {
        match item {
            Item::Module(m) => {
                let has_threads = m.body.iter().any(|i| matches!(i, ModuleBodyItem::Thread(_)));
                if !has_threads {
                    new_items.push(Item::Module(m));
                    continue;
                }
                match lower_module_threads(m) {
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
    Ok(SourceFile { items: result })
}

/// Lower all threads in a single module to a SINGLE merged module.
///
/// All threads become per-thread state machines within one module.
/// Shared registers, lock arbitration, and output muxing are all
/// handled internally — no multi-driver issues.
fn lower_module_threads(m: ModuleDecl) -> Result<(ModuleDecl, Vec<Item>), Vec<CompileError>> {
    let sp = m.span;
    let type_map = build_module_type_map(&m);
    let _reg_map = build_module_reg_map(&m);
    let mut errors: Vec<CompileError> = Vec::new();

    // Collect threads and non-thread body items
    let mut threads: Vec<(String, ThreadBlock)> = Vec::new();
    let mut new_body: Vec<ModuleBodyItem> = Vec::new();
    let mut thread_idx = 0usize;

    for item in m.body {
        match item {
            ModuleBodyItem::Thread(t) => {
                let name = t.name.as_ref()
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| {
                        let n = if thread_idx == 0 { "thread".to_string() }
                                else { format!("thread{}", thread_idx) };
                        thread_idx += 1; n
                    });
                if t.name.is_some() { thread_idx += 1; }
                threads.push((name, t));
            }
            ModuleBodyItem::Resource(_) => {
                // Resource declarations consumed — lock logic generated inline
            }
            other => new_body.push(other),
        }
    }

    if threads.is_empty() {
        return Ok((ModuleDecl { body: new_body, ..m }, Vec::new()));
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
        let (cd, sd, ar) = collect_thread_signals(&t.body);
        all_comb_driven.extend(cd);
        all_seq_driven.extend(sd);
        all_read.extend(ar);
    }

    // Clock and reset ports (from first thread)
    let (clk_name, rst_name, rst_level) = {
        let t = &threads[0].1;
        let rk = type_map.get(&t.reset.name).and_then(|si| {
            if let TypeExpr::Reset(k, _) = &si.ty { Some(*k) } else { None }
        }).unwrap_or(ResetKind::Async);
        merged_ports.push(PortDecl {
            name: t.clock.clone(), direction: Direction::In,
            ty: type_map.get(&t.clock.name).map(|si| si.ty.clone())
                .unwrap_or(TypeExpr::Clock(Ident::new("SysDomain".to_string(), sp))),
            default: None, reg_info: None, bus_info: None, shared: None, span: sp,
        });
        merged_ports.push(PortDecl {
            name: t.reset.clone(), direction: Direction::In,
            ty: TypeExpr::Reset(rk, t.reset_level),
            default: None, reg_info: None, bus_info: None, shared: None, span: sp,
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

    // Input ports (read-only signals, excluding internal lock signals)
    let read_only: HashSet<String> = all_read.iter()
        .filter(|n| !all_comb_driven.contains(*n) && !all_seq_driven.contains(*n)
                && **n != clk_name && **n != rst_name
                && !n.starts_with("_t") // per-thread counters (_t0_cnt, _t0_loop_cnt, etc.)
                && **n != "_cnt" && **n != "_loop_cnt"
                && !lock_internal.contains(*n))
        .cloned().collect();
    let mut sorted_reads: Vec<&String> = read_only.iter().collect();
    sorted_reads.sort();
    for name in sorted_reads {
        if let Some(info) = type_map.get(name.as_str()) {
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp), direction: Direction::In,
                ty: info.ty.clone(),
                default: None, reg_info: None, bus_info: None, shared: None, span: sp,
            });
        }
    }

    // Output ports (comb-driven, excluding internal lock signals)
    let mut sorted_comb: Vec<&String> = all_comb_driven.iter()
        .filter(|n| !lock_internal.contains(*n))
        .collect();
    sorted_comb.sort();
    for name in sorted_comb {
        if let Some(info) = type_map.get(name.as_str()) {
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp), direction: Direction::Out,
                ty: info.ty.clone(),
                default: Some(make_zero_expr(sp)),
                reg_info: None, bus_info: None, shared: info.shared, span: sp,
            });
        }
    }

    // Output ports (seq-driven) — these are port-regs in the merged module
    let mut sorted_seq: Vec<&String> = all_seq_driven.iter().collect();
    sorted_seq.sort();
    for name in sorted_seq {
        if let Some(info) = type_map.get(name.as_str()) {
            merged_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp), direction: Direction::Out,
                ty: info.ty.clone(), default: None,
                reg_info: Some(PortRegInfo {
                    init: info.reg_init.clone(), reset: info.reg_reset.clone(),
                }),
                bus_info: None, shared: None, span: sp,
            });
        }
    }

    // ── Lock arbiter signals (internal to merged module) ─────────────
    // For each resource, create per-thread req/grant wires + priority arbiter
    let mut all_resources: HashSet<String> = HashSet::new();
    for (_, t) in &threads {
        all_resources.extend(collect_locked_resources(&t.body));
    }
    for res_name in &all_resources {
        let n_threads = threads.len();
        // Req and grant wires per thread
        for ti in 0..n_threads {
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_req_{}", res_name, ti), sp),
                ty: TypeExpr::Bool, span: sp,
            }));
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_grant_{}", res_name, ti), sp),
                ty: TypeExpr::Bool, span: sp,
            }));
        }
        // Default req = 0 — will be added to merged comb block later

        // Priority arbiter: grant[i] = req[i] && !grant[j<i]
        let mut arb_stmts: Vec<CombStmt> = Vec::new();
        for i in 0..n_threads {
            let grant_i = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, i)), sp);
            let mut cond = Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, i)), sp);
            for j in 0..i {
                let grant_j = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, j)), sp);
                cond = Expr::new(ExprKind::Binary(BinOp::And, Box::new(cond),
                    Box::new(Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(grant_j)), sp))), sp);
            }
            arb_stmts.push(CombStmt::Assign(CombAssign { target: grant_i, value: cond, span: sp }));
        }
        merged_body.push(ModuleBodyItem::CombBlock(CombBlock { stmts: arb_stmts, span: sp }));
    }

    // ── Collect shared(or) signal names for OR-accumulation ────────────
    let shared_or_signals: HashSet<String> = type_map.iter()
        .filter(|(_, info)| matches!(info.shared, Some(SharedReduction::Or)))
        .map(|(name, _)| name.clone())
        .collect();

    // shared(or) signals that are seq-driven need per-thread shadow wires + OR reduction
    let shared_or_seq: HashSet<String> = shared_or_signals.iter()
        .filter(|n| all_seq_driven.contains(*n))
        .cloned().collect();
    // shared(or) signals that are comb-driven use inline OR-accumulation (existing behavior)
    let shared_or_comb: HashSet<String> = shared_or_signals.iter()
        .filter(|n| all_comb_driven.contains(*n))
        .cloned().collect();

    // For seq shared(or) signals, create per-thread input wires and OR reduction
    let n_threads = threads.len();
    for sig_name in &shared_or_seq {
        if let Some(info) = type_map.get(sig_name.as_str()) {
            // Per-thread input wires: _sig_in_0, _sig_in_1, ...
            for ti in 0..n_threads {
                let wire_name = format!("_{}_in_{}", sig_name, ti);
                merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                    name: Ident::new(wire_name, sp),
                    ty: info.ty.clone(),
                    span: sp,
                }));
            }
            // OR reduction in comb block: sig_next = _sig_in_0 | _sig_in_1 | ...
            let mut or_expr = Expr::new(ExprKind::Ident(format!("_{}_in_0", sig_name)), sp);
            for ti in 1..n_threads {
                or_expr = Expr::new(ExprKind::Binary(
                    BinOp::BitOr,
                    Box::new(or_expr),
                    Box::new(Expr::new(ExprKind::Ident(format!("_{}_in_{}", sig_name, ti)), sp)),
                ), sp);
            }
            // Wire for OR reduction result
            let next_name = format!("_{}_next", sig_name);
            merged_body.push(ModuleBodyItem::LetBinding(LetBinding {
                name: Ident::new(next_name.clone(), sp),
                ty: Some(info.ty.clone()),
                value: or_expr,
                span: sp,
            }));
        }
    }

    // ── Per-thread state machines ──────────────────────────────────────
    let mut all_thread_comb: Vec<CombStmt> = Vec::new();
    let mut all_thread_seq: Vec<Stmt> = Vec::new();

    for (ti, (_tname, t)) in threads.iter().enumerate() {
        let cnt_width = infer_for_cnt_width(&t.body, &type_map);
        let mut raw_states = match partition_thread_body(&t.body, sp, cnt_width) {
            Ok(s) => s,
            Err(e) => { errors.push(e); continue; }
        };

        // Rename per-thread: lock signals, counter regs
        // Counters: _cnt → _t{ti}_cnt, _loop_cnt → _t{ti}_loop_cnt
        let cnt_renames = vec![
            ("_cnt".to_string(), format!("_t{}_cnt", ti)),
            ("_loop_cnt".to_string(), format!("_t{}_loop_cnt", ti)),
        ];
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
            }
        }
        // Rewrite seq assigns to shared(or) signals → comb assigns to per-thread shadow wires
        // e.g. `r_ready <= 1` in thread 2 → `_r_ready_in_2 = 1` (comb)
        if !shared_or_seq.is_empty() {
            for state in &mut raw_states {
                let mut moved_comb = Vec::new();
                let new_seq = rewrite_shared_or_seq_stmts(
                    &state.seq_stmts, &shared_or_seq, ti, sp, &mut moved_comb);
                state.seq_stmts = new_seq;
                state.comb_stmts.extend(moved_comb);
            }
        }

        if raw_states.is_empty() {
            errors.push(CompileError::general("thread must have at least one wait", sp));
            continue;
        }

        let n_states = raw_states.len();
        let state_reg = format!("_t{}_state", ti);
        let state_bits = if n_states <= 2 { 1u64 } else { ((n_states as f64).log2().ceil()) as u64 };

        // State register
        merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
            name: Ident::new(state_reg.clone(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(state_bits.max(1))), sp))),
            init: Some(make_zero_expr(sp)),
            reset: RegReset::Inherit(
                Ident::new(rst_name.clone(), sp),
                make_zero_expr(sp),
            ),
            span: sp,
        }));

        // Pre-process: add counter loads to states preceding wait_cycles states
        let cnt_name = format!("_t{}_cnt", ti);
        // Collect (state_idx, count_expr, transition_cond) tuples first to avoid borrow conflicts
        let mut counter_loads: Vec<(usize, Expr, Option<Expr>)> = Vec::new();
        for si in 0..raw_states.len() {
            let next = if si + 1 < raw_states.len() { si + 1 } else { 0 };
            if next < raw_states.len() {
                if let Some(ref count_expr) = raw_states[next].wait_cycles {
                    // Find the guard: either transition_cond or multi_transition that targets `next`
                    let cond = if let Some(ref c) = raw_states[si].transition_cond {
                        Some(c.clone())
                    } else {
                        // Check multi_transitions for one targeting `next`
                        raw_states[si].multi_transitions.iter()
                            .find(|(_, tgt)| *tgt == next || (*tgt >= raw_states.len() && next == si + 1))
                            .map(|(c, _)| c.clone())
                    };
                    counter_loads.push((si, count_expr.clone(), cond));
                }
            }
        }
        for (si, count_expr, cond) in counter_loads {
            // cnt <= (count - 32'd1).trunc<32>()
            let sub = Expr::new(ExprKind::Binary(
                BinOp::Sub,
                Box::new(count_expr.clone()),
                Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(32, 1)), sp)),
            ), sp);
            let load = Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(cnt_name.clone()), sp),
                value: Expr::new(ExprKind::MethodCall(
                    Box::new(sub),
                    Ident::new("trunc".to_string(), sp),
                    vec![Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp)],
                ), sp),
                span: sp,
            });
            if let Some(guard) = cond {
                raw_states[si].seq_stmts.push(Stmt::IfElse(IfElse {
                    cond: guard, then_stmts: vec![load],
                    else_stmts: Vec::new(), unique: false, span: sp,
                }));
            } else {
                raw_states[si].seq_stmts.push(load);
            }
        }

        // State transition always_ff
        let mut seq_stmts: Vec<Stmt> = Vec::new();
        for (si, raw) in raw_states.iter().enumerate() {
            // Only skip truly empty states that don't need state advancement
            let needs_transition = si + 1 < n_states || !t.once; // non-terminal states always need advancement
            if raw.seq_stmts.is_empty() && raw.transition_cond.is_none()
                && raw.wait_cycles.is_none() && raw.multi_transitions.is_empty()
                && !needs_transition {
                continue;
            }

            // Build transition + seq logic for this state
            let state_cond = Expr::new(ExprKind::Binary(
                BinOp::Eq,
                Box::new(Expr::new(ExprKind::Ident(state_reg.clone()), sp)),
                Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(si as u64)), sp)),
            ), sp);

            let mut body: Vec<Stmt> = Vec::new();

            // Seq assigns (fire on state entry)
            body.extend(raw.seq_stmts.clone());

            // State transitions
            // For thread_once: last state stays (terminal), otherwise wrap to 0
            let next_state = if si + 1 < n_states {
                si + 1
            } else if t.once {
                si // terminal: stay in last state
            } else {
                0 // repeating: wrap to first state
            };
            if !raw.multi_transitions.is_empty() {
                for (cond, target) in &raw.multi_transitions {
                    let tgt = if *target >= n_states {
                        if t.once { n_states - 1 } else { 0 }
                    } else { *target };
                    body.push(Stmt::IfElse(IfElse {
                        cond: cond.clone(),
                        then_stmts: vec![Stmt::Assign(RegAssign {
                            target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                            value: Expr::new(ExprKind::Literal(LitKind::Dec(tgt as u64)), sp),
                            span: sp,
                        })],
                        else_stmts: Vec::new(), unique: false, span: sp,
                    }));
                }
            } else if let Some(ref cond) = raw.transition_cond {
                body.push(Stmt::IfElse(IfElse {
                    cond: cond.clone(),
                    then_stmts: vec![Stmt::Assign(RegAssign {
                        target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                        value: Expr::new(ExprKind::Literal(LitKind::Dec(next_state as u64)), sp),
                        span: sp,
                    })],
                    else_stmts: Vec::new(), unique: false, span: sp,
                }));
            } else if let Some(ref _count_expr) = raw.wait_cycles {
                // Counter-based wait: decrement counter, transition when 0
                let cnt_name = format!("_t{}_cnt", ti);
                let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                let cnt_zero = Expr::new(ExprKind::Binary(
                    BinOp::Eq, Box::new(cnt_id.clone()),
                    Box::new(make_zero_expr(sp)),
                ), sp);
                // cnt <= (cnt - 32'd1).trunc<32>()
                let sub = Expr::new(ExprKind::Binary(
                    BinOp::Sub, Box::new(cnt_id),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(32, 1)), sp)),
                ), sp);
                body.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(cnt_name.clone()), sp),
                    value: Expr::new(ExprKind::MethodCall(
                        Box::new(sub),
                        Ident::new("trunc".to_string(), sp),
                        vec![Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp)],
                    ), sp),
                    span: sp,
                }));
                // Transition when counter hits 0
                body.push(Stmt::IfElse(IfElse {
                    cond: cnt_zero,
                    then_stmts: vec![Stmt::Assign(RegAssign {
                        target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                        value: Expr::new(ExprKind::Literal(LitKind::Dec(next_state as u64)), sp),
                        span: sp,
                    })],
                    else_stmts: Vec::new(), unique: false, span: sp,
                }));
            } else {
                // Unconditional transition
                body.push(Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                    value: Expr::new(ExprKind::Literal(LitKind::Dec(next_state as u64)), sp),
                    span: sp,
                }));
            }

            seq_stmts.push(Stmt::IfElse(IfElse {
                cond: state_cond,
                then_stmts: body,
                else_stmts: Vec::new(), unique: false, span: sp,
            }));
        }

        all_thread_seq.extend(seq_stmts);

        // Collect comb outputs for this thread (merged into one block later)
        // For shared(or) signals, transform `sig = val` → `sig = sig | val`
        for (si, raw) in raw_states.iter().enumerate() {
            let state_cond = Expr::new(ExprKind::Binary(
                BinOp::Eq,
                Box::new(Expr::new(ExprKind::Ident(state_reg.clone()), sp)),
                Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(si as u64)), sp)),
            ), sp);

            // This state's own comb outputs
            if !raw.comb_stmts.is_empty() {
                let transformed_stmts = transform_shared_or_assigns(&raw.comb_stmts, &shared_or_signals, sp);
                all_thread_comb.push(CombStmt::IfElse(CombIfElse {
                    cond: state_cond.clone(),
                    then_stmts: transformed_stmts,
                    else_stmts: Vec::new(),
                    unique: false, span: sp,
                }));
            }

            // TODO: Comb overlap optimization (drive next state's outputs on
            // transition cycle) — disabled pending proper lock-state awareness.
            // Re-enable when lock body states are tagged to prevent overlap
            // from leaking lock-guarded outputs into preceding states.
        }
    }

    // Add shared(or) seq reduction: sig <= _sig_next
    for sig_name in &shared_or_seq {
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

    // ── Per-thread counter registers ─────────────────────────────────────
    for (ti, (_, t)) in threads.iter().enumerate() {
        let has_counter = thread_has_wait_cycles(&t.body);
        if has_counter {
            merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(format!("_t{}_cnt", ti), sp),
                ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp))),
                init: Some(make_zero_expr(sp)),
                reset: RegReset::None, span: sp,
            }));
        }
        let has_for = thread_has_for(&t.body);
        if has_for {
            let for_cnt_width = infer_for_cnt_width(&t.body, &type_map);
            merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(format!("_t{}_loop_cnt", ti), sp),
                ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(for_cnt_width as u64)), sp))),
                init: Some(make_zero_expr(sp)),
                reset: RegReset::None, span: sp,
            }));
        }
    }

    // ── Merged comb block: defaults + all per-thread comb stmts ──────
    let mut merged_comb: Vec<CombStmt> = Vec::new();
    // Defaults: all comb outputs = 0
    for p in &merged_ports {
        if p.direction == Direction::Out && p.default.is_some() {
            merged_comb.push(CombStmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(p.name.name.clone()), sp),
                value: p.default.as_ref().unwrap().clone(),
                span: sp,
            }));
        }
    }
    // Default lock req = 0
    for res_name in &all_resources {
        for ti in 0..threads.len() {
            merged_comb.push(CombStmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp),
                value: Expr::new(ExprKind::Bool(false), sp),
                span: sp,
            }));
        }
    }
    // Default shared(or) seq per-thread input wires = 0
    for sig_name in &shared_or_seq {
        for ti in 0..n_threads {
            merged_comb.push(CombStmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_in_{}", sig_name, ti)), sp),
                value: make_zero_expr(sp),
                span: sp,
            }));
        }
    }
    // Per-thread state-guarded comb assigns
    merged_comb.extend(all_thread_comb);
    if !merged_comb.is_empty() {
        merged_body.insert(0, ModuleBodyItem::CombBlock(CombBlock {
            stmts: merged_comb, span: sp,
        }));
    }

    let merged_module = ModuleDecl {
        name: Ident::new(merged_name.clone(), sp),
        params: Vec::new(),
        ports: merged_ports.clone(),
        body: merged_body,
        implements: None,
        hooks: Vec::new(),
        cdc_safe: false,
        span: sp,
    };

    // ── Create InstDecl in parent module ───────────────────────────────
    let mut connections: Vec<Connection> = Vec::new();
    for p in &merged_ports {
        let dir = match p.direction {
            Direction::In => ConnectDir::Input,
            Direction::Out => ConnectDir::Output,
        };
        connections.push(Connection {
            port_name: p.name.clone(), direction: dir,
            signal: Expr::new(ExprKind::Ident(p.name.name.clone()), sp),
            reset_override: None, span: sp,
        });
    }
    let inst = InstDecl {
        name: Ident::new("_threads".to_string(), sp),
        module_name: Ident::new(merged_name, sp),
        param_assigns: Vec::new(),
        connections, span: sp,
    };
    new_body.push(ModuleBodyItem::Inst(inst));

    // Remove RegDecls for thread-driven regs (now inside merged module)
    let thread_driven: HashSet<String> = all_seq_driven.iter().chain(all_comb_driven.iter()).cloned().collect();
    new_body.retain(|item| {
        if let ModuleBodyItem::RegDecl(r) = item {
            !thread_driven.contains(&r.name.name)
        } else {
            true
        }
    });

    let new_module = ModuleDecl { body: new_body, ..m };
    Ok((new_module, vec![Item::Module(merged_module)]))
}

// Old multi-FSM approach removed. See git history for reference.

/// Collected type info for a signal in the enclosing module.
#[derive(Clone, Debug)]
struct SignalInfo {
    ty: TypeExpr,
    is_reg: bool,
    reg_reset: RegReset,
    reg_init: Option<Expr>,
    shared: Option<SharedReduction>,
}

fn build_module_type_map(m: &ModuleDecl) -> HashMap<String, SignalInfo> {
    let mut map = HashMap::new();
    for p in &m.ports {
        map.insert(p.name.name.clone(), SignalInfo {
            ty: p.ty.clone(),
            is_reg: p.reg_info.is_some(),
            reg_reset: p.reg_info.as_ref().map(|ri| ri.reset.clone()).unwrap_or(RegReset::None),
            reg_init: p.reg_info.as_ref().and_then(|ri| ri.init.clone()),
            shared: p.shared,
        });
    }
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                map.insert(r.name.name.clone(), SignalInfo {
                    ty: r.ty.clone(),
                    is_reg: true,
                    reg_reset: r.reset.clone(),
                    reg_init: r.init.clone(),
                    shared: None,
                });
            }
            ModuleBodyItem::WireDecl(w) => {
                map.insert(w.name.name.clone(), SignalInfo {
                    ty: w.ty.clone(),
                    is_reg: false,
                    reg_reset: RegReset::None,
                    reg_init: None,
                    shared: None,
                });
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    map.insert(l.name.name.clone(), SignalInfo {
                        ty: ty.clone(),
                        is_reg: false,
                        reg_reset: RegReset::None,
                        reg_init: None,
                        shared: None,
                    });
                }
            }
            _ => {}
        }
    }
    map
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

fn collect_thread_signals(body: &[ThreadStmt]) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
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
                ThreadStmt::SeqAssign(ra) => {
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
                ThreadStmt::For { var: _, start, end, body, .. } => {
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
fn collect_expr_reads(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(name) => { out.insert(name.clone()); }
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
            for a in args { collect_expr_reads(a, out); }
        }
        ExprKind::Cast(e, _) => collect_expr_reads(e, out),
        ExprKind::Concat(parts) => {
            for p in parts { collect_expr_reads(p, out); }
        }
        ExprKind::Repeat(count, val) => {
            collect_expr_reads(count, out);
            collect_expr_reads(val, out);
        }
        ExprKind::Clog2(e) => collect_expr_reads(e, out),
        ExprKind::Signed(e) => collect_expr_reads(e, out),
        ExprKind::Unsigned(e) => collect_expr_reads(e, out),
        ExprKind::FunctionCall(_, args) => {
            for a in args { collect_expr_reads(a, out); }
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
                    if let Stmt::Assign(a) = s { collect_expr_reads(&a.value, out); }
                }
            }
        }
        ExprKind::ExprMatch(scrut, arms) => {
            collect_expr_reads(scrut, out);
            for arm in arms { collect_expr_reads(&arm.value, out); }
        }
        _ => {} // Literal, Bool, Todo, EnumVariant, StructLiteral
    }
}

/// Collect reads from index expressions in a target (e.g. `buf[i]` — `i` is a read).
fn collect_expr_index_reads(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Index(_, idx) => collect_expr_reads(idx, out),
        ExprKind::BitSlice(_, hi, lo) => {
            collect_expr_reads(hi, out);
            collect_expr_reads(lo, out);
        }
        _ => {}
    }
}

// ── State partitioning ──────────────────────────────────────────────────────

/// A single FSM state derived from thread body partitioning.
struct ThreadFsmState {
    /// Combinational assignments active in this state.
    comb_stmts: Vec<CombStmt>,
    /// Sequential assignments that fire on the transition out of this state.
    seq_stmts: Vec<Stmt>,
    /// Transition condition (from `wait until`).  None = unconditional.
    transition_cond: Option<Expr>,
    /// Is this a counter-based wait state? If so, stores the count expression.
    wait_cycles: Option<Expr>,
    /// Multiple transitions (for fork/join product states).
    /// Each entry is (condition, target_state_offset_from_this_group).
    /// When non-empty, `transition_cond` is ignored.
    multi_transitions: Vec<(Expr, usize)>,
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
        ExprKind::Ident(name) => {
            type_map.get(name)
                .and_then(|si| type_expr_uint_width_literal(&si.ty))
                .unwrap_or(16)
        }
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
            if *v == 0 { 1 } else { (u64::BITS - v.leading_zeros()) as u32 }
        }
        _ => 16,
    }
}

/// Find the minimum counter width across all `for` loops in a thread body.
/// Returns 16 if no for loops are found or width cannot be determined.
fn infer_for_cnt_width(stmts: &[ThreadStmt], type_map: &HashMap<String, SignalInfo>) -> u32 {
    let w = infer_for_cnt_width_inner(stmts, type_map);
    if w == 0 { 16 } else { w }
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
        ThreadStmt::IfElse(ie) => thread_has_wait_cycles(&ie.then_stmts) || thread_has_wait_cycles(&ie.else_stmts),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| thread_has_wait_cycles(br)),
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => thread_has_wait_cycles(body),
        ThreadStmt::For { body, .. } => thread_has_wait_cycles(body),
        _ => false,
    })
}

fn thread_has_for(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::For { .. } => true,
        ThreadStmt::IfElse(ie) => thread_has_for(&ie.then_stmts) || thread_has_for(&ie.else_stmts),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| thread_has_for(br)),
        ThreadStmt::Lock { body, .. } | ThreadStmt::DoUntil { body, .. } => thread_has_for(body),
        _ => false,
    })
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

/// Partition thread body into FSM states.
fn partition_thread_body(
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    let mut states: Vec<ThreadFsmState> = Vec::new();
    let mut cur_comb: Vec<CombStmt> = Vec::new();
    let mut cur_seq: Vec<Stmt> = Vec::new();

    for stmt in body {
        match stmt {
            ThreadStmt::CombAssign(ca) => {
                cur_comb.push(CombStmt::Assign(ca.clone()));
            }
            ThreadStmt::SeqAssign(ra) => {
                cur_seq.push(Stmt::Assign(ra.clone()));
            }
            ThreadStmt::WaitUntil(cond, _) => {
                // `wait until` is a pure state boundary.
                // ALL pending assigns (comb + seq) go into a prior state
                // that fires once and advances unconditionally.
                // Use `do..until` instead when comb outputs must be held while waiting.
                if !cur_comb.is_empty() || !cur_seq.is_empty() {
                    states.push(ThreadFsmState {
                        comb_stmts: std::mem::take(&mut cur_comb),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                states.push(ThreadFsmState {
                    comb_stmts: Vec::new(),
                    seq_stmts: Vec::new(),
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                });
            }
            ThreadStmt::WaitCycles(count, _) => {
                // Same: pure boundary, flush all pending assigns
                if !cur_comb.is_empty() || !cur_seq.is_empty() {
                    states.push(ThreadFsmState {
                        comb_stmts: std::mem::take(&mut cur_comb),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                states.push(ThreadFsmState {
                    comb_stmts: Vec::new(),
                    seq_stmts: Vec::new(),
                    transition_cond: None,
                    wait_cycles: Some(count.clone()),
                    multi_transitions: Vec::new(),
                });
            }
            ThreadStmt::IfElse(ie) => {
                if contains_wait(&ie.then_stmts) || contains_wait(&ie.else_stmts) {
                    return Err(CompileError::general(
                        "wait inside if/else branches is not yet supported; \
                         restructure as separate threads or flatten the control flow",
                        ie.span,
                    ));
                }
                // Same-state conditional: convert to CombIfElse / IfElse for comb and seq
                let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                if let Some(c) = comb_if { cur_comb.push(c); }
                if let Some(s) = seq_if { cur_seq.push(s); }
            }
            ThreadStmt::ForkJoin(branches, sp) => {
                // Flush pending statements into a state before fork
                if !cur_comb.is_empty() || !cur_seq.is_empty() {
                    states.push(ThreadFsmState {
                        comb_stmts: std::mem::take(&mut cur_comb),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                // Lower fork/join via product-state expansion
                let mut fork_states = lower_fork_join(branches, *sp, cnt_width)?;
                // Adjust multi_transitions targets: product indices → global state indices
                let fork_base = states.len();
                for fs in &mut fork_states {
                    for (_, target) in &mut fs.multi_transitions {
                        *target += fork_base;
                    }
                }
                states.extend(fork_states);
            }
            ThreadStmt::For { var, start, end, body, span } => {
                // Counter init: merge into the last existing state (if it has
                // unconditional advance) to avoid a dead cycle. Otherwise flush.
                let cnt_name = "_loop_cnt";
                let cnt_init = Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident(cnt_name.to_string()), *span),
                    value: start.clone(),
                    span: *span,
                });
                let merged = if cur_comb.is_empty() && cur_seq.is_empty() {
                    // No pending assigns — merge counter init into last state.
                    // The init fires on the same edge as the state's transition,
                    // so the counter is ready when the for-body starts.
                    if let Some(last) = states.last_mut() {
                        if last.multi_transitions.is_empty() {
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
                    if !cur_comb.is_empty() || !cur_seq.is_empty() {
                        states.push(ThreadFsmState {
                            comb_stmts: std::mem::take(&mut cur_comb),
                            seq_stmts: std::mem::take(&mut cur_seq),
                            transition_cond: None,
                            wait_cycles: None,
                            multi_transitions: Vec::new(),
                        });
                    }
                }
                let mut for_states = lower_thread_for(var, start, end, body, *span, cnt_width)?;
                // Adjust multi_transitions targets (relative → absolute)
                let for_base = states.len();
                let for_len = for_states.len();
                for fs in &mut for_states {
                    for (_, target) in &mut fs.multi_transitions {
                        if *target == usize::MAX {
                            // Sentinel: "next state after this for group"
                            *target = for_base + for_len;
                        } else {
                            *target += for_base;
                        }
                    }
                }
                states.extend(for_states);
            }
            ThreadStmt::Lock { resource, body, span } => {
                // Flush pending statements
                if !cur_comb.is_empty() || !cur_seq.is_empty() {
                    states.push(ThreadFsmState {
                        comb_stmts: std::mem::take(&mut cur_comb),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                let lock_states = lower_thread_lock(&resource.name, body, *span, cnt_width)?;
                states.extend(lock_states);
            }
            ThreadStmt::DoUntil { body, cond, span: _ } => {
                // Flush pending assigns into a prior state
                if !cur_comb.is_empty() || !cur_seq.is_empty() {
                    states.push(ThreadFsmState {
                        comb_stmts: std::mem::take(&mut cur_comb),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                // Collect the do-body's assigns: comb stays in-state, seq stays in-state
                let mut do_comb: Vec<CombStmt> = Vec::new();
                let mut do_seq: Vec<Stmt> = Vec::new();
                for s in body {
                    match s {
                        ThreadStmt::CombAssign(ca) => {
                            do_comb.push(CombStmt::Assign(ca.clone()));
                        }
                        ThreadStmt::SeqAssign(ra) => {
                            do_seq.push(Stmt::Assign(ra.clone()));
                        }
                        ThreadStmt::IfElse(ie) => {
                            let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                            if let Some(c) = comb_if { do_comb.push(c); }
                            if let Some(s) = seq_if { do_seq.push(s); }
                        }
                        _ => {
                            // do..until body should only contain simple assigns
                            // (no waits, forks, loops — those go in the outer thread)
                        }
                    }
                }
                states.push(ThreadFsmState {
                    comb_stmts: do_comb,
                    seq_stmts: do_seq,
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                });
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
    if !cur_comb.is_empty() || !cur_seq.is_empty() {
        let merged_into_exit = if cur_comb.is_empty() && !cur_seq.is_empty() {
            if let Some(last) = states.last_mut() {
                if last.multi_transitions.len() == 2 {
                    // For-loop exit: guard trailing seq assigns by exit condition.
                    // Fires on the same clock edge as the for-loop's exit transition.
                    let exit_cond = last.multi_transitions[1].0.clone();
                    for s in cur_seq.drain(..) {
                        last.seq_stmts.push(Stmt::IfElse(IfElse {
                            cond: exit_cond.clone(),
                            then_stmts: vec![s],
                            else_stmts: Vec::new(),
                            unique: false,
                            span,
                        }));
                    }
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
            states.push(ThreadFsmState {
                comb_stmts: std::mem::take(&mut cur_comb),
                seq_stmts: std::mem::take(&mut cur_seq),
                transition_cond: None,
                wait_cycles: None,
                multi_transitions: Vec::new(),
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
) -> Result<Vec<ThreadFsmState>, CompileError> {
    if branches.len() < 2 {
        return Err(CompileError::general("fork/join requires at least 2 branches", span));
    }

    // Partition each branch, append a "done" hold state to each
    let mut branch_states: Vec<Vec<ThreadFsmState>> = Vec::new();
    for (i, br) in branches.iter().enumerate() {
        let mut states = partition_thread_body(br, span, cnt_width).map_err(|e| {
            CompileError::general(&format!("in fork branch {}: {}", i, e), span)
        })?;
        if states.is_empty() {
            return Err(CompileError::general(&format!("fork branch {} has no wait", i), span));
        }
        states.push(ThreadFsmState {
            comb_stmts: Vec::new(), seq_stmts: Vec::new(),
            transition_cond: None, wait_cycles: None, multi_transitions: Vec::new(),
        });
        branch_states.push(states);
    }

    let branch_lens: Vec<usize> = branch_states.iter().map(|b| b.len()).collect();
    let total: usize = branch_lens.iter().product();
    if total > 64 {
        return Err(CompileError::general(
            &format!("fork/join product expansion too large ({} states)", total), span));
    }

    // Encode branch indices → flat product index
    let encode = |indices: &[usize]| -> usize {
        let (mut idx, mut m) = (0, 1);
        for (bi, &si) in indices.iter().enumerate() { idx += si * m; m *= branch_lens[bi]; }
        idx
    };

    let mut result: Vec<ThreadFsmState> = Vec::new();

    for prod_idx in 0..total {
        // Decode
        let mut indices = Vec::new();
        let mut rem = prod_idx;
        for &len in &branch_lens { indices.push(rem % len); rem /= len; }

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
                        cond: c.clone(), then_stmts: br.seq_stmts.clone(),
                        else_stmts: Vec::new(), unique: false, span,
                    }));
                } else {
                    seq.extend(br.seq_stmts.clone());
                }
            }
        }

        if all_done {
            result.push(ThreadFsmState {
                comb_stmts: comb, seq_stmts: seq,
                transition_cond: None, wait_cycles: None, multi_transitions: Vec::new(),
            });
            continue;
        }

        // Build multi-transitions: enumerate subsets of active branches that can fire
        let active: Vec<(usize, Option<&Expr>)> = indices.iter().enumerate()
            .filter(|&(bi, &si)| si < branch_lens[bi] - 1)
            .map(|(bi, _)| (bi, branch_states[bi][indices[bi]].transition_cond.as_ref()))
            .collect();

        // Unconditional branches (cond_opt=None) always fire — they must be set in every mask
        let unconditional_mask: u32 = active.iter().enumerate()
            .filter(|(_, (_, c))| c.is_none())
            .fold(0u32, |m, (bit, _)| m | (1 << bit));

        let n = active.len();
        let mut multi = Vec::new();

        for mask in (1..(1u32 << n)).rev() {
            // Skip masks that don't include all unconditional branches
            if mask & unconditional_mask != unconditional_mask { continue; }

            let mut next = indices.clone();
            let mut pos: Vec<Expr> = Vec::new();
            let mut neg: Vec<Expr> = Vec::new();
            for (bit, &(bi, cond_opt)) in active.iter().enumerate() {
                if (mask >> bit) & 1 == 1 {
                    next[bi] += 1;
                    if let Some(c) = cond_opt { pos.push(c.clone()); }
                } else if let Some(c) = cond_opt {
                    neg.push(c.clone());
                }
            }
            let mut cond = if pos.is_empty() {
                Expr::new(ExprKind::Bool(true), span)
            } else {
                pos.into_iter().reduce(|a, b|
                    Expr::new(ExprKind::Binary(BinOp::And, Box::new(a), Box::new(b)), span)).unwrap()
            };
            for n in neg {
                cond = Expr::new(ExprKind::Binary(BinOp::And, Box::new(cond),
                    Box::new(Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(n)), span))), span);
            }
            multi.push((cond, encode(&next)));
        }

        result.push(ThreadFsmState {
            comb_stmts: comb, seq_stmts: seq,
            transition_cond: None, wait_cycles: None,
            multi_transitions: multi,
        });
    }

    Ok(result)
}

/// Lower a `for` loop with waits into FSM states.
///
/// Generates: INIT state (set counter = start), body states, LOOP_BACK state
/// (increment counter, check if counter <= end → loop or exit).
///
/// The loop variable is replaced with the `_loop_cnt` register in all body expressions.
/// The counter register is added to the FSM's regs by lower_single_thread (via a naming
/// convention: any state that references `_loop_cnt` triggers the reg creation).
fn lower_thread_for(
    var: &Ident,
    start: &Expr,
    end: &Expr,
    body: &[ThreadStmt],
    span: Span,
    cnt_width: u32,
) -> Result<Vec<ThreadFsmState>, CompileError> {

    // Replace loop variable with `_loop_cnt` in the body
    let cnt = "_loop_cnt";
    let rewritten_body: Vec<ThreadStmt> = body.iter()
        .map(|s| rewrite_loop_var(s, &var.name, cnt))
        .collect();

    // Partition the rewritten body into states
    let body_states = partition_thread_body(&rewritten_body, span, cnt_width)?;
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
    let cnt_ident = Expr::new(ExprKind::Ident(cnt.to_string()), span);
    let cnt_inc = Stmt::Assign(RegAssign {
        target: cnt_ident.clone(),
        value: Expr::new(
            ExprKind::MethodCall(
                Box::new(Expr::new(ExprKind::Binary(BinOp::Add,
                    Box::new(cnt_ident.clone()),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(cnt_width, 1)), span))), span)),
                Ident::new("trunc".to_string(), span),
                vec![Expr::new(ExprKind::Literal(LitKind::Dec(cnt_width as u64)), span)],
            ),
            span,
        ),
        span,
    });

    let loop_back_target = 0;

    // Match the end expression to cnt_width bits for the loop counter comparison.
    // Use trunc (not zext) because for-loop end expressions often come from
    // `burst_len_r - 1` where subtraction widens UInt<8> → UInt<9>.
    // trunc<cnt_width> is safe: the semantically meaningful range fits in cnt_width bits.
    let end_w = Expr::new(ExprKind::MethodCall(
        Box::new(end.clone()),
        Ident::new("trunc".to_string(), span),
        vec![Expr::new(ExprKind::Literal(LitKind::Dec(cnt_width as u64)), span)],
    ), span);

    if let Some(last) = result.last_mut() {
        if let Some(body_cond) = last.transition_cond.take() {
            // Last body state had a transition_cond (e.g. do..until).
            // Replace with multi_transitions: loop-back and exit, both
            // guarded by the original body condition AND counter check.
            let body_cond_clone = body_cond.clone();
            let loop_cond = Expr::new(ExprKind::Binary(
                BinOp::And,
                Box::new(body_cond.clone()),
                Box::new(Expr::new(ExprKind::Binary(
                    BinOp::Lt, Box::new(cnt_ident.clone()), Box::new(end_w.clone()),
                ), span)),
            ), span);
            let exit_cond = Expr::new(ExprKind::Binary(
                BinOp::And,
                Box::new(body_cond),
                Box::new(Expr::new(ExprKind::Binary(
                    BinOp::Gte, Box::new(cnt_ident.clone()), Box::new(end_w.clone()),
                ), span)),
            ), span);

            // Counter increment guarded by the body condition —
            // only increment when a beat is actually accepted
            last.seq_stmts.push(Stmt::IfElse(IfElse {
                cond: body_cond_clone,
                then_stmts: vec![cnt_inc],
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
                ExprKind::Binary(BinOp::Lt, Box::new(cnt_ident.clone()), Box::new(end_w.clone())),
                span,
            );
            let exit_cond = Expr::new(
                ExprKind::Binary(BinOp::Gte, Box::new(cnt_ident.clone()), Box::new(end_w.clone())),
                span,
            );
            last.seq_stmts.push(cnt_inc);
            last.multi_transitions = vec![
                (loop_cond, loop_back_target),
                (exit_cond, usize::MAX),
            ];
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
) -> Result<Vec<ThreadFsmState>, CompileError> {
    let req_signal = format!("_{}_req", resource_name);
    let grant_signal = format!("_{}_grant", resource_name);

    let make_grant = || Expr::new(ExprKind::Ident(grant_signal.clone()), span);
    let req_assign = CombStmt::Assign(CombAssign {
        target: Expr::new(ExprKind::Ident(req_signal.clone()), span),
        value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), span),
        span,
    });

    let mut body_states = partition_thread_body(body, span, cnt_width)?;

    // Add req=1 to all body states
    for bs in &mut body_states {
        bs.comb_stmts.insert(0, req_assign.clone());
    }

    // First body state: gate comb outputs AND transition on grant.
    // Without grant gating, all contending threads would drive outputs simultaneously.
    if let Some(first) = body_states.first_mut() {
        // Wrap ALL comb outputs (except req) in `if (grant) { ... }`
        let non_req_comb: Vec<CombStmt> = first.comb_stmts.iter()
            .filter(|s| {
                if let CombStmt::Assign(a) = s {
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
            if let CombStmt::Assign(a) = s {
                if let ExprKind::Ident(ref n) = a.target.kind {
                    return *n == req_signal;
                }
            }
            false
        });
        // Add grant-gated outputs
        if !non_req_comb.is_empty() {
            first.comb_stmts.push(CombStmt::IfElse(CombIfElse {
                cond: make_grant(),
                then_stmts: non_req_comb,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }

        // AND grant into transition condition
        if let Some(ref existing_cond) = first.transition_cond {
            first.transition_cond = Some(Expr::new(ExprKind::Binary(
                BinOp::And,
                Box::new(make_grant()),
                Box::new(existing_cond.clone()),
            ), span));
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

    // If body is empty (shouldn't happen), add a grant-wait state
    if body_states.is_empty() {
        body_states.push(ThreadFsmState {
            comb_stmts: vec![req_assign],
            seq_stmts: Vec::new(),
            transition_cond: Some(make_grant()),
            wait_cycles: None,
            multi_transitions: Vec::new(),
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
                for br in branches { resources.extend(collect_locked_resources(br)); }
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
fn rewrite_loop_var(stmt: &ThreadStmt, var: &str, replacement: &str) -> ThreadStmt {
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
        ThreadStmt::WaitUntil(cond, sp) => {
            ThreadStmt::WaitUntil(rewrite_var_expr(cond.clone(), var, replacement), *sp)
        }
        ThreadStmt::WaitCycles(n, sp) => {
            ThreadStmt::WaitCycles(rewrite_var_expr(n.clone(), var, replacement), *sp)
        }
        ThreadStmt::IfElse(ie) => ThreadStmt::IfElse(ThreadIfElse {
            cond: rewrite_var_expr(ie.cond.clone(), var, replacement),
            then_stmts: ie.then_stmts.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            else_stmts: ie.else_stmts.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            span: ie.span,
        }),
        ThreadStmt::ForkJoin(branches, sp) => ThreadStmt::ForkJoin(
            branches.iter().map(|br| br.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect()).collect(),
            *sp,
        ),
        ThreadStmt::For { var: fv, start, end, body, span } => ThreadStmt::For {
            var: fv.clone(),
            start: rewrite_var_expr(start.clone(), var, replacement),
            end: rewrite_var_expr(end.clone(), var, replacement),
            body: body.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            span: *span,
        },
        ThreadStmt::Lock { resource, body, span } => ThreadStmt::Lock {
            resource: resource.clone(),
            body: body.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            span: *span,
        },
        ThreadStmt::DoUntil { body, cond, span } => ThreadStmt::DoUntil {
            body: body.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            cond: rewrite_var_expr(cond.clone(), var, replacement),
            span: *span,
        },
    }
}

/// Replace ident `var` with `replacement` in an expression tree.
fn rewrite_var_expr(expr: Expr, var: &str, replacement: &str) -> Expr {
    let new_kind = match &expr.kind {
        ExprKind::Ident(name) if name == var => ExprKind::Ident(replacement.to_string()),
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(rewrite_var_expr(*l.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*r.clone(), var, replacement)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(rewrite_var_expr(*e.clone(), var, replacement))),
        ExprKind::Index(base, idx) => ExprKind::Index(
            Box::new(rewrite_var_expr(*base.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*idx.clone(), var, replacement)),
        ),
        ExprKind::Ternary(c, t, f) => ExprKind::Ternary(
            Box::new(rewrite_var_expr(*c.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*t.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*f.clone(), var, replacement)),
        ),
        _ => return expr,
    };
    Expr { kind: new_kind, span: expr.span, parenthesized: expr.parenthesized }
}

/// Convert a ThreadIfElse (no waits) into FSM comb and seq statements.
fn thread_if_to_fsm_stmts(ie: &ThreadIfElse) -> (Option<CombStmt>, Option<Stmt>) {
    let mut then_comb = Vec::new();
    let mut then_seq = Vec::new();
    let mut else_comb = Vec::new();
    let mut else_seq = Vec::new();

    fn partition_stmts(stmts: &[ThreadStmt], comb: &mut Vec<CombStmt>, seq: &mut Vec<Stmt>) {
        for s in stmts {
            match s {
                ThreadStmt::CombAssign(ca) => comb.push(CombStmt::Assign(ca.clone())),
                ThreadStmt::SeqAssign(ra) => seq.push(Stmt::Assign(ra.clone())),
                ThreadStmt::IfElse(nested) => {
                    let (c, s) = thread_if_to_fsm_stmts(nested);
                    if let Some(c) = c { comb.push(c); }
                    if let Some(s) = s { seq.push(s); }
                }
                _ => {} // wait already excluded by contains_wait check
            }
        }
    }

    partition_stmts(&ie.then_stmts, &mut then_comb, &mut then_seq);
    partition_stmts(&ie.else_stmts, &mut else_comb, &mut else_seq);

    let comb_if = if !then_comb.is_empty() || !else_comb.is_empty() {
        Some(CombStmt::IfElse(CombIfElse {
            cond: ie.cond.clone(),
            then_stmts: then_comb,
            else_stmts: else_comb,
            unique: false,
            span: ie.span,
        }))
    } else { None };

    let seq_if = if !then_seq.is_empty() || !else_seq.is_empty() {
        Some(Stmt::IfElse(IfElse {
            cond: ie.cond.clone(),
            then_stmts: then_seq,
            else_stmts: else_seq,
            unique: false,
            span: ie.span,
        }))
    } else { None };

    (comb_if, seq_if)
}

// ── FSM construction ────────────────────────────────────────────────────────

fn lower_single_thread(
    module_name: &str,
    thread_name: &str,
    t: &ThreadBlock,
    type_map: &HashMap<String, SignalInfo>,
    _reg_map: &HashMap<String, RegDecl>,
    shared_regs: &HashSet<String>,
) -> Result<(FsmDecl, InstDecl), CompileError> {
    let sp = t.span;

    // Step 1: Partition body into states
    let cnt_width = infer_for_cnt_width(&t.body, type_map);
    let raw_states = partition_thread_body(&t.body, sp, cnt_width)?;

    // Step 2: Analyze signals
    let (comb_driven, seq_driven, all_read) = collect_thread_signals(&t.body);

    // Signals that are only read (inputs to the FSM)
    let mut read_only: HashSet<String> = HashSet::new();
    for name in &all_read {
        if !comb_driven.contains(name) && !seq_driven.contains(name)
            && name != &t.clock.name && name != &t.reset.name
            && name != "_cnt" && name != "_loop_cnt"
        {
            read_only.insert(name.clone());
        }
    }

    // Step 3: Build FSM state names and state bodies
    let n_states = raw_states.len();
    let has_done = t.once; // need terminal DONE state
    // Check if any state uses wait_cycles — need a counter register
    let has_counter = raw_states.iter().any(|s| s.wait_cycles.is_some());

    let mut state_names: Vec<Ident> = Vec::new();
    let mut state_bodies: Vec<StateBody> = Vec::new();

    for (i, raw) in raw_states.iter().enumerate() {
        let sname = format!("S{}", i);
        state_names.push(Ident::new(sname.clone(), sp));

        let next_state_idx = if i + 1 < n_states {
            i + 1
        } else if has_done {
            n_states // DONE state
        } else {
            0 // wrap around
        };
        let next_state_name = if next_state_idx == n_states && has_done {
            "DONE".to_string()
        } else {
            format!("S{}", next_state_idx)
        };

        // Build transitions
        let mut transitions = Vec::new();

        // Fork/join and for-loop: multi_transitions targets are absolute state indices
        if !raw.multi_transitions.is_empty() {
            for (cond, target_idx) in &raw.multi_transitions {
                let tgt_name = if *target_idx >= n_states {
                    // Past the end: wrap to S0 (repeating) or DONE (once)
                    if has_done { "DONE".to_string() } else { "S0".to_string() }
                } else {
                    format!("S{}", target_idx)
                };
                transitions.push(Transition {
                    target: Ident::new(tgt_name, sp),
                    condition: cond.clone(),
                    span: sp,
                });
            }
        } else if let Some(ref _wait_count) = raw.wait_cycles {
            // Counter-based wait: transition when counter hits 0
            // The counter is managed as a reg in the FSM.
            // Transition condition: _cnt == 0
            let cnt_zero_cond = Expr::new(
                ExprKind::Binary(
                    BinOp::Eq,
                    Box::new(Expr::new(ExprKind::Ident("_cnt".to_string()), sp)),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
                ),
                sp,
            );
            transitions.push(Transition {
                target: Ident::new(next_state_name, sp),
                condition: cnt_zero_cond,
                span: sp,
            });
        } else if let Some(ref cond) = raw.transition_cond {
            transitions.push(Transition {
                target: Ident::new(next_state_name, sp),
                condition: cond.clone(),
                span: sp,
            });
        } else {
            // Unconditional transition (last state, no wait after it)
            transitions.push(Transition {
                target: Ident::new(next_state_name, sp),
                condition: Expr::new(ExprKind::Bool(true), sp),
                span: sp,
            });
        }

        // Build seq_stmts: fire unconditionally on state entry.
        // (Previously guarded by transition condition, but this prevented
        // state-entry assignments like `active_r <= true` from firing.)
        let seq_stmts = if !raw.seq_stmts.is_empty() {
            if raw.wait_cycles.is_some() {
                // Guard by counter == 0
                let cnt_zero = Expr::new(
                    ExprKind::Binary(
                        BinOp::Eq,
                        Box::new(Expr::new(ExprKind::Ident("_cnt".to_string()), sp)),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
                    ),
                    sp,
                );
                vec![Stmt::IfElse(IfElse {
                    cond: cnt_zero,
                    then_stmts: raw.seq_stmts.clone(),
                    else_stmts: Vec::new(),
                    unique: false,
                    span: sp,
                })]
            } else {
                raw.seq_stmts.clone()
            }
        } else {
            Vec::new()
        };

        // Counter decrement logic in seq_stmts
        let mut final_seq = seq_stmts;
        if raw.wait_cycles.is_some() {
            // _cnt <= _cnt - 1 (only when _cnt != 0, to avoid underflow)
            // But the FSM will transition when _cnt == 0, so decrementing
            // is safe in all other cycles.  Actually for the FSM codegen to
            // work, we need the counter decrement to happen every cycle in
            // this state. The transition guards the state change.
            let cnt_dec = Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident("_cnt".to_string()), sp),
                value: Expr::new(
                    ExprKind::Binary(
                        BinOp::Sub,
                        Box::new(Expr::new(ExprKind::Ident("_cnt".to_string()), sp)),
                        Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(1)), sp)),
                    ),
                    sp,
                ),
                span: sp,
            });
            final_seq.push(cnt_dec);
        }

        state_bodies.push(StateBody {
            name: Ident::new(sname, sp),
            comb_stmts: raw.comb_stmts.clone(),
            seq_stmts: final_seq,
            transitions,
            span: sp,
        });
    }

    // Add DONE state for `thread once` — self-loop to satisfy FSM typecheck
    if has_done {
        let done_name = "DONE".to_string();
        state_names.push(Ident::new(done_name.clone(), sp));
        state_bodies.push(StateBody {
            name: Ident::new(done_name.clone(), sp),
            comb_stmts: Vec::new(),
            seq_stmts: Vec::new(),
            transitions: vec![Transition {
                target: Ident::new(done_name, sp),
                condition: Expr::new(ExprKind::Bool(true), sp),
                span: sp,
            }],
            span: sp,
        });
    }

    // Step 4: Build counter load logic in the state *before* a counter state
    // The previous state's seq_stmts need `_cnt <= N - 1` on transition
    for i in 0..raw_states.len() {
        if let Some(ref count_expr) = raw_states[i].wait_cycles {
            // Find the state that transitions INTO state i.
            // For the first state with a counter, the previous state loads the counter.
            if i > 0 {
                let load_stmt = Stmt::Assign(RegAssign {
                    target: Expr::new(ExprKind::Ident("_cnt".to_string()), sp),
                    value: Expr::new(
                        ExprKind::Binary(
                            BinOp::Sub,
                            Box::new(count_expr.clone()),
                            Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(1)), sp)),
                        ),
                        sp,
                    ),
                    span: sp,
                });
                // Determine the guard for the previous state's transition
                let prev_guard = if let Some(ref prev_cond) = raw_states[i-1].transition_cond {
                    Some(prev_cond.clone())
                } else if raw_states[i-1].wait_cycles.is_some() {
                    // Previous state is also a counter — guard by counter == 0
                    Some(Expr::new(
                        ExprKind::Binary(
                            BinOp::Eq,
                            Box::new(Expr::new(ExprKind::Ident("_cnt".to_string()), sp)),
                            Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
                        ),
                        sp,
                    ))
                } else {
                    None
                };
                if let Some(guard) = prev_guard {
                    state_bodies[i-1].seq_stmts.push(Stmt::IfElse(IfElse {
                        cond: guard,
                        then_stmts: vec![load_stmt],
                        else_stmts: Vec::new(),
                        unique: false,
                        span: sp,
                    }));
                } else {
                    state_bodies[i-1].seq_stmts.push(load_stmt);
                }
            }
            // If i == 0 and it's a counter state, we need to load the counter
            // in the reset block (init value). This is handled by the reg init.
        }
    }

    // Step 5: Build FSM ports
    let mut fsm_ports: Vec<PortDecl> = Vec::new();

    // Clock port
    fsm_ports.push(PortDecl {
        name: t.clock.clone(),
        direction: Direction::In,
        ty: type_map.get(&t.clock.name)
            .map(|si| si.ty.clone())
            .unwrap_or_else(|| TypeExpr::Clock(Ident::new("SysDomain".to_string(), sp))),
        default: None,
        reg_info: None,
        bus_info: None,
        shared: None,
        span: sp,
    });

    // Reset port
    let reset_kind = type_map.get(&t.reset.name).and_then(|si| {
        if let TypeExpr::Reset(k, _) = &si.ty { Some(*k) } else { None }
    }).unwrap_or(ResetKind::Async);
    fsm_ports.push(PortDecl {
        name: t.reset.clone(),
        direction: Direction::In,
        ty: TypeExpr::Reset(reset_kind, t.reset_level),
        default: None,
        reg_info: None,
        bus_info: None,
        shared: None,
        span: sp,
    });

    // Input ports (read-only signals)
    let mut sorted_reads: Vec<&String> = read_only.iter().collect();
    sorted_reads.sort();
    for name in sorted_reads {
        if let Some(info) = type_map.get(name.as_str()) {
            fsm_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp),
                direction: Direction::In,
                ty: info.ty.clone(),
                default: None,
                reg_info: None,
                bus_info: None,
                shared: None,
                span: sp,
            });
        }
    }

    // Output ports (comb-driven signals)
    let mut sorted_comb: Vec<&String> = comb_driven.iter().collect();
    sorted_comb.sort();
    for name in sorted_comb {
        if let Some(info) = type_map.get(name.as_str()) {
            let zero = make_zero_expr(sp);
            fsm_ports.push(PortDecl {
                name: Ident::new(name.clone(), sp),
                direction: Direction::Out,
                ty: info.ty.clone(),
                default: Some(zero),
                reg_info: None,
                bus_info: None,
                shared: None,
                span: sp,
            });
        }
    }

    // Output ports (seq-driven signals)
    let mut fsm_regs: Vec<RegDecl> = Vec::new();
    let mut sorted_seq: Vec<&String> = seq_driven.iter().collect();
    sorted_seq.sort();
    for name in &sorted_seq {
        if let Some(info) = type_map.get(name.as_str()) {
            if shared_regs.contains(name.as_str()) {
                // Shared register: input (read current value) + output (write data+enable)
                // The register stays at the module level; FSM reads and writes via ports.
                fsm_ports.push(PortDecl {
                    name: Ident::new((*name).clone(), sp),
                    direction: Direction::In,
                    ty: info.ty.clone(),
                    default: None, reg_info: None, bus_info: None, shared: None, span: sp,
                });
                fsm_ports.push(PortDecl {
                    name: Ident::new(format!("{}_wr", name), sp),
                    direction: Direction::Out,
                    ty: info.ty.clone(),
                    default: Some(make_zero_expr(sp)),
                    reg_info: None,
                    bus_info: None, shared: None, span: sp,
                });
                fsm_ports.push(PortDecl {
                    name: Ident::new(format!("{}_we", name), sp),
                    direction: Direction::Out,
                    ty: TypeExpr::Bool,
                    default: Some(Expr::new(ExprKind::Bool(false), sp)),
                    reg_info: None, bus_info: None, shared: None, span: sp,
                });
            } else {
                // Non-shared: standard port-reg output
                fsm_ports.push(PortDecl {
                    name: Ident::new((*name).clone(), sp),
                    direction: Direction::Out,
                    ty: info.ty.clone(),
                    default: None,
                    reg_info: Some(PortRegInfo {
                        init: info.reg_init.clone(),
                        reset: info.reg_reset.clone(),
                    }),
                    bus_info: None, shared: None, span: sp,
                });
            }
        }
    }

    // Counter register (if needed)
    if has_counter {
        fsm_regs.push(RegDecl {
            name: Ident::new("_cnt".to_string(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp))),
            init: Some(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
            reset: RegReset::None,
            span: sp,
        });
    }

    // Loop counter register (if any for-loop with wait is used).
    // Width is inferred from the for-loop end expression to avoid over-wide counters.
    let has_for_loop = thread_has_for(&t.body);
    if has_for_loop {
        fsm_regs.push(RegDecl {
            name: Ident::new("_loop_cnt".to_string(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(cnt_width as u64)), sp))),
            init: Some(Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)),
            reset: RegReset::None,
            span: sp,
        });
    }

    // Resource lock req/grant ports
    let locked_resources = collect_locked_resources(&t.body);
    for res_name in &locked_resources {
        let req_name = format!("_{}_req", res_name);
        let grant_name = format!("_{}_grant", res_name);
        // req is an output (driven by the FSM), with default 0
        fsm_ports.push(PortDecl {
            name: Ident::new(req_name.clone(), sp),
            direction: Direction::Out,
            ty: TypeExpr::Bool,
            default: Some(make_zero_expr(sp)),
            reg_info: None,
            bus_info: None,
            shared: None,
            span: sp,
        });
        // grant is an input (provided by the arbiter)
        fsm_ports.push(PortDecl {
            name: Ident::new(grant_name, sp),
            direction: Direction::In,
            ty: TypeExpr::Bool,
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            span: sp,
        });
    }

    // Step 6: Build default_comb — set comb outputs to their default (0)
    let mut default_comb: Vec<CombStmt> = Vec::new();
    for p in &fsm_ports {
        if p.direction == Direction::Out && p.default.is_some() {
            default_comb.push(CombStmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(p.name.name.clone()), sp),
                value: p.default.as_ref().unwrap().clone(),
                span: sp,
            }));
        }
    }

    // Step 6b: Rewrite shared reg seq assigns in state bodies
    // Move `name <= expr` from seq to comb as `name_wr = expr; name_we = true;`
    if !shared_regs.is_empty() {
        let mut total_moved = 0;
        for sb in &mut state_bodies {
            let mut extra_comb = Vec::new();
            let mut new_seq = Vec::new();
            for stmt in std::mem::take(&mut sb.seq_stmts) {
                if let Some(comb_stmts) = extract_shared_reg_comb(&stmt, shared_regs, sp) {
                    total_moved += comb_stmts.len();
                    extra_comb.extend(comb_stmts);
                } else {
                    if let Stmt::Assign(ref ra) = stmt {
                        if let Some(name) = expr_root_name(&ra.target) {
                        }
                    }
                    new_seq.push(stmt);
                }
            }
            sb.seq_stmts = new_seq;
            sb.comb_stmts.extend(extra_comb);
        }
    }

    // Step 7: Build the FsmDecl
    let fsm_name = format!("_{module_name}_{thread_name}");
    let fsm = FsmDecl {
        name: Ident::new(fsm_name.clone(), sp),
        params: Vec::new(),
        ports: fsm_ports.clone(),
        regs: fsm_regs,
        lets: Vec::new(),
        wires: Vec::new(),
        state_names: state_names.clone(),
        default_state: state_names[0].clone(),
        default_comb,
        default_seq: Vec::new(),
        states: state_bodies,
        span: sp,
    };

    // Step 8: Build the InstDecl
    let inst_name = format!("_{thread_name}");
    let mut connections: Vec<Connection> = Vec::new();
    for p in &fsm_ports {
        let dir = match p.direction {
            Direction::In => ConnectDir::Input,
            Direction::Out => ConnectDir::Output,
        };
        connections.push(Connection {
            port_name: p.name.clone(),
            direction: dir,
            signal: Expr::new(ExprKind::Ident(p.name.name.clone()), sp),
            reset_override: None,
            span: sp,
        });
    }

    let inst = InstDecl {
        name: Ident::new(inst_name, sp),
        module_name: Ident::new(fsm_name, sp),
        param_assigns: Vec::new(),
        connections,
        span: sp,
    };

    Ok((fsm, inst))
}

/// Rewrite seq stmts: if a seq assign targets a shared(or) signal, convert it
/// to a comb assign targeting the per-thread shadow wire `_sig_in_ti`.
/// Returns the remaining (non-shared) seq stmts; appends converted comb stmts to `out_comb`.
fn rewrite_shared_or_seq_stmts(
    stmts: &[Stmt],
    shared_or_seq: &HashSet<String>,
    thread_idx: usize,
    sp: Span,
    out_comb: &mut Vec<CombStmt>,
) -> Vec<Stmt> {
    let mut kept = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Assign(ra) => {
                if let Some(name) = expr_root_name(&ra.target) {
                    if shared_or_seq.contains(&name) {
                        let shadow = format!("_{}_in_{}", name, thread_idx);
                        out_comb.push(CombStmt::Assign(CombAssign {
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
                    &ie.then_stmts, shared_or_seq, thread_idx, sp, &mut then_comb);
                let else_seq = rewrite_shared_or_seq_stmts(
                    &ie.else_stmts, shared_or_seq, thread_idx, sp, &mut else_comb);
                // Push rewritten comb assigns under the same if guard
                if !then_comb.is_empty() || !else_comb.is_empty() {
                    out_comb.push(CombStmt::IfElse(CombIfElse {
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
    stmts: &[CombStmt],
    shared_or: &HashSet<String>,
    sp: Span,
) -> Vec<CombStmt> {
    stmts.iter().map(|stmt| {
        match stmt {
            CombStmt::Assign(a) => {
                let target_name = match &a.target.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    _ => None,
                };
                if let Some(ref name) = target_name {
                    if shared_or.contains(name) {
                        // sig = sig | val
                        return CombStmt::Assign(CombAssign {
                            target: a.target.clone(),
                            value: Expr::new(ExprKind::Binary(
                                BinOp::BitOr,
                                Box::new(Expr::new(ExprKind::Ident(name.clone()), sp)),
                                Box::new(a.value.clone()),
                            ), sp),
                            span: a.span,
                        });
                    }
                }
                stmt.clone()
            }
            CombStmt::IfElse(ie) => {
                CombStmt::IfElse(CombIfElse {
                    cond: ie.cond.clone(),
                    then_stmts: transform_shared_or_assigns(&ie.then_stmts, shared_or, sp),
                    else_stmts: transform_shared_or_assigns(&ie.else_stmts, shared_or, sp),
                    unique: ie.unique,
                    span: ie.span,
                })
            }
            _ => stmt.clone(),
        }
    }).collect()
}

/// If a seq stmt assigns to a shared reg, return equivalent comb stmts.
/// Returns None if not a shared-reg assign (keep as seq).
fn extract_shared_reg_comb(
    stmt: &Stmt,
    shared_regs: &HashSet<String>,
    sp: Span,
) -> Option<Vec<CombStmt>> {
    match stmt {
        Stmt::Assign(ra) => {
            if let Some(name) = expr_root_name(&ra.target) {
                if shared_regs.contains(&name) {
                    return Some(vec![
                        CombStmt::Assign(CombAssign {
                            target: Expr::new(ExprKind::Ident(format!("{}_wr", name)), sp),
                            value: ra.value.clone(),
                            span: sp,
                        }),
                        CombStmt::Assign(CombAssign {
                            target: Expr::new(ExprKind::Ident(format!("{}_we", name)), sp),
                            value: Expr::new(ExprKind::Bool(true), sp),
                            span: sp,
                        }),
                    ]);
                }
            }
            None
        }
        // For if/else containing shared-reg assigns, we'd need deeper analysis.
        // For now, only handle top-level assigns (most common pattern).
        _ => None,
    }
}

/// Rename an identifier in an expression tree.
fn rename_ident_in_expr(expr: &mut Expr, old: &str, new: &str) {
    match &mut expr.kind {
        ExprKind::Ident(ref mut name) if name == old => { *name = new.to_string(); }
        ExprKind::Binary(_, l, r) => { rename_ident_in_expr(l, old, new); rename_ident_in_expr(r, old, new); }
        ExprKind::Unary(_, e) => rename_ident_in_expr(e, old, new),
        ExprKind::Index(b, i) => { rename_ident_in_expr(b, old, new); rename_ident_in_expr(i, old, new); }
        ExprKind::BitSlice(b, h, l) => { rename_ident_in_expr(b, old, new); rename_ident_in_expr(h, old, new); rename_ident_in_expr(l, old, new); }
        ExprKind::FieldAccess(b, _) => rename_ident_in_expr(b, old, new),
        ExprKind::MethodCall(recv, _, args) => {
            rename_ident_in_expr(recv, old, new);
            for a in args { rename_ident_in_expr(a, old, new); }
        }
        ExprKind::Ternary(c, t, f) => { rename_ident_in_expr(c, old, new); rename_ident_in_expr(t, old, new); rename_ident_in_expr(f, old, new); }
        ExprKind::Cast(e, _) => rename_ident_in_expr(e, old, new),
        _ => {}
    }
}

fn rename_ident_in_stmts(stmts: &mut [Stmt], old: &str, new: &str) {
    for s in stmts {
        match s {
            Stmt::Assign(ra) => { rename_ident_in_expr(&mut ra.target, old, new); rename_ident_in_expr(&mut ra.value, old, new); }
            Stmt::IfElse(ie) => {
                rename_ident_in_expr(&mut ie.cond, old, new);
                rename_ident_in_stmts(&mut ie.then_stmts, old, new);
                rename_ident_in_stmts(&mut ie.else_stmts, old, new);
            }
            _ => {}
        }
    }
}

fn rename_ident_in_comb_stmts(stmts: &mut [CombStmt], old: &str, new: &str) {
    for s in stmts {
        match s {
            CombStmt::Assign(ca) => { rename_ident_in_expr(&mut ca.target, old, new); rename_ident_in_expr(&mut ca.value, old, new); }
            CombStmt::IfElse(ie) => {
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
