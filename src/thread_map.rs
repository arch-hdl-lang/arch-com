use crate::ast::{BinOp, Expr, ExprKind, InsideMember, LitKind, Stmt, UnaryOp};
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
    pub span: Span,
    pub states: Vec<ThreadMapState>,
}

#[derive(Debug, Clone)]
pub struct ThreadMapState {
    pub index: usize,
    pub state_name: String,
    pub role: String,
    pub span: Span,
    pub labels: Vec<String>,
    pub transitions: Vec<ThreadMapTransition>,
}

#[derive(Debug, Clone)]
pub struct ThreadMapTransition {
    pub condition: String,
    pub target_index: usize,
    pub target_name: String,
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
.layout { display: grid; grid-template-columns: minmax(360px, 1.1fr) minmax(360px, .9fr); gap: 16px; padding: 16px; align-items: start; }
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
.module { padding: 12px 14px 4px; border-bottom: 1px solid var(--line); }
.module:last-child { border-bottom: 0; }
h3 { margin: 0 0 8px; font-size: 15px; }
h4 { margin: 12px 0 8px; font-size: 13px; color: #34445d; }
table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-bottom: 12px; }
th, td { border-top: 1px solid #edf1f6; padding: 7px 8px; vertical-align: top; text-align: left; overflow-wrap: anywhere; }
th { color: #53647c; font-size: 12px; font-weight: 650; background: #fbfcfe; }
.state-chip { display: inline-block; padding: 2px 6px; border-radius: 5px; font: 11px/16px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-weight: 700; }
.role { color: #5f6f84; font-size: 12px; }
.labels { color: #2d3b50; }
.trans { color: #364860; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.empty { padding: 20px 24px; color: var(--muted); }
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
    out.push_str("</section>\n<section class=\"pane\"><h2>Lowered States</h2>\n");
    for module in &map.modules {
        out.push_str("<div class=\"module\">");
        out.push_str(&format!(
            "<h3>{} <span class=\"role\">→ {}</span></h3>",
            html_escape(&module.module_name),
            html_escape(&module.generated_module_name)
        ));
        for thread in &module.threads {
            out.push_str(&format!(
                "<h4>thread {} <span class=\"role\">index {}</span></h4>",
                html_escape(&thread.name),
                thread.index
            ));
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
        let states = states_overlapping_line(map, src, line_start, line_end);
        out.push_str("<div class=\"src-line\">");
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
) -> Vec<&'a ThreadMapState> {
    let mut states = Vec::new();
    for module in &map.modules {
        if !span_overlaps(module.span, src.start, src.end) {
            continue;
        }
        for thread in &module.threads {
            for state in &thread.states {
                if span_overlaps(state.span, line_start, line_end) {
                    states.push(state);
                }
            }
        }
    }
    states
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
