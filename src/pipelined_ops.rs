//! Pipelined-operator implementation registry.
//!
//! Phase 1 of `doc/proposal_pipelined_operators.md` (APPROVED 2026-07-12):
//! the registry table, a type-check-facing lookup with an enumerated miss
//! error, and the data behind the `arch ops` CLI listing.
//!
//! Phase 2 (this module, additionally) wires up the `fma<pipelined, N>`
//! call surface, parsed as `ast::ExprKind::PipelinedCall`, and its latency
//! typing (see `typecheck.rs`'s `PipelinedCall` handling).
//!
//! Phase 3 (this module, additionally) binds `builtin:fma_f32_s6` to a
//! real codegen shape: **the existing verified combinational sticky-fold
//! FMA (`src/fp_ops.rs`, called the same way bare `fma(a,b,c)` is) feeding
//! the N-deep `pipe_reg` register chain the call is bound to.** The
//! characterized "6-stage, ~260 MHz" datapath in the registry notes is
//! this exact shape — comb cone + N output register stages — run through
//! Yosys/abc retiming (`buffer -N 8; upsize; dnsize`); the compiler does
//! not need to hand-split the datapath because retiming does that job
//! downstream. Concretely: `lower_pipelined_calls` (below) rewrites every
//! `PipelinedCall(op, args, N)` reaching codegen into a plain
//! `FunctionCall(op, args)` — the `elaborate::lower_pipe_reg_ports` cascade
//! rewrite (which runs *before* typecheck) has already turned
//! `acc@N <= op<pipelined, N>(args)` into `acc_stg1 <= op<pipelined, N>(args); acc_stg2 <= acc_stg1; ...`,
//! so once the call itself collapses to the ordinary comb form, the
//! existing pipe_reg register-cascade codegen (shared by `arch build` and
//! `arch sim` — both already emit N flops per pipe_reg port) does the
//! rest, with **no bespoke staged-datapath codegen required**. Sequential
//! equivalence to the comb operator is therefore true *by construction*:
//! the retimed datapath is a pure N-cycle delay of a value computed by the
//! same trusted comb IR node, not an independent hand-written pipeline
//! that could diverge from it.
//!
//! `lower_pipelined_calls` only performs this rewrite for registry entries
//! whose `codegen_impl` is `Some(_)`. Entries with `codegen_impl: None`
//! (future registry rows added ahead of their codegen support landing)
//! still hit the same "typechecks but codegen is not yet implemented"
//! error `arch build` / `arch sim` used to raise unconditionally — so a
//! future un-wired row fails loudly instead of silently falling back to
//! an un-retimed comb cone. `arch check` never calls this pass — typecheck
//! alone is fully supported for any registered row regardless of codegen
//! status.
//!
//! Phase 3.5 (proposal §4) adds a SECOND emission form: measured data
//! showed Yosys/ABC does **not** retime the comb+cascade shape (~113 MHz
//! vs. ~260 MHz hand-staged, same buffered Nangate45 flow — flops never
//! move), so `arch build --staged-ops` emits registry entries carrying a
//! [`StagedSchedule`] as a hand-staged datapath module instead
//! (`fp_ir::render_sv_staged`), with the binding site's cascade regs
//! rewritten into a 1-bit validity chain that keeps reset/warm-up behavior
//! cycle-exact with the cascade (see `lower_staged_sites`). The cascade
//! remains the default — it is retime-friendly RTL for flows with real
//! sequential retiming (commercial synthesis), and the form `arch sim`
//! always runs. The staged datapath is a genuine second implementation, so
//! its equivalence obligation is discharged by the randomized lock-step
//! regression (`tests/pipelined_fma_lockstep_test.rs`), not by
//! construction.
//!
//! The registry key is `(operator, profile, stages)`. Entries carry a
//! verification `status`: `verified` means the staged IR has been proven
//! sequentially equivalent to the trusted combinational operator — true by
//! construction for `builtin:fma_f32_s6` per the paragraph above, since
//! the "staged IR" *is* the comb IR plus registers, not a separate
//! datapath; `unverified` entries (added by future `.archpipe` loading,
//! phase 4) are usable only with an explicit opt-in.

