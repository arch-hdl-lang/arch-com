use crate::ast::{
    BinOp, Expr, ExprKind, FloatLitFmt, ForRange, InsideMember, LitKind, Stmt, UnaryOp,
};
use crate::lexer::Span;

#[derive(Debug, Clone, Default)]
pub struct ThreadMap {
    pub modules: Vec<ThreadMapModule>,
}

#[derive(Debug, Clone)]
pub struct ThreadMapModule {
    pub module_name: String,
    pub generated_module_name: String,
    pub span: Span,
    pub threads: Vec<ThreadMapThread>,
}

#[derive(Debug, Clone)]
pub struct ThreadMapThread {
    pub name: String,
    pub index: usize,
    /// True for `thread once`, where the terminal state holds instead of
    /// wrapping to state 0.
    pub once: bool,
    pub span: Span,
    pub states: Vec<ThreadMapState>,
    /// Dead-skid comb-feedback hazards for this thread (issue #245).  Populated
    /// after lowering from the pre-lowering analysis; empty when clean.
    pub hazards: Vec<CombFeedbackHazard>,
}

/// One dead-skid comb-feedback hazard surfaced in the thread map: the thread
/// reads `read_signal`, a combinational function of `driven_signal` it drives.
#[derive(Debug, Clone)]
pub struct CombFeedbackHazard {
    pub read_signal: String,
    pub driven_signal: String,
    /// Rendered comb path `driven_signal -> … -> read_signal`.
    pub path_summary: String,
    /// Span of the thread's read of `read_signal` (for source highlighting).
    pub read_span: Span,
}

#[derive(Debug, Clone)]
pub struct ThreadMapState {
    pub index: usize,
    pub state_name: String,
    pub role: String,
    /// False when this source partition was absorbed by an optimization such
    /// as folded wait-exit assignment lowering and is not emitted as a runtime
    /// state arm in the generated FSM.
    pub emitted: bool,
    pub span: Span,
    pub labels: Vec<String>,
    /// Source-level fall-through target after local proof-preserving
    /// compaction such as folded wait-exit assignments.
    ///
    /// The lowered FSM transition target is still represented separately in
    /// `transitions`; proof tooling checks the two agree where source
    /// semantics require natural fall-through.
    pub source_next_index: usize,
    pub source_next_name: String,
    /// Rendered `wait N cycle` count expression for counted-wait states.
    ///
    /// `None` for all other state roles. The proof converter only accepts
    /// literal natural counts today, but this field keeps the certificate
    /// source-of-truth separate from human-readable labels.
    pub wait_cycles_count: Option<String>,
    /// Sequential updates that fire while this runtime state is active.
    pub seq_updates: Vec<String>,
    /// Direct sequential assignments that fire while this runtime state is
    /// active. Nested/guarded statements remain represented by `seq_updates`
    /// until the proof certificate grows a full statement language.
    pub seq_assignments: Vec<ThreadMapAssignment>,
    /// Sequential updates folded into this state's guarded exit arm.
    ///
    /// This is populated by the wait-until exit folding optimization. The
    /// absorbed successor state is still present in the map with
    /// `emitted == false`, while these updates document the store effect that
    /// moved onto the wait state's exit edge.
    pub folded_exit_updates: Vec<String>,
    /// Direct sequential assignments folded into this state's guarded exit
    /// arm. This mirrors `folded_exit_updates` with structure when the folded
    /// statement is a plain `target <= value` assignment.
    pub folded_exit_assignments: Vec<ThreadMapAssignment>,
    /// Source-level transition intent for this state, after local compaction.
    ///
    /// This is intentionally separate from `transitions`, which records the
    /// lowered FSM table. Today the lowering path populates both from the same
    /// raw table for most states, but proof tooling treats them as separate
    /// channels so later source-intent extraction can catch lowered-table drift.
    pub source_transitions: Vec<ThreadMapTransition>,
    /// Machine-readable provenance for `source_transitions`.
    ///
    /// v5 uses `pre_fold_snapshot`: transition intent was snapshotted before
    /// folded wait-exit assignment optimization and compacted across folded
    /// states before emission.
    pub source_transition_origin: String,
    /// Lowered FSM transition table emitted by `lower_threads`.
    pub transitions: Vec<ThreadMapTransition>,
}

