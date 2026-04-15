//! `.archi` interface file generation.
//!
//! Emits a minimal ARCH source containing only the module signature
//! (params + ports, no body) for use in separate compilation.

use crate::ast::*;

/// Emit `.archi` content for a single AST item.
/// Returns `Some(content)` for items that have an external interface
/// (module, fsm, counter, pipeline), `None` otherwise.
pub fn emit_interface(item: &Item) -> Option<String> {
    match item {
        Item::Module(m) => Some(emit_module_interface(m)),
        Item::Fsm(f) => Some(emit_fsm_interface(f)),
        Item::Counter(c) => Some(emit_counter_interface(c)),
        Item::Pipeline(p) => Some(emit_pipeline_interface(p)),
        Item::Bus(b) => Some(emit_bus_interface(b)),
        Item::Struct(s) => Some(emit_struct(s)),
        Item::Enum(e) => Some(emit_enum(e)),
        Item::Package(p) => Some(emit_package_interface(p)),
        Item::Synchronizer(s) => Some(emit_generic("synchronizer", &s.name.name, &s.params, &s.ports)),
        Item::Fifo(f) => Some(emit_generic("fifo", &f.name.name, &f.params, &f.ports)),
        Item::Ram(r) => Some(emit_generic("ram", &r.name.name, &r.params, &r.ports)),
        Item::Arbiter(a) => Some(emit_generic("arbiter", &a.name.name, &a.params, &a.ports)),
        Item::Regfile(r) => Some(emit_generic("regfile", &r.name.name, &r.params, &r.ports)),
        Item::Clkgate(c) => Some(emit_generic("clkgate", &c.name.name, &c.params, &c.ports)),
        Item::Linklist(l) => Some(emit_generic("linklist", &l.name.name, &l.params, &l.ports)),
        _ => None,
    }
}

fn emit_module_interface(m: &ModuleDecl) -> String {
    let name = &m.name.name;
    let mut s = format!("module {name}\n");
    emit_params(&mut s, &m.params);
    emit_ports(&mut s, &m.ports);
    s.push_str(&format!("end module {name}\n"));
    s
}

fn emit_fsm_interface(f: &FsmDecl) -> String {
    let name = &f.name.name;
    let mut s = format!("fsm {name}\n");
    emit_params(&mut s, &f.params);
    emit_ports(&mut s, &f.ports);
    s.push_str(&format!("end fsm {name}\n"));
    s
}

fn emit_counter_interface(c: &CounterDecl) -> String {
    let name = &c.name.name;
    let mut s = format!("counter {name}\n");
    emit_params(&mut s, &c.params);
    emit_ports(&mut s, &c.ports);
    s.push_str(&format!("end counter {name}\n"));
    s
}

fn emit_pipeline_interface(p: &PipelineDecl) -> String {
    let name = &p.name.name;
    let mut s = format!("pipeline {name}\n");
    emit_params(&mut s, &p.params);
    emit_ports(&mut s, &p.ports);
    s.push_str(&format!("end pipeline {name}\n"));
    s
}

fn emit_bus_interface(b: &BusDecl) -> String {
    let name = &b.name.name;
    let mut s = format!("bus {name}\n");
    emit_params(&mut s, &b.params);
    emit_ports(&mut s, &b.signals);
    s.push_str(&format!("end bus {name}\n"));
    s
}

/// Generic emitter for constructs with name + params + ports (synchronizer, fifo, ram, etc.)
fn emit_generic(keyword: &str, name: &str, params: &[ParamDecl], ports: &[PortDecl]) -> String {
    let mut s = format!("{keyword} {name}\n");
    emit_params(&mut s, params);
    emit_ports(&mut s, ports);
    s.push_str(&format!("end {keyword} {name}\n"));
    s
}

fn emit_struct(s: &StructDecl) -> String {
    let name = &s.name.name;
    let mut out = format!("struct {name}\n");
    for f in &s.fields {
        out.push_str(&format!("  {}: {};\n", f.name.name, type_str(&f.ty)));
    }
    out.push_str(&format!("end struct {name}\n"));
    out
}

fn emit_enum(e: &EnumDecl) -> String {
    let name = &e.name.name;
    let mut out = format!("enum {name}\n");
    for (i, v) in e.variants.iter().enumerate() {
        if let Some(Some(ref val)) = e.values.get(i) {
            out.push_str(&format!("  {} = {};\n", v.name, expr_str(val)));
        } else {
            out.push_str(&format!("  {};\n", v.name));
        }
    }
    out.push_str(&format!("end enum {name}\n"));
    out
}

