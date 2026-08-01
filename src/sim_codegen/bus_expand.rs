//! `bus_expand` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// Substitute param idents in a TypeExpr (for bus param resolution in sim codegen).
pub(super) fn subst_type_expr_sim(ty: &TypeExpr, params: &HashMap<String, &Expr>) -> TypeExpr {
    match ty {
        TypeExpr::UInt(w) => TypeExpr::UInt(Box::new(subst_expr_sim(w, params))),
        TypeExpr::SInt(w) => TypeExpr::SInt(Box::new(subst_expr_sim(w, params))),
        TypeExpr::Vec(inner, len) => TypeExpr::Vec(
            Box::new(subst_type_expr_sim(inner, params)),
            Box::new(subst_expr_sim(len, params)),
        ),
        other => other.clone(),
    }
}

pub(super) fn subst_expr_sim(expr: &Expr, params: &HashMap<String, &Expr>) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(replacement) = params.get(name.as_str()) {
                return (*replacement).clone();
            } else {
                ExprKind::Ident(name.clone())
            }
        }
        ExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(subst_expr_sim(l, params)),
            Box::new(subst_expr_sim(r, params)),
        ),
        ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(subst_expr_sim(e, params))),
        ExprKind::Ternary(c, t, e) => ExprKind::Ternary(
            Box::new(subst_expr_sim(c, params)),
            Box::new(subst_expr_sim(t, params)),
            Box::new(subst_expr_sim(e, params)),
        ),
        ExprKind::Clog2(e) => ExprKind::Clog2(Box::new(subst_expr_sim(e, params))),
        ExprKind::Index(b, i) => ExprKind::Index(
            Box::new(subst_expr_sim(b, params)),
            Box::new(subst_expr_sim(i, params)),
        ),
        _ => return expr.clone(),
    };
    Expr {
        kind,
        span: expr.span,
        parenthesized: expr.parenthesized,
    }
}

/// Return flattened bus port signals with direction: Vec<(flat_name, Direction, TypeExpr)>.
/// Direction is from the module's perspective (target flips initiator directions).
pub(super) fn flatten_bus_port_with_dir(
    port_name: &str,
    bi: &BusPortInfo,
    symbols: &crate::resolve::SymbolTable,
    module_params: &[ParamDecl],
) -> Vec<(String, Direction, TypeExpr)> {
    let bus_name = &bi.bus_name.name;
    if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(bus_name) {
        let mut param_map: HashMap<String, &Expr> = info
            .params
            .iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect();
        for pa in &bi.params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let eff = info.effective_signals(&param_map);
        let is_target = bi.perspective == BusPerspective::Target;
        // For Vec<Bus, N> ports, emit N copies of each signal with indexed prefix.
        // N is resolved against the enclosing module's params for the
        // param-driven `Vec<Bus, NUM_FOO>` case.
        let prefixes: Vec<String> = match bi.count.as_ref() {
            None => vec![port_name.to_string()],
            Some(count_expr) => {
                let n = eval_const_expr_with_params(count_expr, module_params) as u32;
                (0..n).map(|i| format!("{}_{}", port_name, i)).collect()
            }
        };
        let mut out = Vec::new();
        for prefix in &prefixes {
            for (sname, sdir, sty) in &eff {
                let subst_ty = subst_type_expr_sim(sty, &param_map);
                // Target perspective flips all signal directions
                let dir = if is_target {
                    match sdir {
                        Direction::In => Direction::Out,
                        Direction::Out => Direction::In,
                    }
                } else {
                    *sdir
                };
                out.push((format!("{}_{}", prefix, sname), dir, subst_ty));
            }
        }
        out
    } else {
        Vec::new()
    }
}

/// Return flattened bus port signals: Vec<(flat_name, TypeExpr)>.
/// E.g. port itcm: initiator ItcmIcb → [(itcm_cmd_valid, Bool), (itcm_cmd_addr, UInt<14>), ...]
/// Direction-discarding wrapper around `flatten_bus_port_with_dir` for callers
/// that don't need direction info (e.g. header field generation).
pub(super) fn flatten_bus_port(
    port_name: &str,
    bi: &BusPortInfo,
    symbols: &crate::resolve::SymbolTable,
    module_params: &[ParamDecl],
) -> Vec<(String, TypeExpr)> {
    flatten_bus_port_with_dir(port_name, bi, symbols, module_params)
        .into_iter()
        .map(|(n, _d, t)| (n, t))
        .collect()
}

