//! Module-scope type alias resolution.
//!
//! Replaces `type Name = <TypeExpr>;` aliases declared inside a module
//! body with their RHS at every use site. Runs once, before elaboration,
//! so every downstream pass (typecheck / elaborate / codegen / sim) sees
//! aliases as if they had been inlined by hand.
//!
//! Scope (MVP):
//! - Module-scope only (not file-level / package-level).
//! - No parameterized aliases (no `type Bus<N> = ...`).
//! - No forward references — an alias's RHS may reference only aliases
//!   declared **earlier** in the same module body. (`Item::Struct`,
//!   `Item::Enum`, `Item::Bus` named at file level are always visible.)
//! - No recursion — cycles are detected and reported.
//! - Aliases must not shadow primitive type keywords (`UInt`, `SInt`,
//!   `Bool`, `Bit`, `Clock`, `Reset`, `Vec`). The lexer already keyword-
//!   tokenizes those, so `type UInt = ...` fails at parse for the LHS;
//!   the check below is defensive for clarity and future-proofing.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::CompileError;

/// Top-level entry point — substitute all module-scope type aliases in
/// every `Item::Module` of the source file. Returns the rewritten AST
/// (aliases removed) or a list of errors.
pub fn resolve_type_aliases(mut ast: SourceFile) -> Result<SourceFile, Vec<CompileError>> {
    let mut errors: Vec<CompileError> = Vec::new();
    for item in ast.items.iter_mut() {
        if let Item::Module(m) = item {
            if let Err(mut es) = resolve_module(m) {
                errors.append(&mut es);
            }
        }
    }
    if errors.is_empty() { Ok(ast) } else { Err(errors) }
}

/// Resolved alias table: `name -> (TypeExpr, bus_params)`.
type AliasMap = HashMap<String, (TypeExpr, Vec<ParamAssign>)>;

const PRIMITIVE_TYPE_NAMES: &[&str] =
    &["UInt", "SInt", "Bool", "Bit", "Clock", "Reset", "Vec"];

