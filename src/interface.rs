//! `.archi` interface file generation.
//!
//! Emits a minimal ARCH source containing only the module signature
//! (params + ports, no body) for use in separate compilation.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// Emit `.archi` content for a single AST item.
/// Returns `Some(content)` for items that have an external interface
/// (module, fsm, counter, pipeline, fifo, ram, arbiter, regfile, …),
/// `None` for the rest. Dispatch goes through `Item::as_construct().emit_interface()`
/// which is implemented per-construct in `src/ast.rs`.
pub fn emit_interface(item: &Item) -> Option<String> {
    item.as_construct().emit_interface()
}

/// Regfile uses the generic shape (no construct-specific fields beyond
/// params + ports). Wrapper kept here so the trait impl can take a
/// uniform `fn(&RegfileDecl) -> String`.
pub(crate) fn emit_regfile_interface(r: &RegfileDecl) -> String {
    emit_generic("regfile", &r.name.name, &r.params, &r.ports)
}

pub(crate) fn emit_module_interface(m: &ModuleDecl) -> String {
    let name = &m.name.name;
    let mut s = format!("module {name}\n");
    emit_params(&mut s, &m.params);
    // Compute precise per-output comb-dep sets from the module body so
    // the .archi reflects the actual dataflow shape (issue #246 Phase 2).
    // The opaque "every comb input feeds every comb output" over-
    // approximation is avoided as long as the module has a body.
    let deps = crate::comb_graph::per_output_comb_deps(m);
    emit_ports_with_deps(&mut s, &m.ports, Some(&deps));
    s.push_str(&format!("end module {name}\n"));
    s
}

pub(crate) fn emit_fsm_interface(f: &FsmDecl) -> String {
    let name = &f.name.name;
    let mut s = format!("fsm {name}\n");
    emit_params(&mut s, &f.params);
    // Compute precise per-output comb-dep sets from the FSM body so
    // the .archi reflects the actual dataflow shape (issue #246 Phase 4,
    // mirror of the module path in `emit_module_interface`). The
    // opaque "every comb input feeds every comb output" over-
    // approximation is avoided as long as the fsm has a body.
    let deps = crate::comb_graph::per_output_comb_deps_fsm(f);
    emit_ports_with_deps(&mut s, &f.ports, Some(&deps));
    s.push_str(&format!("end fsm {name}\n"));
    s
}

pub(crate) fn emit_counter_interface(c: &CounterDecl) -> String {
    let name = &c.name.name;
    let mut s = format!("counter {name}\n");
    emit_params(&mut s, &c.params);
    emit_ports(&mut s, &c.ports);
    s.push_str(&format!("end counter {name}\n"));
    s
}

pub(crate) fn emit_cam_interface(c: &CamDecl) -> String {
    // CAM has no construct-specific fields beyond name + params +
    // ports at the interface level (the cam-specific store /
    // match-policy attributes live in the body, not visible to
    // consumers). Generic shape suffices.
    emit_generic("cam", &c.name.name, &c.params, &c.ports)
}

pub(crate) fn emit_pipeline_interface(p: &PipelineDecl) -> String {
    let name = &p.name.name;
    let mut s = format!("pipeline {name}\n");
    emit_params(&mut s, &p.params);
    emit_ports(&mut s, &p.ports);
    s.push_str(&format!("end pipeline {name}\n"));
    s
}

pub(crate) fn emit_bus_interface(b: &BusDecl) -> String {
    let name = &b.name.name;
    let mut s = format!("bus {name}\n");
    emit_params(&mut s, &b.params);
    // Bus members are bare `name: dir Type;` — that is how `parse_bus_signal`
    // reads them. The generic `emit_ports` would prefix each with `port `,
    // producing a `.archi` the parser rejects ("'port' is a reserved
    // keyword"), so the emitted bus interface could never be read back.
    for sig in &b.signals {
        let dir = match sig.direction {
            Direction::In => "in",
            Direction::Out => "out",
        };
        s.push_str(&format!(
            "  {}: {dir} {};\n",
            sig.name.name,
            type_str(&sig.ty)
        ));
    }
    s.push_str(&format!("end bus {name}\n"));
    s
}

/// Generic emitter for constructs with name + params + ports (regfile, etc.)
/// Constructs with additional semantic fields (kind, policy, latency) use
/// their own per-type emitter instead — see emit_synchronizer_interface etc.
pub(crate) fn emit_generic(keyword: &str, name: &str, params: &[ParamDecl], ports: &[PortDecl]) -> String {
    let mut s = format!("{keyword} {name}\n");
    emit_params(&mut s, params);
    emit_ports(&mut s, ports);
    s.push_str(&format!("end {keyword} {name}\n"));
    s
}