/// Expand whole-bus connections in an inst block into per-signal connections.
/// E.g. `axi_rd -> m_axi_mm2s` where axi_rd is a bus port on the target
/// construct expands to `axi_rd_ar_valid -> m_axi_mm2s_ar_valid`, etc.
/// Non-bus connections are returned unchanged.
pub(super) fn expand_bus_connections(
    inst: &InstDecl,
    parent_module: &ModuleDecl,
    source: &SourceFile,
    symbols: &crate::resolve::SymbolTable,
    bus_wire_names: &HashSet<String>,
) -> Vec<Connection> {
    let m = parent_module;
    // Find the target construct's ports + params. Vec-of-bus counts are
    // resolved against the child module's params (with this inst's
    // `param NAME = ...` overrides applied) so that
    // `port chans: initiator Vec<B, N>;` with a param-driven N folds to a
    // concrete element count at the call site.
    let (target_ports, target_params): (Option<&[PortDecl]>, Vec<ParamDecl>) = source
        .items
        .iter()
        .find_map(|item| match item {
            Item::Module(m) if m.name.name == inst.module_name.name => {
                Some((Some(m.ports.as_slice()), m.params.clone()))
            }
            Item::Fsm(f) if f.name.name == inst.module_name.name => {
                Some((Some(f.ports.as_slice()), f.common.params.clone()))
            }
            _ => None,
        })
        .unwrap_or((None, Vec::new()));
    let mut child_params_overridden = target_params.clone();
    for pa in &inst.param_assigns {
        if let Some(p) = child_params_overridden
            .iter_mut()
            .find(|p| p.name.name == pa.name.name)
        {
            p.default = Some(pa.value.clone());
        }
    }
    let target_bus_ports: Vec<(String, &str, BusPerspective, &[ParamAssign])> = target_ports
        .map(|ports| {
            let mut v = Vec::new();
            for p in ports {
                if let Some(bi) = p.bus_info.as_ref() {
                    let bus = bi.bus_name.name.as_str();
                    match bi.count.as_ref() {
                        None => {
                            v.push((
                                p.name.name.clone(),
                                bus,
                                bi.perspective,
                                bi.params.as_slice(),
                            ));
                        }
                        Some(count_expr) => {
                            let n =
                                eval_const_expr_with_params(count_expr, &child_params_overridden)
                                    as u32;
                            for i in 0..n {
                                v.push((
                                    format!("{}_{}", p.name.name, i),
                                    bus,
                                    bi.perspective,
                                    bi.params.as_slice(),
                                ));
                            }
                        }
                    }
                }
            }
            v
        })
        .unwrap_or_default();
    // Whole Vec-of-bus port lookup, keyed by the bare name (no `_<i>` suffix).
    // Lets `chans -> w` (whole-vec inst connection) match against the child's
    // bare Vec<Bus,N> port name; we then expand it to N per-element bus
    // connections fed back into the standard expansion loop below.
    let target_vec_of_bus_ports: Vec<(String, u32)> = target_ports
        .map(|ports| {
            let mut v = Vec::new();
            for p in ports {
                if let Some(bi) = p.bus_info.as_ref() {
                    if let Some(count_expr) = bi.count.as_ref() {
                        let n = eval_const_expr_with_params(count_expr, &child_params_overridden)
                            as u32;
                        if n > 0 {
                            v.push((p.name.name.clone(), n));
                        }
                    }
                }
            }
            v
        })
        .unwrap_or_default();
    // Parent-side Vec<Bus,N> port and wire names → counts. Used together
    // with `target_vec_of_bus_ports` to detect a whole-vec connection
    // `chans -> w` where both sides are arrays.
    let parent_vec_of_bus_wires: HashMap<String, u32> = m
        .body
        .iter()
        .filter_map(|i| {
            if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Vec(elem, size_expr) = &w.ty {
                    if let TypeExpr::Named(id) = elem.as_ref() {
                        if matches!(
                            symbols.globals.get(&id.name),
                            Some((crate::resolve::Symbol::Bus(_), _))
                        ) {
                            let n = eval_const_expr_with_params(size_expr, &m.params) as u32;
                            if n > 0 {
                                return Some((w.name.name.clone(), n));
                            }
                        }
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();
    // 2D bus wires: `wire edges: Vec<Vec<B, N>, M>;` → (M, N).
    // Used by the whole-row inst connection expansion `outs -> edges[i]`,
    // where `edges[i]` is a row (Vec<B,N>) inside a 2D wire.
    let parent_vec_of_bus_wires_2d: HashMap<String, (u32, u32)> = m
        .body
        .iter()
        .filter_map(|i| {
            if let ModuleBodyItem::WireDecl(w) = i {
                if let TypeExpr::Vec(outer_elem, outer_size) = &w.ty {
                    if let TypeExpr::Vec(inner_elem, inner_size) = outer_elem.as_ref() {
                        if let TypeExpr::Named(id) = inner_elem.as_ref() {
                            if matches!(
                                symbols.globals.get(&id.name),
                                Some((crate::resolve::Symbol::Bus(_), _))
                            ) {
                                let m_n = eval_const_expr_with_params(outer_size, &m.params) as u32;
                                let n_n = eval_const_expr_with_params(inner_size, &m.params) as u32;
                                if m_n > 0 && n_n > 0 {
                                    return Some((w.name.name.clone(), (m_n, n_n)));
                                }
                            }
                        }
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();
    let parent_vec_of_bus_ports: HashMap<String, u32> = m
        .ports
        .iter()
        .filter_map(|p| {
            let bi = p.bus_info.as_ref()?;
            let count_expr = bi.count.as_ref()?;
            let n = eval_const_expr_with_params(count_expr, &m.params) as u32;
            if n > 0 {
                Some((p.name.name.clone(), n))
            } else {
                None
            }
        })
        .collect();
    // Pre-expand whole-vec inst connections (`chans -> w`) into N per-element
    // bus connections (`chans_0 -> w[0]; chans_1 -> w[1]; ...`). The body
    // loop below then expands each of those into per-signal connections via
    // the existing scalar+indexed paths.
    let inst_connections: Vec<crate::ast::Connection> = inst
        .connections
        .iter()
        .flat_map(|c| {
            if let Some((_, n)) = target_vec_of_bus_ports
                .iter()
                .find(|(pn, _)| pn == &c.port_name.name)
            {
                // Whole-row connection into a 2D bus wire: `outs -> edges[m]`,
                // where outs is Vec<B,N>, edges is Vec<Vec<B,N>,M>, m is a
                // literal (or static-unrolled loop var). Expand to N per-element
                // connections `outs[j] -> edges[m][j]`.
                if let ExprKind::Index(arr, idx) = &c.signal.kind {
                    if let ExprKind::Ident(parent_name) = &arr.kind {
                        if let Some((_m_n, n_n)) =
                            parent_vec_of_bus_wires_2d.get(parent_name).copied()
                        {
                            if let ExprKind::Literal(LitKind::Dec(m_idx)) = &idx.kind {
                                if (n_n as u32) == *n {
                                    return (0..*n)
                                        .map(|j| {
                                            let port_j = Ident::new(
                                                format!("{}_{}", c.port_name.name, j),
                                                c.port_name.span,
                                            );
                                            let parent_expr = Expr::new(
                                                ExprKind::Index(
                                                    Box::new(Expr::new(
                                                        ExprKind::Index(
                                                            Box::new(Expr::new(
                                                                ExprKind::Ident(
                                                                    parent_name.clone(),
                                                                ),
                                                                c.signal.span,
                                                            )),
                                                            Box::new(Expr::new(
                                                                ExprKind::Literal(LitKind::Dec(
                                                                    *m_idx,
                                                                )),
                                                                c.signal.span,
                                                            )),
                                                        ),
                                                        c.signal.span,
                                                    )),
                                                    Box::new(Expr::new(
                                                        ExprKind::Literal(LitKind::Dec(j as u64)),
                                                        c.signal.span,
                                                    )),
                                                ),
                                                c.signal.span,
                                            );
                                            crate::ast::Connection {
                                                port_name: port_j,
                                                direction: c.direction,
                                                signal: parent_expr,
                                                reset_override: None,
                                                span: c.span,
                                            }
                                        })
                                        .collect::<Vec<_>>();
                                }
                            }
                        }
                    }
                }
                if let ExprKind::Ident(parent_name) = &c.signal.kind {
                    let parent_is_vob_wire = parent_vec_of_bus_wires.contains_key(parent_name);
                    let parent_is_vob_port = parent_vec_of_bus_ports.contains_key(parent_name);
                    if parent_is_vob_wire || parent_is_vob_port {
                        return (0..*n)
                            .map(|i| {
                                let port_i = Ident::new(
                                    format!("{}_{}", c.port_name.name, i),
                                    c.port_name.span,
                                );
                                // Wire: emit Index(Ident(w), i) so downstream sees a
                                // bus-wire-array element. Port: emit Ident("w_<i>") so
                                // it lands at the flat per-element port name on the parent.
                                let parent_expr = if parent_is_vob_wire {
                                    Expr::new(
                                        ExprKind::Index(
                                            Box::new(Expr::new(
                                                ExprKind::Ident(parent_name.clone()),
                                                c.signal.span,
                                            )),
                                            Box::new(Expr::new(
                                                ExprKind::Literal(LitKind::Dec(i as u64)),
                                                c.signal.span,
                                            )),
                                        ),
                                        c.signal.span,
                                    )
                                } else {
                                    Expr::new(
                                        ExprKind::Ident(format!("{}_{}", parent_name, i)),
                                        c.signal.span,
                                    )
                                };
                                crate::ast::Connection {
                                    port_name: port_i,
                                    direction: c.direction,
                                    signal: parent_expr,
                                    reset_override: None,
                                    span: c.span,
                                }
                            })
                            .collect::<Vec<_>>();
                    }
                }
            }
            vec![c.clone()]
        })
        .collect();

    let mut expanded = Vec::new();
    for c in &inst_connections {
        if let Some((_, bus_name, perspective, bus_params)) = target_bus_ports
            .iter()
            .find(|(pn, _, _, _)| pn == &c.port_name.name)
        {
            // Bus connection — expand to individual signal connections
            if let Some((crate::resolve::Symbol::Bus(info), _)) = symbols.globals.get(*bus_name) {
                // Three shapes for the parent-side signal on a whole-bus binding:
                //   * `p -> ident`         where `ident` is a bus port or scalar bus wire
                //   * `p -> base.field`    where `base.field` is a bus port on the parent
                //   * `p -> wire[i]`       where `wire` is a Vec<Bus,N> wire — element i
                //
                // BindKind tells the per-signal emitter how to construct the parent-side
                // expression for a given signal name:
                //   FlatPort(prefix)   → emit `Ident("<prefix>_<sname>")`
                //                        (matches the flattened bus-port shape)
                //   WireStruct(name)   → emit `FieldAccess(Ident("<name>"), sname)`
                //                        (matches the C++ struct-typed bus wire)
                //   WireIndex(name, i) → emit `FieldAccess(Index(Ident("<name>"), i), sname)`
                //                        (matches a `B _let_<name>[N]` struct-array element)
                enum BindKind {
                    FlatPort(String),
                    WireStruct(String),
                    WireIndex(String, u32),
                    /// 2D bus wire element: `wire edges: Vec<Vec<B,N>,M>;` →
                    /// `edges[m][n]` lowers to `_let_edges[m][n].<sig>`.
                    Wire2DIndex(String, u32, u32),
                }
                let bind = match &c.signal.kind {
                    ExprKind::Ident(name) => {
                        if bus_wire_names.contains(name.as_str()) {
                            BindKind::WireStruct(name.clone())
                        } else {
                            BindKind::FlatPort(name.clone())
                        }
                    }
                    ExprKind::FieldAccess(base, field) => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            BindKind::FlatPort(format!("{}_{}", base_name, field.name))
                        } else {
                            continue;
                        }
                    }
                    ExprKind::Index(arr, idx) => {
                        // 2D bus wire element: `edges[m][n]` → arr is itself
                        // an Index(Ident, literal_m), idx is literal_n.
                        if let ExprKind::Index(inner_arr, inner_idx) = &arr.kind {
                            if let (
                                ExprKind::Ident(arr_name),
                                ExprKind::Literal(LitKind::Dec(m)),
                                ExprKind::Literal(LitKind::Dec(n)),
                            ) = (&inner_arr.kind, &inner_idx.kind, &idx.kind)
                            {
                                BindKind::Wire2DIndex(arr_name.clone(), *m as u32, *n as u32)
                            } else {
                                continue;
                            }
                        } else if let (
                            ExprKind::Ident(arr_name),
                            ExprKind::Literal(LitKind::Dec(i)),
                        ) = (&arr.kind, &idx.kind)
                        {
                            if bus_wire_names.contains(arr_name.as_str())
                                || parent_vec_of_bus_wires.contains_key(arr_name.as_str())
                            {
                                // 1-D Vec-of-bus WIRE element (`wire s_int:
                                // Vec<B, N>;` → `s_int[i]`) OR a scalar bus
                                // wire indexed as a struct-array element. Both
                                // are stored as `B _let_<name>[N]`, so the
                                // parent-side ref is `_let_<name>[i].<sig>`.
                                // Without this branch a Vec-of-bus wire element
                                // fell through to `continue`, silently DROPPING
                                // the connection (inst port left undriven in the
                                // ARCH sim while the SV backend wired it) — the
                                // per-slave reg-slice `up <- s_int[j]` bug.
                                BindKind::WireIndex(arr_name.clone(), *i as u32)
                            } else if parent_vec_of_bus_ports.contains_key(arr_name.as_str()) {
                                // Vec-of-bus PORT element on the parent
                                // side (`port s: initiator Vec<B, N>`):
                                // flat name is `<port>_<i>`, mirroring
                                // the port flattening.
                                BindKind::FlatPort(format!("{}_{}", arr_name, i))
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let mut _pm = info.default_param_map();
                for pa in *bus_params {
                    _pm.insert(pa.name.name.clone(), &pa.value);
                }
                let _eff = info.effective_signals(&_pm);
                for (sname, sdir, _) in &_eff {
                    let inst_flat = format!("{}_{}", c.port_name.name, sname);
                    // Determine actual direction from the inst's bus perspective.
                    // For initiator: bus out → inst Output, bus in → inst Input.
                    // For target: bus out → inst Input (flipped), bus in → inst Output (flipped).
                    let actual_dir = match perspective {
                        BusPerspective::Initiator => *sdir,
                        BusPerspective::Target => (*sdir).flip(),
                    };
                    let dir = match actual_dir {
                        Direction::Out => ConnectDir::Output,
                        Direction::In => ConnectDir::Input,
                    };
                    let parent_signal = match &bind {
                        BindKind::FlatPort(prefix) => Expr::new(
                            ExprKind::Ident(format!("{}_{}", prefix, sname)),
                            c.signal.span,
                        ),
                        BindKind::WireStruct(name) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(ExprKind::Ident(name.clone()), c.signal.span)),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                        BindKind::WireIndex(name, i) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(
                                    ExprKind::Index(
                                        Box::new(Expr::new(
                                            ExprKind::Ident(name.clone()),
                                            c.signal.span,
                                        )),
                                        Box::new(Expr::new(
                                            ExprKind::Literal(LitKind::Dec(*i as u64)),
                                            c.signal.span,
                                        )),
                                    ),
                                    c.signal.span,
                                )),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                        BindKind::Wire2DIndex(name, m_idx, n_idx) => Expr::new(
                            ExprKind::FieldAccess(
                                Box::new(Expr::new(
                                    ExprKind::Index(
                                        Box::new(Expr::new(
                                            ExprKind::Index(
                                                Box::new(Expr::new(
                                                    ExprKind::Ident(name.clone()),
                                                    c.signal.span,
                                                )),
                                                Box::new(Expr::new(
                                                    ExprKind::Literal(LitKind::Dec(*m_idx as u64)),
                                                    c.signal.span,
                                                )),
                                            ),
                                            c.signal.span,
                                        )),
                                        Box::new(Expr::new(
                                            ExprKind::Literal(LitKind::Dec(*n_idx as u64)),
                                            c.signal.span,
                                        )),
                                    ),
                                    c.signal.span,
                                )),
                                Ident::new(sname.clone(), c.signal.span),
                            ),
                            c.signal.span,
                        ),
                    };
                    expanded.push(Connection {
                        port_name: Ident::new(inst_flat, c.port_name.span),
                        direction: dir,
                        signal: parent_signal,
                        reset_override: None,
                        span: c.span,
                    });
                }
            }
        } else {
            expanded.push(c.clone());
        }
    }
    expanded
}

/// Walk `stmt` and return true if any expression has the shape
/// `Index(Ident(name), Ident(var))` where `name` is a Vec-of-bus port
/// or wire (keys of `ports` / `wires`). Used by the for-loop emitter
/// to decide whether to statically unroll the body. Recurses into all
/// sub-statements and into both sides of assignments.
pub(super) fn stmt_indexes_vob_with_var(
    stmt: &Stmt,
    var: &str,
    ports: &HashMap<String, u32>,
    wires: &HashMap<String, u32>,
) -> bool {
    fn walk_expr(
        e: &Expr,
        var: &str,
        ports: &HashMap<String, u32>,
        wires: &HashMap<String, u32>,
    ) -> bool {
        if let ExprKind::Index(arr, idx) = &e.kind {
            if let (ExprKind::Ident(arr_name), ExprKind::Ident(idx_name)) = (&arr.kind, &idx.kind) {
                if idx_name == var && (ports.contains_key(arr_name) || wires.contains_key(arr_name))
                {
                    return true;
                }
            }
        }
        match &e.kind {
            ExprKind::Binary(_, l, r) => {
                walk_expr(l, var, ports, wires) || walk_expr(r, var, ports, wires)
            }
            ExprKind::Unary(_, x)
            | ExprKind::Cast(x, _)
            | ExprKind::LatencyAt(x, _)
            | ExprKind::SvaNext(_, x) => walk_expr(x, var, ports, wires),
            ExprKind::FieldAccess(b, _) => walk_expr(b, var, ports, wires),
            ExprKind::Index(b, i) | ExprKind::BitSlice(b, i, _) => {
                walk_expr(b, var, ports, wires) || walk_expr(i, var, ports, wires)
            }
            ExprKind::PartSelect(b, lo, hi, _) => {
                walk_expr(b, var, ports, wires)
                    || walk_expr(lo, var, ports, wires)
                    || walk_expr(hi, var, ports, wires)
            }
            ExprKind::Ternary(c, t, e2) => {
                walk_expr(c, var, ports, wires)
                    || walk_expr(t, var, ports, wires)
                    || walk_expr(e2, var, ports, wires)
            }
            ExprKind::Concat(parts) | ExprKind::FunctionCall(_, parts) => {
                parts.iter().any(|p| walk_expr(p, var, ports, wires))
            }
            ExprKind::MethodCall(b, _, args) => {
                walk_expr(b, var, ports, wires)
                    || args.iter().any(|a| walk_expr(a, var, ports, wires))
            }
            _ => false,
        }
    }
    match stmt {
        Stmt::Assign(a) => {
            walk_expr(&a.target, var, ports, wires) || walk_expr(&a.value, var, ports, wires)
        }
        Stmt::IfElse(ie) => {
            walk_expr(&ie.cond, var, ports, wires)
                || ie
                    .then_stmts
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
                || ie
                    .else_stmts
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
        }
        Stmt::Match(m) => {
            walk_expr(&m.scrutinee, var, ports, wires)
                || m.arms.iter().any(|arm| {
                    arm.body
                        .iter()
                        .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
                })
        }
        Stmt::For(f) => f
            .body
            .iter()
            .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires)),
        Stmt::Init(ib) => ib
            .body
            .iter()
            .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires)),
        Stmt::DoUntil { body, cond, .. } => {
            walk_expr(cond, var, ports, wires)
                || body
                    .iter()
                    .any(|s| stmt_indexes_vob_with_var(s, var, ports, wires))
        }
        Stmt::WaitUntil(e, _) => walk_expr(e, var, ports, wires),
        Stmt::Log(l) => l.args.iter().any(|a| walk_expr(a, var, ports, wires)),
    }
}
