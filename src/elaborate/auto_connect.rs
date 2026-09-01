//! `auto;` expansion — the auto-connect directive inside `inst` bodies.
//!
//! `auto;` fills every child port that the explicit connections left
//! unconnected with the identically-named signal from the enclosing scope.
//! It is a **pure front-end desugar**: this pass rewrites each `auto;` into
//! ordinary [`Connection`]s, so resolve, typecheck, and all three backends
//! (SV, sim, formal) see exactly what a hand-written inst body would have
//! produced. Nothing downstream knows `auto;` exists.
//!
//! Where it runs
//! -------------
//! Immediately after [`crate::elaborate::elaborate`] and before the TLM /
//! thread lowerings. That point matters:
//!
//! - inst-body `for` loops are already flattened into `connections`, and
//!   `generate` blocks are expanded, so the explicit connection set is final;
//! - `lower_tlm_connects` (the `connect a.m -> b.s;` sugar) has already
//!   appended its whole-bus connections, so `auto;` correctly treats those
//!   ports as connected;
//! - module variants are monomorphized, so child param defaults are literals
//!   and the type gate below can actually resolve widths.
//!
//! What it fills
//! -------------
//! | child port | synthesized connection |
//! |---|---|
//! | `in T` / `out T` (incl. `Clock`, `Reset`, `Vec`, struct, enum) | `p <- p` / `p -> p` |
//! | `initiator`/`target` bus, incl. `Vec<Bus, N>` | whole-bus `p -> p` (`ConnectDir::Output`, the shape `lower_tlm_connects` uses; per-signal directions come from the bus decl at expansion) |
//! | `ports[N] <g>` arrays (regfile / arbiter / template) | one connection per flattened `<g><i>_<sig>` name |
//!
//! Safety
//! ------
//! Two invariants are preserved, not weakened:
//!
//! 1. **All ports connected.** A port with no identically-named signal in
//!    scope is an error — `auto;` never silently leaves a port dangling.
//! 2. **No implicit conversion.** Before synthesizing, the child port type
//!    and the parent declaration type are compared structurally; a definite
//!    mismatch (differing width, signedness, `Vec` length, reset kind or
//!    polarity, clock domain, named type) is an error naming both sides.
//!    Comparison is deliberately conservative: when either side stays
//!    param-dependent, or the parent name comes from an inst-output wire
//!    with no declaration, the pass defers to the ordinary connection checks
//!    rather than inventing a new type rule here.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::{CompileError, CompileWarning};
use crate::lexer::Span;
use crate::resolve::BusInfo;

use super::try_eval_i64;

/// One expanded `auto;` site, for `--explain-auto`.
pub struct AutoConnectNote {
    pub inst_name: String,
    pub module_name: String,
    /// Rendered connections in synthesis order (`"clk <- clk"`).
    pub conns: Vec<String>,
}

/// A child construct's connectable surface.
struct ChildInfo {
    ports: Vec<PortDecl>,
    port_arrays: Vec<PortArrayDecl>,
    params: Vec<ParamDecl>,
}

/// A bus-shaped declaration in the parent scope (port or wire).
#[derive(Clone)]
struct BusShape {
    bus_name: String,
    /// `Vec<Bus, N>` element count; `None` for a scalar bus.
    count: Option<Expr>,
}

/// One name visible in the enclosing scope.
#[derive(Clone, Default)]
struct ScopeEntry {
    /// Declared type, when there is one. `None` means the name exists but
    /// its type is unknown here (an inst-output wire that codegen
    /// auto-declares), which disables the type gate for that name.
    ty: Option<TypeExpr>,
    bus: Option<BusShape>,
}

