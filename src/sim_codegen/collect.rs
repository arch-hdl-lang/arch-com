//! `collect` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// Smallest C++ unsigned integer type that fits `bits` (up to 64).
/// Returns true if `name` looks like a thread-lowered FSM state register.
/// Thread lowering in elaborate.rs (line ~1280) names state regs `_t{N}_state`
/// where N is the thread index. This helper is used by --debug-fsm and
/// auto-generated legal-state assertions to identify FSM state regs without
/// mis-matching user regs like `prev_state` or `state_counter`.
pub(super) fn is_thread_fsm_state_reg(name: &str) -> bool {
    // Strip leading underscores (the shadow field is _t0_state, public is t0_state)
    let trimmed = name.trim_start_matches('_');
    if !trimmed.starts_with('t') {
        return false;
    }
    if !trimmed.ends_with("_state") {
        return false;
    }
    // Middle must be digits
    let mid = &trimmed[1..trimmed.len() - "_state".len()];
    !mid.is_empty() && mid.chars().all(|c| c.is_ascii_digit())
}

/// Collect unique log file paths from module body (comb + seq blocks).
pub(super) fn collect_log_files(body: &[ModuleBodyItem]) -> Vec<String> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    fn from_comb(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                Stmt::Log(l) => {
                    if let Some(ref p) = l.file {
                        if seen.insert(p.clone()) {
                            files.push(p.clone());
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    from_comb(&ie.then_stmts, files, seen);
                    from_comb(&ie.else_stmts, files, seen);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        from_comb(&arm.body, files, seen);
                    }
                }
                Stmt::For(f) => from_comb(&f.body, files, seen),
                _ => {}
            }
        }
    }
    fn from_seq(stmts: &[Stmt], files: &mut Vec<String>, seen: &mut HashSet<String>) {
        for s in stmts {
            match s {
                Stmt::Log(l) => {
                    if let Some(ref p) = l.file {
                        if seen.insert(p.clone()) {
                            files.push(p.clone());
                        }
                    }
                }
                Stmt::IfElse(ie) => {
                    from_seq(&ie.then_stmts, files, seen);
                    from_seq(&ie.else_stmts, files, seen);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        from_seq(&arm.body, files, seen);
                    }
                }
                _ => {}
            }
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::CombBlock(cb) => from_comb(&cb.stmts, &mut files, &mut seen),
            ModuleBodyItem::RegBlock(rb) => from_seq(&rb.stmts, &mut files, &mut seen),
            _ => {}
        }
    }
    files
}

pub(super) fn collect_reg_names(body: &[ModuleBodyItem], ports: &[PortDecl]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| {
            if let ModuleBodyItem::RegDecl(r) = i {
                Some(r.name.name.clone())
            } else {
                None
            }
        })
        .chain(ports.iter().filter_map(|p| {
            if p.reg_info.is_some() {
                Some(p.name.name.clone())
            } else {
                None
            }
        }))
        .collect()
}

