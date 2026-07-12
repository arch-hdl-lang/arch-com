//! Pipelined-operator implementation registry.
//!
//! Phase 1 of `doc/proposal_pipelined_operators.md` (APPROVED 2026-07-12):
//! the registry table, a type-check-facing lookup with an enumerated miss
//! error, and the data behind the `arch ops` CLI listing.
//!
//! Phase 2 (this module, additionally) wires up the `fma<pipelined, N>`
//! call surface, parsed as `ast::ExprKind::PipelinedCall`, and its latency
//! typing (see `typecheck.rs`'s `PipelinedCall` handling). Codegen —
//! binding the call to an actual retimed staged datapath in `arch build`
//! / `arch sim` — is deferred: `builtin:fma_f32_s6` today is a single
//! combinational cone (`src/fp_ops.rs`), not staged RTL; "6-stage" is
//! purely a downstream Yosys synthesis-retiming characterization the
//! compiler never sees. Productizing a real staged schedule with an
//! equivalence proof is proposal phase 3. `find_pipelined_calls` below is
//! the codegen-side backstop: `arch build` / `arch sim` scan the
//! elaborated module bodies for any `PipelinedCall` and refuse with a
//! clear "not yet implemented" error rather than silently falling back to
//! comb + delay-line (which would be functionally correct but would
//! misrepresent an un-retimed cone as the requested pipelined operator).
//! `arch check` does not run this scan — typecheck alone is fully
//! supported.
//!
//! The registry key is `(operator, profile, stages)`. Entries carry a
//! verification `status`: `verified` means the staged IR has been proven
//! sequentially equivalent to the trusted combinational operator (see
//! phase 3 of the proposal for wiring the FMA proof obligation);
//! `unverified` entries (added by future `.archpipe` loading, phase 4) are
//! usable only with an explicit opt-in.

use std::fmt;

/// Verification status of a registry entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifyStatus {
    /// Proven sequentially equivalent to the trusted comb operator.
    Verified,
    /// Not (yet) proven; usable only behind an explicit opt-in (phase 4).
    Unverified,
}

impl VerifyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyStatus::Verified => "verified",
            VerifyStatus::Unverified => "unverified",
        }
    }
}

impl fmt::Display for VerifyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the pipelined-implementation registry: a fully staged
/// implementation of `operator` for a given type `profile` at a fixed
/// `stages` depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelinedOpEntry {
    /// Operator name, e.g. `"fma"`.
    pub operator: &'static str,
    /// Type profile, e.g. `"FP32"`, `"BF16"`.
    pub profile: &'static str,
    /// Declared pipeline depth (register stages).
    pub stages: u32,
    /// Verification status — see [`VerifyStatus`].
    pub status: VerifyStatus,
    /// Characterized fmax (Nangate45, typical corner), advisory only.
    /// `None` when not yet characterized.
    pub fmax_ng45_typ: Option<&'static str>,
    /// Implementation id: `builtin:*` for compiler-owned schedules, or a
    /// path-derived id once `.archpipe` loading (phase 4) lands.
    pub impl_id: &'static str,
    /// Free-text characterization / provenance notes.
    pub notes: Option<&'static str>,
}

/// The compiler-owned builtin registry.
///
/// Seeded per `doc/proposal_pipelined_operators.md` §1 "Initial contents":
/// the FP32 FMA, 6-stage sticky-fold, buffered — the characterized knee of
/// the depth sweep (6/7/10 stages; more stages regress because the residual
/// path is a fine-grained logic-depth cone the registers can't usefully
/// bisect further).
///
/// Phase 5 of the proposal generalizes this to `mul`/`add` and additional
/// depths — that is additive rows here, not new code, by design.
pub const BUILTIN_REGISTRY: &[PipelinedOpEntry] = &[PipelinedOpEntry {
    operator: "fma",
    profile: "FP32",
    stages: 6,
    status: VerifyStatus::Verified,
    fmax_ng45_typ: Some("~260 MHz"),
    impl_id: "builtin:fma_f32_s6",
    notes: Some(
        "sticky-fold FMA, buffered (Yosys abc: buffer -N 8; upsize; dnsize); \
         6-stage is the characterized knee vs. 7/10 stages",
    ),
}];

/// Returns the builtin registry rows, sorted deterministically by
/// `(operator, profile, stages)`.
pub fn registry() -> Vec<PipelinedOpEntry> {
    let mut rows: Vec<PipelinedOpEntry> = BUILTIN_REGISTRY.to_vec();
    rows.sort_by(|a, b| (a.operator, a.profile, a.stages).cmp(&(b.operator, b.profile, b.stages)));
    rows
}