use std::fmt;

/// A staged schedule for an entry: the assignment of the operator's
/// linearized IR temps (`fp_ir::linearize` order) to pipeline stages —
/// phase 3.5 of `doc/proposal_pipelined_operators.md` (§4 "Emission forms").
///
/// The schedule is expressed as per-namespace stage-start cut points over
/// contiguous temp ranges: stage `k` (1-based, `1..=stages`) owns main
/// temps `main_starts[k-1] .. main_starts[k]` and inlined-callee temps
/// `callee_starts[k-1] .. callee_starts[k]` (half-open). The builtin FMA
/// schedule was extracted from the externally characterized hand-staged
/// run (259.8 MHz, Nangate45 typ., buffered abc — the `sh6` strategy of
/// the depth sweep), whose temp numbering was verified zero-drift against
/// the current linearization (main result `_t308`, nested call at `_t44`,
/// callee result `_t174`).
///
/// `callee` names the single nested `Kind::Call` whose body is inlined
/// under the `A_` namespace (for FMA: `arch_f32_add`, the prod-zero path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedSchedule {
    /// The IR function being staged (e.g. `"arch_fma_f32"`).
    pub main_fn: &'static str,
    /// Nested call inlined as the `A_` namespace (e.g. `"arch_f32_add"`).
    pub callee: &'static str,
    /// SV module name for the emitted staged datapath.
    pub sv_module: &'static str,
    /// Result width in bits (the staged module's `y` port).
    pub width: u32,
    /// `stages + 1` cut points over the main function's temp ids;
    /// `main_starts[0] == 0`, `main_starts[stages] ==` total main temps.
    pub main_starts: &'static [usize],
    /// `stages + 1` cut points over the inlined callee's temp ids.
    pub callee_starts: &'static [usize],
}

impl StagedSchedule {
    /// Stage (1-based) that computes main temp `id`.
    pub fn main_stage(&self, id: usize) -> u32 {
        Self::stage_of(self.main_starts, id)
    }
    /// Stage (1-based) that computes callee (`A_`) temp `id`.
    pub fn callee_stage(&self, id: usize) -> u32 {
        Self::stage_of(self.callee_starts, id)
    }
    fn stage_of(starts: &[usize], id: usize) -> u32 {
        debug_assert!(id < *starts.last().unwrap(), "temp id beyond schedule");
        // `starts` is tiny (stages + 1 entries); a linear scan is obviously
        // right and the arrays are monotone by construction (unit-tested).
        for k in 1..starts.len() {
            if id < starts[k] {
                return k as u32;
            }
        }
        unreachable!("temp id {id} beyond schedule range")
    }
    /// Declared stage count (`stages` in the registry row).
    pub fn stages(&self) -> u32 {
        (self.main_starts.len() - 1) as u32
    }
}