pub(super) fn collect_port_reg_names(ports: &[PortDecl]) -> HashSet<String> {
    ports
        .iter()
        .filter_map(|p| {
            if p.reg_info.is_some() {
                Some(p.name.name.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn emit_port_reg_public_copy(
    cpp: &mut String,
    name: &str,
    widths: &HashMap<String, u32>,
    vec_count: Option<u64>,
    indent: &str,
) {
    if let Some(count) = vec_count {
        for i in 0..count {
            cpp.push_str(&format!("{indent}{name}_{i} = _{name}[{i}];\n"));
        }
        return;
    }

    let bits = widths.get(name).copied().unwrap_or(0);
    if bits > 64 && bits <= 128 {
        cpp.push_str(&format!(
            "{indent}_arch_u128_to_vl(_{name}, {name}._data, {});\n",
            wide_words(bits)
        ));
    } else {
        cpp.push_str(&format!("{indent}{name} = _{name};\n"));
    }
}

pub(super) fn collect_let_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for i in body {
        match i {
            ModuleBodyItem::LetBinding(l) => {
                // Destructuring: each bound field becomes a _let_ field.
                if !l.destructure_fields.is_empty() {
                    for bind in &l.destructure_fields {
                        out.insert(bind.name.clone());
                    }
                } else {
                    out.insert(l.name.name.clone());
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                out.insert(w.name.name.clone());
            }
            _ => {}
        }
    }
    out
}

/// Map module-scope `let NAME: T = expr;` bindings to their RHS expr.
/// Used by `Stmt::Match` codegen to fold `Pattern::Ident(NAME)` arms
/// into `case <literal>:` labels (instead of the buggy `default:`
/// fall-through that collapses multi-let-bound match arms — see
/// memory/feedback_archsim_match_pattern_ident_default_collision.md).
/// Destructure-let bindings (`let {a, b} = ...;`) are skipped — those
/// don't have a single RHS and aren't referenceable from match patterns.
pub(super) fn collect_let_values(
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> HashMap<String, Expr> {
    let mut out = HashMap::new();
    for item in body {
        if let ModuleBodyItem::LetBinding(l) = item {
            if l.destructure_fields.is_empty() {
                out.insert(l.name.name.clone(), l.value.clone());
            }
        }
    }
    // Compile-time-constant params (`param X: const = N`, `param X[hi:lo]: const = N`,
    // `local param X: T = N`) participate in the same fold so `unique match` arms
    // whose LHS names a param resolve to `case <literal>:` rather than collapsing to
    // `default:`. Required for operator-decoder-style match blocks.
    for p in params {
        if let Some(expr) = &p.default {
            if matches!(&expr.kind, ExprKind::Literal(_)) {
                out.insert(p.name.name.clone(), expr.clone());
            }
        }
    }
    out
}

pub(super) fn collect_pipe_reg_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut s = HashSet::new();
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            for i in 0..p.stages {
                if i == p.stages - 1 {
                    s.insert(p.name.name.clone());
                } else {
                    s.insert(format!("{}_stg{}", p.name.name, i + 1));
                }
            }
        }
    }
    s
}

/// Collect all identifiers read in a comb statement (RHS of assignments).
pub(super) fn collect_comb_reads(stmt: &Stmt, out: &mut std::collections::BTreeSet<String>) {
    match stmt {
        Stmt::Assign(a) => collect_expr_idents(&a.value, out),
        Stmt::IfElse(ie) => {
            collect_expr_idents(&ie.cond, out);
            for s in &ie.then_stmts {
                collect_comb_reads(s, out);
            }
            for s in &ie.else_stmts {
                collect_comb_reads(s, out);
            }
        }
        Stmt::Log(_) => {}
        Stmt::Match(m) => {
            collect_expr_idents(&m.scrutinee, out);
            for arm in &m.arms {
                for s in &arm.body {
                    collect_comb_reads(s, out);
                }
            }
        }
        Stmt::For(f) => {
            for s in &f.body {
                collect_comb_reads(s, out);
            }
        }
        Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
            unreachable!("seq-only Stmt variant inside comb-context walker")
        }
    }
}

/// Collect all identifiers read in a seq (or init) statement — RHS of assignments,
/// branch conditions, loop bounds, wait predicates. Used by `--inputs-start-uninit`.
pub(super) fn collect_stmt_idents(stmt: &Stmt, out: &mut std::collections::BTreeSet<String>) {
    match stmt {
        Stmt::Assign(a) => collect_expr_idents(&a.value, out),
        Stmt::IfElse(ie) => {
            collect_expr_idents(&ie.cond, out);
            for s in &ie.then_stmts {
                collect_stmt_idents(s, out);
            }
            for s in &ie.else_stmts {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::Match(m) => {
            collect_expr_idents(&m.scrutinee, out);
            for arm in &m.arms {
                for s in &arm.body {
                    collect_stmt_idents(s, out);
                }
            }
        }
        Stmt::For(f) => {
            if let ForRange::Range(lo, hi) = &f.range {
                collect_expr_idents(lo, out);
                collect_expr_idents(hi, out);
            } else if let ForRange::ValueList(vs) = &f.range {
                for v in vs {
                    collect_expr_idents(v, out);
                }
            }
            for s in &f.body {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::Init(ib) => {
            for s in &ib.body {
                collect_stmt_idents(s, out);
            }
        }
        Stmt::WaitUntil(e, _) => collect_expr_idents(e, out),
        Stmt::DoUntil { body, cond, .. } => {
            for s in body {
                collect_stmt_idents(s, out);
            }
            collect_expr_idents(cond, out);
        }
        Stmt::Log(_) => {}
    }
}

pub(super) fn collect_expr_idents(expr: &Expr, out: &mut std::collections::BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            out.insert(name.clone());
        }
        ExprKind::Binary(_, lhs, rhs) => {
            collect_expr_idents(lhs, out);
            collect_expr_idents(rhs, out);
        }
        ExprKind::Unary(_, e) => collect_expr_idents(e, out),
        ExprKind::Index(base, idx) => {
            collect_expr_idents(base, out);
            collect_expr_idents(idx, out);
        }
        ExprKind::BitSlice(base, hi, lo) => {
            collect_expr_idents(base, out);
            collect_expr_idents(hi, out);
            collect_expr_idents(lo, out);
        }
        ExprKind::PartSelect(base, start, width, _) => {
            collect_expr_idents(base, out);
            collect_expr_idents(start, out);
            collect_expr_idents(width, out);
        }
        ExprKind::FieldAccess(base, field) => {
            collect_expr_idents(base, out);
            // For bus-style access `port.signal`, the emitted C++ reads
            // the flat name `port_signal` (matching SV bus flattening).
            // Emit that flat name as a candidate so --check-uninit and any
            // other name-indexed downstream analysis catches bus-port reads.
            // Non-bus field access (e.g. struct.field) is also a valid
            // candidate here; the downstream filter (e.g. uninit_inputs
            // membership) decides whether the name warrants action.
            if let ExprKind::Ident(b) = &base.kind {
                out.insert(format!("{}_{}", b, field.name));
            }
        }
        ExprKind::MethodCall(base, _, args) => {
            collect_expr_idents(base, out);
            for a in args {
                collect_expr_idents(a, out);
            }
        }
        ExprKind::FunctionCall(_, args) => {
            for a in args {
                collect_expr_idents(a, out);
            }
        }
        ExprKind::Ternary(cond, then_e, else_e) => {
            collect_expr_idents(cond, out);
            collect_expr_idents(then_e, out);
            collect_expr_idents(else_e, out);
        }
        ExprKind::ExprMatch(scrut, arms) => {
            collect_expr_idents(scrut, out);
            for arm in arms {
                collect_expr_idents(&arm.value, out);
            }
        }
        _ => {}
    }
}

pub(super) fn collect_inst_names(body: &[ModuleBodyItem]) -> HashSet<String> {
    body.iter()
        .filter_map(|i| {
            if let ModuleBodyItem::Inst(inst) = i {
                Some(inst.name.name.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Collect all sub-instance output signal names (auto-declared wires).
/// Pick the C++ field prefix for a Vec signal driven by an inst output.
///
/// The native-sim emitter stores `reg` values under `_<name>`, `let`/`wire`
/// values under `_let_<name>`, and inst output wires under the bare flat
/// name. When an inst Vec output is fanned out element-by-element to a
/// parent-scope Vec signal, the destination prefix depends on which kind
/// of declaration the parent signal is.
///
/// The classification order matters: a signal can sit in both `let_names`
/// (because `wire`s are tracked there) and `inst_out` (because an inst
/// drives it). The wire's storage is `_let_<name>`, so `let_names` must
/// win over `inst_out` — otherwise the write lands on the never-read flat
/// field. See PR #438 for the original fix.
///
/// `reg_names ∩ let_names` is treated as `reg` (regs win); this case is
/// not currently reachable because a name can only be declared once, but
/// the ordering keeps that future-proof.
pub(super) fn vec_storage_prefix<'a>(
    name: &str,
    reg_names: &HashSet<String>,
    let_names: &HashSet<String>,
    inst_out: &HashSet<String>,
) -> &'a str {
    if reg_names.contains(name) {
        "_"
    } else if let_names.contains(name) {
        "_let_"
    } else if inst_out.contains(name) {
        ""
    } else {
        "_let_"
    }
}

pub(super) fn collect_inst_output_signals(body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut signals = HashSet::new();
    for item in body {
        if let ModuleBodyItem::Inst(inst) = item {
            for conn in &inst.connections {
                if conn.direction == ConnectDir::Output {
                    if let ExprKind::Ident(name) = &conn.signal.kind {
                        signals.insert(name.clone());
                    }
                }
            }
        }
    }
    signals
}

/// Collect all LHS targets from comb blocks (recursing into if/else/match arms).
pub(super) fn collect_comb_targets(body: &[ModuleBodyItem]) -> HashSet<String> {
    fn collect_stmt_targets(stmt: &Stmt, out: &mut HashSet<String>) {
        match stmt {
            Stmt::Assign(a) => {
                if let ExprKind::Ident(name) = &a.target.kind {
                    out.insert(name.clone());
                }
            }
            Stmt::IfElse(ie) => {
                for s in &ie.then_stmts {
                    collect_stmt_targets(s, out);
                }
                for s in &ie.else_stmts {
                    collect_stmt_targets(s, out);
                }
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    for s in &arm.body {
                        collect_stmt_targets(s, out);
                    }
                }
            }
            Stmt::Log(_) => {}
            Stmt::For(f) => {
                for s in &f.body {
                    collect_stmt_targets(s, out);
                }
            }
            Stmt::Init(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("seq-only Stmt variant inside comb-context walker")
            }
        }
    }
    let mut targets = HashSet::new();
    for item in body {
        if let ModuleBodyItem::CombBlock(cb) = item {
            for stmt in &cb.stmts {
                collect_stmt_targets(stmt, &mut targets);
            }
        }
    }
    targets
}

pub(super) fn resolve_reg_reset_info(
    reset: &RegReset,
    ports: &[PortDecl],
) -> Option<(String, bool, bool)> {
    match reset {
        RegReset::None => None,
        RegReset::Explicit(sig, kind, level, _) => Some((
            sig.name.clone(),
            *kind == ResetKind::Async,
            *level == ResetLevel::Low,
        )),
        RegReset::Inherit(sig, _) => {
            if let Some(p) = ports.iter().find(|p| p.name.name == sig.name) {
                if let TypeExpr::Reset(kind, level) = &p.ty {
                    Some((
                        sig.name.clone(),
                        *kind == ResetKind::Async,
                        *level == ResetLevel::Low,
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Extract the reset value expression from a RegReset variant.
pub(super) fn reset_value_from_reg_reset(reset: &RegReset) -> Option<&Expr> {
    match reset {
        RegReset::None => None,
        RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => Some(val),
    }
}

/// Build enum_name → Vec<(variant_name, encoding_value)>.
pub(super) fn build_enum_map(symbols: &SymbolTable) -> HashMap<String, Vec<(String, u64)>> {
    let mut m = HashMap::new();
    for (name, (sym, _)) in &symbols.globals {
        if let Symbol::Enum(info) = sym {
            let entries: Vec<(String, u64)> = info
                .variants
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let val = info.values.get(i).and_then(|v| *v).unwrap_or(i as u64);
                    (v.clone(), val)
                })
                .collect();
            m.insert(name.clone(), entries);
        }
    }
    m
}

/// Resolve an enum variant to its ordinal value.
pub(super) fn resolve_enum_variant(
    enum_map: &HashMap<String, Vec<(String, u64)>>,
    enum_name: &str,
    variant_name: &str,
) -> Option<u64> {
    enum_map.get(enum_name).and_then(|variants| {
        variants
            .iter()
            .find(|(n, _)| n == variant_name)
            .map(|(_, v)| *v)
    })
}

/// Collect scalar names whose HDL type is signed. This parallels the width
/// map so fallback storage paths (implicit instance-output fields, pipe_reg
/// stages, and expression casts) can preserve signedness for 33..=64-bit
/// `SInt` values instead of treating them as unsigned bit buckets.
pub(super) fn build_signed_names(ports: &[PortDecl], body: &[ModuleBodyItem]) -> HashSet<String> {
    let mut s = HashSet::new();
    for p in ports {
        if type_is_signed_scalar(&p.ty) {
            s.insert(p.name.name.clone());
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if type_is_signed_scalar(&r.ty) {
                    s.insert(r.name.name.clone());
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                if type_is_signed_scalar(&w.ty) {
                    s.insert(w.name.name.clone());
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if l.ty.as_ref().map_or(false, type_is_signed_scalar) {
                    s.insert(l.name.name.clone());
                }
            }
            _ => {}
        }
    }
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            if s.contains(&p.source.name) {
                for i in 0..p.stages {
                    if i == p.stages - 1 {
                        s.insert(p.name.name.clone());
                    } else {
                        s.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
            }
        }
    }
    s
}

/// Collect scalar signal names whose HDL type is a float (FP32/BF16), mapping
/// each to its format. Parallels [`build_signed_names`]; drives float-op
/// dispatch in the expression emitter.
pub(super) fn build_float_names(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
) -> HashMap<String, FpFmt> {
    let mut m = HashMap::new();
    for p in ports {
        if let Some(f) = type_float_fmt(&p.ty) {
            m.insert(p.name.name.clone(), f);
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if let Some(f) = type_float_fmt(&r.ty) {
                    m.insert(r.name.name.clone(), f);
                }
            }
            ModuleBodyItem::WireDecl(w) => {
                if let Some(f) = type_float_fmt(&w.ty) {
                    m.insert(w.name.name.clone(), f);
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(f) = l.ty.as_ref().and_then(type_float_fmt) {
                    m.insert(l.name.name.clone(), f);
                }
            }
            _ => {}
        }
    }
    m
}

/// Collect names whose bit width exceeds 64 (require wide handling).
pub(super) fn collect_wide_names(
    ports: &[PortDecl],
    body: &[ModuleBodyItem],
    params: &[ParamDecl],
) -> HashSet<String> {
    let mut s = HashSet::new();
    for p in ports {
        if type_bits_te_with_params(&p.ty, params) > 64 {
            s.insert(p.name.name.clone());
        }
    }
    for item in body {
        match item {
            ModuleBodyItem::RegDecl(r) => {
                if type_bits_te_with_params(&r.ty, params) > 64 {
                    s.insert(r.name.name.clone());
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                if let Some(ty) = &l.ty {
                    if type_bits_te_with_params(ty, params) > 64 {
                        s.insert(l.name.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    // Resolve pipe_reg wide from source
    let widths = build_widths(ports, body, params);
    for item in body {
        if let ModuleBodyItem::PipeRegDecl(p) = item {
            let w = widths.get(&p.source.name).copied().unwrap_or(32);
            if w > 64 {
                for i in 0..p.stages {
                    if i == p.stages - 1 {
                        s.insert(p.name.name.clone());
                    } else {
                        s.insert(format!("{}_stg{}", p.name.name, i + 1));
                    }
                }
            }
        }
    }
    s
}