#[derive(Debug, Clone)]
pub struct ThreadMapAssignment {
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ThreadMapTransition {
    pub condition: String,
    pub condition_guard: Option<ThreadMapGuardExpr>,
    pub target_index: usize,
    pub target_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadMapGuardExpr {
    Atom(String),
    True,
    False,
    Not(Box<ThreadMapGuardExpr>),
    And(Box<ThreadMapGuardExpr>, Box<ThreadMapGuardExpr>),
    Or(Box<ThreadMapGuardExpr>, Box<ThreadMapGuardExpr>),
    Lt(ThreadMapNatExpr, ThreadMapNatExpr),
    Ge(ThreadMapNatExpr, ThreadMapNatExpr),
    Eq(ThreadMapNatExpr, ThreadMapNatExpr),
    Ne(ThreadMapNatExpr, ThreadMapNatExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadMapNatExpr {
    Var(String),
    Const(u64),
}

#[derive(Debug, Clone)]
pub struct ThreadMapSource {
    pub start: usize,
    pub end: usize,
    pub filename: String,
    pub source: String,
}

pub fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Assign(a) => a.span,
        Stmt::IfElse(ie) => ie.span,
        Stmt::Match(m) => m.span,
        Stmt::Log(l) => l.span,
        Stmt::For(f) => f.span,
        Stmt::Init(i) => i.span,
        Stmt::WaitUntil(_, sp) => *sp,
        Stmt::DoUntil { span, .. } => *span,
    }
}

pub fn expr_label(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Ident(name) => name.clone(),
        ExprKind::SynthIdent(name, _) => name.clone(),
        ExprKind::Literal(lit) => lit_label(lit),
        ExprKind::Bool(v) => v.to_string(),
        ExprKind::EnumVariant(en, v) => format!("{}::{}", en.name, v.name),
        ExprKind::Unary(op, e) => match op {
            UnaryOp::Not => format!("!{}", paren_expr(e)),
            UnaryOp::BitNot => format!("~{}", paren_expr(e)),
            UnaryOp::Neg => format!("-{}", paren_expr(e)),
            UnaryOp::RedAnd => format!("&{}", paren_expr(e)),
            UnaryOp::RedOr => format!("|{}", paren_expr(e)),
            UnaryOp::RedXor => format!("^{}", paren_expr(e)),
        },
        ExprKind::Binary(op, l, r) => {
            format!("{} {} {}", paren_expr(l), binop_label(*op), paren_expr(r))
        }
        ExprKind::Ternary(c, t, f) => {
            format!("{} ? {} : {}", expr_label(c), expr_label(t), expr_label(f))
        }
        ExprKind::Index(base, idx) => format!("{}[{}]", expr_label(base), expr_label(idx)),
        ExprKind::BitSlice(base, hi, lo) => {
            format!(
                "{}[{}:{}]",
                expr_label(base),
                expr_label(hi),
                expr_label(lo)
            )
        }
        ExprKind::PartSelect(base, start, width, up) => {
            let dir = if *up { "+:" } else { "-:" };
            format!(
                "{}[{} {} {}]",
                expr_label(base),
                expr_label(start),
                dir,
                expr_label(width)
            )
        }
        ExprKind::FieldAccess(base, field) => format!("{}.{}", expr_label(base), field.name),
        ExprKind::MethodCall(base, name, args) => {
            let args = args.iter().map(expr_label).collect::<Vec<_>>().join(", ");
            format!("{}.{}({})", expr_label(base), name.name, args)
        }
        ExprKind::FunctionCall(name, args) => {
            let args = args.iter().map(expr_label).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args)
        }
        ExprKind::PipelinedCall(name, args, stages) => {
            let args = args.iter().map(expr_label).collect::<Vec<_>>().join(", ");
            format!("{}<pipelined, {}>({})", name, stages, args)
        }
        ExprKind::Concat(parts) => {
            let parts = parts.iter().map(expr_label).collect::<Vec<_>>().join(", ");
            format!("{{{parts}}}")
        }
        ExprKind::Repeat(n, e) => format!("{{{}{{{}}}}}", expr_label(n), expr_label(e)),
        ExprKind::Cast(e, ty) => format!("cast<{ty:?}>({})", expr_label(e)),
        ExprKind::StructLiteral(name, fields) => {
            let fields = fields
                .iter()
                .map(|f| format!("{}: {}", f.name.name, expr_label(&f.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}{{{}}}", name.name, fields)
        }
        ExprKind::Match(scrutinee, _) => format!("match {}", expr_label(scrutinee)),
        ExprKind::ExprMatch(scrutinee, _) => format!("match {}", expr_label(scrutinee)),
        ExprKind::Signed(e) => format!("signed({})", expr_label(e)),
        ExprKind::Unsigned(e) => format!("unsigned({})", expr_label(e)),
        ExprKind::Clog2(e) => format!("$clog2({})", expr_label(e)),
        ExprKind::Onehot(e) => format!("onehot({})", expr_label(e)),
        ExprKind::LatencyAt(e, n) => format!("{}@{}", expr_label(e), n),
        ExprKind::SvaNext(n, e) => format!("##{} {}", n, expr_label(e)),
        ExprKind::Inside(e, members) => {
            let members = members
                .iter()
                .map(inside_member_label)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} inside {{{}}}", expr_label(e), members)
        }
        ExprKind::Todo => "todo!".to_string(),
    }
}

pub fn guard_expr(expr: &Expr) -> ThreadMapGuardExpr {
    match &expr.kind {
        ExprKind::Bool(true) => ThreadMapGuardExpr::True,
        ExprKind::Bool(false) => ThreadMapGuardExpr::False,
        ExprKind::Unary(UnaryOp::Not, inner) => {
            ThreadMapGuardExpr::Not(Box::new(guard_expr(inner)))
        }
        ExprKind::Binary(BinOp::And, lhs, rhs) => {
            ThreadMapGuardExpr::And(Box::new(guard_expr(lhs)), Box::new(guard_expr(rhs)))
        }
        ExprKind::Binary(BinOp::Or, lhs, rhs) => {
            ThreadMapGuardExpr::Or(Box::new(guard_expr(lhs)), Box::new(guard_expr(rhs)))
        }
        ExprKind::Binary(BinOp::Lt, lhs, rhs) => {
            ThreadMapGuardExpr::Lt(nat_expr(lhs), nat_expr(rhs))
        }
        ExprKind::Binary(BinOp::Gte, lhs, rhs) => {
            ThreadMapGuardExpr::Ge(nat_expr(lhs), nat_expr(rhs))
        }
        ExprKind::Binary(BinOp::Gt, lhs, rhs) => {
            ThreadMapGuardExpr::Lt(nat_expr(rhs), nat_expr(lhs))
        }
        ExprKind::Binary(BinOp::Lte, lhs, rhs) => {
            ThreadMapGuardExpr::Ge(nat_expr(rhs), nat_expr(lhs))
        }
        ExprKind::Binary(BinOp::Eq, lhs, rhs) => {
            ThreadMapGuardExpr::Eq(nat_expr(lhs), nat_expr(rhs))
        }
        ExprKind::Binary(BinOp::Neq, lhs, rhs) => {
            ThreadMapGuardExpr::Ne(nat_expr(lhs), nat_expr(rhs))
        }
        _ => ThreadMapGuardExpr::Atom(expr_label(expr)),
    }
}

pub fn nat_expr(expr: &Expr) -> ThreadMapNatExpr {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => ThreadMapNatExpr::Const(*v),
        ExprKind::Cast(inner, _) | ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
            nat_expr(inner)
        }
        ExprKind::MethodCall(inner, method, args)
            if matches!(method.name.as_str(), "trunc" | "zext" | "sext" | "resize")
                && args.len() == 1 =>
        {
            nat_expr(inner)
        }
        _ => ThreadMapNatExpr::Var(expr_label(expr)),
    }
}

