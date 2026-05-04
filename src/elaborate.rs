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
        Ok(SourceFile { items: new_items, inner_doc: None, frontmatter: None })
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
                p.default = Some(Expr::new(
                    ExprKind::Literal(lit),
                    p.name.span,
                ));
            }
        }
        p
    }).collect();

    Ok(ModuleDecl { name: new_name, params: new_params, ports: all_ports, body: new_body, implements: m.implements, hooks: m.hooks, cdc_safe: m.cdc_safe, rdc_safe: m.rdc_safe, span: m.span, doc: m.doc, inner_doc: m.inner_doc, is_interface: m.is_interface })
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

    // Before unrolling, enforce Reading B on seq / comb bodies: every LHS
    // must be indexed by the loop variable. Writing to a scalar from inside
    // generate_for would produce N conflicting drivers after unrolling.
    let mut errors: Vec<CompileError> = Vec::new();
    for item in &gf.items {
        match item {
            GenItem::Seq(rb)  => check_gen_for_reg_stmts(&rb.stmts, var, &mut errors),
            GenItem::Comb(cb) => check_gen_for_comb_stmts(&cb.stmts, var, &mut errors),
            _ => {}
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    for i in start..=end {
        for item in &gf.items {
            match item {
                GenItem::Port(p) => ports.push(subst_port(p, var, i)),
                GenItem::Inst(inst) => body.push(ModuleBodyItem::Inst(subst_inst(inst, var, i))),
                GenItem::Thread(t) => body.push(ModuleBodyItem::Thread(subst_thread(t, var, i))),
                GenItem::Assert(a) => body.push(ModuleBodyItem::Assert(subst_assert(a, var, i))),
                GenItem::Seq(rb)  => body.push(ModuleBodyItem::RegBlock(subst_reg_block(rb, var, i))),
                GenItem::Comb(cb) => body.push(ModuleBodyItem::CombBlock(subst_comb_block(cb, var, i))),
            }
        }
    }

    Ok((ports, body))
}

// ── generate_for seq/comb write-target check (Reading B) ──────────────────────
//
// Inside a generate_for's seq/comb body, every assignment LHS must be of the
// form `<ident>[<expr-using-loop-var>]` (with optional nested struct-field or
// bit-slice access). Reads on RHS are unrestricted.

fn expr_mentions_ident(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::Binary(_, l, r) =>
            expr_mentions_ident(l, name) || expr_mentions_ident(r, name),
        ExprKind::Unary(_, x) => expr_mentions_ident(x, name),
        ExprKind::FieldAccess(b, _) => expr_mentions_ident(b, name),
        ExprKind::Index(b, i) =>
            expr_mentions_ident(b, name) || expr_mentions_ident(i, name),
        ExprKind::BitSlice(b, h, l) =>
            expr_mentions_ident(b, name) || expr_mentions_ident(h, name)
            || expr_mentions_ident(l, name),
        ExprKind::Cast(e, _) => expr_mentions_ident(e, name),
        ExprKind::Ternary(c, t, f) =>
            expr_mentions_ident(c, name) || expr_mentions_ident(t, name)
            || expr_mentions_ident(f, name),
        ExprKind::Concat(xs) => xs.iter().any(|x| expr_mentions_ident(x, name)),
        ExprKind::MethodCall(r, _, args) =>
            expr_mentions_ident(r, name)
            || args.iter().any(|a| expr_mentions_ident(a, name)),
        _ => false,
    }
}

/// Every unrolled iteration of a generate_for must write a *distinct* target,
/// otherwise N copies of the loop body all drive the same signal. The only
/// accepted LHS shape is an index by the loop variable:
///
///   `vec_reg[i] <= ...`, or nested through a field / bit-slice, e.g.
///   `vec_reg[i].field <= ...`, `vec_reg[i][7:0] = ...`.
///
/// A bare-identifier LHS — even one with an `_i` suffix — is rejected. The
/// earlier revision accepted suffix names on the reasoning that ports / insts
/// declared inside generate_for get substituted into distinct `_0 / _1 / ...`
/// names. But that only holds when the target IS a generate_for-substituted
/// declaration; a scalar reg at module scope happening to end with `_i` was
/// silently accepted, then substituted to non-existent names like `cnt_0`,
/// leaving `arch check` / `arch build` happy while emitting SV that Verilator
/// rejects. The Vec-at-module-scope pattern (`reg store: Vec<T, N>` + `store[i]
/// <= ...`) supersedes the suffix shape cleanly.
fn lhs_is_loop_indexed(lhs: &Expr, var: &str) -> bool {
    match &lhs.kind {
        ExprKind::Index(_, idx) => expr_mentions_ident(idx, var),
        ExprKind::FieldAccess(base, _) => lhs_is_loop_indexed(base, var),
        ExprKind::BitSlice(base, _, _) => lhs_is_loop_indexed(base, var),
        _ => false,
    }
}

fn reject_bad_lhs(lhs: &Expr, var: &str, errors: &mut Vec<CompileError>) {
    if !lhs_is_loop_indexed(lhs, var) {
        errors.push(CompileError::general(
            &format!(
                "write target inside generate_for must be indexed by the loop \
                 variable `{var}`, e.g. `vec_reg[{var}] <= ...`. Declare the \
                 Vec-typed reg or port at module scope and index it here — \
                 scalar writes would produce multiple drivers after unrolling."
            ),
            lhs.span,
        ));
    }
}

fn check_gen_for_reg_stmts(stmts: &[Stmt], var: &str, errors: &mut Vec<CompileError>) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => reject_bad_lhs(&a.target, var, errors),
            Stmt::IfElse(ie) => {
                check_gen_for_reg_stmts(&ie.then_stmts, var, errors);
                check_gen_for_reg_stmts(&ie.else_stmts, var, errors);
            }
            Stmt::Match(m) => for arm in &m.arms {
                check_gen_for_reg_stmts(&arm.body, var, errors);
            },
            Stmt::For(f)  => check_gen_for_reg_stmts(&f.body, var, errors),
            Stmt::Init(ib) => check_gen_for_reg_stmts(&ib.body, var, errors),
            Stmt::Log(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {}
        }
    }
}

fn check_gen_for_comb_stmts(stmts: &[Stmt], var: &str, errors: &mut Vec<CompileError>) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => reject_bad_lhs(&a.target, var, errors),
            Stmt::IfElse(ie) => {
                check_gen_for_comb_stmts(&ie.then_stmts, var, errors);
                check_gen_for_comb_stmts(&ie.else_stmts, var, errors);
            }
            Stmt::Match(m) => {
                for arm in &m.arms { check_gen_for_comb_stmts(&arm.body, var, errors); }
            }
            Stmt::For(f) => check_gen_for_comb_stmts(&f.body, var, errors),
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => unreachable!("seq-only Stmt variant inside comb-context walker"),
            Stmt::Log(_) => {}
        }
    }
}

// ── Substitution helpers for generate_for's seq / comb bodies ─────────────────

fn subst_reg_block(rb: &RegBlock, var: &str, val: i64) -> RegBlock {
    RegBlock {
        clock: rb.clock.clone(),
        clock_edge: rb.clock_edge,
        stmts: rb.stmts.iter().map(|s| subst_reg_stmt(s, var, val)).collect(),
        span: rb.span,
    }
}

fn subst_reg_stmt(s: &Stmt, var: &str, val: i64) -> Stmt {
    // Use subst_expr_names (suffix-rewriting variant) consistent with how
    // thread bodies and generate_for ports/insts are substituted. That
    // correctly rewrites `rdata_i` → `rdata_0` and also substitutes bare `i`
    // uses in indices like `store[i]` → `store[0]`.
    match s {
        Stmt::Assign(a) => Stmt::Assign(Assign {
            target: subst_expr_names(a.target.clone(), var, val),
            value:  subst_expr_names(a.value.clone(),  var, val),
            span:   a.span,
        }),
        Stmt::IfElse(ie) => Stmt::IfElse(IfElseOf {
            cond:       subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie.then_stmts.iter().map(|x| subst_reg_stmt(x, var, val)).collect(),
            else_stmts: ie.else_stmts.iter().map(|x| subst_reg_stmt(x, var, val)).collect(),
            unique:     ie.unique,
            span:       ie.span,
        }),
        Stmt::Match(m) => Stmt::Match(MatchStmt {
            scrutinee: subst_expr_names(m.scrutinee.clone(), var, val),
            arms: m.arms.iter().map(|arm| MatchArm {
                pattern: arm.pattern.clone(),
                body: arm.body.iter().map(|s| subst_reg_stmt(s, var, val)).collect(),
            }).collect(),
            unique: m.unique,
            span: m.span,
        }),
        // Log/For/Init/WaitUntil/DoUntil: pass through. If we ever want to
        // support loop-var substitution in these nested contexts we can
        // extend this match — for now they're unusual inside generate_for
        // and the LHS check above already guards correctness.
        other => other.clone(),
    }
}

fn subst_comb_block(cb: &CombBlock, var: &str, val: i64) -> CombBlock {
    CombBlock {
        stmts: cb.stmts.iter().map(|s| subst_comb_stmt(s, var, val)).collect(),
        span: cb.span,
    }
}