/// A registry lookup miss: `(operator, profile, stages)` has no entry.
/// `available_depths` lists every registered depth for the same
/// `(operator, profile)` pair (may be empty if the profile itself is
/// unknown for this operator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupMiss {
    pub operator: String,
    pub profile: String,
    pub stages: u32,
    pub available_depths: Vec<u32>,
}

impl fmt::Display for LookupMiss {
    /// Matches the enumerated-miss error shape specified in
    /// `doc/proposal_pipelined_operators.md` §1:
    ///
    /// ```text
    /// no pipelined implementation of fma<FP32> with 5 stages
    ///   available depths: {6}      (run `arch ops` to list all)
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let depths = self
            .available_depths
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "no pipelined implementation of {}<{}> with {} stages\n  available depths: {{{}}}      (run `arch ops` to list all)",
            self.operator, self.profile, self.stages, depths
        )
    }
}

impl std::error::Error for LookupMiss {}

/// Looks up `(operator, profile, stages)` in the builtin registry.
///
/// On a miss, returns a [`LookupMiss`] enumerating every depth registered
/// for the same `(operator, profile)` pair, per
/// `doc/proposal_pipelined_operators.md` §1. This is the enforcement
/// mechanism the proposal calls for: "a registry lookup, not an
/// honor-system spec sentence."
///
/// Phase 2 wires this into typecheck resolution of `fma<pipelined, N>`
/// calls; today it is exposed for direct use (tests, `arch advise`
/// fix-pair seeding) ahead of that surface landing.
pub fn lookup(operator: &str, profile: &str, stages: u32) -> Result<PipelinedOpEntry, LookupMiss> {
    if let Some(entry) = BUILTIN_REGISTRY
        .iter()
        .find(|e| e.operator == operator && e.profile == profile && e.stages == stages)
    {
        return Ok(*entry);
    }
    let mut available_depths: Vec<u32> = BUILTIN_REGISTRY
        .iter()
        .filter(|e| e.operator == operator && e.profile == profile)
        .map(|e| e.stages)
        .collect();
    available_depths.sort_unstable();
    available_depths.dedup();
    Err(LookupMiss {
        operator: operator.to_string(),
        profile: profile.to_string(),
        stages,
        available_depths,
    })
}

/// Renders the registry as the plain-text table printed by `arch ops`.
/// Deterministic column order and row order (sorted by
/// `(operator, profile, stages)`).
pub fn format_text_table() -> String {
    let rows = registry();
    let headers = [
        "operator",
        "profile",
        "stages",
        "status",
        "fmax(ng45,typ)",
        "impl",
    ];
    let mut cols: Vec<Vec<String>> = headers.iter().map(|h| vec![h.to_string()]).collect();
    for e in &rows {
        cols[0].push(e.operator.to_string());
        cols[1].push(e.profile.to_string());
        cols[2].push(e.stages.to_string());
        cols[3].push(e.status.as_str().to_string());
        cols[4].push(e.fmax_ng45_typ.unwrap_or("-").to_string());
        cols[5].push(e.impl_id.to_string());
    }
    let widths: Vec<usize> = cols
        .iter()
        .map(|c| c.iter().map(|s| s.len()).max().unwrap_or(0))
        .collect();

    let mut out = String::new();
    let nrows = 1 + rows.len();
    for r in 0..nrows {
        let mut line = String::new();
        for (c, col) in cols.iter().enumerate() {
            let cell = &col[r];
            if c + 1 == cols.len() {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{:width$}  ", cell, width = widths[c]));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
        if r == 0 {
            // notes go on an indented line right under each row
        }
    }
    // Append notes (if any) as indented follow-up lines under each data row,
    // keeping the main table itself strictly tabular.
    if rows.iter().any(|e| e.notes.is_some()) {
        out.push('\n');
        for e in &rows {
            if let Some(notes) = e.notes {
                out.push_str(&format!(
                    "  {}<{}, {}>: {}\n",
                    e.operator, e.profile, e.stages, notes
                ));
            }
        }
    }
    out
}

/// Renders the registry as a markdown table for `doc/generated/pipelined_ops.md`.
/// This is the "documented outside the normative spec" listing called for
/// by `doc/proposal_pipelined_operators.md` §1 point 3.
pub fn format_markdown_table() -> String {
    let rows = registry();
    let mut out = String::new();
    out.push_str("<!-- GENERATED FILE. DO NOT EDIT BY HAND.\n");
    out.push_str("     Regenerate with `arch ops --markdown > doc/generated/pipelined_ops.md`\n");
    out.push_str("     (or `scripts/gen_pipelined_ops_doc.sh`).\n");
    out.push_str("     Source of truth: src/pipelined_ops.rs::BUILTIN_REGISTRY. -->\n\n");
    out.push_str("# Pipelined-operator registry\n\n");
    out.push_str(
        "Generated listing of the compiler's pipelined-operator implementation registry \
         (`doc/proposal_pipelined_operators.md`). This enumerates what `<pipelined, N>` \
         call sites can resolve today; it is intentionally kept out of the normative spec \
         because it churns as implementations are added (phase 5 generalizes beyond `fma`).\n\n",
    );
    out.push_str("| operator | profile | stages | status | fmax (ng45, typ) | impl | notes |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for e in &rows {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` | {} |\n",
            e.operator,
            e.profile,
            e.stages,
            e.status,
            e.fmax_ng45_typ.unwrap_or("-"),
            e.impl_id,
            e.notes.unwrap_or("-"),
        ));
    }
    out
}