/// Expand every `auto;` in the file.
///
/// `best_effort` (used by the `arch graph` path, which runs on an
/// un-elaborated AST) downgrades every diagnostic to "skip this port": the
/// graph shows the connections it can resolve rather than failing the
/// command. The compile pipeline always passes `false`.
pub fn expand_auto_connect(
    mut ast: SourceFile,
    best_effort: bool,
) -> Result<(SourceFile, Vec<AutoConnectNote>, Vec<CompileWarning>), Vec<CompileError>> {
    if !ast_has_auto_connect(&ast) {
        return Ok((ast, Vec::new(), Vec::new()));
    }

    let children: HashMap<String, ChildInfo> = ast
        .items
        .iter()
        .filter(|item| !item.ports().is_empty() || !item.port_arrays().is_empty())
        .map(|item| {
            (
                item.as_construct().name().name.clone(),
                ChildInfo {
                    ports: item.ports().to_vec(),
                    port_arrays: item.port_arrays().into_iter().cloned().collect(),
                    params: item.params().to_vec(),
                },
            )
        })
        .collect();

    let buses: HashMap<String, BusInfo> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Bus(b) => Some((b.name.name.clone(), BusInfo::from_decl(b))),
            _ => None,
        })
        .collect();

    let mut cx = Cx {
        children,
        buses,
        best_effort,
        errors: Vec::new(),
        notes: Vec::new(),
        warnings: Vec::new(),
    };

    for item in ast.items.iter_mut() {
        match item {
            Item::Module(m) => {
                let params = param_defaults(&m.params);
                let scope = cx.collect_scope(&m.ports, &m.body, &params);
                let mut body = std::mem::take(&mut m.body);
                cx.walk_body(&mut body, &scope, &params);
                m.body = body;
            }
            Item::Pipeline(p) => {
                let params = param_defaults(&p.params);
                let ports = p.ports.clone();
                for stage in p.stages.iter_mut() {
                    let scope = cx.collect_scope(&ports, &stage.body, &params);
                    let mut body = std::mem::take(&mut stage.body);
                    cx.walk_body(&mut body, &scope, &params);
                    stage.body = body;
                }
            }
            _ => {}
        }
    }

    if cx.errors.is_empty() {
        Ok((ast, cx.notes, cx.warnings))
    } else {
        Err(cx.errors)
    }
}

fn ast_has_auto_connect(ast: &SourceFile) -> bool {
    fn in_body(body: &[ModuleBodyItem]) -> bool {
        body.iter().any(|it| match it {
            ModuleBodyItem::Inst(i) => i.auto_connect.is_some(),
            ModuleBodyItem::Generate(g) => gen_items(g).iter().any(|gi| match gi {
                GenItem::Inst(i) => i.auto_connect.is_some(),
                _ => false,
            }),
            _ => false,
        })
    }
    ast.items.iter().any(|item| match item {
        Item::Module(m) => in_body(&m.body),
        Item::Pipeline(p) => p.stages.iter().any(|s| in_body(&s.body)),
        _ => false,
    })
}

fn gen_items(g: &GenerateDecl) -> Vec<&GenItem> {
    match g {
        GenerateDecl::For(gf) => gf.items.iter().collect(),
        GenerateDecl::If(gi) => gi.then_items.iter().chain(gi.else_items.iter()).collect(),
    }
}

fn gen_items_mut(g: &mut GenerateDecl) -> Vec<&mut GenItem> {
    match g {
        GenerateDecl::For(gf) => gf.items.iter_mut().collect(),
        GenerateDecl::If(gi) => gi
            .then_items
            .iter_mut()
            .chain(gi.else_items.iter_mut())
            .collect(),
    }
}

/// Const-foldable param defaults of the enclosing construct, used to resolve
/// widths and counts on the *parent* side.
fn param_defaults(params: &[ParamDecl]) -> HashMap<String, i64> {
    let mut out: HashMap<String, i64> = HashMap::new();
    // Two passes so a derived default (`param B: const = A * 2;`) resolves
    // against an earlier param regardless of declaration order.
    for _ in 0..2 {
        for p in params {
            if let Some(d) = &p.default {
                if let Some(v) = try_eval_i64(d, &out) {
                    out.insert(p.name.name.clone(), v);
                }
            }
        }
    }
    out
}

struct Cx {
    children: HashMap<String, ChildInfo>,
    buses: HashMap<String, BusInfo>,
    best_effort: bool,
    errors: Vec<CompileError>,
    notes: Vec<AutoConnectNote>,
    warnings: Vec<CompileWarning>,
}

impl Cx {
    fn error(&mut self, msg: String, span: Span) {
        if !self.best_effort {
            self.errors.push(CompileError::general(&msg, span));
        }
    }