pub fn stmt_label(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Assign(assign) => format!(
            "{} <= {}",
            expr_label(&assign.target),
            expr_label(&assign.value)
        ),
        Stmt::IfElse(ie) => {
            let then_labels = ie
                .then_stmts
                .iter()
                .map(stmt_label)
                .collect::<Vec<_>>()
                .join("; ");
            let else_labels = ie
                .else_stmts
                .iter()
                .map(stmt_label)
                .collect::<Vec<_>>()
                .join("; ");
            if ie.else_stmts.is_empty() {
                format!("if {} then [{}]", expr_label(&ie.cond), then_labels)
            } else {
                format!(
                    "if {} then [{}] else [{}]",
                    expr_label(&ie.cond),
                    then_labels,
                    else_labels
                )
            }
        }
        Stmt::Match(m) => format!("match {}", expr_label(&m.scrutinee)),
        Stmt::Log(log) => format!("log {}", log.tag),
        Stmt::For(f) => {
            let body = f.body.iter().map(stmt_label).collect::<Vec<_>>().join("; ");
            format!(
                "for {} in {} [{}]",
                f.var.name,
                for_range_label(&f.range),
                body
            )
        }
        Stmt::Init(init) => {
            let body = init
                .body
                .iter()
                .map(stmt_label)
                .collect::<Vec<_>>()
                .join("; ");
            format!("init on {} [{}]", init.reset_signal.name, body)
        }
        Stmt::WaitUntil(cond, _) => format!("wait until {}", expr_label(cond)),
        Stmt::DoUntil { body, cond, .. } => {
            let body = body.iter().map(stmt_label).collect::<Vec<_>>().join("; ");
            format!("do [{}] until {}", body, expr_label(cond))
        }
    }
}

pub fn stmt_assignments(stmts: &[Stmt]) -> Vec<ThreadMapAssignment> {
    stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign(assign) => Some(ThreadMapAssignment {
                target: expr_label(&assign.target),
                value: expr_label(&assign.value),
            }),
            _ => None,
        })
        .collect()
}