/// A `PipelinedCall` found by [`find_pipelined_calls`]: the operator name,
/// declared depth, and source span, for building the deferred-codegen
/// error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundPipelinedCall {
    pub operator: String,
    pub stages: u32,
    pub span: crate::lexer::Span,
}

/// Scans every `module` in `source` for `PipelinedCall` expressions
/// (`fma<pipelined, N>(...)` and friends) reachable from `comb`/`seq`/
/// `latch` block statements. Used by `arch build` / `arch sim` (not
/// `arch check`) to refuse compilation with a clear, explicit error before
/// codegen — see the module doc comment above for why codegen can't yet
/// bind these calls to a real staged datapath.
///
/// Scope note: only module `comb`/`seq`/`latch` blocks are scanned — the
/// only context the proposal's surface syntax and worked example cover in
/// phase 2. Other constructs (`fsm`, `pipeline`, `thread`, ...) don't
/// support the pipelined-operator surface yet.
pub fn find_pipelined_calls(source: &crate::ast::SourceFile) -> Vec<FoundPipelinedCall> {
    use crate::ast::{Item, ModuleBodyItem};
    let mut out = Vec::new();
    for item in &source.items {
        if let Item::Module(m) = item {
            for bi in &m.body {
                match bi {
                    ModuleBodyItem::CombBlock(cb) => scan_stmts(&cb.stmts, &mut out),
                    ModuleBodyItem::RegBlock(rb) => scan_stmts(&rb.stmts, &mut out),
                    ModuleBodyItem::LatchBlock(lb) => scan_stmts(&lb.stmts, &mut out),
                    ModuleBodyItem::LetBinding(l) => scan_expr(&l.value, &mut out),
                    _ => {}
                }
            }
        }
    }
    out
}

fn scan_stmts(stmts: &[crate::ast::Stmt], out: &mut Vec<FoundPipelinedCall>) {
    use crate::ast::Stmt;
    for s in stmts {
        match s {
            Stmt::Assign(a) => scan_expr(&a.value, out),
            Stmt::IfElse(ie) => {
                scan_expr(&ie.cond, out);
                scan_stmts(&ie.then_stmts, out);
                scan_stmts(&ie.else_stmts, out);
            }
            Stmt::Match(m) => {
                scan_expr(&m.scrutinee, out);
                for arm in &m.arms {
                    scan_stmts(&arm.body, out);
                }
            }
            Stmt::Log(l) => {
                for a in &l.args {
                    scan_expr(a, out);
                }
            }
            Stmt::For(f) => scan_stmts(&f.body, out),
            Stmt::Init(i) => scan_stmts(&i.body, out),
            Stmt::WaitUntil(e, _) => scan_expr(e, out),
            Stmt::DoUntil { body, cond, .. } => {
                scan_stmts(body, out);
                scan_expr(cond, out);
            }
        }
    }
}

