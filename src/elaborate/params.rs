//! Param resolution, override application, elaborate-side const-eval, and
//! derived-param variant rewriting — extracted from `elaborate.rs` (P4 phase
//! 2a, move-only). This module owns the machinery that discovers every
//! distinct (effective-param-map, variant-name) a module is instantiated
//! with (`compute_all_variants`), applies inst-site param overrides while
//! keeping derived params (defaults that reference other params) tracking
//! the override (`recompute_derived_params`), and monomorphizes a single
//! module body against one such variant (`elaborate_module_variant`).
//!
//! `try_eval_i64` is also the elaborate-side compile-time constant evaluator
//! used by `typecheck.rs` to enforce `where`-clause constraints (arch#600,
//! PR #695) at both inst-param-override and bus-port-param-override sites —
//! it stays `pub fn` and is re-exported from `elaborate::mod` so every
//! existing `crate::elaborate::try_eval_i64` call site (in this module and
//! in `typecheck.rs`) keeps resolving unchanged.
//!
//! Generate-block expansion (`expand_generate`/`expand_generate_for`/
//! `expand_generate_if` and the `subst_*` loop-variable-substitution family)
//! and inst-body `for`-loop wiring-macro flattening (`flatten_inst_for_loops`)
//! are a related but distinct elaboration concern and stay in `elaborate::mod`
//! — this module calls into them (`expand_generate`, `flatten_inst_for_loops`,
//! `ParentShapeInfo`) the same way `elaborate::mod` calls back into this one.

use super::*;

// ── Step 2: collect raw inst overrides ───────────────────────────────────────

pub(crate) fn collect_raw_overrides_from_body(
    body: &[ModuleBodyItem],
    out: &mut HashMap<String, Vec<HashMap<String, i64>>>,
    enclosing_params: &HashMap<String, i64>,
) {
    for item in body {
        match item {
            ModuleBodyItem::Inst(inst) => record_inst(inst, out, enclosing_params),
            ModuleBodyItem::Generate(gen) => match gen {
                // `generate_for i in start..end { inst foo: M; param P = i; ... }`:
                // each unrolled iteration produces a specialized variant; we must
                // record one (loop-var-substituted) override per (i, inst) so
                // module_variants matches the post-unroll AST. Range bounds may
                // reference the enclosing module's params (e.g. `NUM_MASTERS-1`),
                // so eval with those.
                GenerateDecl::For(gf) => {
                    let start = try_eval_i64(&gf.start, enclosing_params);
                    let end = try_eval_i64(&gf.end, enclosing_params);
                    let var_name = &gf.var.name;
                    for it in &gf.items {
                        if let GenItem::Inst(inst) = it {
                            if let (Some(s), Some(e)) = (start, end) {
                                for v in s..=e {
                                    let inst_subst = subst_inst(inst, var_name, v);
                                    record_inst(&inst_subst, out, enclosing_params);
                                }
                            } else {
                                // Non-literal range — record once with the loop var
                                // unresolved (matches pre-unroll behavior).
                                record_inst(inst, out, enclosing_params);
                            }
                        }
                    }
                }
                // `generate_if cond { inst f: M; param I = SOMETHING; }`:
                // walk BOTH branches conservatively (over-recording produces
                // extra variants that compute_all_variants simply emits and
                // doesn't hurt correctness). The important fix: record_inst
                // must see the enclosing module's params so a param value
                // like `param I = NUM_FOO - 1` evaluates correctly — without
                // this, every iteration's inst silently lands on the
                // default-param variant. Same shape as the generate_for fix
                // earlier in this function.
                GenerateDecl::If(gi) => {
                    for it in gi.then_items.iter().chain(gi.else_items.iter()) {
                        if let GenItem::Inst(inst) = it {
                            record_inst(inst, out, enclosing_params);
                        }
                    }
                }
            },
            ModuleBodyItem::TlmConnect(_) => {}
            _ => {}
        }
    }
}

fn record_inst(
    inst: &InstDecl,
    out: &mut HashMap<String, Vec<HashMap<String, i64>>>,
    enclosing_params: &HashMap<String, i64>,
) {
    let mut overrides = HashMap::new();
    for pa in &inst.param_assigns {
        // Evaluate the inst's param value against the enclosing module's
        // param map. Without these, a param value like `param I = NUM_FOO`
        // that references a module param can't resolve at variant-discovery
        // time, and every inst silently lands on the default-param variant
        // (the bug the PR-394 fix and this audit address — same shape for
        // both generate_for and generate_if cases).
        if let Some(v) = try_eval_i64(&pa.value, enclosing_params) {
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
                overrides.insert(
                    format!("__ro__{pname}__kind"),
                    if kind == &ResetKind::Async { 1 } else { 0 },
                );
                overrides.insert(
                    format!("__ro__{pname}__level"),
                    if level == &ResetLevel::Low { 1 } else { 0 },
                );
            }
        }
    }
    out.entry(inst.module_name.name.clone())
        .or_default()
        .push(overrides);
}