fn subst_comb_stmt(s: &Stmt, var: &str, val: i64) -> Stmt {
    match s {
        Stmt::Assign(a) => Stmt::Assign(Assign {
            target: subst_expr_names(a.target.clone(), var, val),
            value:  subst_expr_names(a.value.clone(),  var, val),
            span:   a.span,
        }),
        Stmt::IfElse(ie) => Stmt::IfElse(IfElseOf {
            cond:       subst_expr_names(ie.cond.clone(), var, val),
            then_stmts: ie.then_stmts.iter().map(|x| subst_comb_stmt(x, var, val)).collect(),
            else_stmts: ie.else_stmts.iter().map(|x| subst_comb_stmt(x, var, val)).collect(),
            unique:     ie.unique,
            span:       ie.span,
        }),
        other => other.clone(),
    }
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
            GenItem::Assert(a) => body.push(ModuleBodyItem::Assert(a)),
            // No loop var in generate_if, so seq/comb pass through verbatim.
            // Reading B's write-target rule only applies to generate_for.
            GenItem::Seq(rb)  => body.push(ModuleBodyItem::RegBlock(rb)),
            GenItem::Comb(cb) => body.push(ModuleBodyItem::CombBlock(cb)),
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
        unpacked: p.unpacked,
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
                ty: pa.ty.clone(),
            })
            .collect(),
        // Connection signals may reference suffix-substituted names from the
        // enclosing generate_for (e.g. `done -> done_i` becomes `done -> done_0`
        // for i=0). `subst_expr` only rewrites bare loop-var idents; using the
        // suffix-aware `subst_expr_names` matches how thread-stmt / seq-stmt
        // substitution already handles this, and fixes a bug where inst
        // outputs connecting to per-iteration output ports didn't propagate
        // the drive through unroll.
        connections: inst
            .connections
            .iter()
            .map(|c| Connection {
                port_name: subst_ident(&c.port_name, var, val),
                direction: c.direction,
                signal: subst_expr_names(c.signal.clone(), var, val),
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
        default_when: t.default_when.as_ref().map(|(cond, stmts)| (
            subst_expr_names(cond.clone(), var, val),
            stmts.iter().map(|s| subst_thread_stmt(s, var, val)).collect(),
        )),
        tlm_target: t.tlm_target.as_ref().map(|tb| TlmTargetBinding {
            port: tb.port.clone(),
            method: tb.method.clone(),
            tag_lane: tb.tag_lane.as_ref().map(|e| subst_expr_names(e.clone(), var, val)),
            args: tb.args.clone(),
        }),
        reentrant: t.reentrant.clone(),
        implement: t.implement.clone(),
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
        ThreadStmt::ForkTlmAssign(ra) => ThreadStmt::ForkTlmAssign(RegAssign {
            target: subst_expr_names(ra.target.clone(), var, val),
            value: subst_expr_names(ra.value.clone(), var, val),
            span: ra.span,
        }),
        ThreadStmt::JoinAll(sp) => ThreadStmt::JoinAll(*sp),
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
            unique: ie.unique,
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
        ThreadStmt::Log(l) => ThreadStmt::Log(l.clone()),
        ThreadStmt::Return(e, span) => ThreadStmt::Return(subst_expr_names(e.clone(), var, val), *span),
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

fn subst_assert(a: &AssertDecl, var: &str, val: i64) -> AssertDecl {
    AssertDecl {
        kind: a.kind.clone(),
        name: a.name.as_ref().map(|n| subst_ident(n, var, val)),
        expr: subst_expr(a.expr.clone(), var, val),
        span: a.span,
    }
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
}

pub fn lower_threads_with_opts(
    ast: SourceFile,
    opts: &ThreadLowerOpts,
) -> Result<SourceFile, Vec<CompileError>> {
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
                match lower_module_threads(m, opts) {
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
    Ok(SourceFile { items: result, inner_doc: None, frontmatter: None })
}

/// Lower all threads in a single module to a SINGLE merged module.
///
/// All threads become per-thread state machines within one module.
/// Shared registers, lock arbitration, and output muxing are all
/// handled internally — no multi-driver issues.
fn lower_module_threads(m: ModuleDecl, opts: &ThreadLowerOpts) -> Result<(ModuleDecl, Vec<Item>), Vec<CompileError>> {
    let sp = m.span;
    let type_map = build_module_type_map(&m);
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
                // PR-tlm-p1 scaffolding: reentrant threads parse but the
                // N-instance FSM lowering ships in PR-tlm-p2 (non-TLM)
                // and PR-tlm-p3 (with issue arbiter for TLM calls).
                if t.reentrant.is_some() {
                    return Err(vec![CompileError::general(
                        "`reentrant` thread lowering is not yet implemented — tracked in doc/plan_tlm_pipelined.md PR-tlm-p2/p3.",
                        t.span,
                    )]);
                }
                // `implement target` threads should have been consumed by
                // lower_tlm_target_threads (which now treats them like
                // v1 tlm_target). If one reaches here, it's an internal
                // error. `implement` (initiator) threads still need
                // lowering (PR-tlm-i3/i4).
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
                            "initiator-side `implement {}.{}()` thread lowering is not yet implemented — tracked in doc/plan_tlm_implement_thread.md PR-tlm-i3/i4.",
                            b.port.name, b.method.name
                        ),
                        t.span,
                    )]);
                }
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
        // Also collect signals referenced in the `default when` clause
        if let Some((dw_cond, dw_stmts)) = &t.default_when {
            let (dw_cd, dw_sd, dw_ar) = collect_thread_signals(dw_stmts);
            all_comb_driven.extend(dw_cd);
            all_seq_driven.extend(dw_sd);
            all_read.extend(dw_ar);
            collect_expr_reads(dw_cond, &mut all_read);
        }
    }

    // Clock and reset ports (from first thread)
    let (clk_name, rst_name, _rst_level) = {
        let t = &threads[0].1;
        let rk = type_map.get(&t.reset.name).and_then(|si| {
            if let TypeExpr::Reset(k, _) = &si.ty { Some(*k) } else { None }
        }).unwrap_or(ResetKind::Async);
        merged_ports.push(PortDecl {
            name: t.clock.clone(), direction: Direction::In,
            ty: type_map.get(&t.clock.name).map(|si| si.ty.clone())
                .unwrap_or(TypeExpr::Clock(Ident::new("SysDomain".to_string(), sp))),
            default: None, reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
        });
        merged_ports.push(PortDecl {
            name: t.reset.clone(), direction: Direction::In,
            ty: TypeExpr::Reset(rk, t.reset_level),
            default: None, reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
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
                default: None, reg_info: None, bus_info: None, shared: None,
                unpacked: info.unpacked,
                span: sp,
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
                reg_info: None, bus_info: None, shared: info.shared,
                unpacked: info.unpacked,
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
                name: Ident::new(name.clone(), sp), direction: Direction::Out,
                ty: info.ty.clone(), default: None,
                reg_info: Some(PortRegInfo {
                    init: info.reg_init.clone(), reset: info.reg_reset.clone(), guard: None,
                    latency: 1,
                    // Synthesized by thread lowering, not user-written;
                    // don't deprecate internal artifacts.
                    legacy_port_reg: false,
                }),
                bus_info: None, shared: None, unpacked: false, span: sp,
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
        // Per-thread scalar req/grant wires (internal to the merged module).
        for ti in 0..n_threads {
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_req_{}", res_name, ti), sp),
                ty: TypeExpr::Bool, unpacked: false, span: sp,
            }));
            merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
                name: Ident::new(format!("_{}_grant_{}", res_name, ti), sp),
                ty: TypeExpr::Bool, unpacked: false, span: sp,
            }));
        }
        // Build packed req/grant vectors used by the arbiter inst.
        let req_packed = format!("_{}_req_packed", res_name);
        let grant_packed = format!("_{}_grant_packed", res_name);
        let n_threads_expr = Expr::new(
            ExprKind::Literal(LitKind::Dec(n_threads as u64)), sp);
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            name: Ident::new(req_packed.clone(), sp),
            ty: TypeExpr::UInt(Box::new(n_threads_expr.clone())), unpacked: false, span: sp,
        }));
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            name: Ident::new(grant_packed.clone(), sp),
            ty: TypeExpr::UInt(Box::new(n_threads_expr.clone())), unpacked: false, span: sp,
        }));
        // Throwaway sinks for arbiter scalar outputs (the lock idiom only
        // consumes the per-thread grant ready bits, not the scalar grant
        // index/valid).
        let gv_sink = format!("_{}_grant_valid", res_name);
        let gr_sink = format!("_{}_grant_requester", res_name);
        let gr_width = crate::width::index_width(n_threads as u64);
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            name: Ident::new(gv_sink.clone(), sp),
            ty: TypeExpr::Bool, unpacked: false, span: sp,
        }));
        merged_body.push(ModuleBodyItem::WireDecl(WireDecl {
            name: Ident::new(gr_sink.clone(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(gr_width as u64)), sp))), unpacked: false, span: sp,
        }));

        // Pack/unpack between scalar wires and packed vectors.
        let mut pack_stmts: Vec<Stmt> = Vec::new();
        for ti in 0..n_threads {
            // _packed[ti] = _req_ti
            pack_stmts.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Index(
                    Box::new(Expr::new(ExprKind::Ident(req_packed.clone()), sp)),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                ), sp),
                value: Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp),
                span: sp,
            }));
            // _grant_ti = _grant_packed[ti]
            pack_stmts.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_grant_{}", res_name, ti)), sp),
                value: Expr::new(ExprKind::Index(
                    Box::new(Expr::new(ExprKind::Ident(grant_packed.clone()), sp)),
                    Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(ti as u64)), sp)),
                ), sp),
                span: sp,
            }));
        }
        merged_body.push(ModuleBodyItem::CombBlock(CombBlock { stmts: pack_stmts, span: sp }));

        // Synthesize the per-resource arbiter Item.
        let (policy, hook) = match resource_decls.get(res_name) {
            Some(rd) => (rd.policy.clone(), rd.hook.clone()),
            None => (ArbiterPolicy::Priority, None),
        };
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
            connections: vec![
                Connection {
                    port_name: Ident::new("clk".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(clk_name.clone()), sp),
                    reset_override: None, span: sp,
                },
                Connection {
                    port_name: Ident::new("rst".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(rst_name.clone()), sp),
                    reset_override: None, span: sp,
                },
                Connection {
                    port_name: Ident::new("request_valid".to_string(), sp),
                    direction: ConnectDir::Input,
                    signal: Expr::new(ExprKind::Ident(req_packed.clone()), sp),
                    reset_override: None, span: sp,
                },
                Connection {
                    port_name: Ident::new("request_ready".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(grant_packed.clone()), sp),
                    reset_override: None, span: sp,
                },
                Connection {
                    port_name: Ident::new("grant_valid".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(gv_sink), sp),
                    reset_override: None, span: sp,
                },
                Connection {
                    port_name: Ident::new("grant_requester".to_string(), sp),
                    direction: ConnectDir::Output,
                    signal: Expr::new(ExprKind::Ident(gr_sink), sp),
                    reset_override: None, span: sp,
                },
            ],
            span: sp,
        }));
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
    let _shared_or_comb: HashSet<String> = shared_or_signals.iter()
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
                    unpacked: false,
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
                destructure_fields: Vec::new(),
            }));
        }
    }

    // ── Per-thread state machines ──────────────────────────────────────
    let mut all_thread_comb: Vec<Stmt> = Vec::new();
    let mut all_thread_seq: Vec<Stmt> = Vec::new();
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
        let state_bits = crate::width::index_width(n_states as u64) as u64;

        // State register
        merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
            name: Ident::new(state_reg.clone(), sp),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(state_bits.max(1))), sp))),
            init: Some(make_zero_expr(sp)),
            reset: RegReset::Inherit(
                Ident::new(rst_name.clone(), sp),
                make_zero_expr(sp),
            ),
            guard: None,
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
            // Counter decrement is intrinsic to a wait_cycles state — it must
            // run regardless of how the transition target is decided. Hoisted
            // out of the wait_cycles transition branch below so that an
            // if/else-with-waits dispatch (which puts a (cnt==0, target)
            // entry in multi_transitions) doesn't accidentally suppress it.
            if raw.wait_cycles.is_some() {
                let cnt_name = format!("_t{}_cnt", ti);
                let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
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
            }

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
            } else if raw.wait_cycles.is_some() {
                // Default wait_cycles transition: cnt==0 ⇒ next_state.
                let cnt_name = format!("_t{}_cnt", ti);
                let cnt_id = Expr::new(ExprKind::Ident(cnt_name.clone()), sp);
                let cnt_zero = Expr::new(ExprKind::Binary(
                    BinOp::Eq, Box::new(cnt_id),
                    Box::new(make_zero_expr(sp)),
                ), sp);
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

            // ── Auto-emit SVA spec-contract properties ─────────────────
            // Gated by `--auto-thread-asserts`. Guarded with `rst_inactive`
            // so they don't fire during reset. Skipped for terminal once
            // states (vacuous) and for threads with `default_when` (the
            // soft-reset escape can preempt any state).
            if opts.auto_asserts
                && t.default_when.is_none()
                && !(t.once && si + 1 >= n_states)
            {
                let mk_bin = |op: BinOp, a: Expr, b: Expr| -> Expr {
                    Expr::new(ExprKind::Binary(op, Box::new(a), Box::new(b)), sp)
                };
                let state_lit = |id: usize| Expr::new(ExprKind::Literal(LitKind::Dec(id as u64)), sp);
                let state_id = || Expr::new(ExprKind::Ident(state_reg.clone()), sp);
                let state_eq = |id: usize| mk_bin(BinOp::Eq, state_id(), state_lit(id));
                let rst_g = rst_inactive.clone().unwrap();
                let in_state = mk_bin(BinOp::And, rst_g.clone(), state_eq(si));
                let push_assert = |name: String, antecedent: Expr, consequent: Expr,
                                   acc: &mut Vec<AssertDecl>| {
                    let prop = mk_bin(BinOp::ImpliesNext, antecedent, consequent);
                    acc.push(AssertDecl {
                        kind: AssertKind::Assert,
                        name: Some(Ident::new(name, sp)),
                        expr: prop,
                        span: sp,
                    });
                };

                if !raw.multi_transitions.is_empty() {
                    // Each branch: when its cond fires, state goes to its target.
                    for (bi, (cond, target)) in raw.multi_transitions.iter().enumerate() {
                        let tgt = if *target >= n_states {
                            if t.once { n_states - 1 } else { 0 }
                        } else { *target };
                        let antecedent = mk_bin(BinOp::And, in_state.clone(), cond.clone());
                        push_assert(
                            format!("_auto_thread_t{}_branch_s{}_b{}", ti, si, bi),
                            antecedent, state_eq(tgt), &mut auto_asserts,
                        );
                    }
                } else if let Some(ref cond) = raw.transition_cond {
                    // wait_until cond — guard fires ⇒ FSM advances next edge.
                    let antecedent = mk_bin(BinOp::And, in_state.clone(), cond.clone());
                    push_assert(
                        format!("_auto_thread_t{}_wait_until_s{}", ti, si),
                        antecedent, state_eq(next_state), &mut auto_asserts,
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
                        stay_ant, state_eq(si), &mut auto_asserts,
                    );
                    push_assert(
                        format!("_auto_thread_t{}_wait_done_s{}", ti, si),
                        done_ant, state_eq(next_state), &mut auto_asserts,
                    );
                }
                // Unconditional transitions (no cond, no wait, no multi)
                // are not asserted: they're already trivially correct
                // ("|=> next") and add noise without catching anything new.
            }

            seq_stmts.push(Stmt::IfElse(IfElse {
                cond: state_cond,
                then_stmts: body,
                else_stmts: Vec::new(), unique: false, span: sp,
            }));
        }

        // Wrap with `default when` if present: priority soft-reset
        // if (cond) { <assigns>; state <= 0; } else { <normal FSM states> }
        if let Some((dw_cond, dw_thread_stmts)) = &t.default_when {
            // Convert ThreadStmt::SeqAssign items to Stmt::Assign
            let mut dw_then: Vec<Stmt> = dw_thread_stmts.iter()
                .filter_map(|ts| {
                    if let ThreadStmt::SeqAssign(ra) = ts {
                        Some(Stmt::Assign(ra.clone()))
                    } else {
                        None // non-seq assigns in default when are silently ignored
                    }
                })
                .collect();
            // Reset state to 0
            dw_then.push(Stmt::Assign(RegAssign {
                target: Expr::new(ExprKind::Ident(state_reg.clone()), sp),
                value: make_zero_expr(sp),
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
            let state_cond = Expr::new(ExprKind::Binary(
                BinOp::Eq,
                Box::new(Expr::new(ExprKind::Ident(state_reg.clone()), sp)),
                Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(si as u64)), sp)),
            ), sp);

            // This state's own comb outputs
            if !raw.comb_stmts.is_empty() {
                let transformed_stmts = transform_shared_or_assigns(&raw.comb_stmts, &shared_or_signals, sp);
                all_thread_comb.push(Stmt::IfElse(IfElse {
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
                reset: RegReset::None, guard: None, span: sp,
            }));
        }
        let has_for = thread_has_for(&t.body);
        if has_for {
            let for_cnt_width = infer_for_cnt_width(&t.body, &type_map);
            merged_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(format!("_t{}_loop_cnt", ti), sp),
                ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(for_cnt_width as u64)), sp))),
                init: Some(make_zero_expr(sp)),
                reset: RegReset::None, guard: None, span: sp,
            }));
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
                            target: Expr::new(ExprKind::Index(
                                Box::new(Expr::new(ExprKind::Ident(p.name.name.clone()), sp)),
                                Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(i)), sp)),
                            ), sp),
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
    // Default lock req = 0
    for res_name in &all_resources {
        for ti in 0..threads.len() {
            merged_comb.push(Stmt::Assign(CombAssign {
                target: Expr::new(ExprKind::Ident(format!("_{}_req_{}", res_name, ti)), sp),
                value: Expr::new(ExprKind::Bool(false), sp),
                span: sp,
            }));
        }
    }
    // Default shared(or) seq per-thread input wires = 0
    for sig_name in &shared_or_seq {
        for ti in 0..n_threads {
            merged_comb.push(Stmt::Assign(CombAssign {
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

    let merged_module = ModuleDecl {
        name: Ident::new(merged_name.clone(), sp),
        params: Vec::new(),
        ports: merged_ports.clone(),
        body: merged_body,
        implements: None,
        hooks: Vec::new(),
        cdc_safe: false,
        rdc_safe: false,
        span: sp,
        doc: None,
        inner_doc: None,
        is_interface: false,
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
fn synthesize_lock_arbiter(
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
    let n_threads_expr = Expr::new(
        ExprKind::Literal(LitKind::Dec(n_threads as u64)), sp);
    let gr_width = crate::width::index_width(n_threads as u64);

    // The arbiter is an internal synthesized module; its port names are
    // canonical (`clk` / `rst`) regardless of the parent's reset signal name.
    let _ = clk_name;
    let _ = rst_name;
    let scalar_ports = vec![
        PortDecl {
            name: Ident::new("clk".to_string(), sp),
            direction: Direction::In, ty: clk_ty, default: None,
            reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
        },
        PortDecl {
            name: Ident::new("rst".to_string(), sp),
            direction: Direction::In, ty: rst_ty, default: None,
            reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
        },
        PortDecl {
            name: Ident::new("grant_valid".to_string(), sp),
            direction: Direction::Out, ty: TypeExpr::Bool, default: None,
            reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
        },
        PortDecl {
            name: Ident::new("grant_requester".to_string(), sp),
            direction: Direction::Out,
            ty: TypeExpr::UInt(Box::new(Expr::new(
                ExprKind::Literal(LitKind::Dec(gr_width as u64)), sp))),
            default: None, reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
        },
    ];

    let request_array = PortArrayDecl {
        count_expr: Expr::new(ExprKind::Ident("NUM_REQ".to_string()), sp),
        name: Ident::new("request".to_string(), sp),
        signals: vec![
            PortDecl {
                name: Ident::new("valid".to_string(), sp),
                direction: Direction::In, ty: TypeExpr::Bool, default: None,
                reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
            },
            PortDecl {
                name: Ident::new("ready".to_string(), sp),
                direction: Direction::Out, ty: TypeExpr::Bool, default: None,
                reg_info: None, bus_info: None, shared: None, unpacked: false, span: sp,
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
                is_local: false,
                span: sp,
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
    }
}

// Old multi-FSM approach removed. See git history for reference.

/// Collected type info for a signal in the enclosing module.
#[derive(Clone, Debug)]
struct SignalInfo {
    ty: TypeExpr,
    reg_reset: RegReset,
    reg_init: Option<Expr>,
    shared: Option<SharedReduction>,
    /// Carried so the threads-submodule's synthesized port declarations
    /// inherit the parent's `unpacked Vec<T,N>` shape — otherwise the
    /// instantiation in the parent gets a packed-vs-unpacked port
    /// connection mismatch.
    unpacked: bool,
}

fn build_module_type_map(m: &ModuleDecl) -> HashMap<String, SignalInfo> {
    let mut map = HashMap::new();
    for p in &m.ports {
        map.insert(p.name.name.clone(), SignalInfo {
            ty: p.ty.clone(),
            reg_reset: p.reg_info.as_ref().map(|ri| ri.reset.clone()).unwrap_or(RegReset::None),
            reg_init: p.reg_info.as_ref().and_then(|ri| ri.init.clone()),
            shared: p.shared,
            unpacked: p.unpacked,
        });
    }
    for item in &m.body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                map.insert(r.name.name.clone(), SignalInfo {
                    ty: r.ty.clone(),
                    reg_reset: r.reset.clone(),
                    reg_init: r.init.clone(),
                    shared: None,
                    unpacked: false,
                });
            }
            ModuleBodyItem::WireDecl(w) => {
                map.insert(w.name.name.clone(), SignalInfo {
                    ty: w.ty.clone(),
                    reg_reset: RegReset::None,
                    reg_init: None,
                    shared: None,
                    unpacked: false,
                });
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    map.insert(l.name.name.clone(), SignalInfo {
                        ty: ty.clone(),
                        reg_reset: RegReset::None,
                        reg_init: None,
                        shared: None,
                        unpacked: false,
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
    comb_stmts: Vec<Stmt>,
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
fn redirect_fallthrough_to(
    states: &mut [ThreadFsmState],
    idx: usize,
    target: usize,
    span: Span,
) {
    let s = &mut states[idx];
    if !s.multi_transitions.is_empty() {
        if !s.multi_transitions.iter().any(|(_, t)| *t == target) {
            s.multi_transitions.push((Expr::new(ExprKind::Bool(true), span), target));
        }
        return;
    }
    if let Some(cond) = s.transition_cond.take() {
        s.multi_transitions = vec![(cond, target)];
        return;
    }
    if s.wait_cycles.is_some() {
        let cnt_id = Expr::new(ExprKind::Ident("_cnt".to_string()), span);
        let cnt_zero = Expr::new(ExprKind::Binary(
            BinOp::Eq, Box::new(cnt_id),
            Box::new(make_zero_expr(span)),
        ), span);
        s.multi_transitions = vec![(cnt_zero, target)];
        return;
    }
    s.multi_transitions = vec![(Expr::new(ExprKind::Bool(true), span), target)];
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
    let mut cur_comb: Vec<Stmt> = Vec::new();
    let mut cur_seq: Vec<Stmt> = Vec::new();

    for stmt in body {
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
                    states.push(ThreadFsmState {
                        comb_stmts: cur_comb.clone(),
                        seq_stmts: std::mem::take(&mut cur_seq),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                }
                states.push(ThreadFsmState {
                    comb_stmts: std::mem::take(&mut cur_comb),
                    seq_stmts: Vec::new(),
                    transition_cond: Some(cond.clone()),
                    wait_cycles: None,
                    multi_transitions: Vec::new(),
                });
                let _ = sp; // span retained for parity with the prior arm
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
                let then_has_wait = contains_wait(&ie.then_stmts);
                let else_has_wait = contains_wait(&ie.else_stmts);
                if then_has_wait || else_has_wait {
                    // Dispatch-and-rejoin (see doc/thread_lowering_proof.md §II.10).
                    // Step 1: flush pending comb/seq into a predecessor state so
                    // `cond` reads post-flush register values.
                    if !cur_comb.is_empty() || !cur_seq.is_empty() {
                        states.push(ThreadFsmState {
                            comb_stmts: std::mem::take(&mut cur_comb),
                            seq_stmts: std::mem::take(&mut cur_seq),
                            transition_cond: None,
                            wait_cycles: None,
                            multi_transitions: Vec::new(),
                        });
                    }
                    // Step 2: insert dispatch state placeholder; M filled below
                    // once branch base indices are known.
                    let dispatch_idx = states.len();
                    states.push(ThreadFsmState {
                        comb_stmts: Vec::new(),
                        seq_stmts: Vec::new(),
                        transition_cond: None,
                        wait_cycles: None,
                        multi_transitions: Vec::new(),
                    });
                    // Step 3: recursively partition `then_stmts` and append at then_base.
                    // Empty branches (§II.10.4) skip the recursive call —
                    // `partition_thread_body` rejects empty bodies, but the
                    // dispatch-and-rejoin lowering treats them as a direct jump
                    // to the rejoin index.
                    let then_base = states.len();
                    if !ie.then_stmts.is_empty() {
                        let mut then_states = partition_thread_body(&ie.then_stmts, ie.span, cnt_width)?;
                        let then_len = then_states.len();
                        for fs in &mut then_states {
                            for (_, target) in &mut fs.multi_transitions {
                                // Sentinel `usize::MAX` is the "next state after
                                // this for group" marker emitted by
                                // `lower_thread_for`. Inside a branch, that
                                // fallthrough should land at the rejoin index;
                                // the redirect step below rewrites it.
                                if *target == usize::MAX {
                                    *target = then_base + then_len;
                                } else {
                                    *target += then_base;
                                }
                            }
                        }
                        states.extend(then_states);
                    }
                    // Step 4: same for `else_stmts` at else_base.
                    let else_base = states.len();
                    if !ie.else_stmts.is_empty() {
                        let mut else_states = partition_thread_body(&ie.else_stmts, ie.span, cnt_width)?;
                        let else_len = else_states.len();
                        for fs in &mut else_states {
                            for (_, target) in &mut fs.multi_transitions {
                                if *target == usize::MAX {
                                    *target = else_base + else_len;
                                } else {
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
                    let then_target = if then_base == else_base { rejoin_idx } else { then_base };
                    let else_target = if else_base == rejoin_idx { rejoin_idx } else { else_base };
                    let neg_cond = Expr::new(
                        ExprKind::Unary(UnaryOp::Not, Box::new(ie.cond.clone())),
                        ie.span,
                    );
                    states[dispatch_idx].multi_transitions = vec![
                        (ie.cond.clone(), then_target),
                        (neg_cond, else_target),
                    ];
                } else {
                    // Same-state conditional: convert to IfElse / IfElse for comb and seq
                    let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                    if let Some(c) = comb_if { cur_comb.push(c); }
                    if let Some(s) = seq_if { cur_seq.push(s); }
                }
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
                // Nested lock blocks would violate mutual exclusion:
                // once a thread is past the first body state (grant-gated), subsequent
                // states do not re-check grant, so a higher-priority thread can enter
                // the same critical section simultaneously.  Reject at compile time.
                let inner_resources = collect_locked_resources(body);
                if !inner_resources.is_empty() {
                    let names: Vec<&str> = inner_resources.iter().map(|s| s.as_str()).collect();
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
                let mut do_comb: Vec<Stmt> = Vec::new();
                let mut do_seq: Vec<Stmt> = Vec::new();
                for s in body {
                    match s {
                        ThreadStmt::CombAssign(ca) => {
                            do_comb.push(Stmt::Assign(ca.clone()));
                        }
                        ThreadStmt::SeqAssign(ra) => {
                            do_seq.push(Stmt::Assign(ra.clone()));
                        }
                        ThreadStmt::IfElse(ie) => {
                            let (comb_if, seq_if) = thread_if_to_fsm_stmts(ie);
                            if let Some(c) = comb_if { do_comb.push(c); }
                            if let Some(s) = seq_if { do_seq.push(s); }
                        }
                        ThreadStmt::Log(l) => {
                            do_seq.push(Stmt::Log(l.clone()));
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
            ThreadStmt::Return(_, span) => {
                // `return expr;` is only valid inside a TLM method target
                // thread body, which has its own dedicated lowering pass
                // that rewrites Return into the rsp_valid/rsp_data drive
                // sequence before this pass runs. Reaching this arm means
                // a regular thread contained `return`, which is a user
                // error.
                return Err(CompileError::general(
                    "`return` is only valid inside a TLM method target thread (`thread port.method(args) ...`). Remove the return or wrap the body in a TLM target binding.",
                    *span,
                ));
            }
            ThreadStmt::ForkTlmAssign(ra) => {
                return Err(CompileError::general(
                    "`target <= fork port.method(...);` is only valid for TLM initiator threads and must be paired with a final `join all;`",
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
            debug_assert!(comb.is_empty() && seq.is_empty(),
                "fork all_done state non-empty — branch done-hold states have unexpected content");
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
    _start: &Expr,
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
    // Use `.resize<cnt_width>()` (direction-agnostic) rather than `.trunc<>()`
    // because:
    //   - End expressions like `burst_len_r - 1` widen above cnt_width
    //     (UInt<8> - UInt<1> → UInt<9>), where we need to truncate.
    //   - End expressions like literal `3` are already cnt_width bits
    //     (since `cnt_width` is computed from the end value's bit-width),
    //     where `.trunc<>()` would be flagged as a no-op by typecheck.
    // `resize` accepts both directions without complaint and lowers to the
    // same SV cast when widths match.
    let end_w = Expr::new(ExprKind::MethodCall(
        Box::new(end.clone()),
        Ident::new("resize".to_string(), span),
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
    let req_assign = Stmt::Assign(CombAssign {
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
        let non_req_comb: Vec<Stmt> = first.comb_stmts.iter()
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
            then_stmts: ie.then_stmts.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            else_stmts: ie.else_stmts.iter().map(|s| rewrite_loop_var(s, var, replacement)).collect(),
            unique: ie.unique,
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
        ThreadStmt::Log(l) => ThreadStmt::Log(l.clone()),
        ThreadStmt::Return(e, span) => ThreadStmt::Return(rewrite_var_expr(e.clone(), var, replacement), *span),
    }
}

/// Replace ident `var` with `replacement` in an expression tree.
fn rewrite_var_expr(expr: Expr, var: &str, replacement: &str) -> Expr {
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
        ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(rewrite_var_expr(*e.clone(), var, replacement))),
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
            parts.iter().map(|p| rewrite_var_expr(p.clone(), var, replacement)).collect(),
        ),
        ExprKind::Repeat(count, inner) => ExprKind::Repeat(
            Box::new(rewrite_var_expr(*count.clone(), var, replacement)),
            Box::new(rewrite_var_expr(*inner.clone(), var, replacement)),
        ),
        ExprKind::MethodCall(recv, name, args) => ExprKind::MethodCall(
            Box::new(rewrite_var_expr(*recv.clone(), var, replacement)),
            name.clone(),
            args.iter().map(|a| rewrite_var_expr(a.clone(), var, replacement)).collect(),
        ),
        ExprKind::Signed(inner) => ExprKind::Signed(
            Box::new(rewrite_var_expr(*inner.clone(), var, replacement)),
        ),
        ExprKind::Unsigned(inner) => ExprKind::Unsigned(
            Box::new(rewrite_var_expr(*inner.clone(), var, replacement)),
        ),
        // Leaf nodes / non-substitutable forms: Ident-not-matching, Literal,
        // Bool, EnumVariant, Todo, etc. Fall through unchanged.
        _ => return expr,
    };
    Expr { kind: new_kind, span: expr.span, parenthesized: expr.parenthesized }
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
        Some(Stmt::IfElse(IfElse {
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
                    &ie.then_stmts, shared_or_seq, thread_idx, sp, &mut then_comb);
                let else_seq = rewrite_shared_or_seq_stmts(
                    &ie.else_stmts, shared_or_seq, thread_idx, sp, &mut else_comb);
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
    shared_or: &HashSet<String>,
    sp: Span,
) -> Vec<Stmt> {
    stmts.iter().map(|stmt| {
        match stmt {
            Stmt::Assign(a) => {
                let target_name = match &a.target.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    _ => None,
                };
                if let Some(ref name) = target_name {
                    if shared_or.contains(name) {
                        // sig = sig | val
                        return Stmt::Assign(CombAssign {
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
            Stmt::IfElse(ie) => {
                Stmt::IfElse(IfElse {
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

/// Rename an identifier in an expression tree.
fn rename_ident_in_expr(expr: &mut Expr, old: &str, new: &str) {
    // Must recurse into every container variant that can hold sub-expressions
    // — counter renames (_loop_cnt → _t{N}_loop_cnt) walk the whole expression
    // tree, and missing a container leaves a bare `_loop_cnt` ident in the
    // lowered SV that references no real variable.
    match &mut expr.kind {
        ExprKind::Ident(ref mut name) if name == old => { *name = new.to_string(); }
        ExprKind::Binary(_, l, r) => { rename_ident_in_expr(l, old, new); rename_ident_in_expr(r, old, new); }
        ExprKind::Unary(_, e) => rename_ident_in_expr(e, old, new),
        ExprKind::Index(b, i) => { rename_ident_in_expr(b, old, new); rename_ident_in_expr(i, old, new); }
        ExprKind::BitSlice(b, h, l) => { rename_ident_in_expr(b, old, new); rename_ident_in_expr(h, old, new); rename_ident_in_expr(l, old, new); }
        ExprKind::PartSelect(b, s, w, _) => { rename_ident_in_expr(b, old, new); rename_ident_in_expr(s, old, new); rename_ident_in_expr(w, old, new); }
        ExprKind::FieldAccess(b, _) => rename_ident_in_expr(b, old, new),
        ExprKind::MethodCall(recv, _, args) => {
            rename_ident_in_expr(recv, old, new);
            for a in args { rename_ident_in_expr(a, old, new); }
        }
        ExprKind::Ternary(c, t, f) => { rename_ident_in_expr(c, old, new); rename_ident_in_expr(t, old, new); rename_ident_in_expr(f, old, new); }
        ExprKind::Cast(e, _) => rename_ident_in_expr(e, old, new),
        ExprKind::Concat(parts) => { for p in parts { rename_ident_in_expr(p, old, new); } }
        ExprKind::Repeat(c, e) => { rename_ident_in_expr(c, old, new); rename_ident_in_expr(e, old, new); }
        ExprKind::Signed(e) | ExprKind::Unsigned(e) | ExprKind::Clog2(e) | ExprKind::Onehot(e) => {
            rename_ident_in_expr(e, old, new);
        }
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

fn rename_ident_in_comb_stmts(stmts: &mut [Stmt], old: &str, new: &str) {
    for s in stmts {
        match s {
            Stmt::Assign(ca) => { rename_ident_in_expr(&mut ca.target, old, new); rename_ident_in_expr(&mut ca.value, old, new); }
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

// ── pipe_reg<T, N> port lowering ─────────────────────────────────────────
//
// Expand every `port q: out pipe_reg<T, N>` with N > 1 into:
//   - The original port keeps `latency = 1` (emits as today's `port reg`)
//   - N-1 synthesized regs `q_stg1` .. `q_stg{N-1}` of type T
//   - Every `q@N <= expr` is rewritten to the cascade:
//         q_stg1 <= expr;
//         q_stg2 <= q_stg1;
//         ...
//         q      <= q_stg{N-1};
//
// Reset/init propagate from the original port's reg_info to every
// intermediate reg (uniform behavior — all stages reset to the same value,
// matching today's pipe_reg semantics).
//
// Called from main.rs after `lower_threads` so every other elaboration
// pass sees the original unexpanded form.

pub fn lower_pipe_reg_ports(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    let mut new_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    for item in ast.items {
        match item {
            Item::Module(m) => match lower_pipe_reg_module(m) {
                Ok(new_m) => new_items.push(Item::Module(new_m)),
                Err(mut errs) => errors.append(&mut errs),
            },
            other => new_items.push(other),
        }
    }
    if !errors.is_empty() { return Err(errors); }
    Ok(SourceFile { items: new_items, inner_doc: None, frontmatter: None })
}

struct PipePortInfoLocal {
    name: String,
    latency: u32,
    ty: TypeExpr,
    reset: RegReset,
    init: Option<Expr>,
    span: Span,
}

fn lower_pipe_reg_module(mut m: ModuleDecl) -> Result<ModuleDecl, Vec<CompileError>> {
    // Collect metadata for every pipe_reg port (latency >= 1).
    // Ports with latency == 1 still participate in the @N validation —
    // legacy `port reg` is equivalent to `pipe_reg<T, 1>`.
    let mut all_pipe_ports: Vec<PipePortInfoLocal> = Vec::new();
    for p in &m.ports {
        if let Some(ri) = &p.reg_info {
            all_pipe_ports.push(PipePortInfoLocal {
                name: p.name.name.clone(),
                latency: ri.latency,
                ty: p.ty.clone(),
                reset: ri.reset.clone(),
                init: ri.init.clone(),
                span: p.span,
            });
        }
    }
    // Validation: walk every seq assignment. Errors for
    //   - q@N <= Y when N != declared depth
    //   - bare q <= Y on pipe_reg with depth > 1 (ambiguous)
    //   - q@K on RHS for K > 0 (intermediate stage reads not supported v1)
    //   - q@0 = Y on combinational port (not a pipe_reg)
    // Build name → total-stages for tap-bound checks of `q@K` reads.
    // Includes module-scope `pipe_reg` decls (depth = `stages`) and
    // pipe_reg ports (depth = port latency).
    let mut pipe_depths: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for pp in &all_pipe_ports {
        pipe_depths.insert(pp.name.clone(), pp.latency);
    }
    for bi in &m.body {
        if let ModuleBodyItem::PipeRegDecl(p) = bi {
            pipe_depths.insert(p.name.name.clone(), p.stages);
        }
    }
    let mut errors: Vec<CompileError> = Vec::new();
    for bi in &m.body {
        if let ModuleBodyItem::RegBlock(rb) = bi {
            validate_pipe_assignments(&rb.stmts, &all_pipe_ports, &pipe_depths, &mut errors);
        }
        if let ModuleBodyItem::CombBlock(cb) = bi {
            validate_comb_pipe_refs(&cb.stmts, &all_pipe_ports, &m.ports, &pipe_depths, &mut errors);
        }
    }
    if !errors.is_empty() { return Err(errors); }

    // Filter to ports that actually need the cascade expansion (latency > 1).
    let pipes: Vec<PipePortInfoLocal> = all_pipe_ports.into_iter()
        .filter(|pp| pp.latency > 1)
        .collect();
    if pipes.is_empty() {
        return Ok(m);
    }
    // Collapse each pipe port to latency=1 (so it emits as a regular port-reg).
    for p in &mut m.ports {
        if let Some(ri) = &mut p.reg_info {
            if ri.latency > 1 {
                ri.latency = 1;
            }
        }
    }

    // For each pipe port, insert N-1 RegDecls for the intermediate stages.
    let mut extra_body: Vec<ModuleBodyItem> = Vec::new();
    for pp in &pipes {
        for stage in 1..pp.latency {
            let stg_name = format!("{}_stg{}", pp.name, stage);
            extra_body.push(ModuleBodyItem::RegDecl(RegDecl {
                name: Ident::new(stg_name, pp.span),
                ty: pp.ty.clone(),
                init: pp.init.clone(),
                reset: pp.reset.clone(),
                guard: None,
                span: pp.span,
            }));
        }
    }

    // Rewrite every `q@N <= expr` assignment inside seq/reg blocks into the
    // cascade. The rewrite happens recursively through if/match/for bodies.
    for bi in &mut m.body {
        if let ModuleBodyItem::RegBlock(rb) = bi {
            rb.stmts = rewrite_seq_stmts(std::mem::take(&mut rb.stmts), &pipes);
        }
    }

    // Prepend the synthesized regs just before the first RegBlock, so
    // module-body ordering stays sane (regs before seq blocks by
    // convention).
    let mut new_body: Vec<ModuleBodyItem> = Vec::with_capacity(m.body.len() + extra_body.len());
    let mut inserted = false;
    for bi in m.body {
        if !inserted && matches!(bi, ModuleBodyItem::RegBlock(_)) {
            new_body.extend(extra_body.drain(..));
            inserted = true;
        }
        new_body.push(bi);
    }
    if !inserted {
        new_body.extend(extra_body.drain(..));
    }
    m.body = new_body;
    Ok(m)
}

// Validation helpers for @N placement / depth consistency.

fn validate_pipe_assignments(
    stmts: &[Stmt],
    ports: &[PipePortInfoLocal],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    for s in stmts {
        validate_pipe_assign_stmt(s, ports, pipe_depths, errors);
    }
}

fn validate_pipe_assign_stmt(
    stmt: &Stmt,
    ports: &[PipePortInfoLocal],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    match stmt {
        Stmt::Assign(a) => {
            // Inspect the target: LatencyAt(Ident, N) or bare Ident into a
            // pipe_reg port. Validate per the error matrix.
            let (target_name, latency_opt) = match &a.target.kind {
                ExprKind::LatencyAt(inner, n) => match &inner.kind {
                    ExprKind::Ident(name) => (name.clone(), Some(*n)),
                    _ => return,
                },
                ExprKind::Ident(name) => (name.clone(), None),
                _ => return,
            };
            let Some(pp) = ports.iter().find(|p| p.name == target_name) else { return; };
            match latency_opt {
                Some(n) if n != pp.latency => {
                    errors.push(CompileError::general(
                        &format!(
                            "`{name}@{n}` exceeds declared latency {depth} — write `{name}@{depth} <= ...` for this port",
                            name = pp.name, n = n, depth = pp.latency
                        ),
                        a.span,
                    ));
                }
                None if pp.latency > 1 => {
                    errors.push(CompileError::general(
                        &format!(
                            "assignment to pipe_reg port `{name}` is ambiguous — write `{name}@{depth} <= ...` to state the latency",
                            name = pp.name, depth = pp.latency
                        ),
                        a.span,
                    ));
                }
                _ => {}
            }
            // RHS `q@K` for pipe_reg `q`: K must be 0..=N.
            validate_rhs_latency_with_depths(&a.value, pipe_depths, errors);
        }
        Stmt::IfElse(ie) => {
            validate_pipe_assignments(&ie.then_stmts, ports, pipe_depths, errors);
            validate_pipe_assignments(&ie.else_stmts, ports, pipe_depths, errors);
        }
        Stmt::Match(m) => {
            for arm in &m.arms {
                validate_pipe_assignments(&arm.body, ports, pipe_depths, errors);
            }
        }
        Stmt::For(f) => validate_pipe_assignments(&f.body, ports, pipe_depths, errors),
        Stmt::Init(ib) => validate_pipe_assignments(&ib.body, ports, pipe_depths, errors),
        _ => {}
    }
}

fn validate_rhs_latency_with_depths(
    e: &Expr,
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    // RHS `q@K` reads the K-th tap of pipe_reg `q` (K=0 = source comb,
    // K=N = final output = bare `q`). Validate K ≤ N when the base is
    // a known pipe_reg name; if the base isn't a pipe_reg, reject @K
    // for K > 0 (legacy "no @ on plain regs" rule).
    match &e.kind {
        ExprKind::LatencyAt(inner, n) => {
            if let ExprKind::Ident(name) = &inner.kind {
                match pipe_depths.get(name) {
                    Some(depth) if *n > *depth => {
                        errors.push(CompileError::general(
                            &format!("`{name}@{n}` exceeds pipe_reg depth {depth} (valid taps: 0..={depth})"),
                            e.span,
                        ));
                    }
                    None if *n != 0 => {
                        errors.push(CompileError::general(
                            &format!("`{name}@{n}` — `{name}` is not a pipe_reg, only `@0` is allowed on plain signals"),
                            e.span,
                        ));
                    }
                    _ => {}
                }
            }
            validate_rhs_latency_with_depths(inner, pipe_depths, errors);
        }
        ExprKind::Binary(_, l, r) => { validate_rhs_latency_with_depths(l, pipe_depths, errors); validate_rhs_latency_with_depths(r, pipe_depths, errors); }
        ExprKind::Unary(_, x) => validate_rhs_latency_with_depths(x, pipe_depths, errors),
        ExprKind::Ternary(c, t, e2) => {
            validate_rhs_latency_with_depths(c, pipe_depths, errors);
            validate_rhs_latency_with_depths(t, pipe_depths, errors);
            validate_rhs_latency_with_depths(e2, pipe_depths, errors);
        }
        ExprKind::FieldAccess(b, _) => validate_rhs_latency_with_depths(b, pipe_depths, errors),
        ExprKind::Index(b, i) => { validate_rhs_latency_with_depths(b, pipe_depths, errors); validate_rhs_latency_with_depths(i, pipe_depths, errors); }
        ExprKind::BitSlice(b, h, l) => {
            validate_rhs_latency_with_depths(b, pipe_depths, errors);
            validate_rhs_latency_with_depths(h, pipe_depths, errors);
            validate_rhs_latency_with_depths(l, pipe_depths, errors);
        }
        ExprKind::MethodCall(b, _, args) => {
            validate_rhs_latency_with_depths(b, pipe_depths, errors);
            for a in args { validate_rhs_latency_with_depths(a, pipe_depths, errors); }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args { validate_rhs_latency_with_depths(a, pipe_depths, errors); }
        }
        _ => {}
    }
}

fn validate_comb_pipe_refs(
    stmts: &[Stmt],
    pipe_ports: &[PipePortInfoLocal],
    all_ports: &[PortDecl],
    pipe_depths: &std::collections::HashMap<String, u32>,
    errors: &mut Vec<CompileError>,
) {
    for s in stmts {
        match s {
            Stmt::Assign(a) => {
                // LHS @0 on a plain (non-pipe_reg) port is an error.
                if let ExprKind::LatencyAt(inner, n) = &a.target.kind {
                    if let ExprKind::Ident(name) = &inner.kind {
                        let is_pipe = pipe_ports.iter().any(|p| &p.name == name);
                        if !is_pipe && all_ports.iter().any(|p| p.name.name == *name) {
                            errors.push(CompileError::general(
                                &format!("`{name}@{n}` is only valid on pipe_reg<T, N> ports; drop the `@{n}` or change the port type"),
                                a.target.span,
                            ));
                        }
                    }
                }
                validate_rhs_latency_with_depths(&a.value, pipe_depths, errors);
            }
            Stmt::IfElse(ie) => {
                validate_comb_pipe_refs(&ie.then_stmts, pipe_ports, all_ports, pipe_depths, errors);
                validate_comb_pipe_refs(&ie.else_stmts, pipe_ports, all_ports, pipe_depths, errors);
            }
                Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => unreachable!("seq-only Stmt variant inside comb-context walker"),
            Stmt::Match(_) | Stmt::For(_) | Stmt::Log(_) => {}
        }
    }
}

fn rewrite_seq_stmts(stmts: Vec<Stmt>, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts {
        out.extend(rewrite_seq_stmt(s, pipes));
    }
    out
}

fn rewrite_seq_stmt(stmt: Stmt, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    match stmt {
        Stmt::Assign(a) => {
            let (root, latency, span) = match &a.target.kind {
                ExprKind::LatencyAt(inner, n) => match &inner.kind {
                    ExprKind::Ident(name) => (name.clone(), *n, a.span),
                    _ => return vec![Stmt::Assign(a)],
                },
                _ => return vec![Stmt::Assign(a)],
            };
            let Some(pp) = pipes.iter().find(|p| p.name == root) else {
                return vec![Stmt::Assign(a)];
            };
            if latency != pp.latency {
                // Typecheck should have rejected this; leave it and let
                // downstream errors surface.
                return vec![Stmt::Assign(a)];
            }
            // Build the cascade: stg1 <= expr; stg2 <= stg1; ...; q <= stg{N-1};
            let value = a.value;
            let n = pp.latency;
            let mut out: Vec<Stmt> = Vec::with_capacity(n as usize);
            // stg1 <= value
            out.push(Stmt::Assign(Assign {
                target: Expr::new(ExprKind::Ident(format!("{}_stg1", pp.name)), span),
                value,
                span,
            }));
            // stg{k} <= stg{k-1} for k = 2..N-1
            for k in 2..n {
                out.push(Stmt::Assign(Assign {
                    target: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, k)), span),
                    value: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, k - 1)), span),
                    span,
                }));
            }
            // q <= stg{N-1}
            out.push(Stmt::Assign(Assign {
                target: Expr::new(ExprKind::Ident(pp.name.clone()), span),
                value: Expr::new(ExprKind::Ident(format!("{}_stg{}", pp.name, n - 1)), span),
                span,
            }));
            out
        }
        Stmt::IfElse(mut ie) => {
            ie.then_stmts = rewrite_seq_stmts_pp(std::mem::take(&mut ie.then_stmts), pipes);
            ie.else_stmts = rewrite_seq_stmts_pp(std::mem::take(&mut ie.else_stmts), pipes);
            vec![Stmt::IfElse(ie)]
        }
        Stmt::Match(mut m) => {
            for arm in &mut m.arms {
                arm.body = rewrite_seq_stmts_pp(std::mem::take(&mut arm.body), pipes);
            }
            vec![Stmt::Match(m)]
        }
        Stmt::For(mut f) => {
            f.body = rewrite_seq_stmts_pp(std::mem::take(&mut f.body), pipes);
            vec![Stmt::For(f)]
        }
        Stmt::Init(mut ib) => {
            ib.body = rewrite_seq_stmts_pp(std::mem::take(&mut ib.body), pipes);
            vec![Stmt::Init(ib)]
        }
        other => vec![other],
    }
}

fn rewrite_seq_stmts_pp(stmts: Vec<Stmt>, pipes: &[PipePortInfoLocal]) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts {
        out.extend(rewrite_seq_stmt(s, pipes));
    }
    out
}


// ── credit_channel method-dispatch (PR #3b-v-β) ─────────────────────────────
//
// Rewrites `port.ch.valid` / `port.ch.data` / `port.ch.can_send` expressions,
// where `port` is a bus port declaring `credit_channel ch`, into
// `ExprKind::SynthIdent(__<port>_<ch>_<member>, ty)` pointing at the SV wires
// emitted by codegen boilerplate in PR #3b-ii / #3b-iii.
//
// Role-gated: `can_send` is valid only on the sender side (initiator of a
// `send` channel, target of a `receive` channel); `valid` and `data` are
// valid only on the receiver side. Mismatches are left as untransformed
// nested FieldAccess and fall through to normal bus-member resolution.

pub fn lower_credit_channel_dispatch(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    use std::collections::HashMap;
    let mut bus_ccs: HashMap<String, Vec<CreditChannelMeta>> = HashMap::new();
    for item in &ast.items {
        match item {
            Item::Bus(b) => {
                if !b.credit_channels.is_empty() {
                    bus_ccs.insert(b.name.name.clone(), b.credit_channels.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if !b.credit_channels.is_empty() {
                        bus_ccs.insert(b.name.name.clone(), b.credit_channels.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_ccs.is_empty() { return Ok(ast); }
    let mut items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    for item in ast.items {
        match item {
            Item::Module(mut m) => {
                let port_buses: HashMap<String, (String, BusPerspective)> = m.ports.iter()
                    .filter_map(|p| p.bus_info.as_ref().map(|bi|
                        (p.name.name.clone(), (bi.bus_name.name.clone(), bi.perspective))
                    ))
                    .collect();
                if port_buses.values().any(|(b, _)| bus_ccs.contains_key(b)) {
                    let ctx = CcDispatchCtx { bus_ccs: &bus_ccs, port_buses: &port_buses };
                    for bi in &mut m.body {
                        rewrite_body_item_cc(bi, &ctx, &mut errors);
                    }
                }
                items.push(Item::Module(m));
            }
            other => items.push(other),
        }
    }
    if !errors.is_empty() { return Err(errors); }
    Ok(SourceFile { items, inner_doc: None, frontmatter: None })
}

struct CcDispatchCtx<'a> {
    bus_ccs: &'a std::collections::HashMap<String, Vec<CreditChannelMeta>>,
    port_buses: &'a std::collections::HashMap<String, (String, BusPerspective)>,
}

fn rewrite_body_item_cc(bi: &mut ModuleBodyItem, ctx: &CcDispatchCtx, errors: &mut Vec<CompileError>) {
    match bi {
        ModuleBodyItem::CombBlock(cb) => {
            for s in &mut cb.stmts { rewrite_stmt_cc(s, ctx, errors); }
        }
        ModuleBodyItem::RegBlock(rb) => {
            for s in &mut rb.stmts { rewrite_stmt_cc(s, ctx, errors); }
        }
        ModuleBodyItem::LetBinding(l) => { rewrite_expr_cc(&mut l.value, ctx, errors); }
        _ => {}
    }
}

/// Rewrite credit-channel field access (`port.ch.data`, `port.ch.valid`,
/// `port.ch.can_send`) into the synthetic identifier the SV codegen emits
/// (`__{port}_{ch}_{suffix}`). Walks every expression position in `Stmt`
/// recursively. The reg/comb/pipeline-stage block context doesn't affect
/// the rewrite — the same field access is invalid for the same reason in
/// every block, and the synthesized identifier is the same.
///
/// History: pre-unification this was two near-identical functions
/// (`rewrite_reg_stmt_cc`, `rewrite_comb_stmt_cc`) — but the seq variant
/// silently skipped scrutinees of `Stmt::Match`, the bodies of
/// `Stmt::Init`, and the cond/body of `Stmt::WaitUntil` / `DoUntil`,
/// leaving CC field accesses inside those positions for the resolver to
/// trip over with a misleading "bus has no signal X" error. Unifying
/// (and exhaustively covering all expression positions) closes that gap.
fn rewrite_stmt_cc(s: &mut Stmt, ctx: &CcDispatchCtx, errors: &mut Vec<CompileError>) {
    match s {
        Stmt::Assign(a) => {
            rewrite_expr_cc(&mut a.target, ctx, errors);
            rewrite_expr_cc(&mut a.value, ctx, errors);
        }
        Stmt::IfElse(ie) => {
            rewrite_expr_cc(&mut ie.cond, ctx, errors);
            for s in &mut ie.then_stmts { rewrite_stmt_cc(s, ctx, errors); }
            for s in &mut ie.else_stmts { rewrite_stmt_cc(s, ctx, errors); }
        }
        Stmt::For(fl) => {
            for s in &mut fl.body { rewrite_stmt_cc(s, ctx, errors); }
        }
        Stmt::Match(m) => {
            rewrite_expr_cc(&mut m.scrutinee, ctx, errors);
            for arm in &mut m.arms {
                for s in &mut arm.body { rewrite_stmt_cc(s, ctx, errors); }
            }
        }
        Stmt::Init(ib) => {
            for s in &mut ib.body { rewrite_stmt_cc(s, ctx, errors); }
        }
        Stmt::WaitUntil(expr, _) => {
            rewrite_expr_cc(expr, ctx, errors);
        }
        Stmt::DoUntil { body, cond, .. } => {
            for s in body { rewrite_stmt_cc(s, ctx, errors); }
            rewrite_expr_cc(cond, ctx, errors);
        }
        Stmt::Log(l) => {
            for arg in &mut l.args { rewrite_expr_cc(arg, ctx, errors); }
        }
    }
}

fn rewrite_expr_cc(e: &mut Expr, ctx: &CcDispatchCtx, errors: &mut Vec<CompileError>) {
    match &mut e.kind {
        ExprKind::Binary(_, l, r) => { rewrite_expr_cc(l, ctx, errors); rewrite_expr_cc(r, ctx, errors); }
        ExprKind::Unary(_, x) | ExprKind::Cast(x, _) | ExprKind::Clog2(x)
        | ExprKind::Onehot(x) | ExprKind::Signed(x) | ExprKind::Unsigned(x)
        | ExprKind::LatencyAt(x, _)
        | ExprKind::SvaNext(_, x) => { rewrite_expr_cc(x, ctx, errors); }
        ExprKind::Index(b, i) => { rewrite_expr_cc(b, ctx, errors); rewrite_expr_cc(i, ctx, errors); }
        ExprKind::BitSlice(b, hi, lo) => {
            rewrite_expr_cc(b, ctx, errors); rewrite_expr_cc(hi, ctx, errors); rewrite_expr_cc(lo, ctx, errors);
        }
        ExprKind::PartSelect(b, s, w, _) => {
            rewrite_expr_cc(b, ctx, errors); rewrite_expr_cc(s, ctx, errors); rewrite_expr_cc(w, ctx, errors);
        }
        ExprKind::Ternary(c, t, el) => {
            rewrite_expr_cc(c, ctx, errors); rewrite_expr_cc(t, ctx, errors); rewrite_expr_cc(el, ctx, errors);
        }
        ExprKind::Concat(xs) | ExprKind::FunctionCall(_, xs) => {
            for x in xs { rewrite_expr_cc(x, ctx, errors); }
        }
        ExprKind::Repeat(n, x) => { rewrite_expr_cc(n, ctx, errors); rewrite_expr_cc(x, ctx, errors); }
        ExprKind::MethodCall(recv, _, args) => {
            rewrite_expr_cc(recv, ctx, errors);
            for a in args { rewrite_expr_cc(a, ctx, errors); }
        }
        ExprKind::FieldAccess(base, _) => { rewrite_expr_cc(base, ctx, errors); }
        ExprKind::StructLiteral(_, fields) => {
            for fi in fields { rewrite_expr_cc(&mut fi.value, ctx, errors); }
        }
        _ => {}
    }
    // Reject the underscored credit_channel access form (`port.<ch>_send_valid`,
    // `port.<ch>_send_data`, `port.<ch>_credit_return`). Tell the user to use
    // the dotted method form instead.
    if let ExprKind::FieldAccess(base, member) = &e.kind {
        if let ExprKind::Ident(port) = &base.kind {
            if let Some((bus_name, _)) = ctx.port_buses.get(port) {
                if let Some(ccs) = ctx.bus_ccs.get(bus_name) {
                    for cc in ccs {
                        let ch = &cc.name.name;
                        let m = &member.name;
                        let suggest = if m == &format!("{ch}_send_valid") || m == &format!("{ch}_send_data") {
                            Some(format!("{port}.{ch}.send(...) or {port}.{ch}.no_send()"))
                        } else if m == &format!("{ch}_credit_return") {
                            Some(format!("{port}.{ch}.pop() or {port}.{ch}.no_pop()"))
                        } else {
                            None
                        };
                        if let Some(s) = suggest {
                            errors.push(CompileError::general(
                                &format!(
                                    "underscored credit_channel access `{port}.{m}` is no longer accepted — use the dotted method form: {s}"
                                ),
                                e.span,
                            ));
                            break;
                        }
                    }
                }
            }
        }
    }
    if let ExprKind::FieldAccess(base, member) = &e.kind {
        if let ExprKind::FieldAccess(inner, ch) = &base.kind {
            if let ExprKind::Ident(port) = &inner.kind {
                if let Some((bus_name, perspective)) = ctx.port_buses.get(port) {
                    if let Some(ccs) = ctx.bus_ccs.get(bus_name) {
                        if let Some(cc) = ccs.iter().find(|c| c.name.name == ch.name) {
                            let is_sender = matches!(
                                (cc.role_dir, perspective),
                                (Direction::Out, BusPerspective::Initiator)
                              | (Direction::In,  BusPerspective::Target)
                            );
                            let synth = match member.name.as_str() {
                                "can_send" if is_sender => Some((TypeExpr::Bool, "can_send")),
                                "valid"    if !is_sender => Some((TypeExpr::Bool, "valid")),
                                "data"     if !is_sender => {
                                    cc.params.iter()
                                        .find(|p| p.name.name == "T")
                                        .and_then(|p| match &p.kind {
                                            ParamKind::Type(te) => Some(te.clone()),
                                            _ => None,
                                        })
                                        .map(|ty| (ty, "data"))
                                }
                                _ => None,
                            };
                            if let Some((ty, suffix)) = synth {
                                let name = format!("__{port}_{}_{suffix}", ch.name);
                                e.kind = ExprKind::SynthIdent(name, ty);
                            } else if matches!(member.name.as_str(),
                                "send_valid" | "send_data" | "credit_return")
                            {
                                // Dotted access to raw wire (escape hatch for
                                // direct conditional drives that no_send/no_pop
                                // can't express). Rewrite to the flat bus signal
                                // name so the resolver finds it via the normal
                                // bus-member path.
                                let flat = format!("{}_{}", ch.name, member.name);
                                let new_member = Ident::new(flat, member.span);
                                e.kind = ExprKind::FieldAccess((*inner).clone(), new_member);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── TLM target thread lowering (PR-tlm-3c) ──────────────────────────────────
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
    for it in &ast.items {
        match it {
            Item::Bus(b) => {
                if !b.tlm_methods.is_empty() {
                    bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if !b.tlm_methods.is_empty() {
                        bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_methods.is_empty() { return Ok(ast); }

    let mut out_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    let mut errors: Vec<CompileError> = Vec::new();
    for it in ast.items {
        match it {
            Item::Module(mut m) => {
                // Build port → bus_name map for this module.
                let port_buses: HashMap<String, String> = m.ports.iter()
                    .filter_map(|p| p.bus_info.as_ref().map(|bi|
                        (p.name.name.clone(), bi.bus_name.name.clone())))
                    .collect();

                // Detect multi-implementer target cases. Indexed target
                // lanes (`thread s.read[t](...)`) are handled below by
                // generating private lane endpoints plus one shared mux.
                // Non-indexed multi-targets still produce multiple drivers.
                {
                    let mut counts: HashMap<(String, String), (usize, usize, Span)> = HashMap::new();
                    for item in &m.body {
                        if let ModuleBodyItem::Thread(t) = item {
                            let key = if let Some(tb) = &t.tlm_target {
                                Some((tb.port.name.clone(), tb.method.name.clone(), tb.tag_lane.is_some()))
                            } else if let Some(ib) = &t.implement {
                                if ib.kind == TlmImplementKind::Target {
                                    Some((ib.port.name.clone(), ib.method.name.clone(), false))
                                } else { None }
                            } else { None };
                            if let Some((port, method, indexed)) = key {
                                let e = counts.entry((port, method)).or_insert((0, 0, t.span));
                                e.0 += 1;
                                if indexed { e.1 += 1; }
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
                let latch_regs: Vec<RegDecl> = Vec::new();
                let mut new_body: Vec<ModuleBodyItem> = Vec::new();
                let mut indexed_target_groups: HashMap<(String, String), Vec<(ThreadBlock, TlmTargetBinding, TlmMethodMeta)>> = HashMap::new();
                for item in std::mem::take(&mut m.body) {
                    if let ModuleBodyItem::Thread(t) = &item {
                        // v1 dotted-name form populates `tlm_target`; v2
                        // `implement target port.method(args)` populates
                        // `implement`. Normalize to a single TlmTargetBinding.
                        let effective_target: Option<TlmTargetBinding> = t.tlm_target.clone()
                            .or_else(|| t.implement.as_ref()
                                .filter(|b| b.kind == TlmImplementKind::Target)
                                .map(|b| TlmTargetBinding {
                                    port: b.port.clone(),
                                    method: b.method.clone(),
                                    tag_lane: None,
                                    args: b.args.clone(),
                                }));
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
                            let method = match bus_methods.get(&bus_name)
                                .and_then(|v| v.iter().find(|mm| mm.name.name == binding.method.name))
                            {
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
                            let t_moved = if let ModuleBodyItem::Thread(t) = item { t } else { unreachable!() };
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
                for ((_port, _method), group) in indexed_target_groups {
                    match lower_indexed_tlm_target_group(group) {
                        Ok(items) => new_body.extend(items),
                        Err(e) => errors.push(e),
                    }
                }
                // Inline lowering emits its own RegDecl / RegBlock /
                // CombBlock items directly into new_body; no additional
                // accumulation needed.
                let _ = latch_regs;
                m.body = new_body;
                out_items.push(Item::Module(m));
            }
            other => out_items.push(other),
        }
    }
    if !errors.is_empty() { return Err(errors); }
    Ok(SourceFile { items: out_items, inner_doc: None, frontmatter: None })
}



// ── TLM initiator call-site lowering (PR-tlm-4) ─────────────────────────────
//
// Recognizes `target_reg <= port.method(args);` as a TLM call site inside a
// thread body and expands it into the two-state issue + wait-response
// sequence described in doc/plan_tlm_method.md §Lowering. Call sites
// outside this shape are rejected with a targeted message.

pub fn lower_tlm_initiator_calls(ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    use std::collections::HashMap;
    let mut bus_methods: HashMap<String, Vec<TlmMethodMeta>> = HashMap::new();
    for it in &ast.items {
        match it {
            Item::Bus(b) => {
                if !b.tlm_methods.is_empty() {
                    bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                }
            }
            Item::Package(pkg) => {
                for b in &pkg.buses {
                    if !b.tlm_methods.is_empty() {
                        bus_methods.insert(b.name.name.clone(), b.tlm_methods.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if bus_methods.is_empty() { return Ok(ast); }

    let mut errors: Vec<CompileError> = Vec::new();
    let mut out_items: Vec<Item> = Vec::with_capacity(ast.items.len());
    for it in ast.items {
        match it {
            Item::Module(mut m) => {
                let port_buses: HashMap<String, String> = m.ports.iter()
                    .filter_map(|p| p.bus_info.as_ref().map(|bi|
                        (p.name.name.clone(), bi.bus_name.name.clone())))
                    .collect();

                let mut direct_groups: HashMap<(String, String), Vec<DirectTlmThread>> = HashMap::new();
                for item in &m.body {
                    if let ModuleBodyItem::Thread(t) = item {
                        if t.tlm_target.is_some() || t.implement.is_some() { continue; }
                        for dt in direct_tlm_threads(t, &port_buses, &bus_methods) {
                            direct_groups.entry((dt.call.port.clone(), dt.call.method.clone()))
                                .or_default()
                                .push(dt);
                        }
                    }
                }
                let cohort_groups: std::collections::HashSet<(String, String)> = direct_groups.iter()
                    .filter_map(|(k, v)| if v.len() > 1 { Some(k.clone()) } else { None })
                    .collect();
                let cohort_thread_spans: std::collections::HashSet<(usize, usize)> = direct_groups.values()
                    .filter(|v| v.len() > 1)
                    .flat_map(|v| v.iter().map(|dt| (dt.thread.span.start, dt.thread.span.end)))
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
                            if t.tlm_target.is_some() { continue; }
                            // Threads carrying `implement` are the opt-in
                            // mechanism for multi-thread TLM — skip the
                            // lock-idiom diagnostic on them. Multi-
                            // implementer rejection is handled below by
                            // PR-tlm-i3 (initiator) with its own targeted
                            // message.
                            if t.implement.is_some() { continue; }
                            collect_bare_tlm_calls(&t.body, t.span, &port_buses, &bus_methods, &mut bare_uses);
                        }
                    }
                    for ((port, method), spans) in &bare_uses {
                        if cohort_groups.contains(&(port.clone(), method.clone())) {
                            continue;
                        }
                        let mut sorted_offsets: Vec<(usize, usize)> = spans.iter()
                            .map(|s| (s.start, s.end)).collect();
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
                // PR-tlm-i3: count initiator-side `implement m.method()`
                // threads per (port, method). Single-implementer routes
                // through existing inline lowering (equivalent to v1
                // single-thread); multi-implementer is PR-tlm-i4.
                let mut init_impl_counts: HashMap<(String, String), usize> = HashMap::new();
                for item in &m.body {
                    if let ModuleBodyItem::Thread(t) = item {
                        if let Some(ib) = &t.implement {
                            if ib.kind == TlmImplementKind::Initiator {
                                *init_impl_counts.entry((ib.port.name.clone(), ib.method.name.clone()))
                                    .or_insert(0) += 1;
                            }
                        }
                    }
                }

                let mut new_body: Vec<ModuleBodyItem> = Vec::new();
                let mut emitted_cohorts: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
                for item in std::mem::take(&mut m.body) {
                    if let ModuleBodyItem::Thread(t) = &item {
                        let t_key = (t.span.start, t.span.end);
                        if cohort_thread_spans.contains(&t_key) {
                            if let Some(dt) = direct_tlm_threads(t, &port_buses, &bus_methods).into_iter().next() {
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
                        if t.tlm_target.is_some() {
                            new_body.push(item);
                            continue;
                        }
                        if thread_has_fork_tlm_assign(&t.body) {
                            match inline_lower_tlm_fork_join_all(t.clone(), &port_buses, &bus_methods) {
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
                                let count = init_impl_counts
                                    .get(&(ib.port.name.clone(), ib.method.name.clone()))
                                    .copied().unwrap_or(0);
                                if count > 1 {
                                    // Multi-implementer — PR-tlm-i4.
                                    errors.push(CompileError::general(
                                        &format!(
                                            "multi-implementer initiator for `{}.{}()` is not yet implemented — {count} threads carry `implement` on this method. id-tagged request arbitration ships in PR-tlm-i4 (see doc/plan_tlm_implement_thread.md).",
                                            ib.port.name, ib.method.name,
                                        ),
                                        t.span,
                                    ));
                                    new_body.push(item);
                                    continue;
                                }
                                // Single-implementer initiator — fall through
                                // to the inline lowering (v1 equivalent).
                            } else {
                                // Target kind here is unexpected (should've
                                // been consumed earlier). Leave for the
                                // lower_threads defensive error.
                                new_body.push(item);
                                continue;
                            }
                        }
                        if thread_body_has_tlm_call(&t.body, &port_buses, &bus_methods) {
                            let t_moved = if let ModuleBodyItem::Thread(t) = item { t } else { unreachable!() };
                            match inline_lower_tlm_initiator(t_moved, &port_buses, &bus_methods) {
                                Ok(items) => new_body.extend(items),
                                Err(e) => errors.push(e),
                            }
                            continue;
                        }
                    }
                    new_body.push(item);
                }
                m.body = new_body;
                out_items.push(Item::Module(m));
            }
            other => out_items.push(other),
        }
    }
    if !errors.is_empty() { return Err(errors); }
    Ok(SourceFile { items: out_items, inner_doc: None, frontmatter: None })
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
    if t.default_when.is_some() || t.once || t.body.len() != 1 {
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
            if out.len() > 1 { out } else { Vec::new() }
        }
        _ => Vec::new(),
    }
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
        return Err(CompileError::general("internal error: TLM cohort lowering requires at least two threads", module_span));
    }
    let first = &group[0];
    let port = first.call.port.clone();
    let method = first.call.method.clone();
    let method_meta = first.call.method_meta.clone();
    let span = first.thread.span;
    let tag_width = if let Some(e) = &method_meta.out_of_order_tags {
        Some(literal_expr_u64(e).ok_or_else(|| CompileError::general(
            "`out_of_order tags` must be a literal width in the first implementation",
            span,
        ))? as u32)
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
                    dt.call.args.len(), method, method_meta.args.len()
                ),
                dt.thread.span,
            ));
        }
    }

    let n = group.len();
    if let Some(tag_w) = tag_width {
        let tag_slots = if tag_w >= 64 { u128::MAX } else { 1u128 << tag_w };
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
    let bin = |op: BinOp, l: Expr, r: Expr| Expr::new(ExprKind::Binary(op, Box::new(l), Box::new(r)), span);
    let not = |e: Expr| Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span);
    let tern = |c: Expr, t: Expr, e: Expr| Expr::new(ExprKind::Ternary(Box::new(c), Box::new(t), Box::new(e)), span);
    let index = |base: Expr, idx: Expr| Expr::new(ExprKind::Index(Box::new(base), Box::new(idx)), span);
    let trunc = |e: Expr, w: u32| Expr::new(
        ExprKind::MethodCall(
            Box::new(e),
            ident("trunc".to_string()),
            vec![dec(w as u64)],
        ),
        span,
    );
    let port_member = |member: String| Expr::new(
        ExprKind::FieldAccess(Box::new(id(port.clone())), ident(member)),
        span,
    );
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
            span,
        }));
    }
    items.push(ModuleBodyItem::RegDecl(RegDecl {
        name: ident(fifo_name.clone()),
        ty: fifo_ty,
        init: None,
        reset: RegReset::Inherit(rst.clone(), zero()),
        guard: None,
        span,
    }));
    for ptr in [&head_name, &tail_name] {
        items.push(ModuleBodyItem::RegDecl(RegDecl {
            name: ident(ptr.clone()),
            ty: idx_ty.clone(),
            init: None,
            reset: RegReset::Inherit(rst.clone(), zero()),
            guard: None,
            span,
        }));
    }
    items.push(ModuleBodyItem::RegDecl(RegDecl {
        name: ident(occ_name.clone()),
        ty: occ_ty,
        init: None,
        reset: RegReset::Inherit(rst.clone(), zero()),
        guard: None,
        span,
    }));

    let occ_nonzero = bin(BinOp::Gt, id(occ_name.clone()), sized(occ_w, 0));
    let occ_not_full = bin(BinOp::Lt, id(occ_name.clone()), sized(occ_w, n as u64));
    let rsp_pop = bin(BinOp::And, port_member(format!("{method}_rsp_valid")), occ_nonzero.clone());
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
    let req_fire = bin(BinOp::And, req_valid.clone(), port_member(format!("{method}_req_ready")));
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
        let push_i = bin(BinOp::And, grants[i].clone(), port_member(format!("{method}_req_ready")));
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
                bin(BinOp::Eq, port_member(format!("{method}_rsp_tag")), sized(tag_w, i as u64)),
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
            value: trunc(bin(BinOp::Add, id(occ_name.clone()), sized(occ_w, 1)), occ_w),
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
            value: trunc(bin(BinOp::Sub, id(occ_name.clone()), sized(occ_w, 1)), occ_w),
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
    items.push(ModuleBodyItem::CombBlock(CombBlock { stmts: comb_stmts, span }));
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
            ThreadStmt::ForkJoin(branches, _) => for branch in branches {
                let branch_span = branch.first().map(thread_stmt_span).unwrap_or(owner_span);
                collect_bare_tlm_calls(branch, branch_span, port_buses, bus_methods, out);
            },
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

fn thread_stmt_span(stmt: &ThreadStmt) -> Span {
    match stmt {
        ThreadStmt::SeqAssign(a) | ThreadStmt::CombAssign(a) | ThreadStmt::ForkTlmAssign(a) => a.span,
        ThreadStmt::WaitUntil(_, sp)
        | ThreadStmt::WaitCycles(_, sp)
        | ThreadStmt::ForkJoin(_, sp)
        | ThreadStmt::JoinAll(sp)
        | ThreadStmt::Return(_, sp) => *sp,
        ThreadStmt::IfElse(ie) => ie.span,
        ThreadStmt::For { span, .. }
        | ThreadStmt::Lock { span, .. }
        | ThreadStmt::DoUntil { span, .. } => *span,
        ThreadStmt::Log(l) => l.span,
    }
}

fn thread_body_has_tlm_call(
    stmts: &[ThreadStmt],
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::SeqAssign(ra) =>
            contains_tlm_call(&ra.value, port_buses, bus_methods)
            || contains_tlm_call(&ra.target, port_buses, bus_methods),
        ThreadStmt::ForkTlmAssign(ra) =>
            contains_tlm_call(&ra.value, port_buses, bus_methods)
            || contains_tlm_call(&ra.target, port_buses, bus_methods),
        ThreadStmt::CombAssign(ca) =>
            contains_tlm_call(&ca.value, port_buses, bus_methods)
            || contains_tlm_call(&ca.target, port_buses, bus_methods),
        ThreadStmt::WaitUntil(e, _) =>
            contains_tlm_call(e, port_buses, bus_methods),
        ThreadStmt::ForkJoin(branches, _) =>
            branches.iter().any(|branch| thread_body_has_tlm_call(branch, port_buses, bus_methods)),
        _ => false,
    })
}

fn thread_has_fork_tlm_assign(stmts: &[ThreadStmt]) -> bool {
    stmts.iter().any(|s| match s {
        ThreadStmt::ForkTlmAssign(_) => true,
        ThreadStmt::IfElse(ie) => thread_has_fork_tlm_assign(&ie.then_stmts)
            || thread_has_fork_tlm_assign(&ie.else_stmts),
        ThreadStmt::ForkJoin(branches, _) => branches.iter().any(|b| thread_has_fork_tlm_assign(b)),
        ThreadStmt::For { body, .. }
        | ThreadStmt::Lock { body, .. }
        | ThreadStmt::DoUntil { body, .. } => thread_has_fork_tlm_assign(body),
        _ => false,
    })
}

#[derive(Clone)]
struct ForkedTlmIssue {
    delay: u64,
    target: Expr,
    call: TlmCall,
    span: Span,
}

fn collect_fork_join_all_issues(
    t: &ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<Vec<ForkedTlmIssue>, CompileError> {
    let mut delay = 0u64;
    let mut issues = Vec::new();
    let mut saw_join = false;
    for (idx, stmt) in t.body.iter().enumerate() {
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
                        "statements after `join all;` in a forked TLM group are not supported in v1",
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
                    return Err(CompileError::general("duplicate `join all;` in forked TLM group", *sp));
                }
                saw_join = true;
                if idx + 1 != t.body.len() {
                    return Err(CompileError::general(
                        "v1 forked TLM groups require `join all;` to be the final statement in the thread",
                        *sp,
                    ));
                }
            }
            other => {
                return Err(CompileError::general(
                    &format!(
                        "v1 forked TLM groups only support `target <= fork port.method(...);`, literal `wait N cycle;`, and final `join all;` (found {:?})",
                        std::mem::discriminant(other),
                    ),
                    thread_stmt_span(other),
                ));
            }
        }
    }
    if issues.is_empty() {
        return Err(CompileError::general("`join all;` has no preceding forked TLM calls", t.span));
    }
    if !saw_join {
        return Err(CompileError::general(
            "forked TLM calls require an explicit final `join all;` barrier",
            t.span,
        ));
    }
    Ok(issues)
}

fn inline_lower_tlm_fork_join_all(
    t: ThreadBlock,
    port_buses: &std::collections::HashMap<String, String>,
    bus_methods: &std::collections::HashMap<String, Vec<TlmMethodMeta>>,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    let issues = collect_fork_join_all_issues(&t, port_buses, bus_methods)?;
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
                    issue.call.args.len(), method, method_meta.args.len()
                ),
                issue.span,
            ));
        }
    }
    let n = issues.len();
    let tag_width = if let Some(e) = &method_meta.out_of_order_tags {
        Some(literal_expr_u64(e).ok_or_else(|| CompileError::general(
            "`out_of_order tags` must be a literal width in the first implementation",
            e.span,
        ))? as u32)
    } else {
        None
    };
    if let Some(tag_w) = tag_width {
        let tag_slots = if tag_w >= 64 { u128::MAX } else { 1u128 << tag_w };
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
    let tag = t.name.as_ref().map(|n| n.name.clone()).unwrap_or_else(|| "anon".to_string());
    let prefix = format!("_tlm_fork_{}_{}_{}", tag, port, method);

    let ident = |name: String| Ident { name, span };
    let id = |name: String| Expr::new(ExprKind::Ident(name), span);
    let dec = |v: u64| Expr::new(ExprKind::Literal(LitKind::Dec(v)), span);
    let sized = |w: u32, v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(w, v)), span);
    let zero = || Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
    let bool_lit = |b: bool| Expr::new(ExprKind::Bool(b), span);
    let bin = |op: BinOp, l: Expr, r: Expr| Expr::new(ExprKind::Binary(op, Box::new(l), Box::new(r)), span);
    let not = |e: Expr| Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), span);
    let tern = |c: Expr, t: Expr, e: Expr| Expr::new(ExprKind::Ternary(Box::new(c), Box::new(t), Box::new(e)), span);
    let index = |base: Expr, idx: Expr| Expr::new(ExprKind::Index(Box::new(base), Box::new(idx)), span);
    let trunc = |e: Expr, w: u32| Expr::new(
        ExprKind::MethodCall(Box::new(e), ident("trunc".to_string()), vec![dec(w as u64)]),
        span,
    );
    let port_member = |member: String| Expr::new(
        ExprKind::FieldAccess(Box::new(id(port.clone())), ident(member)),
        span,
    );
    let state_name = |i: usize| format!("{prefix}_t{i}_state");
    let fifo_name = format!("{prefix}_fifo");
    let head_name = format!("{prefix}_head");
    let tail_name = format!("{prefix}_tail");
    let occ_name = format!("{prefix}_occ");
    let age_name = format!("{prefix}_age");

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
            span,
        }));
    }

    let occ_nonzero = bin(BinOp::Gt, id(occ_name.clone()), sized(occ_w, 0));
    let occ_not_full = bin(BinOp::Lt, id(occ_name.clone()), sized(occ_w, n as u64));
    let rsp_pop = bin(BinOp::And, port_member(format!("{method}_rsp_valid")), occ_nonzero.clone());
    let fifo_head = index(id(fifo_name.clone()), id(head_name.clone()));
    let all_done = {
        let mut acc = bin(BinOp::Eq, id(state_name(0)), sized(2, 2));
        for i in 1..n {
            acc = bin(BinOp::And, acc, bin(BinOp::Eq, id(state_name(i)), sized(2, 2)));
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
        let want_i = bin(BinOp::And, bin(BinOp::And, pending, aged), occ_not_full.clone());
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
    let req_fire = bin(BinOp::And, req_valid.clone(), port_member(format!("{method}_req_ready")));
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

    let mut seq_body: Vec<Stmt> = Vec::new();
    seq_body.push(Stmt::IfElse(IfElse {
        cond: all_done.clone(),
        then_stmts: (0..n).map(|i| Stmt::Assign(RegAssign {
            target: id(state_name(i)),
            value: sized(2, 0),
            span,
        })).chain(std::iter::once(Stmt::Assign(RegAssign {
            target: id(age_name.clone()),
            value: sized(age_w, 0),
            span,
        }))).collect(),
        else_stmts: if max_delay > 0 {
            vec![Stmt::IfElse(IfElse {
                cond: bin(BinOp::Lt, id(age_name.clone()), sized(age_w, max_delay)),
                then_stmts: vec![Stmt::Assign(RegAssign {
                    target: id(age_name.clone()),
                    value: trunc(bin(BinOp::Add, id(age_name.clone()), sized(age_w, 1)), age_w),
                    span,
                })],
                else_stmts: Vec::new(),
                unique: false,
                span,
            })]
        } else { Vec::new() },
        unique: false,
        span,
    }));
    for i in 0..n {
        let push_i = bin(BinOp::And, grants[i].clone(), port_member(format!("{method}_req_ready")));
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
                bin(BinOp::And, rsp_pop.clone(), bin(BinOp::Eq, id(state_name(i)), sized(2, 1))),
                bin(BinOp::Eq, port_member(format!("{method}_rsp_tag")), sized(tag_w, i as u64)),
            )
        } else {
            bin(
                BinOp::And,
                bin(BinOp::And, rsp_pop.clone(), bin(BinOp::Eq, id(state_name(i)), sized(2, 1))),
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
            value: trunc(bin(BinOp::Add, id(occ_name.clone()), sized(occ_w, 1)), occ_w),
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
            value: trunc(bin(BinOp::Sub, id(occ_name.clone()), sized(occ_w, 1)), occ_w),
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
    items.push(ModuleBodyItem::CombBlock(CombBlock { stmts: comb_stmts, span }));
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
    let tag = t.name.as_ref().map(|n| n.name.clone()).unwrap_or_else(|| "tlm_init".to_string());

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

    for stmt in t.body {
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
                            seq_on_exit: std::mem::take(&mut pending_seq),
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
            other => {
                return Err(CompileError::general(
                    &format!(
                        "v1 TLM initiator thread body only supports SeqAssign statements (found {:?}). Refactor more complex control flow into a `thread` without TLM calls.",
                        std::mem::discriminant(&other),
                    ),
                    span,
                ));
            }
        }
    }
    // Trailing pending seq becomes a Compute state too.
    if !pending_seq.is_empty() {
        states.push(StateKind::Compute { seq_on_exit: std::mem::take(&mut pending_seq) });
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
    let state_eq = |v: u64| Expr::new(
        ExprKind::Binary(BinOp::Eq, Box::new(state_expr.clone()), Box::new(mk_state_lit(v))),
        span,
    );
    let state_reg_decl = RegDecl {
        name: mk_ident(state_reg_name.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(state_width as u64)), span,
        ))),
        init: None,
        reset: RegReset::Inherit(t.reset.clone(), Expr::new(ExprKind::Literal(LitKind::Dec(0)), span)),
        guard: None,
        span,
    };

    let mk_port_member = |port: &str, member: String| Expr::new(
        ExprKind::FieldAccess(
            Box::new(Expr::new(ExprKind::Ident(port.to_string()), span)),
            mk_ident(member),
        ),
        span,
    );

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
        issues: Vec<(u64, Vec<Expr>)>,  // (state_idx, args at that call site)
        waits: Vec<u64>,                 // state_idx
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
            StateKind::TlmIssue { port, method, args, method_meta } => {
                let key = format!("{port}.{method}");
                aggs.entry(key).or_insert_with(|| MethodAgg {
                    port: port.clone(),
                    method: method.clone(),
                    ret_ty: method_meta.ret.clone(),
                    arg_decls: method_meta.args.clone(),
                    tag_width: method_meta.out_of_order_tags.clone(),
                    issues: Vec::new(),
                    waits: Vec::new(),
                }).issues.push((cur_idx, args.clone()));
                // Seq: advance on req_ready.
                let advance_cond = Expr::new(
                    ExprKind::Binary(BinOp::And,
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
            StateKind::TlmWait { port, method, method_meta, dest } => {
                let key = format!("{port}.{method}");
                aggs.entry(key).or_insert_with(|| MethodAgg {
                    port: port.clone(),
                    method: method.clone(),
                    ret_ty: method_meta.ret.clone(),
                    arg_decls: method_meta.args.clone(),
                    tag_width: method_meta.out_of_order_tags.clone(),
                    issues: Vec::new(),
                    waits: Vec::new(),
                }).waits.push(cur_idx);
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
                                    Box::new(Expr::new(ExprKind::Literal(LitKind::Sized(tag_w, 0)), span)),
                                ),
                                span,
                            )),
                        ),
                        span,
                    );
                }
                let advance_cond = Expr::new(
                    ExprKind::Binary(BinOp::And,
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
    if match_tlm_call(e, port_buses, bus_methods).is_some() { return true; }
    match &e.kind {
        ExprKind::Binary(_, l, r) => contains_tlm_call(l, port_buses, bus_methods) || contains_tlm_call(r, port_buses, bus_methods),
        ExprKind::Unary(_, x) | ExprKind::Cast(x, _) | ExprKind::Clog2(x)
        | ExprKind::Onehot(x) | ExprKind::Signed(x) | ExprKind::Unsigned(x)
        | ExprKind::LatencyAt(x, _)
        | ExprKind::SvaNext(_, x) => contains_tlm_call(x, port_buses, bus_methods),
        ExprKind::Index(b, i) => contains_tlm_call(b, port_buses, bus_methods) || contains_tlm_call(i, port_buses, bus_methods),
        ExprKind::FieldAccess(b, _) => contains_tlm_call(b, port_buses, bus_methods),
        ExprKind::MethodCall(recv, _, args) => {
            contains_tlm_call(recv, port_buses, bus_methods)
                || args.iter().any(|a| contains_tlm_call(a, port_buses, bus_methods))
        }
        ExprKind::Ternary(c, t, el) => contains_tlm_call(c, port_buses, bus_methods) || contains_tlm_call(t, port_buses, bus_methods) || contains_tlm_call(el, port_buses, bus_methods),
        ExprKind::Concat(xs) | ExprKind::FunctionCall(_, xs) => xs.iter().any(|x| contains_tlm_call(x, port_buses, bus_methods)),
        ExprKind::Repeat(n, x) => contains_tlm_call(n, port_buses, bus_methods) || contains_tlm_call(x, port_buses, bus_methods),
        _ => false,
    }
}


// ── TLM target in-place lowering (PR-tlm-4b) ────────────────────────────────
//
// Replaces the previous "transform into regular thread" approach with
// direct emission of RegDecl + RegBlock + CombBlock items into the
// parent module body. This bypasses lower_threads entirely for TLM
// target threads and avoids the sub-module bus-flattening bridging that
// the thread-extraction path doesn't handle for FieldAccess(bus_port,
// member) drives.
//
// Supported user-body shape (v1):
//   <SeqAssign | CombAssign | WaitUntil>*
//   return <expr>;
//
// Any other statement in the body (nested IfElse / ForkJoin / For /
// Lock / DoUntil / Log) is rejected with a targeted error.

fn lower_indexed_tlm_target_group(
    mut group: Vec<(ThreadBlock, TlmTargetBinding, TlmMethodMeta)>,
) -> Result<Vec<ModuleBodyItem>, CompileError> {
    if group.is_empty() {
        return Ok(Vec::new());
    }
    let span = group[0].0.span;
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
        CompileError::general("`out_of_order tags` must be a literal width for indexed target lowering", tag_w_expr.span)
    })? as u32;
    let tag_slots = if tag_w >= 64 { u128::MAX } else { 1u128 << tag_w };

    let mut seen = std::collections::HashSet::new();
    let mut lanes: Vec<(u64, ThreadBlock, TlmTargetBinding, TlmMethodMeta)> = Vec::new();
    for (t, binding, method_meta) in group.drain(..) {
        let lane_expr = binding.tag_lane.as_ref().ok_or_else(|| {
            CompileError::general("internal error: indexed TLM target group contains an unindexed target", t.span)
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
    let port_member = |member: String| Expr::new(
        ExprKind::FieldAccess(
            Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
            mk_ident(member),
        ),
        span,
    );
    let lit0 = Expr::new(ExprKind::Literal(LitKind::Dec(0)), span);
    let lit1 = Expr::new(ExprKind::Literal(LitKind::Sized(1, 1)), span);
    let tag_lit = |lane: u64| Expr::new(ExprKind::Literal(LitKind::Sized(tag_w, lane)), span);
    let tag_eq = |lane: u64| Expr::new(
        ExprKind::Binary(
            BinOp::Eq,
            Box::new(port_member(format!("{method_name}_req_tag"))),
            Box::new(tag_lit(lane)),
        ),
        span,
    );

    let mut out = Vec::new();
    let mut lane_infos = Vec::new();
    for (lane, t, binding, method_meta) in lanes {
        let prefix = format!("_tlm_{port}_{method_name}_tag{lane}");
        let req_ready = format!("{prefix}_req_ready");
        let rsp_valid = format!("{prefix}_rsp_valid");
        let rsp_ready = format!("{prefix}_rsp_ready");
        let rsp_tag = format!("{prefix}_rsp_tag");
        let rsp_data = format!("{prefix}_rsp_data");

        out.push(ModuleBodyItem::WireDecl(WireDecl { name: mk_ident(req_ready.clone()), ty: TypeExpr::Bool, unpacked: false, span }));
        out.push(ModuleBodyItem::WireDecl(WireDecl { name: mk_ident(rsp_valid.clone()), ty: TypeExpr::Bool, unpacked: false, span }));
        out.push(ModuleBodyItem::WireDecl(WireDecl { name: mk_ident(rsp_ready.clone()), ty: TypeExpr::Bool, unpacked: false, span }));
        out.push(ModuleBodyItem::WireDecl(WireDecl {
            name: mk_ident(rsp_tag.clone()),
            ty: TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(tag_w as u64)), span))),
            unpacked: false,
            span,
        }));
        if let Some(ret_ty) = &method.ret {
            out.push(ModuleBodyItem::WireDecl(WireDecl { name: mk_ident(rsp_data.clone()), ty: ret_ty.clone(), unpacked: false, span }));
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
        out.extend(inline_lower_tlm_target_with_io(t, &binding, &method_meta, io)?);
        lane_infos.push((lane, req_ready, rsp_valid, rsp_ready, rsp_data, rsp_tag));
    }

    let mut comb_stmts = Vec::new();
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
    for (_lane, _req_ready, rsp_valid, rsp_ready, rsp_data, rsp_tag) in &lane_infos {
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
            cond: ident_expr(rsp_valid.clone()),
            then_stmts,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    out.push(ModuleBodyItem::CombBlock(CombBlock { stmts: comb_stmts, span }));
    Ok(out)
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
    let mk_port_member = |member: String| Expr::new(
        ExprKind::FieldAccess(
            Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
            mk_ident(member),
        ),
        span,
    );
    let io = TlmTargetIo {
        suffix: String::new(),
        req_valid: mk_port_member(format!("{method_name}_req_valid")),
        rsp_ready: mk_port_member(format!("{method_name}_rsp_ready")),
        req_ready_target: mk_port_member(format!("{method_name}_req_ready")),
        rsp_valid_target: mk_port_member(format!("{method_name}_rsp_valid")),
        rsp_data_target: method.ret.as_ref().map(|_| mk_port_member(format!("{method_name}_rsp_data"))),
        rsp_tag_target: method.out_of_order_tags.as_ref().map(|_| mk_port_member(format!("{method_name}_rsp_tag"))),
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
    let mk_port_member = |member: String| Expr::new(
        ExprKind::FieldAccess(
            Box::new(Expr::new(ExprKind::Ident(port.clone()), span)),
            mk_ident(member),
        ),
        span,
    );
    let lit_one = Expr::new(ExprKind::Literal(LitKind::Sized(1, 1)), span);
    let lit_zero = Expr::new(ExprKind::Literal(LitKind::Sized(1, 0)), span);

    // Walk user body into a list of "user states". Each state is a
    // vector of seq assigns fired on entry to the next state + a
    // transition condition (the WaitUntil). A Return terminates the
    // walk and becomes the respond state.
    struct UserState {
        seq_on_exit: Vec<Stmt>,       // fires on transition out of this state
        comb_in_state: Vec<Stmt>, // active during this state
        transition_cond: Expr,
    }
    let mut user_states: Vec<UserState> = Vec::new();
    let mut cur_seq: Vec<Stmt> = Vec::new();
    let mut cur_comb: Vec<Stmt> = Vec::new();
    let mut return_expr: Option<Expr> = None;

    // Arg renames: user-bound arg name → latched reg name.
    let mut arg_renames: Vec<(String, String)> = Vec::new();
    let mut latch_regs: Vec<RegDecl> = Vec::new();
    let tag_latch_name = method.out_of_order_tags.as_ref().map(|tag_w| {
        let latch_name = format!("_tlm_{port}_{method_name}{}_tag_latched", io.suffix);
        latch_regs.push(RegDecl {
            name: mk_ident(latch_name.clone()),
            ty: TypeExpr::UInt(Box::new(tag_w.clone())),
            init: None,
            reset: RegReset::Inherit(t.reset.clone(), Expr::new(ExprKind::Literal(LitKind::Dec(0)), span)),
            guard: None,
            span,
        });
        latch_name
    });
    for (user_arg, method_arg) in binding.args.iter().zip(method.args.iter()) {
        let latch_name = format!("_tlm_{port}_{method_name}{}_{}_latched", io.suffix, method_arg.0.name);
        latch_regs.push(RegDecl {
            name: mk_ident(latch_name.clone()),
            ty: method_arg.1.clone(),
            init: None,
            reset: RegReset::Inherit(t.reset.clone(), Expr::new(ExprKind::Literal(LitKind::Dec(0)), span)),
            guard: None,
            span,
        });
        arg_renames.push((user_arg.name.clone(), latch_name));
    }

    // Helper: apply arg renames to an expression.
    let rename_args = |e: Expr, renames: &[(String, String)]| -> Expr {
        let mut cur = e;
        for (from, to) in renames {
            cur = rewrite_var_expr(cur, from, to);
        }
        cur
    };

    for stmt in t.body {
        match stmt {
            ThreadStmt::SeqAssign(ra) => {
                cur_seq.push(Stmt::Assign(RegAssign {
                    target: rename_args(ra.target, &arg_renames),
                    value: rename_args(ra.value, &arg_renames),
                    span: ra.span,
                }));
            }
            ThreadStmt::CombAssign(ca) => {
                cur_comb.push(Stmt::Assign(CombAssign {
                    target: rename_args(ca.target, &arg_renames),
                    value: rename_args(ca.value, &arg_renames),
                    span: ca.span,
                }));
            }
            ThreadStmt::WaitUntil(cond, _) => {
                user_states.push(UserState {
                    seq_on_exit: std::mem::take(&mut cur_seq),
                    comb_in_state: std::mem::take(&mut cur_comb),
                    transition_cond: rename_args(cond, &arg_renames),
                });
            }
            ThreadStmt::Return(e, _) => {
                return_expr = Some(rename_args(e, &arg_renames));
                break;
            }
            other => {
                return Err(CompileError::general(
                    &format!("TLM target thread body statement not supported in v1 (only SeqAssign / CombAssign / WaitUntil / Return allowed inline). Offender: {:?}", std::mem::discriminant(&other)),
                    span,
                ));
            }
        }
    }
    if return_expr.is_none() && method.ret.is_some() {
        return Err(CompileError::general(
            &format!(
                "`thread {}.{}(...)` must end with `return <expr>;` (method declares return type {:?})",
                port, method_name, method.ret,
            ),
            span,
        ));
    }

    // Final pending seq/comb from body (between last wait and return).
    let final_seq_on_exit = cur_seq;
    let final_comb_in_state = cur_comb;

    // Total states: ENTRY (0) + user_states + RESPOND (last).
    let total_states = 2 + user_states.len();
    let state_width = clog2_width(total_states as u64);
    let entry_idx = 0u64;
    let respond_idx = (total_states - 1) as u64;

    let state_reg_name = format!("_tlm_{port}_{method_name}{}_state", io.suffix);
    let state_ident = Expr::new(ExprKind::Ident(state_reg_name.clone()), span);
    let mk_state_lit = |v: u64| Expr::new(ExprKind::Literal(LitKind::Sized(state_width, v)), span);
    let state_eq = |v: u64| Expr::new(
        ExprKind::Binary(BinOp::Eq, Box::new(state_ident.clone()), Box::new(mk_state_lit(v))),
        span,
    );

    // ── State register declaration ───────────────────────────────────────
    let state_reg = RegDecl {
        name: mk_ident(state_reg_name.clone()),
        ty: TypeExpr::UInt(Box::new(Expr::new(
            ExprKind::Literal(LitKind::Dec(state_width as u64)), span,
        ))),
        init: None,
        reset: RegReset::Inherit(t.reset.clone(), Expr::new(ExprKind::Literal(LitKind::Dec(0)), span)),
        guard: None,
        span,
    };

    // ── Seq block: state transitions + arg latches + user seq assigns ──
    // Build nested if/elsif over state_reg.
    let mut seq_body: Vec<Stmt> = Vec::new();
    // State 0: ENTRY — if req_valid, latch args and advance to 1.
    let mut entry_then: Vec<Stmt> = Vec::new();
    for (user_arg, method_arg) in binding.args.iter().zip(method.args.iter()) {
        let latch_name = format!("_tlm_{port}_{method_name}{}_{}_latched", io.suffix, method_arg.0.name);
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
    entry_then.push(Stmt::Assign(RegAssign {
        target: state_ident.clone(),
        value: mk_state_lit(1),
        span,
    }));
    let entry_branch_cond = Expr::new(
        ExprKind::Binary(BinOp::And,
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
    // User states 1..N
    for (i, us) in user_states.iter().enumerate() {
        let state_idx = (i + 1) as u64;
        let next_idx = (i + 2) as u64;
        let mut then_stmts: Vec<Stmt> = us.seq_on_exit.clone();
        then_stmts.push(Stmt::Assign(RegAssign {
            target: state_ident.clone(),
            value: mk_state_lit(next_idx),
            span,
        }));
        let branch_cond = Expr::new(
            ExprKind::Binary(BinOp::And,
                Box::new(state_eq(state_idx)),
                Box::new(us.transition_cond.clone()),
            ),
            span,
        );
        seq_body.push(Stmt::IfElse(IfElseOf {
            cond: branch_cond,
            then_stmts,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }
    // Last-user-state → respond state. Falls through from the body walk:
    // the state immediately before respond fires `final_seq_on_exit` on
    // the natural transition.
    let pre_respond_idx = (user_states.len() + 1) as u64;
    if pre_respond_idx != entry_idx {
        // Only if there are user states — otherwise the entry → respond
        // transition is needed. Handle both:
        let mut then_stmts: Vec<Stmt> = final_seq_on_exit.clone();
        if !user_states.is_empty() {
            then_stmts.push(Stmt::Assign(RegAssign {
                target: state_ident.clone(),
                value: mk_state_lit(respond_idx),
                span,
            }));
            seq_body.push(Stmt::IfElse(IfElseOf {
                cond: state_eq(pre_respond_idx),
                then_stmts,
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }
    // Respond state → entry (loop back) when rsp_ready.
    let mut respond_then: Vec<Stmt> = Vec::new();
    respond_then.push(Stmt::Assign(RegAssign {
        target: state_ident.clone(),
        value: mk_state_lit(entry_idx),
        span,
    }));
    let respond_branch_cond = Expr::new(
        ExprKind::Binary(BinOp::And,
            Box::new(state_eq(respond_idx)),
            Box::new(io.rsp_ready.clone()),
        ),
        span,
    );
    seq_body.push(Stmt::IfElse(IfElseOf {
        cond: respond_branch_cond,
        then_stmts: respond_then,
        else_stmts: Vec::new(),
        unique: false,
        span,
    }));

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
    // rsp_valid = (state == respond)
    comb_stmts.push(Stmt::Assign(CombAssign {
        target: io.rsp_valid_target.clone(),
        value: state_eq(respond_idx),
        span,
    }));
    // rsp_data = <return expr> (always driven; only observed when rsp_valid).
    if let Some(expr) = return_expr {
        if method.ret.is_some() {
            if let Some(target) = io.rsp_data_target.clone() {
                comb_stmts.push(Stmt::Assign(CombAssign { target, value: expr, span }));
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
    for (i, us) in user_states.iter().enumerate() {
        let state_idx = (i + 1) as u64;
        if !us.comb_in_state.is_empty() {
            comb_stmts.push(Stmt::IfElse(IfElse {
                cond: state_eq(state_idx),
                then_stmts: us.comb_in_state.clone(),
                else_stmts: Vec::new(),
                unique: false,
                span,
            }));
        }
    }
    if !final_comb_in_state.is_empty() {
        comb_stmts.push(Stmt::IfElse(IfElse {
            cond: state_eq(pre_respond_idx),
            then_stmts: final_comb_in_state,
            else_stmts: Vec::new(),
            unique: false,
            span,
        }));
    }

    let comb_block = CombBlock {
        stmts: comb_stmts,
        span,
    };

    // ── Assemble output items ────────────────────────────────────────────
    let mut items: Vec<ModuleBodyItem> = Vec::new();
    items.push(ModuleBodyItem::RegDecl(state_reg));
    for r in latch_regs { items.push(ModuleBodyItem::RegDecl(r)); }
    items.push(ModuleBodyItem::RegBlock(reg_block));
    items.push(ModuleBodyItem::CombBlock(comb_block));
    let _ = lit_one; let _ = lit_zero;
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