    /// Every name visible to an inst in this body: ports (with bus fields
    /// flattened), `reg` / `wire` / `let` / `pipe_reg` declarations, and the
    /// targets of *explicit* inst output connections (codegen auto-declares
    /// those wires). Names introduced by another inst's `auto;` are
    /// deliberately excluded, so expansion is order-independent.
    fn collect_scope(
        &self,
        ports: &[PortDecl],
        body: &[ModuleBodyItem],
        params: &HashMap<String, i64>,
    ) -> HashMap<String, ScopeEntry> {
        let mut scope: HashMap<String, ScopeEntry> = HashMap::new();

        for p in ports {
            let entry = ScopeEntry {
                ty: Some(p.ty.clone()),
                bus: p.bus_info.as_ref().map(|bi| BusShape {
                    bus_name: bi.bus_name.name.clone(),
                    count: bi.count.clone(),
                }),
            };
            self.insert_with_bus_fields(&mut scope, &p.name.name, entry, params);
        }

        let add_body = |body: &[ModuleBodyItem], scope: &mut HashMap<String, ScopeEntry>| {
            for item in body {
                match item {
                    ModuleBodyItem::RegDecl(r) => {
                        scope.entry(r.name.name.clone()).or_insert(ScopeEntry {
                            ty: Some(r.ty.clone()),
                            bus: None,
                        });
                    }
                    ModuleBodyItem::WireDecl(w) => {
                        let bus = bus_shape_of_type(&w.ty, &self.buses);
                        let entry = ScopeEntry {
                            ty: Some(w.ty.clone()),
                            bus,
                        };
                        self.insert_with_bus_fields(scope, &w.name.name, entry, params);
                    }
                    ModuleBodyItem::LetBinding(l) => {
                        scope.entry(l.name.name.clone()).or_insert(ScopeEntry {
                            ty: l.ty.clone(),
                            bus: None,
                        });
                    }
                    ModuleBodyItem::PipeRegDecl(p) => {
                        scope.entry(p.name.name.clone()).or_default();
                    }
                    ModuleBodyItem::Inst(inst) => {
                        for c in &inst.connections {
                            if c.direction == ConnectDir::Output {
                                if let ExprKind::Ident(n) = &c.signal.kind {
                                    scope.entry(n.clone()).or_default();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        };
        add_body(body, &mut scope);
        // Items inside a surviving `generate` block are visible at module
        // scope in the emitted SV, so an inst-output wire declared there
        // counts too.
        for item in body {
            if let ModuleBodyItem::Generate(g) = item {
                for gi in gen_items(g) {
                    match gi {
                        GenItem::Wire(w) => {
                            let bus = bus_shape_of_type(&w.ty, &self.buses);
                            let entry = ScopeEntry {
                                ty: Some(w.ty.clone()),
                                bus,
                            };
                            self.insert_with_bus_fields(&mut scope, &w.name.name, entry, params);
                        }
                        GenItem::Inst(inst) => {
                            for c in &inst.connections {
                                if c.direction == ConnectDir::Output {
                                    if let ExprKind::Ident(n) = &c.signal.kind {
                                        scope.entry(n.clone()).or_default();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        scope
    }

    /// Insert a name, plus — for a bus-shaped declaration — the flattened
    /// `<name>_<sig>` / `<name>_<i>_<sig>` field names that a per-field
    /// connection would target.
    fn insert_with_bus_fields(
        &self,
        scope: &mut HashMap<String, ScopeEntry>,
        name: &str,
        entry: ScopeEntry,
        params: &HashMap<String, i64>,
    ) {
        let bus = entry.bus.clone();
        scope.insert(name.to_string(), entry);
        let Some(shape) = bus else { return };
        let Some(info) = self.buses.get(&shape.bus_name) else {
            return;
        };
        let pm = info.default_param_map();
        let prefixes: Vec<String> = match &shape.count {
            None => vec![name.to_string()],
            Some(count) => match try_eval_i64(count, params) {
                Some(n) if n > 0 => (0..n).map(|i| format!("{name}_{i}")).collect(),
                _ => vec![name.to_string()],
            },
        };
        for prefix in prefixes {
            for (sig, _, ty) in info.effective_signals(&pm) {
                scope
                    .entry(format!("{prefix}_{sig}"))
                    .or_insert(ScopeEntry {
                        ty: Some(ty.clone()),
                        bus: None,
                    });
            }
        }
    }

    fn walk_body(
        &mut self,
        body: &mut [ModuleBodyItem],
        scope: &HashMap<String, ScopeEntry>,
        params: &HashMap<String, i64>,
    ) {
        for item in body.iter_mut() {
            match item {
                ModuleBodyItem::Inst(inst) => self.expand_inst(inst, scope, params),
                ModuleBodyItem::Generate(g) => {
                    for gi in gen_items_mut(g) {
                        if let GenItem::Inst(inst) = gi {
                            self.expand_inst(inst, scope, params);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Is this bus port already bound by an explicit connection?
    ///
    /// A bus port binds either whole (`p -> w;`) or field-by-field
    /// (`p.cmd_valid <- x;`, which the parser flattens to `p_cmd_valid`).
    /// Deciding that from a bare `p_` **prefix** would be wrong: a sibling
    /// port literally named `p_extra` also starts with `p_`, so connecting
    /// *it* would suppress the auto-fill of bus port `p` and leave `p`
    /// dangling — a silent wrong answer, which is exactly what `auto;` must
    /// never produce. So match against the precise set of names this port
    /// can flatten to, derived from the bus declaration.
    fn bus_port_connected(
        &self,
        connected: &HashSet<String>,
        port_name: &str,
        bi: &BusPortInfo,
        child_params: &HashMap<String, i64>,
    ) -> bool {
        // Whole-bus binding.
        if connected.contains(port_name) {
            return true;
        }
        let Some(info) = self.buses.get(&bi.bus_name.name) else {
            // The bus declaration isn't in this compilation unit, so the
            // field names are unknowable here. The design won't elaborate
            // anyway; fall back to the prefix test rather than assume the
            // port is unconnected and double-drive it.
            let prefix = format!("{port_name}_");
            return connected.iter().any(|c| c.starts_with(&prefix));
        };
        let mut pm = info.default_param_map();
        for pa in &bi.params {
            pm.insert(pa.name.name.clone(), &pa.value);
        }
        let signals = info.effective_signals(&pm);
        // `Vec<Bus, N>` flattens per element (`p_0_<sig>`); a scalar bus
        // flattens directly (`p_<sig>`). An unresolvable count falls back to
        // the scalar spelling.
        let prefixes: Vec<String> = match bi.count.as_ref() {
            None => vec![port_name.to_string()],
            Some(count) => match try_eval_i64(count, child_params) {
                Some(n) if n > 0 => (0..n).map(|i| format!("{port_name}_{i}")).collect(),
                _ => vec![port_name.to_string()],
            },
        };
        prefixes.iter().any(|p| {
            // `p` alone covers a per-element whole-bus binding (`mm[0] <- w`
            // flattens to `mm_0`).
            connected.contains(p)
                || signals
                    .iter()
                    .any(|(sig, _, _)| connected.contains(&format!("{p}_{sig}")))
        })
    }

    fn expand_inst(
        &mut self,
        inst: &mut InstDecl,
        scope: &HashMap<String, ScopeEntry>,
        parent_params: &HashMap<String, i64>,
    ) {
        let Some(auto_span) = inst.auto_connect else {
            return;
        };
        let Some(child) = self.children.get(&inst.module_name.name) else {
            self.error(
                format!(
                    "auto-connect: cannot resolve ports of `{}` in inst `{}` \
                     (no definition and no `{}.archi` interface found)",
                    inst.module_name.name, inst.name.name, inst.module_name.name
                ),
                auto_span,
            );
            return;
        };
        // Cloned so the borrow on `self.children` ends here — the fill loop
        // needs `&mut self` for diagnostics.
        let ports = child.ports.clone();
        let port_arrays = child.port_arrays.clone();
        let child_params = self.child_param_map(child, inst, parent_params);

        let connected: HashSet<String> = inst
            .connections
            .iter()
            .map(|c| c.port_name.name.clone())
            .collect();

        let mut fills: Vec<Connection> = Vec::new();

        for port in &ports {
            if let Some(bi) = &port.bus_info {
                if self.bus_port_connected(&connected, &port.name.name, bi, &child_params) {
                    continue;
                }
                self.fill_bus_port(
                    port,
                    bi,
                    inst,
                    scope,
                    &child_params,
                    parent_params,
                    auto_span,
                    &mut fills,
                );
                continue;
            }
            if connected.contains(&port.name.name) {
                continue;
            }
            self.fill_plain_port(
                port,
                &port.name.name,
                port.direction,
                inst,
                scope,
                &child_params,
                parent_params,
                auto_span,
                &mut fills,
            );
        }

        for group in &port_arrays {
            let Some(n) = try_eval_i64(&group.count_expr, &child_params) else {
                self.error(
                    format!(
                        "auto-connect: cannot resolve count for port group `{}` of `{}` \
                         in inst `{}` — connect its signals explicitly",
                        group.name.name, inst.module_name.name, inst.name.name
                    ),
                    auto_span,
                );
                continue;
            };
            // A literal count-1 group flattens to `<g>_<sig>` (see
            // `normalize_count1_portarray_conns`); every other count uses
            // `<g><i>_<sig>`.
            let count1 = matches!(&group.count_expr.kind, ExprKind::Literal(LitKind::Dec(1)));
            for i in 0..n.max(0) {
                for sig in &group.signals {
                    let flat = if count1 {
                        format!("{}_{}", group.name.name, sig.name.name)
                    } else {
                        format!("{}{}_{}", group.name.name, i, sig.name.name)
                    };
                    // Accept either spelling as "already connected": the
                    // normalization only runs over module bodies, so a
                    // pipeline-stage inst may still carry `<g>0_<sig>`.
                    let alt = format!("{}{}_{}", group.name.name, i, sig.name.name);
                    if connected.contains(&flat) || connected.contains(&alt) {
                        continue;
                    }
                    self.fill_plain_port(
                        sig,
                        &flat,
                        sig.direction,
                        inst,
                        scope,
                        &child_params,
                        parent_params,
                        auto_span,
                        &mut fills,
                    );
                }
            }
        }

        if fills.is_empty() {
            self.warnings.push(CompileWarning {
                message: format!(
                    "`auto;` in inst `{}` filled no ports — every port of `{}` is already \
                     connected explicitly",
                    inst.name.name, inst.module_name.name
                ),
                span: auto_span,
            });
        } else {
            self.notes.push(AutoConnectNote {
                inst_name: inst.name.name.clone(),
                module_name: inst.module_name.name.clone(),
                conns: fills.iter().map(render_conn).collect(),
            });
        }
        inst.connections.extend(fills);
        inst.auto_connect = None;
    }

    /// The child's effective param values: its own defaults with the
    /// inst-site `param NAME = ...;` overrides applied on top (evaluated in
    /// the parent's param scope, where they are written).
    fn child_param_map(
        &self,
        child: &ChildInfo,
        inst: &InstDecl,
        parent_params: &HashMap<String, i64>,
    ) -> HashMap<String, i64> {
        let mut map = param_defaults(&child.params);
        for pa in &inst.param_assigns {
            if let Some(v) = try_eval_i64(&pa.value, parent_params) {
                map.insert(pa.name.name.clone(), v);
            }
        }
        map
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_plain_port(
        &mut self,
        port: &PortDecl,
        flat_name: &str,
        direction: Direction,
        inst: &InstDecl,
        scope: &HashMap<String, ScopeEntry>,
        child_params: &HashMap<String, i64>,
        parent_params: &HashMap<String, i64>,
        auto_span: Span,
        fills: &mut Vec<Connection>,
    ) {
        let Some(entry) = scope.get(flat_name) else {
            self.error(
                format!(
                    "auto-connect: no signal `{}` in scope for {} port `{}` of `{}` in inst \
                     `{}` — declare `{}` or connect the port explicitly",
                    flat_name,
                    dir_word(direction),
                    flat_name,
                    inst.module_name.name,
                    inst.name.name,
                    flat_name,
                ),
                auto_span,
            );
            return;
        };
        if let Some(parent_ty) = &entry.ty {
            if let Some((pt, st)) = type_conflict(&port.ty, child_params, parent_ty, parent_params)
            {
                let hint = if matches!(port.ty, TypeExpr::Reset(_, _)) {
                    format!(
                        " — connect it explicitly as `{flat_name} <- {flat_name} as {pt}` if the \
                         reset really is re-typed here"
                    )
                } else {
                    " — connect it explicitly with the cast you intend".to_string()
                };
                self.error(
                    format!(
                        "auto-connect: type mismatch for port `{}` of `{}` in inst `{}`: port is \
                         {}, signal `{}` is {}{}",
                        flat_name, inst.module_name.name, inst.name.name, pt, flat_name, st, hint,
                    ),
                    auto_span,
                );
                return;
            }
        }
        fills.push(mk_conn(
            flat_name,
            match direction {
                Direction::In => ConnectDir::Input,
                Direction::Out => ConnectDir::Output,
            },
            auto_span,
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_bus_port(
        &mut self,
        port: &PortDecl,
        bi: &BusPortInfo,
        inst: &InstDecl,
        scope: &HashMap<String, ScopeEntry>,
        child_params: &HashMap<String, i64>,
        parent_params: &HashMap<String, i64>,
        auto_span: Span,
        fills: &mut Vec<Connection>,
    ) {
        let name = &port.name.name;
        let Some(entry) = scope.get(name) else {
            self.error(
                format!(
                    "auto-connect: no signal `{}` in scope for bus port `{}` of `{}` in inst \
                     `{}` — declare a `{}` port or wire, or connect the bus explicitly",
                    name, name, inst.module_name.name, inst.name.name, bi.bus_name.name,
                ),
                auto_span,
            );
            return;
        };
        let Some(shape) = &entry.bus else {
            self.error(
                format!(
                    "auto-connect: `{}` is not a `{}` bus in this scope but port `{}` of `{}` \
                     in inst `{}` is — connect the bus explicitly",
                    name, bi.bus_name.name, name, inst.module_name.name, inst.name.name,
                ),
                auto_span,
            );
            return;
        };
        if shape.bus_name != bi.bus_name.name {
            self.error(
                format!(
                    "auto-connect: bus type mismatch for port `{}` of `{}` in inst `{}`: port is \
                     `{}`, signal `{}` is `{}`",
                    name,
                    inst.module_name.name,
                    inst.name.name,
                    bi.bus_name.name,
                    name,
                    shape.bus_name,
                ),
                auto_span,
            );
            return;
        }
        // `Vec<Bus, N>` on one side only, or a different N, is a definite
        // mismatch; an unresolvable count on either side defers.
        let child_n = bi.count.as_ref().map(|c| try_eval_i64(c, child_params));
        let parent_n = shape.count.as_ref().map(|c| try_eval_i64(c, parent_params));
        let mismatched = match (&child_n, &parent_n) {
            (None, Some(_)) | (Some(_), None) => true,
            (Some(Some(a)), Some(Some(b))) => a != b,
            _ => false,
        };
        if mismatched {
            let render = |n: &Option<Option<i64>>| match n {
                None => "a scalar bus".to_string(),
                Some(Some(v)) => format!("`Vec<{}, {}>`", bi.bus_name.name, v),
                Some(None) => format!("`Vec<{}, ...>`", bi.bus_name.name),
            };
            self.error(
                format!(
                    "auto-connect: bus shape mismatch for port `{}` of `{}` in inst `{}`: port is \
                     {}, signal `{}` is {}",
                    name,
                    inst.module_name.name,
                    inst.name.name,
                    render(&child_n),
                    name,
                    render(&parent_n),
                ),
                auto_span,
            );
            return;
        }
        // Whole-bus connections are always recorded as `Output` — the
        // per-signal directions come from the bus declaration when the
        // connection is expanded. Same convention as the `connect` sugar in
        // `elaborate::tlm::push_tlm_connect_connection`.
        fills.push(mk_conn(name, ConnectDir::Output, auto_span));
    }
}

fn dir_word(d: Direction) -> &'static str {
    match d {
        Direction::In => "input",
        Direction::Out => "output",
    }
}

fn mk_conn(name: &str, direction: ConnectDir, span: Span) -> Connection {
    Connection {
        port_name: Ident::new(name.to_string(), span),
        direction,
        signal: Expr::new(ExprKind::Ident(name.to_string()), span),
        reset_override: None,
        span,
    }
}

fn render_conn(c: &Connection) -> String {
    let arrow = match c.direction {
        ConnectDir::Input => "<-",
        ConnectDir::Output => "->",
    };
    format!("{} {} {}", c.port_name.name, arrow, c.port_name.name)
}

/// Recognize a bus-typed `wire`: `wire w: BusName;` or `wire w: Vec<BusName, N>;`.
fn bus_shape_of_type(ty: &TypeExpr, buses: &HashMap<String, BusInfo>) -> Option<BusShape> {
    match ty {
        TypeExpr::Named(id) if buses.contains_key(&id.name) => Some(BusShape {
            bus_name: id.name.clone(),
            count: None,
        }),
        TypeExpr::Vec(inner, count) => match inner.as_ref() {
            TypeExpr::Named(id) if buses.contains_key(&id.name) => Some(BusShape {
                bus_name: id.name.clone(),
                count: Some((**count).clone()),
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Compare a child port type against a parent declaration type.
///
/// Returns `Some((port_ty, signal_ty))` — both rendered for the diagnostic —
/// only on a **definite** mismatch. Anything that cannot be decided here
/// (param-dependent width, cross-category comparison such as `Named` vs
/// `UInt`, unknown parent type) returns `None` and defers to the ordinary
/// connection checks, so this gate never invents a type rule of its own.
fn type_conflict(
    port: &TypeExpr,
    port_params: &HashMap<String, i64>,
    sig: &TypeExpr,
    sig_params: &HashMap<String, i64>,
) -> Option<(String, String)> {
    if !definitely_differs(port, port_params, sig, sig_params) {
        return None;
    }
    Some((render_ty(port, port_params), render_ty(sig, sig_params)))
}

fn definitely_differs(
    a: &TypeExpr,
    ap: &HashMap<String, i64>,
    b: &TypeExpr,
    bp: &HashMap<String, i64>,
) -> bool {
    use TypeExpr::*;
    match (a, b) {
        (Reset(k1, l1), Reset(k2, l2)) => k1 != k2 || l1 != l2,
        (Clock(d1), Clock(d2)) => d1.name != d2.name,
        (UInt(w1), UInt(w2)) | (SInt(w1), SInt(w2)) => {
            match (try_eval_i64(w1, ap), try_eval_i64(w2, bp)) {
                (Some(x), Some(y)) => x != y,
                _ => false,
            }
        }
        // Signedness is never implicit in ARCH.
        (UInt(_), SInt(_)) | (SInt(_), UInt(_)) => true,
        // A 1-bit `UInt` against `Bool`/`Bit` is a spelling difference, not a
        // mismatch — any other width is. Each width is evaluated against its
        // OWN side's param map.
        (UInt(w), Bool) | (UInt(w), Bit) => {
            matches!(try_eval_i64(w, ap), Some(n) if n != 1)
        }
        (Bool, UInt(w)) | (Bit, UInt(w)) => {
            matches!(try_eval_i64(w, bp), Some(n) if n != 1)
        }
        (Vec(t1, n1), Vec(t2, n2)) => {
            let len_differs = match (try_eval_i64(n1, ap), try_eval_i64(n2, bp)) {
                (Some(x), Some(y)) => x != y,
                _ => false,
            };
            len_differs || definitely_differs(t1, ap, t2, bp)
        }
        (Named(x), Named(y)) => x.name != y.name,
        // Float / MX formats are distinct types with no implicit conversion.
        (FP32, FP32) | (BF16, BF16) => false,
        (FP32, BF16) | (BF16, FP32) => true,
        _ => false,
    }
}

fn render_ty(ty: &TypeExpr, params: &HashMap<String, i64>) -> String {
    use TypeExpr::*;
    match ty {
        UInt(w) => match try_eval_i64(w, params) {
            Some(n) => format!("UInt<{n}>"),
            None => "UInt<...>".to_string(),
        },
        SInt(w) => match try_eval_i64(w, params) {
            Some(n) => format!("SInt<{n}>"),
            None => "SInt<...>".to_string(),
        },
        Bool => "Bool".to_string(),
        Bit => "Bit".to_string(),
        Clock(d) => format!("Clock<{}>", d.name),
        Reset(k, l) => format!(
            "Reset<{}, {}>",
            match k {
                ResetKind::Sync => "Sync",
                ResetKind::Async => "Async",
            },
            match l {
                ResetLevel::High => "High",
                ResetLevel::Low => "Low",
            }
        ),
        Vec(t, n) => match try_eval_i64(n, params) {
            Some(c) => format!("Vec<{}, {}>", render_ty(t, params), c),
            None => format!("Vec<{}, ...>", render_ty(t, params)),
        },
        Named(id) => id.name.clone(),
        other => format!("{other:?}"),
    }
}