// ── Step 3: compute variants ──────────────────────────────────────────────────

/// Returns `module_name → Vec<(effective_params, variant_name)>`.
pub(crate) fn compute_all_variants(
    items: &[Item],
    module_defaults: &HashMap<String, HashMap<String, i64>>,
    inst_raw: &HashMap<String, Vec<HashMap<String, i64>>>,
) -> HashMap<String, Vec<(HashMap<String, i64>, String)>> {
    let mut result = HashMap::new();

    for item in items {
        if let Item::Module(m) = item {
            let defaults = module_defaults
                .get(&m.name.name)
                .cloned()
                .unwrap_or_default();

            // Compute effective params for each inst site (deduped)
            let mut effective_sets: Vec<HashMap<String, i64>> = Vec::new();

            if let Some(raw_list) = inst_raw.get(&m.name.name) {
                for raw in raw_list {
                    let mut effective = defaults.clone();
                    effective.extend(raw.iter().map(|(k, v)| (k.clone(), *v)));
                    // Re-evaluate derived params (defaults that reference other
                    // params) against the overridden values. Without this, an
                    // inst override of a base param (`param W = 5`) leaves a
                    // dependent param (`param DERIVED = W + 2`) stuck at its
                    // default-computed value, so port/wire types sized by the
                    // derived param resolve to the wrong width. Params the inst
                    // explicitly overrode are pinned and never recomputed.
                    recompute_derived_params(&m.params, raw, &mut effective);
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
                vec![(
                    effective_sets.into_iter().next().unwrap(),
                    m.name.name.clone(),
                )]
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
    // Regular param suffixes (skip __ro__* synthetic reset-override keys).
    // A param value whose bit 63 is set is a negative `i64`, and formatting it
    // directly would splice a bare `-` into the variant name — illegal in both
    // SystemVerilog and C++ identifiers, so `arch build`/`arch sim` would emit
    // an uncompilable `Mod__P_-9223372036854775807`. Render the leading minus as
    // `n` (e.g. `P_-5` → `P_n5`); positive values are pure digits, so this stays
    // collision-free while keeping the magnitude readable.
    let regular: Vec<String> = varying
        .iter()
        .filter(|k| !k.starts_with("__ro__"))
        .map(|k| {
            let v = params.get(k).copied().unwrap_or(0);
            format!("{}_{}", k, format!("{v}").replace('-', "n"))
        })
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
            let kind_str = if kind_val == 1 { "Async" } else { "Sync" };
            let level_str = if level_val == 1 { "Low" } else { "High" };
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

pub(crate) fn elaborate_module_variant(
    m: ModuleDecl,
    param_vals: HashMap<String, i64>,
    variant_name: String,
    module_variants: &HashMap<String, Vec<(HashMap<String, i64>, String)>>,
    module_defaults: &HashMap<String, HashMap<String, i64>>,
    child_module_ports: &HashMap<String, Vec<PortDecl>>,
) -> Result<ModuleDecl, Vec<CompileError>> {
    // Build the parent module's shape info BEFORE we move m.body — used
    // by expand_generate_for to classify inst-bearing bodies as shape-
    // stable (SV-genvar-preservable) or needing per-iteration unroll.
    let parent_shape = ParentShapeInfo::from_module(&m);

    // Expand generate blocks
    let mut extra_ports: Vec<PortDecl> = Vec::new();
    let mut pre_rewrite: Vec<ModuleBodyItem> = Vec::new();
    let mut errors: Vec<CompileError> = Vec::new();

    for item in m.body {
        match item {
            ModuleBodyItem::Generate(gen) => {
                match expand_generate(gen, &param_vals, &parent_shape, child_module_ports) {
                    Ok((ports, items)) => {
                        extra_ports.extend(ports);
                        pre_rewrite.extend(items);
                    }
                    Err(mut errs) => errors.append(&mut errs),
                }
            }
            other => pre_rewrite.push(other),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Flatten any `for` loops inside inst bodies. The loop is a parse-time
    // wiring macro — its body is a list of Connections that gets unrolled
    // here, with the loop var substituted into each Connection's signal
    // expression and port_name suffix. After this pass every inst has
    // `for_loops` empty and all wiring lives in `connections`, so downstream
    // passes see the same shape as a hand-enumerated inst.
    let mut flattened: Vec<ModuleBodyItem> = Vec::with_capacity(pre_rewrite.len());
    for item in pre_rewrite {
        match item {
            ModuleBodyItem::Inst(inst) => match flatten_inst_for_loops(inst, &param_vals) {
                Ok(inst) => flattened.push(ModuleBodyItem::Inst(inst)),
                Err(mut errs) => errors.append(&mut errs),
            },
            other => flattened.push(other),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Rewrite inst module-names → variant names
    let new_body = flattened
        .into_iter()
        .map(|item| match item {
            ModuleBodyItem::Inst(inst) => ModuleBodyItem::Inst(rewrite_inst(
                inst,
                module_variants,
                module_defaults,
                &param_vals,
            )),
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
            let kind_key = format!("__ro__{}__kind", port.name.name);
            let level_key = format!("__ro__{}__level", port.name.name);
            if let Some(&k) = param_vals.get(&kind_key) {
                let l = param_vals.get(&level_key).copied().unwrap_or(0);
                let new_kind = if k == 1 {
                    ResetKind::Async
                } else {
                    ResetKind::Sync
                };
                let new_level = if l == 1 {
                    ResetLevel::Low
                } else {
                    ResetLevel::High
                };
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
    let param_names: std::collections::HashSet<&str> =
        param_vals.keys().map(|s| s.as_str()).collect();
    let new_params: Vec<ParamDecl> = m
        .params
        .into_iter()
        .map(|mut p| {
            if let Some(&val) = param_vals.get(&p.name.name) {
                // A derived default (one that references other params) is only
                // safe to *preserve* when this variant's resolved value still
                // equals re-evaluating that expression under the variant's
                // params. If the inst site explicitly overrode the param, the
                // resolved `val` diverges from the expression — preserving the
                // expression would silently drop the override (the SV backend
                // re-applies it as an inst param, but the sim backend bakes the
                // module default into a `#define`, so the override is lost).
                // In that case fall through to the literal-replacement path so
                // both backends see the overridden value.
                let derived_default_tracks = p
                    .default
                    .as_ref()
                    .map_or(false, |d| expr_references_params(d, &param_names))
                    && p.default
                        .as_ref()
                        .and_then(|d| try_eval_i64(d, &param_vals))
                        .map_or(false, |dv| dv == val);
                if matches!(p.kind, ParamKind::EnumConst(_)) {
                    // Preserve the EnumVariant expression for clean SV output
                } else if derived_default_tracks {
                    // Preserve original expression for derived params that were
                    // NOT overridden (value still tracks the parent param).
                } else {
                    // Width-typed params (`param NAME[hi:lo]: const = ...`) emit
                    // SV `parameter [hi:lo] NAME = <default>`. If we replaced
                    // the default with a bare `LitKind::Dec(val)`, the SV
                    // initializer would be unsized (32-bit by default) and
                    // Verilator's WIDTHTRUNC fires on the parameter init when
                    // the typed width is narrower. Emit a sized literal that
                    // matches the declared width so the init is width-clean.
                    let lit = if let ParamKind::WidthConst(hi, lo) = &p.kind {
                        let hi_val = try_eval_i64(hi, &param_vals);
                        let lo_val = try_eval_i64(lo, &param_vals);
                        match (hi_val, lo_val) {
                            (Some(h), Some(l)) if h >= l => {
                                let width = (h - l + 1) as u32;
                                LitKind::Sized(width, val as u64)
                            }
                            _ => LitKind::Dec(val as u64),
                        }
                    } else {
                        LitKind::Dec(val as u64)
                    };
                    p.default = Some(Expr::new(ExprKind::Literal(lit), p.name.span));
                }
            }
            p
        })
        .collect();

    Ok(ModuleDecl {
        name: new_name,
        params: new_params,
        ports: all_ports,
        body: new_body,
        implements: m.implements,
        hooks: m.hooks,
        cdc_safe: m.cdc_safe,
        rdc_safe: m.rdc_safe,
        comb_loops_allowed: m.comb_loops_allowed,
        allow_dead_skid_feedback: m.allow_dead_skid_feedback,
        span: m.span,
        doc: m.doc,
        inner_doc: m.inner_doc,
        is_interface: m.is_interface,
    })
}

/// Rewrite an inst's `module_name` to the correct variant name.
fn rewrite_inst(
    inst: InstDecl,
    module_variants: &HashMap<String, Vec<(HashMap<String, i64>, String)>>,
    module_defaults: &HashMap<String, HashMap<String, i64>>,
    enclosing_params: &HashMap<String, i64>,
) -> InstDecl {
    let variants = match module_variants.get(&inst.module_name.name) {
        Some(v) if v.len() > 1 => v,
        _ => return inst, // single variant → name unchanged
    };

    // Compute effective params for this inst (regular + reset-override synthetic params).
    // Param values must evaluate against the enclosing module's params so an
    // expression like `param I = NUM_FOO - 1` resolves to a literal that
    // matches one of the discovered variants. Same shape as the
    // record_inst fix.
    let defaults = module_defaults
        .get(&inst.module_name.name)
        .cloned()
        .unwrap_or_default();
    let mut effective = defaults;
    for pa in &inst.param_assigns {
        if let Some(v) = try_eval_i64(&pa.value, enclosing_params) {
            effective.insert(pa.name.name.clone(), v);
        }
    }
    for conn in &inst.connections {
        if let ExprKind::Cast(_, ty) = &conn.signal.kind {
            if let TypeExpr::Reset(kind, level) = ty.as_ref() {
                let pname = &conn.port_name.name;
                effective.insert(
                    format!("__ro__{pname}__kind"),
                    if kind == &ResetKind::Async { 1 } else { 0 },
                );
                effective.insert(
                    format!("__ro__{pname}__level"),
                    if level == &ResetLevel::Low { 1 } else { 0 },
                );
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
/// After inst-site overrides have been merged into `effective`, recompute any
/// *derived* param — one whose default expression references other params — so
/// it tracks the overridden value. Params the inst explicitly overrode (keys in
/// `raw`) are pinned and never recomputed: an explicit `param X = ...` at the
/// inst site always wins over the module's default expression.
///
/// Params are processed in declaration order so a derived param may depend on an
/// earlier derived param (`B = A + 1; C = B + 1;`). Only params that already
/// have an evaluable entry in `effective` are touched, mirroring
/// `compute_defaults_with_enums` (which silently drops params it can't fold).
fn recompute_derived_params(
    params: &[ParamDecl],
    raw: &HashMap<String, i64>,
    effective: &mut HashMap<String, i64>,
) {
    // All param names that exist — used to decide whether a default is "derived"
    // (references another param) vs a self-contained literal/const expr.
    let param_names: std::collections::HashSet<&str> =
        params.iter().map(|p| p.name.name.as_str()).collect();

    for p in params {
        // Explicitly overridden at the inst site → pinned, leave as-is.
        if raw.contains_key(&p.name.name) {
            continue;
        }
        let Some(default) = &p.default else { continue };
        // Only derived defaults (those that reference other params) can change
        // when an upstream param is overridden; literal-only defaults are stable.
        if !expr_references_params(default, &param_names) {
            continue;
        }
        if let Some(v) = try_eval_i64(default, effective) {
            effective.insert(p.name.name.clone(), v);
        }
    }
}

pub(crate) fn compute_defaults_with_enums(
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
                        enum_values
                            .get(enum_name)
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
            if rv == 0 {
                None
            } else {
                Some(try_eval_i64(l, param_vals)? / rv)
            }
        }
        ExprKind::Binary(BinOp::Mod, l, r) => {
            let rv = try_eval_i64(r, param_vals)?;
            if rv == 0 {
                None
            } else {
                Some(try_eval_i64(l, param_vals)? % rv)
            }
        }
        ExprKind::Unary(UnaryOp::Neg, e) => Some(-try_eval_i64(e, param_vals)?),
        ExprKind::Unary(UnaryOp::Not, e) => Some(if try_eval_i64(e, param_vals)? != 0 {
            0
        } else {
            1
        }),
        // Comparison operators → 0 or 1
        ExprKind::Binary(BinOp::Eq, l, r) => Some(
            if try_eval_i64(l, param_vals)? == try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Neq, l, r) => Some(
            if try_eval_i64(l, param_vals)? != try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Lt, l, r) => Some(
            if try_eval_i64(l, param_vals)? < try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Gt, l, r) => Some(
            if try_eval_i64(l, param_vals)? > try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Lte, l, r) => Some(
            if try_eval_i64(l, param_vals)? <= try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Gte, l, r) => Some(
            if try_eval_i64(l, param_vals)? >= try_eval_i64(r, param_vals)? {
                1
            } else {
                0
            },
        ),
        // Logical operators
        ExprKind::Binary(BinOp::And, l, r) => Some(
            if try_eval_i64(l, param_vals)? != 0 && try_eval_i64(r, param_vals)? != 0 {
                1
            } else {
                0
            },
        ),
        ExprKind::Binary(BinOp::Or, l, r) => Some(
            if try_eval_i64(l, param_vals)? != 0 || try_eval_i64(r, param_vals)? != 0 {
                1
            } else {
                0
            },
        ),
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
            if v <= 1 {
                Some(1)
            } else {
                Some(64 - (v - 1).leading_zeros() as i64)
            }
        }
        _ => None,
    }
}

pub(crate) fn try_eval_bool(expr: &Expr, param_vals: &HashMap<String, i64>) -> Option<bool> {
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