/// The builtin 6-stage FMA schedule (see [`StagedSchedule`] docs for
/// provenance). Stage ownership:
///
/// | stage | main `arch_fma_f32` temps | inlined `arch_f32_add` temps |
/// |---|---|---|
/// | 1 | t0..t43    | A_t0..A_t81    |
/// | 2 | —          | A_t82..A_t102  |
/// | 3 | —          | A_t103..A_t161 |
/// | 4 | t44..t230  | A_t162..A_t174 |
/// | 5 | t231..t272 | —              |
/// | 6 | t273..t308 | —              |
pub const FMA_F32_S6_SCHEDULE: StagedSchedule = StagedSchedule {
    main_fn: "arch_fma_f32",
    callee: "arch_f32_add",
    sv_module: "ArchF32FmaStaged6",
    width: 32,
    main_starts: &[0, 44, 44, 44, 231, 273, 309],
    callee_starts: &[0, 82, 103, 162, 175, 175, 175],
};

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
    /// Codegen binding: the name of the plain combinational operator this
    /// entry retimes (e.g. `Some("fma")` — codegen calls it exactly the
    /// way bare `fma(a, b, c)` is called, then relies on the surrounding
    /// `pipe_reg` cascade for the N register stages). `None` means the
    /// entry has typecheck support (via `lookup`) but no codegen binding
    /// yet — `arch build` / `arch sim` refuse with a "not yet implemented"
    /// error rather than silently emitting an un-retimed comb cone.
    pub codegen_impl: Option<&'static str>,
    /// Staged emission schedule (phase 3.5): `Some(_)` when the entry can
    /// be emitted as a hand-staged datapath under `arch build --staged-ops`.
    /// `None` → `--staged-ops` falls back to the cascade form with a
    /// warning (never an error).
    pub staged_schedule: Option<StagedSchedule>,
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
    fmax_ng45_typ: Some("~260 MHz (external run — see notes)"),
    impl_id: "builtin:fma_f32_s6",
    notes: Some(
        "sticky-fold FMA; EXTERNAL Nangate45 (typ.) Yosys+OpenSTA+Liberty \
         characterization, buffered abc flow (buffer -N 8; upsize; dnsize) — \
         not reproducible by this repo's checked-in flow (no Liberty/OpenSTA \
         in the dev/CI sandbox); 6-stage is the characterized knee vs. 7/10 \
         stages. TWO emission forms (proposal §4): the default comb+cascade \
         (retime-friendly RTL; also what `arch sim` runs) measures ~113 MHz \
         on Yosys/ABC, which does NOT retime it (flops never move); `arch \
         build --staged-ops` emits the hand-staged datapath this row's \
         ~260 MHz characterizes. Staged↔cascade equivalence is discharged \
         by the randomized lock-step regression \
         (tests/pipelined_fma_lockstep_test.rs). Reproducible logic-depth \
         proxy (not fmax): tests/fp_v1/synth/run_synth.sh --stages 6 F32Fma \
         (tests/fp_v1/synth/README.md 'Staged/pipelined operators')",
    ),
    codegen_impl: Some("fma"),
    staged_schedule: Some(FMA_F32_S6_SCHEDULE),
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

/// Resolves the codegen binding for `(operator, stages)` — Phase 3. Scans
/// every registry row matching `operator`/`stages` (profile-agnostic: the
/// bound comb function, e.g. `"fma"`, is itself polymorphic over profile
/// the same way bare `fma(a, b, c)` is, so codegen does not need to
/// rediscover the profile typecheck already resolved). Returns:
///
/// - `Ok(fn_name)` if at least one matching row has `codegen_impl: Some(_)`.
/// - `Err(())` if every matching row (there must be at least one — a
///   program reaching this call already passed typecheck's `lookup`) has
///   `codegen_impl: None`: a registered-but-not-yet-implemented row, which
///   must fail loudly rather than silently falling back to an un-retimed
///   comb cone.
fn resolve_codegen_impl(operator: &str, stages: u32) -> Result<&'static str, ()> {
    BUILTIN_REGISTRY
        .iter()
        .filter(|e| e.operator == operator && e.stages == stages)
        .find_map(|e| e.codegen_impl)
        .ok_or(())
}

/// Phase-3 codegen-facing lowering: rewrites every `PipelinedCall(op, args, N)`
/// reachable from a module's `comb`/`seq`/`latch` blocks or `let` bindings
/// into the plain `FunctionCall(op, args)` the existing comb-operator
/// codegen (`arch build` and `arch sim` alike) already fully supports.
///
/// Must run **after** typecheck (which validated the `(operator, profile,
/// stages)` registry lookup and all the latency-alignment / binding rules)
/// and **after** `elaborate::lower_pipe_reg_ports` (which already turned
/// `acc@N <= op<pipelined, N>(args)` into the register cascade
/// `acc_stg1 <= op<pipelined, N>(args); acc_stg2 <= acc_stg1; ...; acc <= acc_stg{N-1};`
/// — so by the time this pass runs, every remaining `PipelinedCall` sits as
/// the direct RHS of the first cascade stage, and stripping it down to the
/// bare comb call is sufficient: the surrounding N-deep register chain
/// (already emitted by ordinary pipe_reg codegen) supplies the retiming.
///
/// Returns the first registry-lacks-codegen error encountered (mirroring
/// the Phase 2 `reject_pipelined_calls_before_codegen` error text) so
/// `arch build` / `arch sim` can still refuse loudly for any future
/// registry row added ahead of its codegen support landing. `arch check`
/// must not call this — it only cares about typecheck's `lookup`, not
/// codegen availability.
pub fn lower_pipelined_calls(
    source: &mut crate::ast::SourceFile,
) -> Result<(), FoundPipelinedCall> {
    lower_pipelined_calls_mode(
        source,
        PipelinedEmission::Cascade,
        crate::FpCompat::default(),
    )
    .map(|_| ())
}

