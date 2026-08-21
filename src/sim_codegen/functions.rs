//! `functions` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
    pub(super) fn gen_functions(&self, fns: &[&FunctionDecl]) -> SimModel {
        let mut h = String::new();
        h.push_str("#pragma once\n#include \"verilated.h\"\n\n");

        // Hoist package- and module-level const params as `#define`s so that
        // function bodies referencing them (e.g. `x >> REGION_BITS` where
        // `REGION_BITS` is a `package` param) compile. The SV path resolves
        // this via `import Pkg::*`; the sim path has no equivalent scope —
        // module-internal functions get hoisted to free C++ functions, and
        // VFunctions.h is included from each V{Module}.h *before* the
        // per-module `#define`s, so without this block the identifier is
        // simply undeclared. `#ifndef`-guarded so re-definitions in
        // per-module headers are harmless.
        let mut emitted_param_defines: HashSet<String> = HashSet::new();
        let mut function_param_macros: Vec<String> = Vec::new();
        for item in &self.source.items {
            let (params, _ctx_label): (&[ParamDecl], &str) = match item {
                Item::Package(pkg) => (&pkg.params, "package"),
                Item::Module(m) => (&m.params, "module"),
                _ => continue,
            };
            for p in params {
                if !emitted_param_defines.insert(p.name.name.clone()) {
                    continue;
                }
                match &p.kind {
                    ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_) => {
                        if let Some(val) = eval_param_const_value(p, params) {
                            function_param_macros.push(p.name.name.clone());
                            h.push_str(&format!(
                                "#ifndef {}\n#define {} {val}ULL\n#define ARCH_SIM_VFUNCTIONS_DEFINED_{}\n#endif\n",
                                p.name.name, p.name.name, p.name.name
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        h.push('\n');

        for f in fns {
            // Free functions hoisted out of a module body. The function's
            // param-resolving context is the enclosing module's params (see
            // L4329 above where param defines are emitted from the module's
            // param list), but that slice isn't threaded into this loop yet.
            // The bare-form-equivalent `&[]` is acceptable as a residual
            // since user-written `function ... -> T` signatures typically use
            // concrete-width types; tracked as a follow-up to arch-com#463.
            let ret_ty = cpp_internal_type_with_params(&f.ret_ty, &[]);
            let args_str: Vec<String> = f
                .args
                .iter()
                .map(|a| {
                    format!(
                        "{} {}",
                        cpp_internal_type_with_params(&a.ty, &[]),
                        a.name.name
                    )
                })
                .collect();
            h.push_str(&format!(
                "inline {ret_ty} {}({}) {{\n",
                f.name.name,
                args_str.join(", ")
            ));

            let empty_regs: HashSet<String> = HashSet::new();
            let empty_lets: HashSet<String> = HashSet::new();
            let empty_insts: HashSet<String> = HashSet::new();
            let empty_wide: HashSet<String> = HashSet::new();
            let enum_map = build_enum_map(self.symbols);

            // Build arg + local-let names as bare ports (resolve_name hits
            // them via port_names → no `_let_` prefix, matching the
            // `const T name = ...;` emitted line). Their widths are
            // registered so `infer_expr_width` returns the right size
            // when the name is used inside a `Concat` — pre-fix, every
            // Concat part fell back to width=8, so a
            // `{bool, bool, UInt<3>}` concat emitted shifts at offsets
            // 0/8/16 instead of 0/3/4 and produced wildly wrong values.
            fn collect_function_locals(
                items: &[FunctionBodyItem],
                names: &mut HashSet<String>,
                widths: &mut HashMap<String, u32>,
                signed: &mut HashSet<String>,
            ) {
                for item in items {
                    match item {
                        FunctionBodyItem::Let(l) => {
                            names.insert(l.name.name.clone());
                            let w = match l.ty.as_ref() {
                                Some(TypeExpr::UInt(w)) | Some(TypeExpr::SInt(w)) => eval_width(w),
                                Some(TypeExpr::Bool) | Some(TypeExpr::Bit) => 1,
                                _ => 32,
                            };
                            widths.insert(l.name.name.clone(), w);
                            if l.ty.as_ref().map_or(false, type_is_signed_scalar) {
                                signed.insert(l.name.name.clone());
                            }
                        }
                        FunctionBodyItem::For(fl) => {
                            names.insert(fl.var.name.clone());
                            widths.insert(fl.var.name.clone(), 32);
                            collect_function_locals(&fl.body, names, widths, signed);
                        }
                        FunctionBodyItem::IfElse(ie) => {
                            collect_function_locals(&ie.then_body, names, widths, signed);
                            collect_function_locals(&ie.else_body, names, widths, signed);
                        }
                        FunctionBodyItem::Return(_) | FunctionBodyItem::Assign(_) => {}
                    }
                }
            }
            let empty_bus: HashSet<String> = HashSet::new();
            let mut local_widths: HashMap<String, u32> = HashMap::new();
            let mut local_signed_names: HashSet<String> = HashSet::new();
            // Float formats of args + typed locals, so float `+ - *` /
            // compares / fma inside the body dispatch to the _arch_fp
            // helpers instead of integer ops on the bit pattern.
            let mut local_float_names: HashMap<String, FpFmt> = HashMap::new();
            fn collect_fn_float_lets(items: &[FunctionBodyItem], out: &mut HashMap<String, FpFmt>) {
                for item in items {
                    match item {
                        FunctionBodyItem::Let(l) => {
                            if let Some(fmt) = l.ty.as_ref().and_then(type_float_fmt) {
                                out.insert(l.name.name.clone(), fmt);
                            }
                        }
                        FunctionBodyItem::IfElse(ie) => {
                            collect_fn_float_lets(&ie.then_body, out);
                            collect_fn_float_lets(&ie.else_body, out);
                        }
                        FunctionBodyItem::For(fl) => collect_fn_float_lets(&fl.body, out),
                        _ => {}
                    }
                }
            }
            for a in &f.args {
                if let Some(fmt) = type_float_fmt(&a.ty) {
                    local_float_names.insert(a.name.name.clone(), fmt);
                }
            }
            collect_fn_float_lets(&f.body, &mut local_float_names);
            let mut arg_ports: HashSet<String> =
                f.args.iter().map(|a| a.name.name.clone()).collect();
            for a in &f.args {
                local_widths.insert(
                    a.name.name.clone(),
                    match &a.ty {
                        TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval_width(w),
                        TypeExpr::Bool | TypeExpr::Bit => 1,
                        _ => 32,
                    },
                );
                if type_is_signed_scalar(&a.ty) {
                    local_signed_names.insert(a.name.name.clone());
                }
            }
            collect_function_locals(
                &f.body,
                &mut arg_ports,
                &mut local_widths,
                &mut local_signed_names,
            );
            let function_loop_var_subst: std::cell::RefCell<HashMap<String, u32>> =
                std::cell::RefCell::new(HashMap::new());
            let ctx_base = Ctx::new(
                &empty_regs,
                &arg_ports,
                &empty_lets,
                &empty_insts,
                &empty_wide,
                &local_widths,
                &enum_map,
                &empty_bus,
            )
            .with_signed_names(&local_signed_names)
            .with_float_names(&local_float_names);
            let ctx = Ctx {
                loop_var_subst: Some(&function_loop_var_subst),
                ..ctx_base
            };

            // Recursive emitter for nested function-body items (if/elsif/else
            // with return statements inside). Pre-fix the if/for/assign arms
            // were no-ops, so a function whose entire body was an if/else
            // emitted as `inline T fn(...) { }` and called sites failed C++
            // compile with "non-void function does not return a value".
            fn emit_fn_items(
                items: &[FunctionBodyItem],
                ctx: &Ctx,
                ret_ty: &str,
                indent: &str,
                out: &mut String,
            ) {
                for item in items {
                    match item {
                        FunctionBodyItem::Let(l) => {
                            let ty =
                                l.ty.as_ref()
                                    .map(|t| cpp_internal_type_with_params(t, &[]))
                                    .unwrap_or_else(|| "uint32_t".to_string());
                            let val = cpp_expr(&l.value, ctx);
                            out.push_str(&format!("{indent}{ty} {} = {};\n", l.name.name, val));
                        }
                        FunctionBodyItem::Return(e) => {
                            let val = cpp_expr(e, ctx);
                            out.push_str(&format!("{indent}return {val};\n"));
                        }
                        FunctionBodyItem::IfElse(ie) => {
                            let cond = cpp_expr(&ie.cond, ctx);
                            out.push_str(&format!("{indent}if ({cond}) {{\n"));
                            emit_fn_items(&ie.then_body, ctx, ret_ty, &format!("{indent}  "), out);
                            out.push_str(&format!("{indent}}}"));
                            if !ie.else_body.is_empty() {
                                out.push_str(" else {\n");
                                emit_fn_items(
                                    &ie.else_body,
                                    ctx,
                                    ret_ty,
                                    &format!("{indent}  "),
                                    out,
                                );
                                out.push_str(&format!("{indent}}}\n"));
                            } else {
                                out.push_str("\n");
                            }
                        }
                        FunctionBodyItem::For(fl) => {
                            let var = &fl.var.name;
                            match &fl.range {
                                ForRange::Range(lo, hi) => {
                                    let lo_s = cpp_expr(lo, ctx);
                                    let hi_s = cpp_expr(hi, ctx);
                                    out.push_str(&format!("{indent}for (int {var} = {lo_s}; {var} <= {hi_s}; {var}++) {{\n"));
                                    emit_fn_items(
                                        &fl.body,
                                        ctx,
                                        ret_ty,
                                        &format!("{indent}  "),
                                        out,
                                    );
                                    out.push_str(&format!("{indent}}}\n"));
                                }
                                ForRange::ValueList(vals) => {
                                    for val in vals {
                                        let v = cpp_expr(val, ctx);
                                        out.push_str(&format!("{indent}{{\n"));
                                        out.push_str(&format!("{indent}  int {var} = {v};\n"));
                                        emit_fn_items(
                                            &fl.body,
                                            ctx,
                                            ret_ty,
                                            &format!("{indent}  "),
                                            out,
                                        );
                                        out.push_str(&format!("{indent}}}\n"));
                                    }
                                }
                            }
                        }
                        FunctionBodyItem::Assign(a) => {
                            let target = cpp_expr_lhs(&a.target, ctx);
                            let val = cpp_expr(&a.value, ctx);
                            out.push_str(&format!("{indent}{target} = {val};\n"));
                        }
                    }
                }
            }
            // Reuse the same recursive pattern below; legacy direct-loop is
            // kept around the existing match-as-switch shortcut for `Return`.
            for item in &f.body {
                match item {
                    FunctionBodyItem::Let(l) => {
                        let ty =
                            l.ty.as_ref()
                                .map(|t| cpp_internal_type_with_params(t, &[]))
                                .unwrap_or_else(|| "uint32_t".to_string());
                        let val = cpp_expr(&l.value, &ctx);
                        h.push_str(&format!("  {ty} {} = {};\n", l.name.name, val));
                    }
                    FunctionBodyItem::IfElse(ie) => {
                        let cond = cpp_expr(&ie.cond, &ctx);
                        h.push_str(&format!("  if ({cond}) {{\n"));
                        emit_fn_items(&ie.then_body, &ctx, &ret_ty, "    ", &mut h);
                        h.push_str("  }");
                        if !ie.else_body.is_empty() {
                            h.push_str(" else {\n");
                            emit_fn_items(&ie.else_body, &ctx, &ret_ty, "    ", &mut h);
                            h.push_str("  }\n");
                        } else {
                            h.push_str("\n");
                        }
                    }
                    FunctionBodyItem::For(_) | FunctionBodyItem::Assign(_) => {
                        emit_fn_items(std::slice::from_ref(item), &ctx, &ret_ty, "  ", &mut h);
                    }
                    FunctionBodyItem::Return(e) => {
                        // If it's a match expression, emit as switch for efficiency
                        if let ExprKind::ExprMatch(scrut, arms) = &e.kind {
                            let s = cpp_expr(scrut, &ctx);
                            h.push_str(&format!("  switch ({s}) {{\n"));
                            for arm in arms {
                                let val = cpp_expr(&arm.value, &ctx);
                                match &arm.pattern {
                                    Pattern::Wildcard | Pattern::Ident(_) => {
                                        h.push_str(&format!("    default: return {val};\n"));
                                    }
                                    Pattern::Literal(le) => {
                                        let pat = cpp_expr(le, &ctx);
                                        h.push_str(&format!("    case {pat}: return {val};\n"));
                                    }
                                    Pattern::EnumVariant(en, vr) => {
                                        if let Some(variants) = enum_map.get(&en.name) {
                                            let idx = variants
                                                .iter()
                                                .find(|(n, _)| *n == vr.name)
                                                .map(|(_, v)| *v)
                                                .unwrap_or(0);
                                            h.push_str(&format!("    case {idx}: return {val};\n"));
                                        }
                                    }
                                }
                            }
                            h.push_str("  }\n");
                            h.push_str(&format!("  return ({ret_ty})0;\n"));
                        } else {
                            let val = cpp_expr(e, &ctx);
                            h.push_str(&format!("  return {val};\n"));
                        }
                    }
                }
            }
            h.push_str("}\n\n");
        }

        for name in &function_param_macros {
            h.push_str(&format!(
                "#ifdef ARCH_SIM_VFUNCTIONS_DEFINED_{name}\n#undef {name}\n#undef ARCH_SIM_VFUNCTIONS_DEFINED_{name}\n#endif\n"
            ));
        }

        SimModel {
            class_name: "VFunctions".to_string(),
            header: h,
            impl_: String::new(), // header-only
        }
    }
}

pub(super) fn collect_stmt_assigns(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign(a) => {
                // Walk the LHS unwrapping Index / BitSlice / PartSelect /
                // FieldAccess until we hit the base Ident. `counter_q[hi:lo]`,
                // `counter_q[i]`, `reg.field`, and chained forms all bind to
                // `counter_q` for reset-walk purposes — the partial-write
                // reg is still subject to its declared reset.
                let mut cursor: &Expr = &a.target;
                loop {
                    match &cursor.kind {
                        ExprKind::Ident(n) => {
                            out.insert(n.clone());
                            break;
                        }
                        ExprKind::Index(base, _)
                        | ExprKind::BitSlice(base, _, _)
                        | ExprKind::PartSelect(base, _, _, _)
                        | ExprKind::FieldAccess(base, _) => {
                            cursor = base;
                        }
                        _ => break,
                    }
                }
            }
            Stmt::IfElse(ie) => {
                collect_stmt_assigns(&ie.then_stmts, out);
                collect_stmt_assigns(&ie.else_stmts, out);
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    collect_stmt_assigns(&arm.body, out);
                }
            }
            Stmt::Log(_) => {}
            Stmt::For(f) => {
                collect_stmt_assigns(&f.body, out);
            }
            Stmt::Init(ib) => {
                collect_stmt_assigns(&ib.body, out);
            }
            Stmt::WaitUntil(_, _) => {}
            Stmt::DoUntil { body, .. } => {
                collect_stmt_assigns(body, out);
            }
        }
    }
}