fn scan_expr(expr: &crate::ast::Expr, out: &mut Vec<FoundPipelinedCall>) {
    use crate::ast::ExprKind::*;
    match &expr.kind {
        PipelinedCall(name, args, stages) => {
            out.push(FoundPipelinedCall {
                operator: name.clone(),
                stages: *stages,
                span: expr.span,
            });
            for a in args {
                scan_expr(a, out);
            }
        }
        Binary(_, a, b) => {
            scan_expr(a, out);
            scan_expr(b, out);
        }
        Unary(_, e)
        | Cast(e, _)
        | LatencyAt(e, _)
        | SvaNext(_, e)
        | Signed(e)
        | Unsigned(e)
        | Clog2(e)
        | Onehot(e)
        | Repeat(e, _) => scan_expr(e, out),
        FieldAccess(e, _) => scan_expr(e, out),
        MethodCall(recv, _, args) => {
            scan_expr(recv, out);
            for a in args {
                scan_expr(a, out);
            }
        }
        Index(base, idx) => {
            scan_expr(base, out);
            scan_expr(idx, out);
        }
        BitSlice(base, hi, lo) => {
            scan_expr(base, out);
            scan_expr(hi, out);
            scan_expr(lo, out);
        }
        PartSelect(base, start, width, _) => {
            scan_expr(base, out);
            scan_expr(start, out);
            scan_expr(width, out);
        }
        StructLiteral(_, fields) => {
            for f in fields {
                scan_expr(&f.value, out);
            }
        }
        Match(scrut, arms) => {
            scan_expr(scrut, out);
            for arm in arms {
                scan_stmts(&arm.body, out);
            }
        }
        ExprMatch(scrut, arms) => {
            scan_expr(scrut, out);
            for arm in arms {
                scan_expr(&arm.value, out);
            }
        }
        Concat(xs) | FunctionCall(_, xs) => {
            for x in xs {
                scan_expr(x, out);
            }
        }
        Inside(e, members) => {
            scan_expr(e, out);
            for m in members {
                match m {
                    crate::ast::InsideMember::Single(v) => scan_expr(v, out),
                    crate::ast::InsideMember::Range(lo, hi) => {
                        scan_expr(lo, out);
                        scan_expr(hi, out);
                    }
                }
            }
        }
        Ternary(c, t, e) => {
            scan_expr(c, out);
            scan_expr(t, out);
            scan_expr(e, out);
        }
        Literal(_) | Ident(_) | SynthIdent(_, _) | EnumVariant(_, _) | Todo | Bool(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_hit_returns_verified_fma_f32_s6() {
        let entry = lookup("fma", "FP32", 6).expect("fma<FP32> stages=6 must be registered");
        assert_eq!(entry.operator, "fma");
        assert_eq!(entry.profile, "FP32");
        assert_eq!(entry.stages, 6);
        assert_eq!(entry.status, VerifyStatus::Verified);
        assert_eq!(entry.impl_id, "builtin:fma_f32_s6");
    }

    #[test]
    fn lookup_miss_enumerates_available_depths_exact_text() {
        let err = lookup("fma", "FP32", 5).unwrap_err();
        assert_eq!(err.available_depths, vec![6]);
        let msg = err.to_string();
        assert_eq!(
            msg,
            "no pipelined implementation of fma<FP32> with 5 stages\n  available depths: {6}      (run `arch ops` to list all)"
        );
    }

    #[test]
    fn lookup_miss_unknown_profile_has_empty_available_depths() {
        let err = lookup("fma", "BF16", 6).unwrap_err();
        assert!(err.available_depths.is_empty());
        assert_eq!(
            err.to_string(),
            "no pipelined implementation of fma<BF16> with 6 stages\n  available depths: {}      (run `arch ops` to list all)"
        );
    }

    #[test]
    fn lookup_miss_unknown_operator() {
        let err = lookup("mul", "FP32", 6).unwrap_err();
        assert!(err.available_depths.is_empty());
        assert_eq!(err.operator, "mul");
    }

    #[test]
    fn registry_is_sorted_and_deterministic() {
        let a = registry();
        let b = registry();
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort_by(|x, y| {
            (x.operator, x.profile, x.stages).cmp(&(y.operator, y.profile, y.stages))
        });
        assert_eq!(a, sorted);
    }

    #[test]
    fn text_table_has_header_and_verified_row() {
        let t = format_text_table();
        let mut lines = t.lines();
        let header = lines.next().unwrap();
        assert!(header.contains("operator"));
        assert!(header.contains("profile"));
        assert!(header.contains("stages"));
        assert!(header.contains("status"));
        let row = lines.next().unwrap();
        assert!(row.contains("fma"));
        assert!(row.contains("FP32"));
        assert!(row.contains("6"));
        assert!(row.contains("verified"));
        assert!(row.contains("builtin:fma_f32_s6"));
    }

    #[test]
    fn markdown_table_contains_generated_marker_and_row() {
        let m = format_markdown_table();
        assert!(m.starts_with("<!-- GENERATED FILE. DO NOT EDIT BY HAND."));
        assert!(m.contains("| `fma` | FP32 | 6 | verified |"));
    }
}