/// Emission form selector for `lower_pipelined_calls_mode` — proposal §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelinedEmission {
    /// Phase-3 default: comb call + `pipe_reg` register cascade.
    Cascade,
    /// Phase 3.5 (`arch build --staged-ops`): registry entries WITH a
    /// staged schedule emit the hand-staged datapath module; entries
    /// without one fall back to the cascade with a warning.
    Staged,
}

/// One staged call site after `PipelinedEmission::Staged` lowering: codegen
/// emits `sv_text` once per `sv_module` at `$unit` scope, and an instance +
/// result wire inside `module_name`. The site's cascade registers were
/// rewritten into a 1-bit validity chain (see `lower_staged_sites`), so the
/// binding port's reset/warm-up behavior stays cycle-exact with the cascade
/// emission.
#[derive(Debug, Clone)]
pub struct StagedSite {
    /// ARCH module containing the call site.
    pub module_name: String,
    /// Staged SV module (e.g. `ArchF32FmaStaged6`).
    pub sv_module: &'static str,
    /// Rendered staged module text (drift-verified at lowering time).
    pub sv_text: String,
    /// Instance name, unique per module (e.g. `__staged_fma_0`).
    pub instance: String,
    /// Result wire name (e.g. `__staged_fma_0_y`).
    pub wire: String,
    /// Result width in bits.
    pub width: u32,
    /// Clock signal driving the seq block the call was bound in.
    pub clk: String,
    /// The staged module's input port names (the IR function's params),
    /// paired positionally with `args`.
    pub ports: Vec<String>,
    /// Typechecked argument expressions, connected positionally to `ports`.
    pub args: Vec<crate::ast::Expr>,
}

/// A `--staged-ops` site that fell back to the cascade emission, with the
/// reason (no schedule, unsupported shape, renderer drift). Surfaced as a
/// warning — never an error.
#[derive(Debug, Clone)]
pub struct StagedFallback {
    pub operator: String,
    pub stages: u32,
    pub reason: String,
    pub span: crate::lexer::Span,
}

/// Outcome of `lower_pipelined_calls_mode`.
#[derive(Debug, Default)]
pub struct PipelinedLowering {
    /// Staged sites for codegen (empty in `Cascade` mode).
    pub staged_sites: Vec<StagedSite>,
    /// Cascade fallbacks under `Staged` mode (advisory warnings).
    pub fallbacks: Vec<StagedFallback>,
}

pub fn lower_pipelined_calls_mode(
    source: &mut crate::ast::SourceFile,
    mode: PipelinedEmission,
    profile: crate::FpCompat,
) -> Result<PipelinedLowering, FoundPipelinedCall> {
    use crate::ast::{Item, ModuleBodyItem};
    let mut out = PipelinedLowering::default();
    // Staged pass first: rewrites eligible sites in place and removes their
    // `PipelinedCall` nodes; anything left standing (fallbacks, cascade
    // mode) is collapsed to the comb call below, exactly as in phase 3.
    if mode == PipelinedEmission::Staged {
        for item in &mut source.items {
            if let Item::Module(m) = item {
                lower_staged_sites(m, profile, &mut out);
            }
        }
    }
    for item in &mut source.items {
        if let Item::Module(m) = item {
            for bi in &mut m.body {
                match bi {
                    ModuleBodyItem::CombBlock(cb) => lower_stmts(&mut cb.stmts)?,
                    ModuleBodyItem::RegBlock(rb) => lower_stmts(&mut rb.stmts)?,
                    ModuleBodyItem::LatchBlock(lb) => lower_stmts(&mut lb.stmts)?,
                    ModuleBodyItem::LetBinding(l) => lower_expr(&mut l.value)?,
                    _ => {}
                }
            }
        }
    }
    Ok(out)
}