fn resolve_module(m: &mut ModuleDecl) -> Result<(), Vec<CompileError>> {
    let mut errors: Vec<CompileError> = Vec::new();

    // 1. Walk body in source order, collecting aliases. Each alias's RHS
    //    is resolved against the already-collected map, so earlier
    //    aliases can be referenced by later ones (chains). Self-reference
    //    is detected as a cycle.
    let mut aliases: AliasMap = HashMap::new();
    // Preserve declaration order for diagnostics.
    let mut alias_order: Vec<(String, crate::lexer::Span)> = Vec::new();

    for item in &m.body {
        if let ModuleBodyItem::TypeAlias(a) = item {
            let name = a.name.name.clone();

            // Reject primitive-type shadowing.
            if PRIMITIVE_TYPE_NAMES.contains(&name.as_str()) {
                errors.push(CompileError::general(
                    &format!("type alias name '{}' shadows a primitive type", name),
                    a.name.span,
                ));
                continue;
            }

            // Reject duplicate alias decls.
            if aliases.contains_key(&name) {
                errors.push(CompileError::general(
                    &format!("duplicate type alias '{}'", name),
                    a.name.span,
                ));
                continue;
            }

            // Resolve the RHS using the already-collected aliases.
            // Forward references and self-reference both manifest here
            // as "unknown alias" — the alias is not yet in the map.
            let mut resolving: HashSet<String> = HashSet::new();
            resolving.insert(name.clone());
            let resolved_ty = match resolve_in_type(&a.ty, &aliases, &resolving) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            // Bus params declared on the alias are kept as-is (no inner
            // alias substitution — they're value expressions, not types).
            let resolved_bus_params = a.bus_params.clone();

            aliases.insert(name.clone(), (resolved_ty, resolved_bus_params));
            alias_order.push((name, a.name.span));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // 2. Substitute aliases through the rest of the module: params,
    //    ports, body items. The TypeAlias items themselves are dropped.
    if !aliases.is_empty() {
        for p in m.params.iter_mut() {
            substitute_in_param(p, &aliases, &mut errors);
        }
        for p in m.ports.iter_mut() {
            substitute_in_port(p, &aliases, &mut errors);
        }
        // Drain body so we can rebuild it without the TypeAlias entries.
        let body = std::mem::take(&mut m.body);
        let mut new_body: Vec<ModuleBodyItem> = Vec::with_capacity(body.len());
        for item in body {
            match item {
                ModuleBodyItem::TypeAlias(_) => { /* drop after resolution */ }
                mut other => {
                    substitute_in_body_item(&mut other, &aliases, &mut errors);
                    new_body.push(other);
                }
            }
        }
        m.body = new_body;
    } else {
        // No aliases declared — still need to scan for any stray uses
        // (a bare `type` body item with no aliases is impossible by the
        // parser, so this branch is exercised when the module had a
        // single bogus alias that was already errored on above).
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// Walk a TypeExpr; replace every `Named(name)` that matches an alias
/// with the alias's stored TypeExpr. `resolving` carries the alias names
/// currently being resolved (for cycle detection during alias-RHS
/// resolution; empty for substitution into non-alias bodies).
fn resolve_in_type(
    ty: &TypeExpr,
    aliases: &AliasMap,
    resolving: &HashSet<String>,
) -> Result<TypeExpr, CompileError> {
    match ty {
        TypeExpr::Vec(inner, size) => {
            let new_inner = resolve_in_type(inner, aliases, resolving)?;
            Ok(TypeExpr::Vec(Box::new(new_inner), size.clone()))
        }
        TypeExpr::Named(ident) => {
            if resolving.contains(&ident.name) {
                // Reached an alias we're currently resolving — cycle.
                return Err(CompileError::general(
                    &format!("circular type alias '{}'", ident.name),
                    ident.span,
                ));
            }
            if let Some((aliased_ty, _bus_params)) = aliases.get(&ident.name) {
                // Recursively expand: alias may itself reference another
                // earlier alias (already substituted in storage, but we
                // walk anyway to be safe and to keep cycle detection
                // sound).
                let mut next_resolving = resolving.clone();
                next_resolving.insert(ident.name.clone());
                resolve_in_type(aliased_ty, aliases, &next_resolving)
            } else {
                // Not an alias — could be a struct / enum / bus name, or
                // a forward-referenced alias (which is rejected by
                // unknown-alias diagnostic at the surface use site).
                // Leave Named in place; typecheck will resolve it.
                Ok(TypeExpr::Named(ident.clone()))
            }
        }
        // Primitive types — no Named inside. Width Exprs may reference
        // user identifiers (params, const lets) but never type aliases,
        // so we leave them alone.
        other => Ok(other.clone()),
    }
}

/// Apply alias substitution to a TypeExpr. Errors are collected; on
/// error the TypeExpr is returned unchanged.
fn substitute_type(ty: &mut TypeExpr, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    let empty: HashSet<String> = HashSet::new();
    match resolve_in_type(ty, aliases, &empty) {
        Ok(new_ty) => *ty = new_ty,
        Err(e) => errors.push(e),
    }
}

/// If the TypeExpr resolves through an alias whose RHS carries
/// bus_params, return them so the caller can splice into its own
/// bus_params slot (wire/port). This is called *before* substitute_type.
fn alias_bus_params(ty: &TypeExpr, aliases: &AliasMap) -> Vec<ParamAssign> {
    // Walk Vec wrappers — bus params attach to the innermost Named.
    let mut cur = ty;
    loop {
        match cur {
            TypeExpr::Vec(inner, _) => { cur = inner; }
            TypeExpr::Named(ident) => {
                if let Some((_, bus_params)) = aliases.get(&ident.name) {
                    return bus_params.clone();
                }
                return Vec::new();
            }
            _ => return Vec::new(),
        }
    }
}

fn substitute_in_param(p: &mut ParamDecl, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    match &mut p.kind {
        ParamKind::Type(ty) => substitute_type(ty, aliases, errors),
        ParamKind::ConstVec(ty) => substitute_type(ty, aliases, errors),
        ParamKind::Logic(ty) => substitute_type(ty, aliases, errors),
        _ => {}
    }
}

fn substitute_in_port(p: &mut PortDecl, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    // Bus ports: the alias must resolve to a bare Named (a bus name), and
    // the alias's bus_params merge with the port's existing params.
    if let Some(bi) = p.bus_info.as_mut() {
        // Look up alias by current bus_name (which is what the parser
        // recorded for `port p: initiator AliasName;`).
        if let Some((aliased_ty, alias_params)) = aliases.get(&bi.bus_name.name).cloned() {
            match aliased_ty {
                TypeExpr::Named(real_bus) => {
                    bi.bus_name = real_bus.clone();
                    p.ty = TypeExpr::Named(real_bus);
                    // Prepend alias params (so explicit port-level params
                    // take precedence on key collisions — last-wins
                    // matches parse_port_decl's existing merge order).
                    let mut merged = alias_params;
                    merged.append(&mut bi.params);
                    bi.params = merged;
                }
                _ => {
                    errors.push(CompileError::general(
                        &format!(
                            "type alias '{}' does not resolve to a bus type and cannot be used as `initiator`/`target`",
                            bi.bus_name.name
                        ),
                        bi.bus_name.span,
                    ));
                }
            }
        }
        // Non-alias bus port: leave bi.bus_name and p.ty alone. typecheck
        // will diagnose unknown bus names.
        return;
    }
    // Non-bus port: just substitute through the type tree.
    substitute_type(&mut p.ty, aliases, errors);
}

fn substitute_in_body_item(
    item: &mut ModuleBodyItem,
    aliases: &AliasMap,
    errors: &mut Vec<CompileError>,
) {
    match item {
        ModuleBodyItem::RegDecl(r) => substitute_type(&mut r.ty, aliases, errors),
        ModuleBodyItem::WireDecl(w) => {
            // If the wire's type references an alias that carries
            // bus_params, propagate those onto the wire's bus_params
            // slot before substituting the type tree.
            let extra = alias_bus_params(&w.ty, aliases);
            if !extra.is_empty() {
                // Wire's own bus_params (explicit) take precedence on
                // collision — append alias params last, then dedupe
                // last-wins by walking through. Matches the merge
                // strategy in parse_port_decl.
                let mut merged = extra;
                merged.append(&mut w.bus_params);
                w.bus_params = merged;
            }
            substitute_type(&mut w.ty, aliases, errors);
        }
        ModuleBodyItem::LetBinding(l) => {
            if let Some(ty) = l.ty.as_mut() {
                substitute_type(ty, aliases, errors);
            }
            substitute_in_expr(&mut l.value, aliases, errors);
        }
        ModuleBodyItem::CombBlock(b) => {
            substitute_in_stmts(&mut b.stmts, aliases, errors);
        }
        ModuleBodyItem::RegBlock(b) => {
            substitute_in_stmts(&mut b.stmts, aliases, errors);
        }
        ModuleBodyItem::LatchBlock(b) => {
            substitute_in_stmts(&mut b.stmts, aliases, errors);
        }
        ModuleBodyItem::Assert(a) => substitute_in_expr(&mut a.expr, aliases, errors),
        ModuleBodyItem::Generate(g) => substitute_in_generate(g, aliases, errors),
        ModuleBodyItem::Inst(i) => {
            for pa in i.param_assigns.iter_mut() {
                if let Some(ty) = pa.ty.as_mut() {
                    substitute_type(ty, aliases, errors);
                }
                substitute_in_expr(&mut pa.value, aliases, errors);
            }
            for c in i.connections.iter_mut() {
                substitute_in_expr(&mut c.signal, aliases, errors);
            }
        }
        ModuleBodyItem::Thread(_)
        | ModuleBodyItem::Resource(_)
        | ModuleBodyItem::Function(_)
        | ModuleBodyItem::PipeRegDecl(_)
        | ModuleBodyItem::TlmConnect(_) => {
            // MVP: these constructs don't carry top-level TypeExpr nodes
            // that users would typically alias. (Function args/ret types
            // could in principle reference an alias; deferred to a
            // follow-up if needed.)
        }
        ModuleBodyItem::TypeAlias(_) => { /* removed in caller */ }
    }
}

fn substitute_in_generate(
    g: &mut GenerateDecl,
    aliases: &AliasMap,
    errors: &mut Vec<CompileError>,
) {
    let items: &mut Vec<GenItem> = match g {
        GenerateDecl::For(f) => &mut f.items,
        GenerateDecl::If(i) => {
            substitute_in_gen_items(&mut i.then_items, aliases, errors);
            substitute_in_gen_items(&mut i.else_items, aliases, errors);
            return;
        }
    };
    substitute_in_gen_items(items, aliases, errors);
}

fn substitute_in_gen_items(
    items: &mut Vec<GenItem>,
    aliases: &AliasMap,
    errors: &mut Vec<CompileError>,
) {
    for it in items.iter_mut() {
        match it {
            GenItem::Port(p) => substitute_in_port(p, aliases, errors),
            GenItem::Inst(i) => {
                for pa in i.param_assigns.iter_mut() {
                    if let Some(ty) = pa.ty.as_mut() {
                        substitute_type(ty, aliases, errors);
                    }
                    substitute_in_expr(&mut pa.value, aliases, errors);
                }
                for c in i.connections.iter_mut() {
                    substitute_in_expr(&mut c.signal, aliases, errors);
                }
            }
            GenItem::Assert(a) => substitute_in_expr(&mut a.expr, aliases, errors),
            GenItem::Seq(b) => substitute_in_stmts(&mut b.stmts, aliases, errors),
            GenItem::Comb(b) => substitute_in_stmts(&mut b.stmts, aliases, errors),
            GenItem::Wire(w) => {
                // Substitute any alias references in the wire's type tree
                // and its bus_params overrides. Same treatment as a
                // top-level WireDecl (see the ModuleBodyItem::WireDecl arm
                // above), minus the alias-bus-param propagation — for
                // generate_for-of-wire we keep things simple: the wire
                // type is taken verbatim from source and substituted by
                // subst_wire_decl per iteration in elaborate.rs.
                substitute_type(&mut w.ty, aliases, errors);
                for pa in w.bus_params.iter_mut() {
                    substitute_in_expr(&mut pa.value, aliases, errors);
                }
            }
            GenItem::Thread(_) | GenItem::TlmConnect(_) => {}
        }
    }
}

fn substitute_in_stmts(stmts: &mut Vec<Stmt>, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    for s in stmts.iter_mut() {
        substitute_in_stmt(s, aliases, errors);
    }
}

fn substitute_in_stmt(s: &mut Stmt, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    match s {
        Stmt::Assign(a) => {
            substitute_in_expr(&mut a.target, aliases, errors);
            substitute_in_expr(&mut a.value, aliases, errors);
        }
        Stmt::IfElse(i) => {
            substitute_in_expr(&mut i.cond, aliases, errors);
            substitute_in_stmts(&mut i.then_stmts, aliases, errors);
            substitute_in_stmts(&mut i.else_stmts, aliases, errors);
        }
        Stmt::Match(m) => {
            substitute_in_expr(&mut m.scrutinee, aliases, errors);
            for arm in m.arms.iter_mut() {
                substitute_in_stmts(&mut arm.body, aliases, errors);
            }
        }
        Stmt::For(f) => {
            match &mut f.range {
                ForRange::Range(a, b) => {
                    substitute_in_expr(a, aliases, errors);
                    substitute_in_expr(b, aliases, errors);
                }
                ForRange::ValueList(vs) => {
                    for v in vs.iter_mut() {
                        substitute_in_expr(v, aliases, errors);
                    }
                }
            }
            substitute_in_stmts(&mut f.body, aliases, errors);
        }
        Stmt::Init(b) => {
            substitute_in_stmts(&mut b.body, aliases, errors);
        }
        Stmt::WaitUntil(e, _) => substitute_in_expr(e, aliases, errors),
        Stmt::DoUntil { body, cond, .. } => {
            substitute_in_stmts(body, aliases, errors);
            substitute_in_expr(cond, aliases, errors);
        }
        Stmt::Log(l) => {
            for a in l.args.iter_mut() {
                substitute_in_expr(a, aliases, errors);
            }
        }
    }
}

fn substitute_in_expr(e: &mut Expr, aliases: &AliasMap, errors: &mut Vec<CompileError>) {
    match &mut e.kind {
        ExprKind::Cast(inner, ty) => {
            substitute_in_expr(inner, aliases, errors);
            substitute_type(ty.as_mut(), aliases, errors);
        }
        ExprKind::Binary(_, a, b) => {
            substitute_in_expr(a, aliases, errors);
            substitute_in_expr(b, aliases, errors);
        }
        ExprKind::Unary(_, a) => substitute_in_expr(a, aliases, errors),
        ExprKind::FieldAccess(a, _) => substitute_in_expr(a, aliases, errors),
        ExprKind::MethodCall(recv, _, args) => {
            substitute_in_expr(recv, aliases, errors);
            for a in args.iter_mut() {
                substitute_in_expr(a, aliases, errors);
            }
        }
        ExprKind::Index(a, b) => {
            substitute_in_expr(a, aliases, errors);
            substitute_in_expr(b, aliases, errors);
        }
        ExprKind::BitSlice(a, b, c) => {
            substitute_in_expr(a, aliases, errors);
            substitute_in_expr(b, aliases, errors);
            substitute_in_expr(c, aliases, errors);
        }
        ExprKind::PartSelect(a, b, c, _) => {
            substitute_in_expr(a, aliases, errors);
            substitute_in_expr(b, aliases, errors);
            substitute_in_expr(c, aliases, errors);
        }
        ExprKind::StructLiteral(_, fields) => {
            for f in fields.iter_mut() {
                substitute_in_expr(&mut f.value, aliases, errors);
            }
        }
        ExprKind::Match(scrut, arms) => {
            substitute_in_expr(scrut, aliases, errors);
            for arm in arms.iter_mut() {
                substitute_in_stmts(&mut arm.body, aliases, errors);
            }
        }
        ExprKind::ExprMatch(scrut, arms) => {
            substitute_in_expr(scrut, aliases, errors);
            for arm in arms.iter_mut() {
                substitute_in_expr(&mut arm.value, aliases, errors);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts.iter_mut() {
                substitute_in_expr(p, aliases, errors);
            }
        }
        ExprKind::Repeat(n, v) => {
            substitute_in_expr(n, aliases, errors);
            substitute_in_expr(v, aliases, errors);
        }
        ExprKind::Clog2(inner) | ExprKind::Onehot(inner)
        | ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
            substitute_in_expr(inner, aliases, errors);
        }
        ExprKind::LatencyAt(inner, _) => substitute_in_expr(inner, aliases, errors),
        // Leaf / non-type-bearing kinds.
        _ => {}
    }
}