pub(crate) fn emit_synchronizer_interface(sync: &SynchronizerDecl) -> String {
    let name = &sync.name.name;
    let kind = match sync.kind {
        SyncKind::Ff => "ff",
        SyncKind::Gray => "gray",
        SyncKind::Handshake => "handshake",
        SyncKind::Reset => "reset",
        SyncKind::Pulse => "pulse",
    };
    let mut s = format!("synchronizer {name}\n");
    s.push_str(&format!("  kind {kind};\n"));
    emit_params(&mut s, &sync.params);
    emit_ports(&mut s, &sync.ports);
    s.push_str(&format!("end synchronizer {name}\n"));
    s
}

pub(crate) fn emit_fifo_interface(f: &FifoDecl) -> String {
    let name = &f.name.name;
    let mut s = format!("fifo {name}\n");
    // FifoKind::Fifo is the default (sync/async detected from clock ports); only emit `kind lifo`
    if f.kind == FifoKind::Lifo {
        s.push_str("  kind lifo;\n");
    }
    emit_params(&mut s, &f.params);
    emit_ports(&mut s, &f.ports);
    s.push_str(&format!("end fifo {name}\n"));
    s
}

pub(crate) fn emit_ram_interface(r: &RamDecl) -> String {
    let name = &r.name.name;
    let kind = match r.kind {
        RamKind::Single => "single",
        RamKind::SimpleDual => "simple_dual",
        RamKind::TrueDual => "true_dual",
        RamKind::Rom => "rom",
    };
    let mut s = format!("ram {name}\n");
    s.push_str(&format!("  kind {kind};\n"));
    s.push_str(&format!("  latency {};\n", r.latency));
    emit_params(&mut s, &r.params);
    emit_ports(&mut s, &r.ports);
    s.push_str(&format!("end ram {name}\n"));
    s
}

pub(crate) fn emit_arbiter_interface(a: &ArbiterDecl) -> String {
    let name = &a.name.name;
    let policy = match &a.policy {
        ArbiterPolicy::RoundRobin => "round_robin".to_string(),
        ArbiterPolicy::Priority => "priority".to_string(),
        ArbiterPolicy::Lru => "lru".to_string(),
        ArbiterPolicy::Weighted(w) => format!("weighted<{}>", expr_str(w)),
        ArbiterPolicy::Custom(fn_name) => format!("custom {}", fn_name.name),
    };
    let mut s = format!("arbiter {name}\n");
    s.push_str(&format!("  policy {policy};\n"));
    if a.latency > 0 {
        s.push_str(&format!("  latency {};\n", a.latency));
    }
    emit_params(&mut s, &a.params);
    emit_ports(&mut s, &a.ports);
    // Per-requester ports group (`ports[N] request { valid; ready; }`).
    // Without this, the .archi shows only the scalar control ports
    // and a downstream consumer can't see that this arbiter exposes
    // an array of per-requester valid/ready signals — which is
    // information the inst-site connection writer needs.
    for pa in &a.port_arrays {
        let count = expr_str(&pa.count_expr);
        s.push_str(&format!("  ports[{count}] {}\n", pa.name.name));
        for sig in &pa.signals {
            let dir = match sig.direction {
                Direction::In => "in",
                Direction::Out => "out",
            };
            s.push_str(&format!("    {}: {dir} {};\n", sig.name.name, type_str(&sig.ty)));
        }
        s.push_str(&format!("  end ports {}\n", pa.name.name));
    }
    s.push_str(&format!("end arbiter {name}\n"));
    s
}

pub(crate) fn emit_clkgate_interface(c: &ClkGateDecl) -> String {
    let name = &c.name.name;
    let kind = match c.kind {
        ClkGateKind::Latch => "latch",
        ClkGateKind::And => "and",
    };
    let mut s = format!("clkgate {name}\n");
    s.push_str(&format!("  kind {kind};\n"));
    emit_params(&mut s, &c.params);
    emit_ports(&mut s, &c.ports);
    s.push_str(&format!("end clkgate {name}\n"));
    s
}

pub(crate) fn emit_linklist_interface(l: &LinklistDecl) -> String {
    let name = &l.name.name;
    let kind = match l.kind {
        LinklistKind::Singly => "singly",
        LinklistKind::Doubly => "doubly",
        LinklistKind::CircularSingly => "circular_singly",
        LinklistKind::CircularDoubly => "circular_doubly",
    };
    let mut s = format!("linklist {name}\n");
    s.push_str(&format!("  kind {kind};\n"));
    if l.track_tail {
        s.push_str("  track tail: true;\n");
    }
    if l.track_length {
        s.push_str("  track length: true;\n");
    }
    emit_params(&mut s, &l.params);
    emit_ports(&mut s, &l.ports);
    s.push_str(&format!("end linklist {name}\n"));
    s
}