fn emit_package_interface(p: &PackageDecl) -> String {
    let name = &p.name.name;
    let mut out = format!("package {name}\n");
    // Params
    emit_params(&mut out, &p.params);
    // Enums
    for e in &p.enums {
        out.push_str(&indent(&emit_enum(e)));
    }
    // Structs
    for s in &p.structs {
        out.push_str(&indent(&emit_struct(s)));
    }
    // Function signatures (no body)
    for f in &p.functions {
        let fname = &f.name.name;
        let params: Vec<String> = f.args.iter()
            .map(|fp| format!("{}: {}", fp.name.name, type_str(&fp.ty)))
            .collect();
        out.push_str(&format!("  function {fname}({}) -> {};\n",
            params.join(", "), type_str(&f.ret_ty)));
    }
    out.push_str(&format!("end package {name}\n"));
    out
}

/// Indent each line by 2 spaces (for nesting structs/enums inside packages).
fn indent(s: &str) -> String {
    s.lines().map(|l| format!("  {l}\n")).collect()
}

fn emit_params(s: &mut String, params: &[ParamDecl]) {
    for p in params {
        let local = if p.is_local { "local " } else { "" };
        let name = &p.name.name;
        match &p.kind {
            ParamKind::Const => {
                if let Some(ref def) = p.default {
                    s.push_str(&format!("  {local}param {name}: const = {};\n", expr_str(def)));
                } else {
                    s.push_str(&format!("  {local}param {name}: const;\n"));
                }
            }
            ParamKind::WidthConst(hi, lo) => {
                let default_str = p.default.as_ref()
                    .map(|d| format!(" = {}", expr_str(d)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}[{}:{}]: const{default_str};\n",
                    expr_str(hi), expr_str(lo)
                ));
            }
            ParamKind::Type(ty) => {
                s.push_str(&format!(
                    "  {local}param {name}: type = {};\n",
                    type_str(ty)
                ));
            }
            ParamKind::EnumConst(enum_name) => {
                let default_str = p.default.as_ref()
                    .map(|d| format!(" = {}", expr_str(d)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}: {enum_name}{default_str};\n"
                ));
            }
        }
    }
}

fn emit_ports(s: &mut String, ports: &[PortDecl]) {
    for p in ports {
        let dir = match p.direction {
            Direction::In => "in",
            Direction::Out => "out",
        };
        let name = &p.name.name;

        if let Some(ref bi) = p.bus_info {
            let persp = match bi.perspective {
                BusPerspective::Initiator => "initiator",
                BusPerspective::Target => "target",
            };
            let bus_name = &bi.bus_name.name;
            // TODO: bus param assignments
            s.push_str(&format!("  port {name}: {persp} {bus_name};\n"));
        } else {
            let ty = type_str(&p.ty);
            // For port reg with reset, include reset clause
            if let Some(ref ri) = p.reg_info {
                match &ri.reset {
                    RegReset::Inherit(rst, val) | RegReset::Explicit(rst, _, _, val) => {
                        let rst_name = &rst.name;
                        let rst_val = expr_str(val);
                        s.push_str(&format!("  port reg {name}: {dir} {ty} reset {rst_name} => {rst_val};\n"));
                    }
                    RegReset::None => {
                        s.push_str(&format!("  port reg {name}: {dir} {ty};\n"));
                    }
                }
            } else {
                s.push_str(&format!("  port {name}: {dir} {ty};\n"));
            }
        }
    }
}

/// Format a TypeExpr as ARCH syntax.
fn type_str(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::UInt(w) => format!("UInt<{}>", expr_str(w)),
        TypeExpr::SInt(w) => format!("SInt<{}>", expr_str(w)),
        TypeExpr::Bool => "Bool".to_string(),
        TypeExpr::Bit => "Bit".to_string(),
        TypeExpr::Clock(domain) => format!("Clock<{}>", domain.name),
        TypeExpr::Reset(kind, level) => {
            let k = match kind {
                ResetKind::Sync => "Sync",
                ResetKind::Async => "Async",
            };
            let l = match level {
                ResetLevel::High => "",
                ResetLevel::Low => ", Low",
            };
            format!("Reset<{k}{l}>")
        }
        TypeExpr::Vec(elem, count) => format!("Vec<{}, {}>", type_str(elem), expr_str(count)),
        TypeExpr::Named(n) => n.name.clone(),
    }
}

/// Format an Expr as ARCH syntax (simplified — handles common width expressions).
fn expr_str(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match lit {
            LitKind::Dec(v) => v.to_string(),
            LitKind::Hex(v) => format!("0x{:X}", v),
            LitKind::Bin(v) => format!("0b{:b}", v),
            LitKind::Sized(width, val) => format!("{width}'d{val}"),
        },
        ExprKind::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        ExprKind::Ident(name) => name.clone(),
        ExprKind::Binary(op, l, r) => format!("({} {} {})", expr_str(l), op, expr_str(r)),
        ExprKind::Clog2(inner) => format!("clog2({})", expr_str(inner)),
        _ => "0".to_string(), // fallback for complex expressions
    }
}