pub fn render_html(map: &ThreadMap, sources: &[ThreadMapSource], title: &str) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{}</title>\n", html_escape(title)));
    out.push_str("<style>\n");
    out.push_str(
        r#"
:root { color-scheme: light; --bg: #f7f8fb; --ink: #18202f; --muted: #607086; --line: #d9e0ea; --panel: #ffffff; }
* { box-sizing: border-box; }
body { margin: 0; font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: var(--ink); background: var(--bg); }
header { padding: 18px 24px 12px; border-bottom: 1px solid var(--line); background: #fff; }
h1 { margin: 0 0 4px; font-size: 20px; font-weight: 700; letter-spacing: 0; }
.sub { color: var(--muted); font-size: 13px; }
.layout { display: grid; grid-template-columns: minmax(360px, 1fr) minmax(420px, 1fr); gap: 16px; padding: 16px; align-items: start; }
.pane { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; overflow: hidden; }
.pane h2 { margin: 0; padding: 12px 14px; font-size: 14px; border-bottom: 1px solid var(--line); background: #fbfcfe; }
.source-file { border-bottom: 1px solid var(--line); }
.source-file:last-child { border-bottom: 0; }
.file-title { padding: 10px 14px; font-weight: 650; color: #324157; background: #f2f5f9; border-bottom: 1px solid var(--line); }
pre { margin: 0; overflow: auto; }
.src-line { display: grid; grid-template-columns: 56px 92px minmax(0, 1fr); min-height: 22px; font: 12px/22px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.ln { color: #7d8ca1; text-align: right; padding-right: 12px; user-select: none; border-right: 1px solid #eef2f6; }
.bands { display: flex; gap: 3px; padding: 2px 8px; overflow: hidden; }
.band { min-width: 18px; height: 18px; border-radius: 4px; text-align: center; line-height: 18px; font-size: 10px; font-weight: 700; color: #1f2c3d; }
.code { white-space: pre; padding: 0 10px; min-width: 0; }
.c0 { background: #d7ebff; } .c1 { background: #d9f2df; } .c2 { background: #ffe6c7; } .c3 { background: #eadcff; }
.c4 { background: #d8f3f0; } .c5 { background: #ffe0e6; } .c6 { background: #edf0b9; } .c7 { background: #dde5f8; }
.flow-node.c0 { fill: #d7ebff; } .flow-node.c1 { fill: #d9f2df; } .flow-node.c2 { fill: #ffe6c7; } .flow-node.c3 { fill: #eadcff; }
.flow-node.c4 { fill: #d8f3f0; } .flow-node.c5 { fill: #ffe0e6; } .flow-node.c6 { fill: #edf0b9; } .flow-node.c7 { fill: #dde5f8; }
.module { padding: 12px 14px 4px; border-bottom: 1px solid var(--line); }
.module:last-child { border-bottom: 0; }
h3 { margin: 0 0 8px; font-size: 15px; }
h4 { margin: 12px 0 8px; font-size: 13px; color: #34445d; }
.flow-wrap { overflow: auto; border: 1px solid #edf1f6; border-radius: 6px; background: #fbfcfe; margin: 8px 0 12px; padding: 12px; }
.thread-flow-chart { display: block; width: 100%; height: auto; }
.graph-edge { fill: none; stroke: #606975; stroke-width: 1.6; }
.graph-label { fill: #232a34; font: 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; paint-order: stroke; stroke: #fbfcfe; stroke-width: 5px; stroke-linejoin: round; }
.graph-node { fill: #ffffff; stroke: #dfe5ee; stroke-width: 1.4; }
.graph-node-title { fill: #1f2630; font: 15px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
.graph-node-sub { fill: #647186; font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.graph-node.c0 { fill: #eef6ff; } .graph-node.c1 { fill: #eefaf0; } .graph-node.c2 { fill: #fff4e4; } .graph-node.c3 { fill: #f4ecff; }
.graph-node.c4 { fill: #edf9f7; } .graph-node.c5 { fill: #fff0f3; } .graph-node.c6 { fill: #fbfce8; } .graph-node.c7 { fill: #f0f3fb; }
table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-bottom: 12px; }
th, td { border-top: 1px solid #edf1f6; padding: 7px 8px; vertical-align: top; text-align: left; overflow-wrap: anywhere; }
th { color: #53647c; font-size: 12px; font-weight: 650; background: #fbfcfe; }
.state-chip { display: inline-block; padding: 2px 6px; border-radius: 5px; font: 11px/16px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-weight: 700; }
.role { color: #5f6f84; font-size: 12px; }
.labels { color: #2d3b50; }
.trans { color: #364860; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.empty { padding: 20px 24px; color: var(--muted); }
table.hazards th { background: #fff4e5; color: #9a4a00; }
table.hazards td { border-top: 1px solid #ffe0bf; }
.src-line.hazard { background: #fff4e5; }
.src-line.hazard .ln { color: #c2640a; font-weight: 700; }
.src-line.hazard .code::after { content: "  ⚠ dead-skid read"; color: #c2640a; font-weight: 700; }
@media (max-width: 900px) { .layout { grid-template-columns: 1fr; } }
"#,
    );
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str(&format!(
        "<header><h1>{}</h1><div class=\"sub\">ARCH thread lowering map</div></header>\n",
        html_escape(title)
    ));

    if map.modules.is_empty() {
        out.push_str("<main class=\"empty\">No lowered thread states were recorded.</main>\n</body>\n</html>\n");
        return out;
    }

    out.push_str("<main class=\"layout\">\n<section class=\"pane\"><h2>Source Partitions</h2>\n");
    for src in sources.iter().filter(|src| source_has_map(map, src)) {
        render_source_file(&mut out, map, src);
    }
    out.push_str("</section>\n<section class=\"pane\"><h2>Thread Flow</h2>\n");
    for module in &map.modules {
        out.push_str("<div class=\"module\">");
        out.push_str(&format!(
            "<h3>{} <span class=\"role\">→ {}</span></h3>",
            html_escape(&module.module_name),
            html_escape(&module.generated_module_name)
        ));
        for thread in &module.threads {
            let warn = if thread.hazards.is_empty() {
                ""
            } else {
                "⚠ "
            };
            out.push_str(&format!(
                "<h4>{}thread {} <span class=\"role\">index {}</span></h4>",
                warn,
                html_escape(&thread.name),
                thread.index
            ));
            if !thread.hazards.is_empty() {
                out.push_str(
                    "<table class=\"hazards\"><thead><tr><th colspan=\"2\">⚠ dead-skid comb feedback (issue #245)</th></tr><tr><th style=\"width:30%\">Reads</th><th>Comb path</th></tr></thead><tbody>",
                );
                for hz in &thread.hazards {
                    out.push_str(&format!(
                        "<tr><td class=\"trans\">{}</td><td class=\"trans\">{}</td></tr>",
                        html_escape(&hz.read_signal),
                        html_escape(&hz.path_summary),
                    ));
                }
                out.push_str("</tbody></table>");
            }
            render_thread_flow_chart(&mut out, sources, thread);
            out.push_str("<table><thead><tr><th style=\"width:22%\">State</th><th style=\"width:13%\">Lines</th><th style=\"width:25%\">Labels</th><th>Transitions</th></tr></thead><tbody>");
            for state in &thread.states {
                let lines = find_line_range(sources, state.span)
                    .map(|(_, a, b)| {
                        if a == b {
                            a.to_string()
                        } else {
                            format!("{a}-{b}")
                        }
                    })
                    .unwrap_or_else(|| "-".to_string());
                out.push_str("<tr>");
                out.push_str(&format!(
                    "<td><span class=\"state-chip c{}\">S{}</span><br>{}<br><span class=\"role\">{}</span></td>",
                    state.index % 8,
                    state.index,
                    html_escape(&state.state_name),
                    html_escape(&state.role)
                ));
                out.push_str(&format!("<td>{}</td>", html_escape(&lines)));
                out.push_str("<td class=\"labels\">");
                if state.labels.is_empty() {
                    out.push_str("&nbsp;");
                } else {
                    out.push_str(&html_escape(&state.labels.join("; ")));
                }
                out.push_str("</td><td class=\"trans\">");
                if state.transitions.is_empty() {
                    out.push_str("&nbsp;");
                } else {
                    for (i, tr) in state.transitions.iter().enumerate() {
                        if i > 0 {
                            out.push_str("<br>");
                        }
                        out.push_str(&format!(
                            "{} → S{} {}",
                            html_escape(&tr.condition),
                            tr.target_index,
                            html_escape(&tr.target_name)
                        ));
                    }
                }
                out.push_str("</td></tr>");
            }
            out.push_str("</tbody></table>");
        }
        out.push_str("</div>");
    }
    out.push_str("</section>\n</main>\n</body>\n</html>\n");
    out
}

fn render_source_file(out: &mut String, map: &ThreadMap, src: &ThreadMapSource) {
    out.push_str("<div class=\"source-file\">");
    out.push_str(&format!(
        "<div class=\"file-title\">{}</div><pre>",
        html_escape(&src.filename)
    ));
    let mut offset = src.start;
    for (idx, raw_line) in src.source.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
        let line_text = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line_start = offset;
        let line_end = offset + raw_line.len().max(1);
        offset += raw_line.len();
        let states = states_overlapping_line(map, src, line_start, line_end, line_no, line_text);
        let hazard = line_has_hazard(map, line_start, line_end);
        out.push_str(if hazard {
            "<div class=\"src-line hazard\">"
        } else {
            "<div class=\"src-line\">"
        });
        out.push_str(&format!(
            "<span class=\"ln\">{line_no}</span><span class=\"bands\">"
        ));
        for state in states.iter().take(4) {
            out.push_str(&format!(
                "<span class=\"band c{}\">S{}</span>",
                state.index % 8,
                state.index
            ));
        }
        out.push_str("</span>");
        out.push_str(&format!(
            "<span class=\"code\">{}</span></div>",
            html_escape(line_text)
        ));
    }
    out.push_str("</pre></div>");
}

/// True when any thread hazard's read span overlaps `[line_start, line_end)`.
fn line_has_hazard(map: &ThreadMap, line_start: usize, line_end: usize) -> bool {
    map.modules.iter().any(|m| {
        m.threads.iter().any(|t| {
            t.hazards
                .iter()
                .any(|h| span_overlaps(h.read_span, line_start, line_end))
        })
    })
}

fn render_thread_flow_chart(
    out: &mut String,
    sources: &[ThreadMapSource],
    thread: &ThreadMapThread,
) {
    let positions = graph_positions(thread);
    let width = GRAPH_W;
    let height = positions
        .iter()
        .map(|p| p.y + GRAPH_NODE_H + 120)
        .max()
        .unwrap_or(180);
    let marker_id = format!("graph-arrow-t{}", thread.index);

    out.push_str("<div class=\"flow-wrap\">");
    out.push_str(&format!(
        "<svg class=\"thread-flow-chart\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Thread {} control-flow chart\">",
        html_escape(&thread.name)
    ));
    out.push_str(&format!(
        "<defs><marker id=\"{}\" markerWidth=\"10\" markerHeight=\"10\" refX=\"8\" refY=\"5\" orient=\"auto\" markerUnits=\"strokeWidth\"><path d=\"M0,0 L10,5 L0,10 z\" fill=\"#606975\"/></marker></defs>",
        html_escape(&marker_id)
    ));

    for state in &thread.states {
        let from = positions[state.index];
        for tr in &state.transitions {
            if let Some(to) = positions.get(tr.target_index).copied() {
                render_graph_edge(out, state, tr, from, to, &marker_id);
            }
        }
    }

    for state in &thread.states {
        let pos = positions[state.index];
        let lines = find_line_range(sources, state.span)
            .map(|(_, a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            })
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "<g class=\"flow-state\" data-state=\"S{}\"><rect class=\"graph-node c{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            state.index,
            state.index % 8,
            pos.x,
            pos.y,
            GRAPH_NODE_W,
            GRAPH_NODE_H
        ));
        out.push_str(&format!(
            "<text class=\"graph-node-title\" x=\"{}\" y=\"{}\">{}</text>",
            pos.x + 22,
            pos.y + 31,
            html_escape(&graph_node_title(state))
        ));
        out.push_str(&format!(
            "<text class=\"graph-node-sub\" x=\"{}\" y=\"{}\">{} - line {}</text>",
            pos.x + 22,
            pos.y + 52,
            html_escape(&state.role),
            html_escape(&lines)
        ));
        out.push_str("</g>");
    }
    out.push_str("</svg></div>");
}

const GRAPH_NODE_W: i32 = 210;
const GRAPH_NODE_H: i32 = 64;
const GRAPH_W: i32 = 860;

#[derive(Clone, Copy)]
struct GraphPos {
    x: i32,
    y: i32,
}

fn graph_positions(thread: &ThreadMapThread) -> Vec<GraphPos> {
    let mut positions = (0..thread.states.len())
        .map(|i| GraphPos {
            x: 380,
            y: 24 + i as i32 * 122,
        })
        .collect::<Vec<_>>();

    for state in &thread.states {
        let forward_targets = state
            .transitions
            .iter()
            .filter(|tr| tr.target_index > state.index)
            .map(|tr| tr.target_index)
            .collect::<Vec<_>>();
        if forward_targets.len() == 2 {
            let branch_y = positions[state.index].y + 132;
            if let Some(pos) = positions.get_mut(forward_targets[0]) {
                *pos = GraphPos {
                    x: 250,
                    y: branch_y,
                };
            }
            if let Some(pos) = positions.get_mut(forward_targets[1]) {
                *pos = GraphPos {
                    x: 560,
                    y: branch_y,
                };
            }
            break;
        }
    }
    positions
}

fn render_graph_edge(
    out: &mut String,
    state: &ThreadMapState,
    tr: &ThreadMapTransition,
    from: GraphPos,
    to: GraphPos,
    marker_id: &str,
) {
    let label = transition_summary(state, tr);
    if tr.target_index <= state.index {
        let from_center = from.x + GRAPH_NODE_W / 2;
        let to_center = to.x + GRAPH_NODE_W / 2;
        let use_left_lane = from_center <= to_center;
        let lane_offset = (state.index.saturating_sub(tr.target_index) as i32 * 30).min(96);
        let lane = if use_left_lane {
            24 + lane_offset
        } else {
            GRAPH_W - 24 - lane_offset
        };
        let sx = if use_left_lane {
            from.x
        } else {
            from.x + GRAPH_NODE_W
        };
        let sy = from.y + GRAPH_NODE_H / 2;
        let ex = if use_left_lane {
            to.x
        } else {
            to.x + GRAPH_NODE_W
        };
        let ey = to.y + GRAPH_NODE_H / 2;
        out.push_str(&format!(
            "<path class=\"graph-edge\" marker-end=\"url(#{})\" d=\"M{sx},{sy} C{lane},{sy} {lane},{ey} {ex},{ey}\"/>",
            html_escape(marker_id)
        ));
        render_graph_label(
            out,
            if use_left_lane { lane + 8 } else { lane - 42 },
            (sy + ey) / 2,
            &label,
        );
    } else {
        let sx = from.x + GRAPH_NODE_W / 2;
        let sy = from.y + GRAPH_NODE_H;
        let ex = to.x + GRAPH_NODE_W / 2;
        let ey = to.y;
        let mid_y = (sy + ey) / 2;
        if sx == ex {
            out.push_str(&format!(
                "<path class=\"graph-edge\" marker-end=\"url(#{})\" d=\"M{sx},{sy} L{ex},{ey}\"/>",
                html_escape(marker_id)
            ));
        } else {
            let bend = ((ex - sx).abs() / 3).clamp(36, 96);
            let c1x = if ex >= sx { sx + bend } else { sx - bend };
            let c2x = if ex >= sx { ex - bend } else { ex + bend };
            out.push_str(&format!(
                "<path class=\"graph-edge\" marker-end=\"url(#{})\" d=\"M{sx},{sy} C{c1x},{mid_y} {c2x},{mid_y} {ex},{ey}\"/>",
                html_escape(marker_id)
            ));
        }
        render_graph_label(out, (sx + ex) / 2 + 8, mid_y - 6, &label);
    }
}

fn render_graph_label(out: &mut String, x: i32, y: i32, label: &str) {
    if label.is_empty() {
        return;
    }
    out.push_str(&format!(
        "<text class=\"graph-label\" x=\"{x}\" y=\"{y}\">{}</text>",
        html_escape(label)
    ));
}

fn graph_node_title(state: &ThreadMapState) -> String {
    if let Some(label) = state.labels.iter().find(|l| l.starts_with("wait until ")) {
        return format!("S{}: {}", state.index, label);
    }
    if let Some(label) = state
        .labels
        .iter()
        .find(|l| l.starts_with("wait ") && l.ends_with(" cycle"))
    {
        return format!("S{}: {}", state.index, label);
    }
    if state.role == "dispatch" && state.transitions.len() == 2 {
        return format!("S{}: branch", state.index);
    }
    if state.role == "dispatch" {
        return format!("S{}: loop / exit", state.index);
    }
    if state.role == "entry" {
        return format!("S{}: entry", state.index);
    }
    format!("S{}: action", state.index)
}

fn transition_summary(state: &ThreadMapState, tr: &ThreadMapTransition) -> String {
    transition_summary_with(
        state.role.as_str(),
        state.transitions.len(),
        state.index,
        tr,
    )
}

fn transition_summary_with(
    role: &str,
    n_transitions: usize,
    from_index: usize,
    tr: &ThreadMapTransition,
) -> String {
    if tr.condition == "always" {
        return String::new();
    }
    if tr.condition == "true" {
        return "join/rejoin".to_string();
    }
    if role == "dispatch" && n_transitions == 2 {
        if tr.condition.starts_with("!(") {
            return "else".to_string();
        }
        return "then".to_string();
    }
    if tr.target_index <= from_index {
        if tr.target_index == 0 {
            return String::new();
        }
        return String::new();
    }
    if tr.target_index > from_index + 1 {
        return "branch".to_string();
    }
    tr.condition.clone()
}

fn source_has_map(map: &ThreadMap, src: &ThreadMapSource) -> bool {
    map.modules.iter().any(|m| {
        span_overlaps(m.span, src.start, src.end)
            || m.threads.iter().any(|t| {
                span_overlaps(t.span, src.start, src.end)
                    || t.states
                        .iter()
                        .any(|s| span_overlaps(s.span, src.start, src.end))
            })
    })
}

fn states_overlapping_line<'a>(
    map: &'a ThreadMap,
    src: &ThreadMapSource,
    line_start: usize,
    line_end: usize,
    line_no: usize,
    line_text: &str,
) -> Vec<&'a ThreadMapState> {
    let mut states = Vec::new();
    let trimmed = line_text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return states;
    }
    for module in &map.modules {
        if !span_overlaps(module.span, src.start, src.end) {
            continue;
        }
        for thread in &module.threads {
            for state in &thread.states {
                if state_marks_line(state, src, line_start, line_end, line_no) {
                    states.push(state);
                }
            }
        }
    }
    states
}

fn state_marks_line(
    state: &ThreadMapState,
    src: &ThreadMapSource,
    line_start: usize,
    line_end: usize,
    line_no: usize,
) -> bool {
    if !span_overlaps(state.span, line_start, line_end) {
        return false;
    }
    let Some((start_line, end_line)) = line_range_in_source(src, state.span) else {
        return true;
    };
    if start_line == end_line {
        return true;
    }
    anchor_line_for_span(src, state.span).map_or(line_no == start_line, |anchor| line_no == anchor)
}

fn line_range_in_source(src: &ThreadMapSource, span: Span) -> Option<(usize, usize)> {
    if span.start < src.start || span.start > src.end {
        return None;
    }
    let start = line_for_offset(&src.source, span.start.saturating_sub(src.start));
    let end = line_for_offset(&src.source, span.end.saturating_sub(src.start));
    Some((start, end.max(start)))
}

fn anchor_line_for_span(src: &ThreadMapSource, span: Span) -> Option<usize> {
    let (start_line, end_line) = line_range_in_source(src, span)?;
    for line_no in start_line..=end_line {
        let text = src
            .source
            .lines()
            .nth(line_no.saturating_sub(1))
            .unwrap_or("");
        let trimmed = text.trim_start();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            return Some(line_no);
        }
    }
    Some(start_line)
}

fn find_line_range(sources: &[ThreadMapSource], span: Span) -> Option<(&str, usize, usize)> {
    let src = sources
        .iter()
        .find(|src| span.start >= src.start && span.start <= src.end)?;
    let start = line_for_offset(&src.source, span.start.saturating_sub(src.start));
    let end = line_for_offset(&src.source, span.end.saturating_sub(src.start));
    Some((&src.filename, start, end.max(start)))
}

fn line_for_offset(source: &str, local_offset: usize) -> usize {
    let mut line = 1;
    for (idx, b) in source.as_bytes().iter().enumerate() {
        if idx >= local_offset {
            break;
        }
        if *b == b'\n' {
            line += 1;
        }
    }
    line
}

fn span_overlaps(span: Span, start: usize, end: usize) -> bool {
    span.start < end && span.end > start
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn paren_expr(expr: &Expr) -> String {
    match expr.kind {
        ExprKind::Binary(..) | ExprKind::Ternary(..) => format!("({})", expr_label(expr)),
        _ => expr_label(expr),
    }
}

fn lit_label(lit: &LitKind) -> String {
    match lit {
        LitKind::Dec(v) => v.to_string(),
        LitKind::Hex(v) => format!("0x{v:x}"),
        LitKind::Bin(v) => format!("0b{v:b}"),
        LitKind::Sized(w, v) => format!("{w}'d{v}"),
        LitKind::ParamSized(name, v) => format!("{name}'d{v}"),
        LitKind::Float(bits) => f64::from_bits(*bits).to_string(),
        LitKind::TypedFloat(fmt, bits) => {
            let v = match fmt {
                FloatLitFmt::Fp32 => f32::from_bits(*bits as u32) as f64,
                FloatLitFmt::Bf16 => f32::from_bits((*bits as u32) << 16) as f64,
            };
            v.to_string()
        }
    }
}

fn binop_label(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Lt => "<",
        BinOp::Lte => "<=",
        BinOp::Gt => ">",
        BinOp::Gte => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Implies => "|->",
        BinOp::ImpliesNext => "|=>",
        BinOp::AddWrap => "+%",
        BinOp::SubWrap => "-%",
        BinOp::MulWrap => "*%",
    }
}

fn inside_member_label(member: &InsideMember) -> String {
    match member {
        InsideMember::Single(e) => expr_label(e),
        InsideMember::Range(lo, hi) => format!("{}..{}", expr_label(lo), expr_label(hi)),
    }
}

fn for_range_label(range: &ForRange) -> String {
    match range {
        ForRange::Range(start, end) => format!("{}..{}", expr_label(start), expr_label(end)),
        ForRange::ValueList(values) => values.iter().map(expr_label).collect::<Vec<_>>().join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Ident};

    fn dec(value: u64) -> Expr {
        Expr::new(ExprKind::Literal(LitKind::Dec(value)), Span::new(0, 1))
    }

    fn ident(name: &str) -> Expr {
        Expr::new(ExprKind::Ident(name.to_string()), Span::new(0, 1))
    }

    fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::new(
            ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
            Span::new(0, 1),
        )
    }

    fn method(base: Expr, name: &str, args: Vec<Expr>) -> Expr {
        Expr::new(
            ExprKind::MethodCall(
                Box::new(base),
                Ident::new(name.to_string(), Span::new(0, 1)),
                args,
            ),
            Span::new(0, 1),
        )
    }

    #[test]
    fn nat_expr_preserves_literal_through_width_method_call() {
        assert_eq!(
            nat_expr(&method(dec(3), "resize", vec![dec(2)])),
            ThreadMapNatExpr::Const(3)
        );
        assert_eq!(
            nat_expr(&method(dec(7), "trunc", vec![dec(3)])),
            ThreadMapNatExpr::Const(7)
        );
    }

    #[test]
    fn guard_expr_preserves_equality_as_structured_nat_comparison() {
        assert_eq!(
            guard_expr(&binary(BinOp::Eq, ident("idx"), dec(3))),
            ThreadMapGuardExpr::Eq(
                ThreadMapNatExpr::Var("idx".to_string()),
                ThreadMapNatExpr::Const(3)
            )
        );
        assert_eq!(
            guard_expr(&binary(BinOp::Neq, ident("idx"), dec(3))),
            ThreadMapGuardExpr::Ne(
                ThreadMapNatExpr::Var("idx".to_string()),
                ThreadMapNatExpr::Const(3)
            )
        );
    }
}