pub(crate) fn emit_struct(s: &StructDecl) -> String {
    let name = &s.name.name;
    let mut out = format!("struct {name}\n");
    for f in &s.fields {
        out.push_str(&format!("  {}: {};\n", f.name.name, type_str(&f.ty)));
    }
    out.push_str(&format!("end struct {name}\n"));
    out
}

pub(crate) fn emit_enum(e: &EnumDecl) -> String {
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

pub(crate) fn emit_package_interface(p: &PackageDecl) -> String {
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

pub(crate) fn emit_params(s: &mut String, params: &[ParamDecl]) {
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
                let unpacked = p.unpacked_size.as_ref()
                    .map(|s| format!(" [{}]", expr_str(s)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}[{}:{}]: const{unpacked}{default_str};\n",
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
                let unpacked = p.unpacked_size.as_ref()
                    .map(|s| format!(" [{}]", expr_str(s)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}: {enum_name}{unpacked}{default_str};\n"
                ));
            }
            ParamKind::ConstVec(ty) => {
                let default_str = p.default.as_ref()
                    .map(|d| format!(" = {}", expr_str(d)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}: {}{default_str};\n",
                    type_str(ty)
                ));
            }
            ParamKind::Logic(ty) => {
                let default_str = p.default.as_ref()
                    .map(|d| format!(" = {}", expr_str(d)))
                    .unwrap_or_default();
                let unpacked = p.unpacked_size.as_ref()
                    .map(|s| format!(" [{}]", expr_str(s)))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "  {local}param {name}: {}{unpacked}{default_str};\n",
                    type_str(ty)
                ));
            }
        }
    }
}

pub(crate) fn emit_ports(s: &mut String, ports: &[PortDecl]) {
    emit_ports_with_deps(s, ports, None)
}

/// Like `emit_ports` but augments comb-driven output ports with an
/// optional `comb_dep_on(...)` annotation computed by
/// `comb_graph::per_output_comb_deps`. Module-shaped constructs pass
/// `Some(&deps)`; constructs without a comb-driven body shape (counter,
/// arbiter, regfile, ...) pass `None`, in which case the annotation is
/// not emitted (so consumers fall back to the opaque interpretation).
pub(crate) fn emit_ports_with_deps(
    s: &mut String,
    ports: &[PortDecl],
    per_output_deps: Option<&HashMap<String, HashSet<String>>>,
) {
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
            // Preserve the `unpacked` modifier (SV unpacked-array port
            // shape) so .archi reflects the same SV emit as .sv. Without
            // this, downstream consumers reading the .archi to decide
            // port shape would silently see packed-Vec when the source
            // is unpacked-Vec, causing port-shape mismatches at the
            // SV inst boundary.
            let unpacked_kw = match (p.unpacked, p.unpacked_ascending) {
                (true, true)  => "unpacked ascending ",
                (true, false) => "unpacked ",
                _             => "",
            };
            // Registered output ports are emitted in the canonical
            // `pipe_reg<T, N>` spelling. This includes legacy `port reg`
            // source declarations and compiler-synthesized thread ports;
            // `.archi` files should expose latency in the port signature.
            if let Some(ref ri) = p.reg_info {
                let reg_ty = format!("pipe_reg<{unpacked_kw}{ty}, {}>", ri.latency);
                match &ri.reset {
                    RegReset::Inherit(rst, val) | RegReset::Explicit(rst, _, _, val) => {
                        let rst_name = &rst.name;
                        let rst_val = expr_str(val);
                        s.push_str(&format!("  port {name}: {dir} {reg_ty} reset {rst_name} => {rst_val};\n"));
                    }
                    RegReset::None => {
                        s.push_str(&format!("  port {name}: {dir} {reg_ty};\n"));
                    }
                }
            } else {
                // Compute comb_dep_on(...) annotation for comb-driven
                // outputs when a per-output dep map is available.
                // Priority: caller-supplied (computed) map > preserved
                // user-written `p.comb_deps`. The preserved field path
                // covers .archi round-trip (parse → re-emit) without
                // recomputing.
                let dep_suffix = if p.direction == Direction::Out {
                    if let Some(map) = per_output_deps {
                        if let Some(set) = map.get(name) {
                            let mut sorted: Vec<&String> = set.iter().collect();
                            sorted.sort();
                            let inner: String = sorted.iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!(" comb_dep_on({inner})")
                        } else {
                            String::new()
                        }
                    } else if let Some(deps) = &p.comb_deps {
                        let mut sorted: Vec<&String> = deps.iter().map(|i| &i.name).collect();
                        sorted.sort();
                        let inner: String = sorted.iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(" comb_dep_on({inner})")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                s.push_str(&format!("  port {name}: {dir} {unpacked_kw}{ty}{dep_suffix};\n"));
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
