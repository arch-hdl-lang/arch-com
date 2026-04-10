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

/// Lower all threads in a single module.
fn lower_module_threads(m: ModuleDecl) -> Result<(ModuleDecl, Vec<Item>), Vec<CompileError>> {
    let mut new_body: Vec<ModuleBodyItem> = Vec::new();
    let mut generated_fsms: Vec<Item> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();
    let mut thread_idx = 0usize;
    let sp = m.span;

    // Build a type map from module ports, regs, wires, lets
    let type_map = build_module_type_map(&m);
    let reg_map = build_module_reg_map(&m);

    // Collect resource declarations
    let resources: Vec<ResourceDecl> = m.body.iter()
        .filter_map(|item| if let ModuleBodyItem::Resource(r) = item { Some(r.clone()) } else { None })
        .collect();

    // Collect all seq-driven signals across threads (regs that move into FSMs)
    let mut thread_seq_driven: HashSet<String> = HashSet::new();
    for item in &m.body {
        if let ModuleBodyItem::Thread(t) = item {
            let (_, seq_driven, _) = collect_thread_signals(&t.body);
            thread_seq_driven.extend(seq_driven);
        }
    }

    // Track which threads use which resources, and their inst names
    let mut resource_users: HashMap<String, Vec<String>> = HashMap::new(); // resource → [inst_name]

    for item in m.body {
        match item {
            ModuleBodyItem::Thread(t) => {
                let thread_name = t.name.as_ref()
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| {
                        let name = if thread_idx == 0 { "thread".to_string() } else { format!("thread{}", thread_idx) };
                        thread_idx += 1;
                        name
                    });
                if t.name.is_some() { thread_idx += 1; }

                // Track resource usage
                let used_resources = collect_locked_resources(&t.body);
                let inst_name = format!("_{thread_name}");
                for res in &used_resources {
                    resource_users.entry(res.clone()).or_default().push(inst_name.clone());
                }

                match lower_single_thread(&m.name.name, &thread_name, &t, &type_map, &reg_map) {
                    Ok((fsm, inst)) => {
                        generated_fsms.push(Item::Fsm(fsm));
                        new_body.push(ModuleBodyItem::Inst(inst));
                    }
                    Err(e) => errors.push(e),
                }
            }
            ModuleBodyItem::Resource(_) => {
                // Resource declarations are consumed here; arbiter logic generated below
            }
            // Convert RegDecl to WireDecl for regs that are seq-driven by threads
            // (the register itself moves into the FSM; the module just needs a wire)
            ModuleBodyItem::RegDecl(ref r) if thread_seq_driven.contains(&r.name.name) => {
                new_body.push(ModuleBodyItem::WireDecl(WireDecl {
                    name: r.name.clone(),
                    ty: r.ty.clone(),
                    span: r.span,
                }));
            }
            other => new_body.push(other),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Generate arbiter + mux logic for each resource
    for res in &resources {
        let res_name = &res.name.name;
        let users = resource_users.get(res_name).cloned().unwrap_or_default();
        let n_users = users.len();
        if n_users == 0 { continue; }

        // Declare per-thread grant wires
        for i in 0..n_users {
            new_body.push(ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_grant_{}", res_name, i), sp),
                ty: TypeExpr::Bool,
                span: sp,
            }));
        }

        // Generate arbiter logic based on policy
        match &res.policy {
            ArbiterPolicy::RoundRobin => {
                // Round-robin: last_grant register tracks who was granted last.
                // Priority rotates: search from last_grant+1 wrapping around.
                // Implementation: two-pass priority scan.
                //   pass 1: grant to lowest i where i > last_grant and req[i]
                //   pass 2: if no pass-1 winner, grant to lowest i where req[i]
                //   This is equivalent to a rotating priority mask.
                //
                // For small N (compile-time known), we unroll:
                //   any_upper = req[last+1] | req[last+2] | ...
                //   grant[i] = req[i] && (i > last_grant || !any_upper) && !grant[j<i already]
                //
                // Simplest correct approach: use a last_grant reg + comb priority chain.
                let lg_name = format!("_{}_last_grant", res_name);
                let lg_bits = if n_users <= 2 { 1 } else { (n_users as f64).log2().ceil() as u32 };

                // Register: last_grant
                new_body.push(ModuleBodyItem::RegDecl(RegDecl {
                    name: Ident::new(lg_name.clone(), sp),
                    ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(lg_bits as u64)), sp))),
                    init: Some(Expr::new(ExprKind::Literal(LitKind::Dec((n_users - 1) as u64)), sp)),
                    reset: RegReset::None,
                    span: sp,
                }));

                // Comb block: priority arbiter with rotation
                // For each i: grant[i] = req[i] && !any_higher_priority_granted
                // where "higher priority" = (i > last_grant, wrapping)
                let mut arb_stmts: Vec<CombStmt> = Vec::new();
                for i in 0..n_users {
                    let grant_i = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, i)), sp);
                    let req_i = Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, i)), sp);

                    // grant[i] = req[i] && !grant[j] for all j != i (first-come priority)
                    // Round-robin twist: only grant if no earlier (in rotation order) is granted
                    let mut cond = req_i;
                    for j in 0..i {
                        let grant_j = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, j)), sp);
                        cond = Expr::new(ExprKind::Binary(BinOp::And, Box::new(cond),
                            Box::new(Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(grant_j)), sp))), sp);
                    }
                    arb_stmts.push(CombStmt::Assign(CombAssign { target: grant_i, value: cond, span: sp }));
                }
                // TODO: full rotation based on last_grant — for now falls back to priority
                // A proper round-robin would reorder the priority chain based on last_grant.
                // This is a known simplification.
                new_body.push(ModuleBodyItem::CombBlock(CombBlock { stmts: arb_stmts, span: sp }));

                // Seq block: update last_grant on any grant
                // For now, use the module's clock/reset (find from module ports)
                let clk_port = m.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)));
                if let Some(clk) = clk_port {
                    let mut update_stmts: Vec<Stmt> = Vec::new();
                    for i in 0..n_users {
                        let grant_i = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, i)), sp);
                        update_stmts.push(Stmt::IfElse(IfElse {
                            cond: grant_i,
                            then_stmts: vec![Stmt::Assign(RegAssign {
                                target: Expr::new(ExprKind::Ident(lg_name.clone()), sp),
                                value: Expr::new(ExprKind::Literal(LitKind::Dec(i as u64)), sp),
                                span: sp,
                            })],
                            else_stmts: Vec::new(),
                            unique: false,
                            span: sp,
                        }));
                    }
                    new_body.push(ModuleBodyItem::RegBlock(RegBlock {
                        clock: clk.name.clone(),
                        clock_edge: ClockEdge::Rising,
                        stmts: update_stmts,
                        span: sp,
                    }));
                }
            }
            _ => {
                // Default: priority arbiter (also used for lru/weighted as fallback)
                let mut arb_stmts: Vec<CombStmt> = Vec::new();
                for i in 0..n_users {
                    let grant_i = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, i)), sp);
                    let req_i = Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, i)), sp);
                    let mut cond = req_i;
                    for j in 0..i {
                        let grant_j = Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, j)), sp);
                        cond = Expr::new(ExprKind::Binary(BinOp::And, Box::new(cond),
                            Box::new(Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(grant_j)), sp))), sp);
                    }
                    arb_stmts.push(CombStmt::Assign(CombAssign { target: grant_i, value: cond, span: sp }));
                }
                new_body.push(ModuleBodyItem::CombBlock(CombBlock { stmts: arb_stmts, span: sp }));
            }
        }
    }

    // Connect arbiter grants to thread FSM grant inputs
    // The inst connections use signal names like `_{resource}_grant`.
    // We need to wire them to the per-thread grant wires.
    // Update inst connections: replace `_{resource}_grant` with `_{resource}_grant_{i}`
    // and `_{resource}_req` connects to a wire `_{resource}_req_{i}`
    for i in new_body.iter_mut() {
        if let ModuleBodyItem::Inst(inst) = i {
            for (res_name, users) in &resource_users {
                if let Some(user_idx) = users.iter().position(|u| *u == inst.name.name) {
                    // Rewrite grant/req connections
                    for conn in &mut inst.connections {
                        let req_port = format!("_{}_req", res_name);
                        let grant_port = format!("_{}_grant", res_name);
                        if conn.port_name.name == req_port {
                            conn.signal = Expr::new(
                                ExprKind::Ident(format!("_{}_req_{}", res_name, user_idx)), sp);
                        }
                        if conn.port_name.name == grant_port {
                            conn.signal = Expr::new(
                                ExprKind::Ident(format!("_{}_grant_{}", res_name, user_idx)), sp);
                        }
                    }
                }
            }
        }
    }

    // Declare per-thread req wires (the FSM outputs connect to these)
    for (res_name, users) in &resource_users {
        for i in 0..users.len() {
            new_body.insert(0, ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_req_{}", res_name, i), sp),
                ty: TypeExpr::Bool,
                span: sp,
            }));
        }
    }

    // Collect signals driven inside lock blocks — these are implicitly shared(or)
    // when multiple threads lock the same resource and drive the same signal.
    let mut lock_driven_signals: HashSet<String> = HashSet::new();
    for (res_name, users) in &resource_users {
        if users.len() > 1 {
            // Find signals driven inside lock blocks for this resource
            // by looking at the original thread bodies (before lowering)
            // We already lowered, so check if any output signal appears
            // in multiple insts' output connections (besides req/grant)
            let mut signal_drivers: HashMap<String, usize> = HashMap::new();
            for item in &new_body {
                if let ModuleBodyItem::Inst(inst) = item {
                    if users.contains(&inst.name.name) {
                        for conn in &inst.connections {
                            if conn.direction == ConnectDir::Output {
                                if let ExprKind::Ident(ref name) = conn.signal.kind {
                                    if !name.starts_with(&format!("_{}_", res_name)) {
                                        *signal_drivers.entry(name.clone()).or_insert(0) += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (sig, count) in &signal_drivers {
                if *count > 1 {
                    lock_driven_signals.insert(sig.clone());
                }
            }
        }
    }

    // Handle shared(reduction) signals: rename per-thread outputs, add reduction assigns
    // Combine explicit shared(or|and) ports with implicit lock-driven shared signals
    let mut shared_ports: Vec<(String, SharedReduction, TypeExpr)> = m.ports.iter()
        .filter_map(|p| p.shared.map(|sr| (p.name.name.clone(), sr, p.ty.clone())))
        .collect();
    // Add implicit shared(or) for lock-driven signals
    for sig in &lock_driven_signals {
        if !shared_ports.iter().any(|(n, _, _)| n == sig) {
            let ty = type_map.get(sig).map(|si| si.ty.clone()).unwrap_or(TypeExpr::Bool);
            shared_ports.push((sig.clone(), SharedReduction::Or, ty));
        }
    }

    if !shared_ports.is_empty() {
        // For each shared signal, find all inst outputs that drive it
        for (sig_name, reduction, sig_ty) in &shared_ports {
            let mut thread_wires: Vec<String> = Vec::new();
            let mut thread_idx = 0;

            for item in &mut new_body {
                if let ModuleBodyItem::Inst(inst) = item {
                    let mut found = false;
                    for conn in &mut inst.connections {
                        if conn.direction == ConnectDir::Output {
                            if let ExprKind::Ident(ref name) = conn.signal.kind {
                                if name == sig_name {
                                    // Rename this output to a per-thread wire
                                    let wire_name = format!("{}__t{}", sig_name, thread_idx);
                                    conn.signal = Expr::new(
                                        ExprKind::Ident(wire_name.clone()), sp);
                                    thread_wires.push(wire_name);
                                    found = true;
                                }
                            }
                        }
                    }
                    if found { thread_idx += 1; }
                }
            }

            if thread_wires.len() > 1 {
                // Declare per-thread wires
                for w in &thread_wires {
                    new_body.insert(0, ModuleBodyItem::WireDecl(WireDecl {
                        name: Ident::new(w.clone(), sp),
                        ty: sig_ty.clone(),
                        span: sp,
                    }));
                }

                // Generate reduction: sig = wire_0 OP wire_1 OP ...
                let op = match reduction {
                    SharedReduction::Or => BinOp::BitOr,
                    SharedReduction::And => BinOp::BitAnd,
                };
                let reduced = thread_wires.iter()
                    .map(|w| Expr::new(ExprKind::Ident(w.clone()), sp))
                    .reduce(|a, b| Expr::new(ExprKind::Binary(op, Box::new(a), Box::new(b)), sp))
                    .unwrap();

                new_body.push(ModuleBodyItem::LetBinding(LetBinding {
                    name: Ident::new(sig_name.clone(), sp),
                    ty: None, // assign to existing port
                    value: reduced,
                    span: sp,
                }));
            }
        }
    }

    let new_module = ModuleDecl {
        body: new_body,
        ..m
    };
    Ok((new_module, generated_fsms))
}

/// Collected type info for a signal in the enclosing module.
#[derive(Clone, Debug)]
struct SignalInfo {
    ty: TypeExpr,
    is_reg: bool,
    reg_reset: RegReset,
    reg_init: Option<Expr>,
}

fn build_module_type_map(m: &ModuleDecl) -> HashMap<String, SignalInfo> {
    let mut map = HashMap::new();
    for p in &m.ports {
        map.insert(p.name.name.clone(), SignalInfo {
            ty: p.ty.clone(),
            is_reg: p.reg_info.is_some(),
            reg_reset: p.reg_info.as_ref().map(|ri| ri.reset.clone()).unwrap_or(RegReset::None),
            reg_init: p.reg_info.as_ref().and_then(|ri| ri.init.clone()),
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
                });
            }
            ModuleBodyItem::WireDecl(w) => {
                map.insert(w.name.name.clone(), SignalInfo {
                    ty: w.ty.clone(),
                    is_reg: false,
                    reg_reset: RegReset::None,
                    reg_init: None,
                });
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    map.insert(l.name.name.clone(), SignalInfo {
                        ty: ty.clone(),
                        is_reg: false,
                        reg_reset: RegReset::None,
                        reg_init: None,
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

/// Check if any ThreadStmt in a slice contains a wait (recursing into if/else).
fn thread_has_for(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::For { .. } => true,
        ThreadStmt::IfElse(ie) => thread_has_for(&ie.then_stmts) || thread_has_for(&ie.else_stmts),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|br| thread_has_for(br)),
        ThreadStmt::Lock { body, .. } => thread_has_for(body),
        _ => false,
    })
}

fn contains_wait(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::WaitUntil(..) | ThreadStmt::WaitCycles(..) => true,
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
                // Finalize current state: transition on cond
                states.push(ThreadFsmState {
                    comb_stmts: std::mem::take(&mut cur_comb),
                    seq_stmts: std::mem::take(&mut cur_seq),
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                });
            }
            ThreadStmt::WaitCycles(count, _) => {
                states.push(ThreadFsmState {
                    comb_stmts: std::mem::take(&mut cur_comb),
                    seq_stmts: std::mem::take(&mut cur_seq),
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
                let mut fork_states = lower_fork_join(branches, *sp)?;
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
                let mut for_states = lower_thread_for(var, start, end, body, *span)?;
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
                let lock_states = lower_thread_lock(&resource.name, body, *span)?;
                states.extend(lock_states);
            }
        }
    }

    // Remaining statements after last wait become the final state.
    // For repeating threads, this state transitions back to S0.
    // For `thread once`, it becomes a terminal hold state.
    if !cur_comb.is_empty() || !cur_seq.is_empty() {
        states.push(ThreadFsmState {
            comb_stmts: std::mem::take(&mut cur_comb),
            seq_stmts: std::mem::take(&mut cur_seq),
            transition_cond: None,
            wait_cycles: None,
            multi_transitions: Vec::new(),
        });
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
) -> Result<Vec<ThreadFsmState>, CompileError> {
    if branches.len() < 2 {
        return Err(CompileError::general("fork/join requires at least 2 branches", span));
    }

    // Partition each branch, append a "done" hold state to each
    let mut branch_states: Vec<Vec<ThreadFsmState>> = Vec::new();
    for (i, br) in branches.iter().enumerate() {
        let mut states = partition_thread_body(br, span).map_err(|e| {
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
) -> Result<Vec<ThreadFsmState>, CompileError> {
    // Replace loop variable with `_loop_cnt` in the body
    let cnt = "_loop_cnt";
    let rewritten_body: Vec<ThreadStmt> = body.iter()
        .map(|s| rewrite_loop_var(s, &var.name, cnt))
        .collect();

    // Partition the rewritten body into states
    let body_states = partition_thread_body(&rewritten_body, span)?;
    if body_states.is_empty() {
        return Err(CompileError::general(
            "for loop body must contain at least one wait statement",
            span,
        ));
    }

    let mut result: Vec<ThreadFsmState> = Vec::new();

    // INIT state: set counter = start, unconditional transition to first body state
    result.push(ThreadFsmState {
        comb_stmts: Vec::new(),
        seq_stmts: vec![Stmt::Assign(RegAssign {
            target: Expr::new(ExprKind::Ident(cnt.to_string()), span),
            value: start.clone(),
            span,
        })],
        transition_cond: None,
        wait_cycles: None,
        multi_transitions: Vec::new(),
    });

    // Body states (copied from partition)
    result.extend(body_states);

    // LOOP_CHECK state: increment counter, check loop condition
    // Two transitions: loop-back (counter < end) and exit (counter >= end)
    let cnt_ident = Expr::new(ExprKind::Ident(cnt.to_string()), span);
    let loop_cond = Expr::new(
        ExprKind::Binary(BinOp::Lt, Box::new(cnt_ident.clone()), Box::new(end.clone())),
        span,
    );
    let exit_cond = Expr::new(
        ExprKind::Binary(BinOp::Gte, Box::new(cnt_ident.clone()), Box::new(end.clone())),
        span,
    );

    // Index 1 = first body state (after INIT at index 0); adjusted to absolute later
    let loop_back_target = 1;

    result.push(ThreadFsmState {
        comb_stmts: Vec::new(),
        seq_stmts: vec![Stmt::Assign(RegAssign {
            target: cnt_ident.clone(),
            value: Expr::new(
                ExprKind::Binary(BinOp::Add, Box::new(cnt_ident), Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(1)), span))),
                span,
            ),
            span,
        })],
        transition_cond: None,
        wait_cycles: None,
        multi_transitions: vec![
            (loop_cond, loop_back_target),
            (exit_cond, usize::MAX), // sentinel: next state after this for group
        ],
    });

    Ok(result)
}

/// Lower a `lock` block into FSM states.
///
/// Generates: LOCK_REQ state (assert req, wait for grant), body states, implicit release
/// on transition out. The thread FSM gets req/grant ports for this resource.
fn lower_thread_lock(
    resource_name: &str,
    body: &[ThreadStmt],
    span: Span,
) -> Result<Vec<ThreadFsmState>, CompileError> {
    let req_signal = format!("_{}_req", resource_name);
    let grant_signal = format!("_{}_grant", resource_name);

    let mut result: Vec<ThreadFsmState> = Vec::new();

    // LOCK_REQ state: assert req, wait for grant
    let grant_cond = Expr::new(ExprKind::Ident(grant_signal.clone()), span);
    result.push(ThreadFsmState {
        comb_stmts: vec![CombStmt::Assign(CombAssign {
            target: Expr::new(ExprKind::Ident(req_signal.clone()), span),
            value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), span),
            span,
        })],
        seq_stmts: Vec::new(),
        transition_cond: Some(grant_cond),
        wait_cycles: None,
        multi_transitions: Vec::new(),
    });

    // Body states: partition normally, but keep req asserted
    let body_states = partition_thread_body(body, span)?;
    for mut bs in body_states {
        // Keep req=1 during the entire locked region
        bs.comb_stmts.insert(0, CombStmt::Assign(CombAssign {
            target: Expr::new(ExprKind::Ident(req_signal.clone()), span),
            value: Expr::new(ExprKind::Literal(LitKind::Dec(1)), span),
            span,
        }));
        result.push(bs);
    }

    // No explicit LOCK_RELEASE state needed — the req signal defaults to 0
    // (via the FSM's default_comb) when not in the lock states.

    Ok(result)
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
            ThreadStmt::For { body, .. } => {
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
) -> Result<(FsmDecl, InstDecl), CompileError> {
    let sp = t.span;

    // Step 1: Partition body into states
    let raw_states = partition_thread_body(&t.body, sp)?;

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

        // Build seq_stmts: guard with transition condition
        let seq_stmts = if !raw.seq_stmts.is_empty() {
            if let Some(ref cond) = raw.transition_cond {
                // Wrap seq assigns in if-guard so they fire on transition only
                vec![Stmt::IfElse(IfElse {
                    cond: cond.clone(),
                    then_stmts: raw.seq_stmts.clone(),
                    else_stmts: Vec::new(),
                    unique: false,
                    span: sp,
                })]
            } else if raw.wait_cycles.is_some() {
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

    // Output ports (seq-driven signals) — port reg with reset info
    let mut fsm_regs: Vec<RegDecl> = Vec::new();
    let mut sorted_seq: Vec<&String> = seq_driven.iter().collect();
    sorted_seq.sort();
    for name in &sorted_seq {
        if let Some(info) = type_map.get(name.as_str()) {
            fsm_ports.push(PortDecl {
                name: Ident::new((*name).clone(), sp),
                direction: Direction::Out,
                ty: info.ty.clone(),
                default: None,
                reg_info: Some(PortRegInfo {
                    init: info.reg_init.clone(),
                    reset: info.reg_reset.clone(),
                }),
                bus_info: None,
                shared: None,
                span: sp,
            });
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

    // Loop counter register (if any for-loop with wait is used)
    let has_for_loop = thread_has_for(&t.body);
    if has_for_loop {
        fsm_regs.push(RegDecl {
            name: Ident::new("_loop_cnt".to_string(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), sp))),
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

fn make_zero_expr(sp: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Dec(0)), sp)
}