/// Rewrites every eligible `X_stg1 <= op<pipelined, N>(args)` site of one
/// module into the staged-instance form:
///
/// - the `N-1` synthesized cascade regs `X_stg1..X_stg{N-1}` become a 1-bit
///   **validity chain** (same reset, value `false`) — reusing the ordinary
///   reg/reset codegen for the post-reset warm-up gate;
/// - `X_stg1 <= <call>` becomes `X_stg1 <= true`;
/// - `X <= X_stg{N-1}` becomes `X <= X_stg{N-1} ? <wire> : <reset value>`;
/// - the staged module text is rendered (and drift-verified) here, and the
///   instance/wire recorded for codegen.
///
/// Timing note: the staged module holds register layers 1..N-1 internally
/// and a combinational final stage; the port's own output register supplies
/// edge N — total latency is exactly N edges, matching the cascade, and the
/// validity chain reproduces the cascade's N-1 reset-value warm-up cycles.
///
/// Ineligible sites (falling-edge clock, conditional/nested assignment,
/// missing schedule, renderer drift) are left untouched → the cascade
/// collapse handles them; a `StagedFallback` records why.
fn lower_staged_sites(
    m: &mut crate::ast::ModuleDecl,
    profile: crate::FpCompat,
    out: &mut PipelinedLowering,
) {
    use crate::ast::{ClockEdge, Expr, ExprKind, ModuleBodyItem, RegReset, Stmt, TypeExpr};

    // ── collect candidate sites: top-level `X_stg1 <= PipelinedCall` ──
    struct Cand {
        base: String,
        operator: String,
        stages: u32,
        span: crate::lexer::Span,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for bi in &m.body {
        let ModuleBodyItem::RegBlock(rb) = bi else {
            continue;
        };
        for s in &rb.stmts {
            let Stmt::Assign(a) = s else { continue };
            let ExprKind::PipelinedCall(op, _, n) = &a.value.kind else {
                continue;
            };
            let ExprKind::Ident(t) = &a.target.kind else {
                continue;
            };
            if let Some(base) = t.strip_suffix("_stg1") {
                if rb.clock_edge == ClockEdge::Falling {
                    out.fallbacks.push(StagedFallback {
                        operator: op.clone(),
                        stages: *n,
                        reason: "falling-edge clock (staged emission supports rising only)"
                            .to_string(),
                        span: a.span,
                    });
                    continue;
                }
                cands.push(Cand {
                    base: base.to_owned(),
                    operator: op.clone(),
                    stages: *n,
                    span: a.span,
                });
            }
        }
        // Nested (conditional) PipelinedCalls: report the fallback, leave
        // them for the cascade collapse.
        let mut nested = Vec::new();
        for s in &rb.stmts {
            if let Stmt::Assign(a) = s {
                if matches!(a.value.kind, ExprKind::PipelinedCall(..))
                    && matches!(a.target.kind, ExprKind::Ident(_))
                {
                    continue; // top-level candidate handled above
                }
            }
            scan_stmts(std::slice::from_ref(s), &mut nested);
        }
        for f in nested {
            out.fallbacks.push(StagedFallback {
                operator: f.operator,
                stages: f.stages,
                reason: "pipelined call is conditional or nested (staged emission requires an \
                         unconditional top-level binding)"
                    .to_string(),
                span: f.span,
            });
        }
    }

    let fp_funcs = crate::fp_ops::fp_functions(profile);
    for cand in cands {
        // Registry row with a schedule?
        let sched = match BUILTIN_REGISTRY
            .iter()
            .find(|e| e.operator == cand.operator && e.stages == cand.stages)
            .and_then(|e| e.staged_schedule)
        {
            Some(s) => s,
            None => {
                out.fallbacks.push(StagedFallback {
                    operator: cand.operator,
                    stages: cand.stages,
                    reason: "registry entry has no staged schedule".to_string(),
                    span: cand.span,
                });
                continue;
            }
        };
        // Render + drift-verify NOW so a failed site can still fall back.
        let main_fn = fp_funcs.iter().find(|f| f.name == sched.main_fn);
        let callee_fn = fp_funcs.iter().find(|f| f.name == sched.callee);
        let (Some(main_fn), Some(callee_fn)) = (main_fn, callee_fn) else {
            out.fallbacks.push(StagedFallback {
                operator: cand.operator,
                stages: cand.stages,
                reason: format!(
                    "IR functions `{}`/`{}` not found",
                    sched.main_fn, sched.callee
                ),
                span: cand.span,
            });
            continue;
        };
        let sv_text =
            match crate::fp_ir::render_sv_staged(main_fn, callee_fn, &sched, sched.sv_module) {
                Ok(t) => t,
                Err(e) => {
                    out.fallbacks.push(StagedFallback {
                        operator: cand.operator,
                        stages: cand.stages,
                        reason: format!("staged renderer rejected the schedule: {e}"),
                        span: cand.span,
                    });
                    continue;
                }
            };

        // The port's declared reset value (cold-pipe output during warm-up).
        let port_reset_val: Option<Expr> = m
            .ports
            .iter()
            .find(|p| p.name.name == cand.base)
            .and_then(|p| p.reg_info.as_ref())
            .and_then(|ri| match &ri.reset {
                RegReset::None => None,
                RegReset::Inherit(_, v) | RegReset::Explicit(_, _, _, v) => Some(v.clone()),
            });

        let idx = out
            .staged_sites
            .iter()
            .filter(|s| s.module_name == m.name.name)
            .count();
        let instance = format!("__staged_{}_{}", cand.operator, idx);
        let wire = format!("{instance}_y");
        let span = cand.span;
        let mk = |kind: ExprKind| Expr {
            kind,
            span,
            parenthesized: false,
        };

        // ── mutate the cascade group in place ──
        let last_stg = format!("{}_stg{}", cand.base, cand.stages - 1);
        let mut clk: Option<String> = None;
        let mut call_args: Option<Vec<Expr>> = None;
        let mut rewrote_last = false;
        for bi in &mut m.body {
            match bi {
                ModuleBodyItem::RegDecl(r)
                    if r.name
                        .name
                        .strip_prefix(&format!("{}_stg", cand.base))
                        .is_some_and(|k| {
                            k.parse::<u32>().is_ok_and(|k| k >= 1 && k < cand.stages)
                        }) =>
                {
                    // 32-bit data stage → 1-bit validity stage, same reset
                    // signal/kind/polarity, value `false`.
                    r.ty = TypeExpr::Bool;
                    match &mut r.reset {
                        RegReset::None => {}
                        RegReset::Inherit(_, v) | RegReset::Explicit(_, _, _, v) => {
                            *v = mk(ExprKind::Bool(false));
                        }
                    }
                    if r.init.is_some() {
                        r.init = Some(mk(ExprKind::Bool(false)));
                    }
                }
                ModuleBodyItem::RegBlock(rb) => {
                    for s in &mut rb.stmts {
                        let Stmt::Assign(a) = s else { continue };
                        let ExprKind::Ident(t) = &a.target.kind else {
                            continue;
                        };
                        if *t == format!("{}_stg1", cand.base)
                            && matches!(a.value.kind, ExprKind::PipelinedCall(..))
                        {
                            let old = std::mem::replace(&mut a.value, mk(ExprKind::Bool(true)));
                            let ExprKind::PipelinedCall(_, args, _) = old.kind else {
                                unreachable!("matched PipelinedCall above")
                            };
                            call_args = Some(args);
                            clk = Some(rb.clock.name.clone());
                        } else if *t == cand.base {
                            if let ExprKind::Ident(src) = &a.value.kind {
                                if *src == last_stg {
                                    let fallback = port_reset_val
                                        .clone()
                                        .unwrap_or_else(|| mk(ExprKind::Bool(false)));
                                    a.value = mk(ExprKind::Ternary(
                                        Box::new(mk(ExprKind::Ident(src.clone()))),
                                        Box::new(mk(ExprKind::Ident(wire.clone()))),
                                        Box::new(fallback),
                                    ));
                                    rewrote_last = true;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let (Some(args), Some(clk), true) = (call_args, clk, rewrote_last) else {
            // Shape mismatch — should not happen for the elaborate-generated
            // cascade. The site may be partially rewritten; that is still
            // sound only if BOTH halves happened, so surface loudly.
            out.fallbacks.push(StagedFallback {
                operator: cand.operator.clone(),
                stages: cand.stages,
                reason: "cascade shape not recognized (staged rewrite incomplete — please \
                         report; falling back may leave a stale validity chain)"
                    .to_string(),
                span: cand.span,
            });
            continue;
        };
        if args.len() != main_fn.params.len() {
            out.fallbacks.push(StagedFallback {
                operator: cand.operator.clone(),
                stages: cand.stages,
                reason: "call arity != staged IR function arity".to_string(),
                span: cand.span,
            });
            continue;
        }
        out.staged_sites.push(StagedSite {
            module_name: m.name.name.clone(),
            sv_module: sched.sv_module,
            sv_text,
            instance,
            wire,
            width: sched.width,
            clk,
            ports: main_fn.params.iter().map(|(n, _)| n.clone()).collect(),
            args,
        });
    }
}

fn lower_stmts(stmts: &mut [crate::ast::Stmt]) -> Result<(), FoundPipelinedCall> {
    for s in stmts {
        lower_stmt(s)?;
    }
    Ok(())
}

fn lower_stmt(stmt: &mut crate::ast::Stmt) -> Result<(), FoundPipelinedCall> {
    use crate::ast::Stmt;
    match stmt {
        Stmt::Assign(a) => lower_expr(&mut a.value)?,
        Stmt::IfElse(ie) => {
            lower_expr(&mut ie.cond)?;
            lower_stmts(&mut ie.then_stmts)?;
            lower_stmts(&mut ie.else_stmts)?;
        }
        Stmt::Match(m) => {
            lower_expr(&mut m.scrutinee)?;
            for arm in &mut m.arms {
                lower_stmts(&mut arm.body)?;
            }
        }
        Stmt::Log(l) => {
            for a in &mut l.args {
                lower_expr(a)?;
            }
        }
        Stmt::For(f) => lower_stmts(&mut f.body)?,
        Stmt::Init(i) => lower_stmts(&mut i.body)?,
        Stmt::WaitUntil(e, _) => lower_expr(e)?,
        Stmt::DoUntil { body, cond, .. } => {
            lower_stmts(body)?;
            lower_expr(cond)?;
        }
    }
    Ok(())
}

fn lower_expr(expr: &mut crate::ast::Expr) -> Result<(), FoundPipelinedCall> {
    use crate::ast::ExprKind::*;
    // First recurse into children so a `PipelinedCall` nested inside a
    // larger expression (not just the direct RHS of a cascade stage) is
    // also lowered — matches `scan_expr`'s traversal shape.
    match &mut expr.kind {
        Binary(_, a, b) => {
            lower_expr(a)?;
            lower_expr(b)?;
        }
        Unary(_, e)
        | Cast(e, _)
        | LatencyAt(e, _)
        | SvaNext(_, e)
        | Signed(e)
        | Unsigned(e)
        | Clog2(e)
        | Onehot(e)
        | Repeat(e, _) => lower_expr(e)?,
        FieldAccess(e, _) => lower_expr(e)?,
        MethodCall(recv, _, args) => {
            lower_expr(recv)?;
            for a in args {
                lower_expr(a)?;
            }
        }
        Index(base, idx) => {
            lower_expr(base)?;
            lower_expr(idx)?;
        }
        BitSlice(base, hi, lo) => {
            lower_expr(base)?;
            lower_expr(hi)?;
            lower_expr(lo)?;
        }
        PartSelect(base, start, width, _) => {
            lower_expr(base)?;
            lower_expr(start)?;
            lower_expr(width)?;
        }
        StructLiteral(_, fields) => {
            for f in fields {
                lower_expr(&mut f.value)?;
            }
        }
        Match(scrut, arms) => {
            lower_expr(scrut)?;
            for arm in arms {
                lower_stmts(&mut arm.body)?;
            }
        }
        ExprMatch(scrut, arms) => {
            lower_expr(scrut)?;
            for arm in arms {
                lower_expr(&mut arm.value)?;
            }
        }
        Concat(xs) | FunctionCall(_, xs) => {
            for x in xs {
                lower_expr(x)?;
            }
        }
        Inside(e, members) => {
            lower_expr(e)?;
            for m in members {
                match m {
                    crate::ast::InsideMember::Single(v) => lower_expr(v)?,
                    crate::ast::InsideMember::Range(lo, hi) => {
                        lower_expr(lo)?;
                        lower_expr(hi)?;
                    }
                }
            }
        }
        Ternary(c, t, e) => {
            lower_expr(c)?;
            lower_expr(t)?;
            lower_expr(e)?;
        }
        PipelinedCall(_, args, _) => {
            for a in args {
                lower_expr(a)?;
            }
        }
        Literal(_) | Ident(_) | SynthIdent(_, _) | EnumVariant(_, _) | Todo | Bool(_) => {}
    }
    // Now lower this node itself, if it is a PipelinedCall.
    if let PipelinedCall(name, _, stages) = &expr.kind {
        let (name, stages) = (name.clone(), *stages);
        match resolve_codegen_impl(&name, stages) {
            Ok(fn_name) => {
                // Replace `PipelinedCall(name, args, stages)` with the
                // equivalent bare `FunctionCall(name, args)` — see the
                // module doc comment: the N register stages are supplied
                // by the surrounding pipe_reg cascade, not by this node.
                let old = std::mem::replace(&mut expr.kind, Todo);
                let PipelinedCall(_, args, _) = old else {
                    unreachable!("matched PipelinedCall above")
                };
                expr.kind = FunctionCall(fn_name.to_string(), args);
            }
            Err(()) => {
                return Err(FoundPipelinedCall {
                    operator: name,
                    stages,
                    span: expr.span,
                });
            }
        }
    }
    Ok(())
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
    fn fma_s6_schedule_is_monotone_and_covers_both_namespaces_exactly() {
        let s = FMA_F32_S6_SCHEDULE;
        assert_eq!(s.stages(), 6);
        assert_eq!(s.callee, "arch_f32_add");
        for starts in [s.main_starts, s.callee_starts] {
            assert_eq!(starts.len(), 7);
            assert_eq!(starts[0], 0);
            assert!(starts.windows(2).all(|w| w[0] <= w[1]), "monotone cuts");
        }
        // Exact coverage of the verified linearization: main t0..t308
        // (result _t308), callee A_t0..A_t174 (result _t174) — no gaps or
        // overlaps by construction of half-open contiguous ranges.
        assert_eq!(*s.main_starts.last().unwrap(), 309);
        assert_eq!(*s.callee_starts.last().unwrap(), 175);
        // Spot-check stage ownership at the boundaries of the extracted table.
        assert_eq!(s.main_stage(0), 1);
        assert_eq!(s.main_stage(43), 1);
        assert_eq!(s.main_stage(44), 4);
        assert_eq!(s.main_stage(230), 4);
        assert_eq!(s.main_stage(231), 5);
        assert_eq!(s.main_stage(272), 5);
        assert_eq!(s.main_stage(273), 6);
        assert_eq!(s.main_stage(308), 6);
        assert_eq!(s.callee_stage(0), 1);
        assert_eq!(s.callee_stage(81), 1);
        assert_eq!(s.callee_stage(82), 2);
        assert_eq!(s.callee_stage(102), 2);
        assert_eq!(s.callee_stage(103), 3);
        assert_eq!(s.callee_stage(161), 3);
        assert_eq!(s.callee_stage(162), 4);
        assert_eq!(s.callee_stage(174), 4);
    }

    #[test]
    fn markdown_table_contains_generated_marker_and_row() {
        let m = format_markdown_table();
        assert!(m.starts_with("<!-- GENERATED FILE. DO NOT EDIT BY HAND."));
        assert!(m.contains("| `fma` | FP32 | 6 | verified |"));
    }
}
