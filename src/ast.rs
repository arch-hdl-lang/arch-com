use crate::lexer::Span;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub items: Vec<Item>,
    /// Concatenated text of leading `//!` lines at the top of the file
    /// (with the `//! ` prefix stripped per line). `None` when the file
    /// has no leading `//!` block. The frontmatter (delimited by
    /// `//! ---`) is included verbatim in this field for fidelity.
    pub inner_doc: Option<String>,
    /// Raw text of the YAML-style frontmatter block at the top of the
    /// file: the contiguous `//! ---\n…\n//! ---` lines, with `//! `
    /// prefixes stripped. `None` when no frontmatter is present.
    /// Always a substring of `inner_doc` when both are present.
    /// The compiler does not parse the YAML; downstream tooling does.
    pub frontmatter: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Domain(DomainDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Module(ModuleDecl),
    Fsm(FsmDecl),
    Fifo(FifoDecl),
    Ram(RamDecl),
    Cam(CamDecl),
    Counter(CounterDecl),
    Arbiter(ArbiterDecl),
    Regfile(RegfileDecl),
    Pipeline(PipelineDecl),
    Function(FunctionDecl),
    Linklist(LinklistDecl),
    Template(TemplateDecl),
    Synchronizer(SynchronizerDecl),
    Clkgate(ClkGateDecl),
    Bus(BusDecl),
    Package(PackageDecl),
    Use(UseDecl),
}

#[derive(Debug, Clone)]
pub struct DomainDecl {
    pub name: Ident,
    pub fields: Vec<DomainField>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DomainField {
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Ident,
    pub ty: TypeExpr,
}

// ── Bus (port bundle) ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BusDecl {
    pub name: Ident,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
    pub params: Vec<ParamDecl>,
    pub signals: Vec<PortDecl>,  // direction = from initiator's perspective
    pub generates: Vec<BusGenerateIf>,  // conditional signal groups
    /// Handshake sub-constructs declared in this bus. The synthesized
    /// PortDecls already live in `signals` (or inside `generates`); this
    /// list preserves the grouping so codegen can emit per-variant SVA
    /// protocol assertions.
    pub handshakes: Vec<HandshakeMeta>,
    /// Credit channel sub-constructs declared in this bus. PR #3 scaffolding:
    /// parser populates this, but elaboration (counter reg + fifo synthesis,
    /// method dispatch for `ch.send()`/`ch.pop()`/`ch.can_send`) is not yet
    /// implemented. Typecheck rejects any bus port whose bus carries a
    /// credit_channel until the elaboration PR lands.
    pub credit_channels: Vec<CreditChannelMeta>,
    /// TLM method sub-constructs declared in this bus. PR-tlm-1 scaffolding:
    /// parser populates this, but wire flattening + call-site / thread-body
    /// lowering land in follow-up PRs. See doc/plan_tlm_method.md.
    pub tlm_methods: Vec<TlmMethodMeta>,
    pub span: Span,
}

/// Metadata for one `tlm_method` sub-construct inside a bus. PR-tlm-1
/// scaffolding: parser captures the declaration shape; subsequent PRs
/// materialize the req/rsp wires and the FSM lowering. See
/// doc/plan_tlm_method.md.
#[derive(Debug, Clone)]
pub struct TlmMethodMeta {
    /// Method name (e.g. `read`).
    pub name: Ident,
    /// Declared args — each is `(name, type)`, flowing initiator → target
    /// on the request channel. No per-arg direction keyword in v1.
    pub args: Vec<(Ident, TypeExpr)>,
    /// Return type, `None` for void methods (response channel carries
    /// only valid/ready, no payload).
    pub ret: Option<TypeExpr>,
    /// Concurrency mode. `blocking` uses the base req/rsp protocol;
    /// `out_of_order tags N` adds compiler-managed req/rsp tag wires.
    pub mode: Ident,
    pub out_of_order_tags: Option<Expr>,
    pub span: Span,
}

/// Metadata for one `handshake` channel inside a bus. Flattened PortDecls
/// for the control and payload signals already live in BusDecl::signals
/// (or inside a BusGenerateIf branch); this carries the grouping.
#[derive(Debug, Clone)]
pub struct HandshakeMeta {
    /// Channel name (e.g. `aw`).
    pub name: Ident,
    /// Variant keyword (e.g. `valid_ready`, `req_ack_4phase`).
    pub variant: Ident,
    /// True if the declaration used the legacy `handshake` keyword rather
    /// than `handshake_channel`. Typecheck emits a deprecation warning for
    /// the legacy form — semantics are identical. See plan_bus_unification.md.
    pub legacy_handshake_kw: bool,
    /// Role on the initiator side: `Out` = send, `In` = receive.
    pub role_dir: Direction,
    /// Field names of the payload (without the channel prefix). Used only
    /// for documentation in generated SV comments — directions/types are
    /// already materialized as PortDecls in BusDecl::signals.
    pub payload_names: Vec<Ident>,
    pub span: Span,
}

/// Metadata for one `credit_channel` sub-construct inside a bus. PR #3
/// scaffolding: parser stores shape + params, but no PortDecls are
/// materialized yet — the wire protocol (send_valid / send_data /
/// credit_return) and the per-port-site counter + fifo synthesis land in a
/// follow-up PR. See doc/plan_credit_channel.md.
#[derive(Debug, Clone)]
pub struct CreditChannelMeta {
    /// Channel name (e.g. `data`).
    pub name: Ident,
    /// Role on the initiator side: `Out` = send, `In` = receive.
    pub role_dir: Direction,
    /// Params local to this credit_channel (`T`, `DEPTH`). Same ParamDecl
    /// shape as bus-level params; scope is limited to this channel.
    pub params: Vec<ParamDecl>,
    pub span: Span,
}

/// Conditional signal group inside a bus definition.
/// `generate_if COND ... [generate_else ...] end generate_if`
#[derive(Debug, Clone)]
pub struct BusGenerateIf {
    pub cond: Expr,
    pub then_signals: Vec<PortDecl>,
    pub else_signals: Vec<PortDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusPerspective {
    Initiator,
    Target,
}

#[derive(Debug, Clone)]
pub struct BusPortInfo {
    pub bus_name: Ident,
    pub perspective: BusPerspective,
    pub params: Vec<ParamAssign>,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<Ident>,
    /// Optional explicit encoding value per variant (index matches `variants`).
    /// None = auto-assign sequential from 0.
    pub values: Vec<Option<Expr>>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModuleDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub body: Vec<ModuleBodyItem>,
    pub implements: Option<Ident>,
    pub hooks: Vec<ModuleHookDecl>,
    pub cdc_safe: bool,
    /// `pragma rdc_safe;` — suppress all RDC checks (phases 1 + 2a–2d)
    /// for this module. Independent of `cdc_safe`; either pragma alone
    /// disables phase 1's structural cross-clock async-reset rule
    /// (which sits at the CDC/RDC boundary).
    pub rdc_safe: bool,
    pub span: Span,
    /// Outer doc comment from `///` lines preceding the `module` keyword.
    /// See `doc/plan_arch_doc_comments.md`.
    pub doc: Option<String>,
    /// Inner doc comment from `//!` lines after `module Name` and before
    /// any other body item.
    pub inner_doc: Option<String>,
    /// True when this declaration was loaded from a `.archi` interface
    /// stub (port-only, no body). Set post-parse from the source-file
    /// extension. Body-only passes (output-driven check, codegen,
    /// .archi re-emit) skip these to avoid spurious diagnostics and
    /// duplicate output.
    pub is_interface: bool,
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: Ident,
    pub kind: ParamKind,
    pub default: Option<Expr>,
    /// `local param` → emits SV `localparam` (not overridable at inst site)
    pub is_local: bool,
    pub span: Span,
    /// Optional unpacked-array post-name dimension: `param NAME: T [N] = ...`.
    /// Emits SV `parameter T NAME [N] = <default>` — the SV unpacked-array
    /// param shape used by upstream Ibex for `pmp_cfg_t [PMP_MAX_REGIONS]`
    /// and `logic [W:0] [N]` style declarations. arch-com forwards the
    /// dimension verbatim and treats the param as opaque (no value
    /// evaluation), since unpacked-array param values are SV-side only.
    pub unpacked_size: Option<Expr>,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    /// Untyped const (emits `parameter int`)
    Const,
    /// Width-qualified const: `param NAME[hi:lo]: const` (emits `parameter [hi:lo]`)
    WidthConst(Expr, Expr),
    Type(TypeExpr),
    /// Enum-typed const: `param MODE: EnumName = EnumName::Variant`
    EnumConst(String),
    /// Vec-of-const: `param NAME: Vec<T, N> = {a, b, c};` — fixed-length
    /// array of compile-time constants. Emits SV
    /// `parameter logic [W-1:0] NAME [0:N-1] = '{a, b, c, ...}`.
    /// Indexable as `NAME[i]` returning `T`. Inst-site overrides via
    /// `param NAME = {…};`.
    ConstVec(TypeExpr),
    /// Logic-typed value const: `param NAME: UInt<W> = ...;` (or SInt /
    /// Bool). Emits SV `parameter [W-1:0] NAME = <default>` —
    /// the same shape as `WidthConst` but with type-first surface
    /// syntax matching how ARCH writes ports / regs / wires
    /// elsewhere. Used together with the post-name unpacked-dim
    /// to express upstream-SV `parameter logic [W:0] NAME [N] = ...`.
    Logic(TypeExpr),
}

#[derive(Debug, Clone)]
pub struct PortDecl {
    pub name: Ident,
    pub direction: Direction,
    pub ty: TypeExpr,
    /// Optional default value for FSM output ports.  When present, the FSM
    /// codegen uses this expression instead of `'0` in the defaults block, and
    /// the type-checker no longer requires the port to be driven in every state.
    pub default: Option<Expr>,
    /// When present, this output port is also a register (assigned in `seq` blocks).
    /// Syntax: `port reg name: out Type [init V] [reset R=V];`
    pub reg_info: Option<PortRegInfo>,
    /// When present, this port is a bus bundle (initiator or target perspective).
    /// Syntax: `port name: initiator BusName<PARAM=val>;`
    pub bus_info: Option<BusPortInfo>,
    /// Shared reduction annotation: `shared(or)` or `shared(and)`.
    /// Allows multiple drivers with compiler-synthesized reduction logic.
    pub shared: Option<SharedReduction>,
    /// `unpacked` modifier on a `Vec<T,N>` port: SV emission becomes
    /// `logic [W-1:0] name [N-1:0]` (unpacked array) instead of the default
    /// `logic [N-1:0][W-1:0] name` (packed). For interop with external SV
    /// modules whose port shape is fixed unpacked. Has no effect on
    /// ARCH-internal semantics. Only legal on `Vec<T,N>` types.
    pub unpacked: bool,
    pub span: Span,
}

/// Reduction operator for `shared(or|and)` signal annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedReduction {
    Or,
    And,
}

/// Register metadata for a `port reg` declaration OR a
/// `port X: out pipe_reg<T, N>` declaration (the latter carries
/// `latency = N`; legacy `port reg` implies `latency = 1`).
#[derive(Debug, Clone)]
pub struct PortRegInfo {
    pub init: Option<Expr>,
    pub reset: RegReset,
    /// Optional valid-signal guard — tells tools this reg is intentionally
    /// uninitialized as long as the guard signal is low. See
    /// `doc/plan_reg_guard_syntax.md` for semantics.
    pub guard: Option<Ident>,
    /// Pipeline depth (number of clock edges between internal write and
    /// external observation). Legacy `port reg` syntax: 1.
    /// New `port X: out pipe_reg<T, N>` syntax: N (≥ 1).
    pub latency: u32,
    /// True if this port was declared with the legacy `port reg` keyword
    /// rather than the recommended `port X: out pipe_reg<T, N>` form.
    /// Used by the typecheck pass to emit a deprecation warning pointing
    /// users at the new spelling. Semantics are identical for N=1.
    pub legacy_port_reg: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

impl Direction {
    pub fn flip(self) -> Self {
        match self {
            Direction::In => Direction::Out,
            Direction::Out => Direction::In,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModuleBodyItem {
    RegDecl(RegDecl),
    RegBlock(RegBlock),
    LatchBlock(LatchBlock),
    CombBlock(CombBlock),
    LetBinding(LetBinding),
    Inst(InstDecl),
    Generate(GenerateDecl),
    PipeRegDecl(PipeRegDecl),
    WireDecl(WireDecl),
    Thread(ThreadBlock),
    Resource(ResourceDecl),
    Assert(AssertDecl),
    Function(FunctionDecl),
}

impl ModuleBodyItem {
    pub fn span(&self) -> Span {
        match self {
            ModuleBodyItem::RegDecl(r)    => r.span,
            ModuleBodyItem::RegBlock(r)   => r.span,
            ModuleBodyItem::LatchBlock(l) => l.span,
            ModuleBodyItem::CombBlock(c)  => c.span,
            ModuleBodyItem::LetBinding(l) => l.span,
            ModuleBodyItem::Inst(i)       => i.span,
            ModuleBodyItem::Generate(g)   => match g {
                GenerateDecl::For(f) => f.span,
                GenerateDecl::If(i)  => i.span,
            },
            ModuleBodyItem::PipeRegDecl(p) => p.span,
            ModuleBodyItem::WireDecl(w) => w.span,
            ModuleBodyItem::Thread(t) => t.span,
            ModuleBodyItem::Resource(r) => r.span,
            ModuleBodyItem::Assert(a) => a.span,
            ModuleBodyItem::Function(f) => f.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipeRegDecl {
    pub name: Ident,
    pub source: Ident,
    pub stages: u32,
    pub span: Span,
}

// ── Assert / Cover ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AssertKind { Assert, Cover }

#[derive(Debug, Clone)]
pub struct AssertDecl {
    pub kind: AssertKind,
    pub name: Option<Ident>,   // optional label (e.g. `assert no_overflow: expr;`)
    pub expr: Expr,
    pub span: Span,
}

// ── Generate ──────────────────────────────────────────────────────────────────

/// An item inside a generate block: port, instance, thread, or a
/// restricted-form seq / comb block.
///
/// Inside `generate_for`, `seq` / `comb` blocks may only drive targets of the
/// form `<ident>[<expr-referencing-loop-var>]` — writing to a scalar reg from
/// the loop body would produce N conflicting drivers after unrolling. This
/// constraint is enforced in `elaborate::expand_generate_for` before the
/// block is substituted and emitted. Module-scope Vec regs are the intended
/// write target; scalar-reg reads in RHS expressions remain unrestricted.
#[derive(Debug, Clone)]
pub enum GenItem {
    Port(PortDecl),
    Inst(InstDecl),
    Thread(ThreadBlock),
    Assert(AssertDecl),
    Seq(RegBlock),
    Comb(CombBlock),
}

/// `generate for VAR in START..END ... end generate for VAR`
#[derive(Debug, Clone)]
pub struct GenerateFor {
    pub var: Ident,
    pub start: Expr,
    pub end: Expr,
    pub items: Vec<GenItem>,
    pub span: Span,
}

/// `generate if COND ... end generate if`
#[derive(Debug, Clone)]
pub struct GenerateIf {
    pub cond: Expr,
    pub then_items: Vec<GenItem>,
    pub else_items: Vec<GenItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GenerateDecl {
    For(GenerateFor),
    If(GenerateIf),
}

#[derive(Debug, Clone)]
pub struct RegDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    /// Optional SV declaration initializer (`logic [W-1:0] x = VALUE;`)
    pub init: Option<Expr>,
    pub reset: RegReset,
    /// Optional valid-signal guard — documents that this reg is intentionally
    /// uninitialized as long as the guard signal is low.
    pub guard: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum RegReset {
    /// No reset for this register
    None,
    /// Inherit sync/async and polarity from the named reset port declaration;
    /// reset value is the `=VALUE` expression after the signal name.
    Inherit(Ident, Expr),
    /// Explicit override: reset signal, sync/async, high/low, reset value
    Explicit(Ident, ResetKind, ResetLevel, Expr),
}

#[derive(Debug, Clone)]
pub struct RegBlock {
    pub clock: Ident,
    pub clock_edge: ClockEdge,
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LatchBlock {
    pub enable: Ident,
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockEdge {
    Rising,
    Falling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetLevel {
    High,
    Low,
}

// ── Thread ───────────────────────────────────────────────────────────────────

/// A `thread` block inside a module.  Lowered to an FSM + inst by elaboration.
#[derive(Debug, Clone)]
pub struct ThreadBlock {
    /// Optional name (e.g. `thread WriteHandler ...`).  None = anonymous.
    pub name: Option<Ident>,
    pub clock: Ident,
    pub clock_edge: ClockEdge,
    pub reset: Ident,
    pub reset_level: ResetLevel,
    /// `thread once` — one-shot, terminal state after completion.
    pub once: bool,
    /// `default when <cond> ... end default` — soft-reset clause.
    /// When `cond` is true in any state, the listed seq assigns fire and the
    /// thread returns to state 0, taking priority over normal transitions.
    pub default_when: Option<(Expr, Vec<ThreadStmt>)>,
    /// Set when the thread is a TLM method target body:
    ///   `thread PORT.METHOD(ARG1, ARG2, ...) on clk rising, rst high`.
    /// Captured at parse time (PR-tlm-3); lowering to an FSM (entry gate
    /// on req_valid, arg bindings, `return` → rsp drive) ships next.
    pub tlm_target: Option<TlmTargetBinding>,
    /// `implement <port>.<method>()` (initiator) or `implement target
    /// <port>.<method>(args)` (target) clause on the thread header.
    /// Opts the thread into the compiler's id allocation + arbitration
    /// machinery across N co-implementers (see doc/plan_tlm_implement_thread.md).
    /// Initiator form is NEW in v2; target form is a generalization of
    /// the v1 dotted-name binding (both populate `tlm_target` for
    /// downstream lowering compat).
    pub implement: Option<TlmImplementBinding>,
    /// Reentrant threads allow a fresh invocation to start before the
    /// previous one completes. Captured at parse time by the optional
    /// `reentrant [max N]` clause on the thread header (see
    /// `doc/plan_tlm_pipelined.md`). Encoding:
    ///   - `None`                  — v1 semantics; exactly one instance.
    ///   - `Some(None)`            — `reentrant` alone (unbounded — v1
    ///     lowering rejects; reserved for future use).
    ///   - `Some(Some(Expr))`      — `reentrant max <expr>` (const-reducible).
    pub reentrant: Option<Option<Expr>>,
    pub body: Vec<ThreadStmt>,
    pub span: Span,
}

/// Binding of a `thread` body to a TLM method declaration on a bus port.
/// See `doc/plan_tlm_method.md` for the lowering semantics.
#[derive(Debug, Clone)]
pub struct TlmTargetBinding {
    /// Bus port name that carries the method (e.g. `s`).
    pub port: Ident,
    /// Method name (e.g. `read`).
    pub method: Ident,
    /// Optional compile-time tag lane for indexed target implementations:
    /// `thread s.read[t](...)`. After `generate_for` expansion this must
    /// reduce to a literal lane id and is used to replicate target servers
    /// without a dynamic target-side scheduler.
    pub tag_lane: Option<Expr>,
    /// Argument names bound as thread-local values for the body.
    /// Types come from the bus's `TlmMethodMeta.args` at lowering time.
    pub args: Vec<Ident>,
}

/// `implement` clause on a thread header — glues the thread to a TLM
/// method declaration. Initiator form binds the thread as one of
/// potentially N id-tagged issue agents; target form generalizes the
/// v1 dotted-name target syntax. See `doc/plan_tlm_implement_thread.md`.
#[derive(Debug, Clone)]
pub struct TlmImplementBinding {
    pub kind: TlmImplementKind,
    pub port: Ident,
    pub method: Ident,
    /// Target form binds the declared method args as thread-local names
    /// (same semantics as v1 `thread s.read(addr) ...`). Initiator form
    /// has empty args — the thread body supplies args at each call site.
    pub args: Vec<Ident>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmImplementKind {
    Initiator,
    Target,
}

/// A statement inside a thread block.
#[derive(Debug, Clone)]
pub enum ThreadStmt {
    /// Combinational assign: `target = expr;`
    CombAssign(CombAssign),
    /// Sequential assign: `target <= expr;`
    SeqAssign(RegAssign),
    /// Nonblocking TLM issue: `target <= fork port.method(args);`
    ForkTlmAssign(RegAssign),
    /// Join all outstanding nonblocking TLM issues in the current group.
    JoinAll(Span),
    /// `wait until cond;`
    WaitUntil(Expr, Span),
    /// `wait N cycle;`
    WaitCycles(Expr, Span),
    /// `if cond ... elsif ... else ... end if`
    IfElse(ThreadIfElse),
    /// `fork ... and ... join` — parallel branches
    ForkJoin(Vec<Vec<ThreadStmt>>, Span),
    /// `for var in start..end ... end for` — counted loop with waits
    For { var: Ident, start: Expr, end: Expr, body: Vec<ThreadStmt>, span: Span },
    /// `lock resource_name ... end lock resource_name` — exclusive bus access
    Lock { resource: Ident, body: Vec<ThreadStmt>, span: Span },
    /// `do ... until cond;` — hold comb outputs while waiting for condition
    DoUntil { body: Vec<ThreadStmt>, cond: Expr, span: Span },
    /// `log(Level, "TAG", "fmt", args...);` — debug output
    Log(LogStmt),
    /// `return expr;` — valid only inside a TLM target thread body
    /// (`thread port.method(args) ...`). The `lower_tlm_target_threads`
    /// pass rewrites this into the rsp_valid / rsp_data / wait_for_ready
    /// sequence before regular thread lowering runs.
    Return(Expr, Span),
}

/// Generic if/else statement, parameterized by statement body type `S`.
/// Used as `IfElse = IfElseOf<Stmt>` (seq blocks), `CombIfElse = IfElseOf<CombStmt>`
/// (comb blocks), and `ThreadIfElse = IfElseOf<ThreadStmt>` (thread bodies).
/// `unique` is only meaningful in CombIfElse/IfElse (from `unique if` syntax);
/// it's always false for ThreadIfElse.
#[derive(Debug, Clone)]
pub struct IfElseOf<S> {
    pub cond: Expr,
    pub then_stmts: Vec<S>,
    pub else_stmts: Vec<S>,
    pub unique: bool,
    pub span: Span,
}

pub type ThreadIfElse = IfElseOf<ThreadStmt>;

/// `resource name : mutex<policy>;` — shared bus arbitration declaration.
///
/// One-liner: `resource bus: mutex<round_robin>;` / `mutex<priority>` / `mutex<lru>`
/// / `mutex<weighted<W>>` / `mutex<MyFn>` (the last picks the `Custom(MyFn)` policy).
///
/// Block form (for custom policies needing a hook):
/// ```text
/// resource bus: mutex<MyFn>
///   hook grant_select(req_mask: UInt<N>, last_grant: UInt<N>) -> UInt<N>
///        = MyFn(req_mask, last_grant);
/// end resource bus
/// ```
///
/// The lock arbiter is synthesized per resource by `lower_module_threads`,
/// reusing the existing `arbiter` construct's codegen by emitting an
/// `ArbiterDecl` Item with `policy` and `hook` carried over from this
/// declaration.
#[derive(Debug, Clone)]
pub struct ResourceDecl {
    pub name: Ident,
    pub policy: ArbiterPolicy,
    pub hook: Option<ArbiterHookDecl>,
    pub span: Span,
}

/// Block context — propagated through typecheck and codegen so a single
/// `Stmt` enum covers both comb (`=`) and seq (`<=`) blocks. The
/// distinction is *where* the statement lives, not *what* it carries:
/// the parser already enforces `=` only inside `comb { }` and `<=` only
/// inside `seq { }`, so the AST node is unbiased and the context decides
/// the rules at use sites.
///
/// - `Comb`: stmts inside a `comb` block. Assigns to `wire`/port targets only;
///   blocking `=` in SV.
/// - `Seq`: stmts inside a `seq on clk` block. Assigns to `reg` targets only;
///   non-blocking `<=` in SV.
/// - `PipelineStage`: a pipeline-stage seq block — same rules as `Seq` plus
///   `wait until` / `do until` are legal here only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Comb,
    Seq,
    PipelineStage,
}

#[derive(Debug, Clone)]
pub struct CombBlock {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// Assignment statement: `target = expr;` (combinational, in `comb` blocks)
/// or `target <= expr;` (sequential, in `seq` blocks / thread seq-assigns).
/// Pre-5b this was wrapped in `CombStmt::Assign` vs `Stmt::Assign` to encode
/// blocking vs non-blocking; that distinction now lives in the enclosing
/// block context (`BlockKind` for typecheck, `AssignCtx` for codegen).
#[derive(Debug, Clone)]
pub struct Assign {
    pub target: Expr,
    pub value: Expr,
    pub span: Span,
}

/// Readability alias for `Assign` used at thread/sites where the *blocking*
/// (combinational) form is the intent — `target = expr;`. The struct itself
/// is unbiased; the enclosing context (or the wrapping enum variant)
/// decides emit semantics.
pub type CombAssign = Assign;

#[derive(Debug, Clone)]
pub struct LetBinding {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
    /// When non-empty, this is a struct-destructuring let binding:
    /// `let {a, b, c} = expr;` binds each listed field name to the
    /// corresponding field of the (struct-typed) RHS. The `name` field
    /// above is unused in this mode (set to a synthesized placeholder
    /// by the parser) and `ty` is always None because types are inferred
    /// from the RHS's struct definition.
    pub destructure_fields: Vec<Ident>,
}

#[derive(Debug, Clone)]
pub struct WireDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    /// `wire name: unpacked Vec<T,N>;` flips SV emission to unpacked-array
    /// shape (`logic [W-1:0] name [N-1:0]`) so the wire can mate with an
    /// `unpacked Vec<T,N>` port across an `inst` connection without
    /// Verilator rejecting the packed/unpacked shape mismatch. Mirrors the
    /// `unpacked` modifier on port declarations (§3.7).
    pub unpacked: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InstDecl {
    pub name: Ident,
    pub module_name: Ident,
    pub param_assigns: Vec<ParamAssign>,
    pub connections: Vec<Connection>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParamAssign {
    pub name: Ident,
    pub value: Expr,
    /// When the parent module's matching `param NAME: type = ...` is a
    /// type parameter, this holds the override type expression (e.g.
    /// `UInt<DATA_WIDTH>`). The parser populates this when the inst-site
    /// RHS parses as a type rather than a value expression. SV codegen
    /// emits `.NAME(<type>)`; elaborate substitutes through type-using
    /// declarations in the child.
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub port_name: Ident,
    pub direction: ConnectDir,
    pub signal: Expr,
    /// Optional reset-type override: `rst <- my_rst as Reset<Async, Low>`
    /// Allows instantiation-time override of the reset port kind/polarity.
    pub reset_override: Option<(ResetKind, ResetLevel)>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectDir {
    Input,  // <-
    Output, // ->
}

// Simulation log verbosity levels (0 = always print, higher = more verbose).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Always = 0,
    Low    = 1,
    Medium = 2,
    High   = 3,
    Full   = 4,
    Debug  = 5,
}

impl LogLevel {
    pub fn name(self) -> &'static str {
        match self {
            LogLevel::Always => "ALWAYS",
            LogLevel::Low    => "LOW",
            LogLevel::Medium => "MEDIUM",
            LogLevel::High   => "HIGH",
            LogLevel::Full   => "FULL",
            LogLevel::Debug  => "DEBUG",
        }
    }

    pub fn value(self) -> u8 { self as u8 }
}

// Statements inside reg blocks
#[derive(Debug, Clone)]
pub enum Stmt {
    Assign(RegAssign),
    IfElse(IfElse),
    Match(MatchStmt),
    Log(LogStmt),
    For(ForLoop),
    /// `init on RST.asserted ... end init`
    /// Reset initialization block: body runs when reset is asserted.
    /// Determines async sensitivity when the reset port is Reset<Async, ...>.
    Init(InitBlock),
    /// `wait until cond;` — pipeline stage multi-cycle stall boundary.
    /// Only valid inside a pipeline stage `seq` block.
    WaitUntil(Expr, Span),
    /// `do ... until cond;` — hold comb/seq outputs while waiting for condition.
    /// Only valid inside a pipeline stage `seq` block.
    DoUntil { body: Vec<Stmt>, cond: Expr, span: Span },
}

#[derive(Debug, Clone)]
pub struct InitBlock {
    /// The reset signal referenced (e.g. `rst` from `init on rst.asserted`)
    pub reset_signal: Ident,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ForRange {
    Range(Expr, Expr),      // start..end
    ValueList(Vec<Expr>),   // {a, b, c}
}

#[derive(Debug, Clone)]
/// `for VAR in RANGE ... end for` — generic over the body statement type.
/// `ForLoop<Stmt>` for seq-block / pipeline-stage for loops; `ForLoop<CombStmt>`
/// for comb-block for loops. Previously hard-coded to `Vec<Stmt>`, which
/// forced comb-context for-loop bodies to be wrapped as seq stmts and then
/// re-checked under seq semantics in typecheck — the bug this generalization
/// removes.
pub struct ForLoop<S = Stmt> {
    pub var: Ident,
    pub range: ForRange,
    pub body: Vec<S>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LogStmt {
    pub level: LogLevel,
    pub tag: String,
    pub fmt: String,
    pub args: Vec<Expr>,
    pub file: Option<String>,
    pub span: Span,
}

pub type RegAssign = Assign;

pub type IfElse = IfElseOf<Stmt>;

/// `match SCRUTINEE ... end match` — generic over the arm-body statement type.
/// `MatchStmt<Stmt>` for seq-block matches; `MatchStmt<CombStmt>` for
/// comb-block matches (aliased as `CombMatch`). Previously hard-coded to
/// `Vec<MatchArm>` (i.e. `Vec<MatchArm<Stmt>>`), forcing comb match-arm
/// bodies to be wrapped as seq stmts.
#[derive(Debug, Clone)]
pub struct MatchStmt<S = Stmt> {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm<S>>,
    pub unique: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm<S = Stmt> {
    pub pattern: Pattern,
    pub body: Vec<S>,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Ident(Ident),
    EnumVariant(Ident, Ident), // EnumName::Variant
    Literal(Expr),
    Wildcard,
}

// Types
#[derive(Debug, Clone)]
pub enum TypeExpr {
    UInt(Box<Expr>),
    SInt(Box<Expr>),
    Bool,
    Bit,
    Clock(Ident),
    Reset(ResetKind, ResetLevel),
    Vec(Box<TypeExpr>, Box<Expr>),
    Named(Ident),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetKind {
    Sync,
    Async,
}

// Expressions
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// True when this expression was wrapped in parentheses in source.
    #[doc(hidden)]
    pub parenthesized: bool,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span, parenthesized: false }
    }
    pub fn parens(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span, parenthesized: true }
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(LitKind),
    Ident(String),
    /// Compiler-synthesized identifier — behaves exactly like `Ident(name)`
    /// for codegen / sim / formal purposes, but carries its own known type
    /// so typecheck doesn't need to resolve it via the symbol table. Used
    /// by the credit_channel method-dispatch elaborate pass (PR #3b-v) to
    /// point expressions at codegen-emitted SV wires (`__<port>_<ch>_valid`,
    /// `_data`, `_can_send`) whose declaration lives in the emitted SV
    /// boilerplate rather than in the ARCH module body.
    SynthIdent(String, TypeExpr),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    FieldAccess(Box<Expr>, Ident),
    MethodCall(Box<Expr>, Ident, Vec<Expr>),  // receiver, method, type_args encoded as exprs
    Cast(Box<Expr>, Box<TypeExpr>),
    Index(Box<Expr>, Box<Expr>),
    BitSlice(Box<Expr>, Box<Expr>, Box<Expr>),  // base[hi:lo]
    PartSelect(Box<Expr>, Box<Expr>, Box<Expr>, bool),  // base[start +: width] (true=+:, false=-:)
    StructLiteral(Ident, Vec<FieldInit>),
    EnumVariant(Ident, Ident), // EnumName::Variant
    Todo,
    Bool(bool),
    Match(Box<Expr>, Vec<MatchArm>),
    /// Expression-level match: each arm produces a value (emitted as nested ternary)
    ExprMatch(Box<Expr>, Vec<ExprMatchArm>),
    /// Bit concatenation: {a, b, c} → {a, b, c} in SV; MSB first.
    Concat(Vec<Expr>),
    /// Bit replication: {N{expr}} → {N{expr}} in SV.
    Repeat(Box<Expr>, Box<Expr>),
    /// $clog2(expr) — ceiling log2, used in type width expressions.
    Clog2(Box<Expr>),
    /// onehot(index) — one-hot decode: 1 << index. Width inferred from context.
    Onehot(Box<Expr>),
    /// `expr @ N` — latency annotation. On LHS of a seq assignment, marks
    /// the cycle offset at which the write materializes (e.g. `q@3 <= Y`
    /// reads as "Y arrives at q's output in 3 cycles"). On RHS, names the
    /// stage (v1: only `@0` as explicit "current value"). Typecheck enforces
    /// placement and N validity based on the signal's declared pipe depth.
    LatencyAt(Box<Expr>, u32),
    /// signed(expr) — same-width reinterpret cast to SInt.
    Signed(Box<Expr>),
    /// unsigned(expr) — same-width reinterpret cast to UInt.
    Unsigned(Box<Expr>),
    /// Pure combinational function call: Name(arg, ...)
    FunctionCall(String, Vec<Expr>),
    /// SVA delay-shift: `##N expr`. Inside an `assert`/`cover` body, shifts
    /// the cycle of `expr` forward by `N` (i.e. evaluates `expr` at cycle
    /// `t + N` when the surrounding property is checked at cycle `t`).
    /// `N` is a parse-time integer literal.
    SvaNext(u32, Box<Expr>),
    /// Set membership: expr inside {val, lo..hi, ...}
    Inside(Box<Expr>, Vec<InsideMember>),
    /// Ternary conditional: cond ? then_expr : else_expr
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum InsideMember {
    Single(Expr),
    Range(Expr, Expr), // lo..hi inclusive
}

#[derive(Debug, Clone)]
pub struct ExprMatchArm {
    pub pattern: Pattern,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct FieldInit {
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum LitKind {
    Dec(u64),
    Hex(u64),
    Bin(u64),
    Sized(u32, u64), // width, value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    AddWrap,
    SubWrap,
    MulWrap,
    Implies,
    /// `a |=> b` — SVA-style next-cycle implication. Sugar for
    /// `past(a, 1) implies b`. Valid only inside assert/cover bodies.
    ImpliesNext,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Eq => write!(f, "=="),
            BinOp::Neq => write!(f, "!="),
            BinOp::Lt => write!(f, "<"),
            BinOp::Gt => write!(f, ">"),
            BinOp::Lte => write!(f, "<="),
            BinOp::Gte => write!(f, ">="),
            BinOp::And => write!(f, "and"),
            BinOp::Or => write!(f, "or"),
            BinOp::BitAnd => write!(f, "&"),
            BinOp::BitOr => write!(f, "|"),
            BinOp::BitXor => write!(f, "^"),
            BinOp::Shl => write!(f, "<<"),
            BinOp::Shr => write!(f, ">>"),
            BinOp::AddWrap => write!(f, "+%"),
            BinOp::SubWrap => write!(f, "-%"),
            BinOp::MulWrap => write!(f, "*%"),
            BinOp::Implies => write!(f, "implies"),
            BinOp::ImpliesNext => write!(f, "|=>"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    BitNot,
    Neg,
    RedAnd,
    RedOr,
    RedXor,
}

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: String, span: Span) -> Self {
        Self { name, span }
    }
}

impl Item {
    pub fn span(&self) -> Span {
        self.as_construct().span()
    }

    /// Centralized accessor that converts an `Item` to its trait object —
    /// the single match site that replaces the historical N-arm dispatch
    /// in every consumer pass.
    ///
    /// Approach (a) of refactor plan item #6: the `Item` enum stays, but
    /// every pass that previously did `match item { Item::Counter(c) =>
    /// emit_counter(c), ... }` can now do `item.as_construct().<method>()`
    /// — the trait dispatch goes through this one match.
    pub fn as_construct(&self) -> &dyn Construct {
        match self {
            Item::Domain(d) => d,
            Item::Struct(s) => s,
            Item::Enum(e) => e,
            Item::Module(m) => m,
            Item::Fsm(f) => f,
            Item::Fifo(f) => f,
            Item::Ram(r) => r,
            Item::Cam(c) => c,
            Item::Counter(c) => c,
            Item::Arbiter(a) => a,
            Item::Regfile(r) => r,
            Item::Pipeline(p) => p,
            Item::Function(f) => f,
            Item::Linklist(l) => l,
            Item::Template(t) => t,
            Item::Synchronizer(s) => s,
            Item::Clkgate(c) => c,
            Item::Bus(b) => b,
            Item::Package(p) => p,
            Item::Use(u) => u,
        }
    }

    /// True when this item was loaded from a `.archi` interface stub
    /// (port-only, no body). Set by the post-parse tagger in `main.rs`
    /// based on the source-file extension. Body-only passes (codegen,
    /// sim model emission, .archi re-emit, body-driven typecheck)
    /// short-circuit on this so the stub doesn't shadow the real
    /// implementation that lives in a separately-built `.sv`/`.cpp`.
    /// Only the construct variants that can be instantiated across
    /// `.archi` boundaries carry the flag — `module` (via
    /// `ModuleDecl.is_interface`) and the `ConstructCommon`-bearing
    /// variants (`fsm`, `fifo`, `ram`, `cam`, `counter`, `arbiter`,
    /// `regfile`, `pipeline`, `linklist`).
    pub fn is_interface(&self) -> bool {
        match self {
            Item::Module(m)        => m.is_interface,
            Item::Fsm(f)           => f.common.is_interface,
            Item::Fifo(f)          => f.common.is_interface,
            Item::Ram(r)           => r.common.is_interface,
            Item::Cam(c)           => c.common.is_interface,
            Item::Counter(c)       => c.common.is_interface,
            Item::Arbiter(a)       => a.common.is_interface,
            Item::Regfile(r)       => r.common.is_interface,
            Item::Pipeline(p)      => p.common.is_interface,
            Item::Linklist(l)      => l.common.is_interface,
            // The remaining variants (Domain, Struct, Enum, Function,
            // Template, Synchronizer, Clkgate, Bus, Package, Use)
            // either don't get instantiated across `.archi` boundaries
            // or aren't `ConstructCommon`-bearing. No is_interface for
            // them today; extend if/when a new case appears.
            _ => false,
        }
    }

    /// Set the interface-stub flag. Mirror of [`Self::is_interface`].
    /// Used by the post-parse tagger in `main.rs` to mark items loaded
    /// from `.archi` files. Returns `true` if the variant supports the
    /// flag (and the assignment took effect); `false` otherwise.
    pub fn set_is_interface(&mut self, val: bool) -> bool {
        match self {
            Item::Module(m)        => { m.is_interface = val; true }
            Item::Fsm(f)           => { f.common.is_interface = val; true }
            Item::Fifo(f)          => { f.common.is_interface = val; true }
            Item::Ram(r)           => { r.common.is_interface = val; true }
            Item::Cam(c)           => { c.common.is_interface = val; true }
            Item::Counter(c)       => { c.common.is_interface = val; true }
            Item::Arbiter(a)       => { a.common.is_interface = val; true }
            Item::Regfile(r)       => { r.common.is_interface = val; true }
            Item::Pipeline(p)      => { p.common.is_interface = val; true }
            Item::Linklist(l)      => { l.common.is_interface = val; true }
            _ => false,
        }
    }
}

/// Centralizing trait for all top-level constructs (every `Item::*`
/// variant). Phase 1 (this PR) covers only the always-applicable
/// accessors — `name`, `span`, `doc`, `inner_doc`, `kind_label`. Future
/// PRs will add pass methods (`typecheck`, `emit_sv`, `emit_sim`, …) one
/// at a time, each replacing one N-arm `match item { Item::* => self.X }`
/// dispatch with `item.as_construct().X(...)`.
///
/// Item #6 of `doc/plan_compiler_refactor.md`, approach (a): the `Item`
/// enum stays; this trait provides a single dispatch point via
/// [`Item::as_construct`] so consumer passes don't have to keep their
/// own variant-by-variant matches.
pub trait Construct {
    /// The lowercase keyword that introduces this construct in source
    /// (`"module"`, `"counter"`, `"fsm"`, `"struct"`, `"use"`, etc.).
    /// Used by `arch advise` doc-event emission and diagnostics.
    fn kind_label(&self) -> &'static str;

    /// The construct's name as declared (e.g. `Counter` in
    /// `counter Counter ... end counter Counter`).
    fn name(&self) -> &Ident;

    /// Source span covering the construct from opening keyword to closing
    /// `end <keyword> <Name>` (or single-line span for `use` / inline
    /// constructs).
    fn span(&self) -> Span;

    /// Outer doc comment from `///` lines immediately preceding the
    /// construct. `None` if no doc-comment block was attached.
    fn doc(&self) -> Option<&str>;

    /// Inner doc comment from `//!` lines that appear between the opening
    /// keyword and the first body item. `None` if absent. Some constructs
    /// (`Use`) have no body and always return `None`.
    fn inner_doc(&self) -> Option<&str>;

    /// Emit the `.archi` interface — the public-facing construct signature
    /// without the body. Returns `Some(content)` for constructs that have
    /// an external interface (module, fsm, counter, pipeline, fifo, ram,
    /// arbiter, regfile, synchronizer, clkgate, linklist, bus, struct,
    /// enum, package); `None` for the rest. Default returns `None`,
    /// matching the original `_ => None` arm in `interface::emit_interface`.
    fn emit_interface(&self) -> Option<String> { None }

    /// Run typecheck on this construct. Default is a no-op (matches the
    /// `Item::Use(_) => {}` arm in the original dispatch). Each
    /// construct that has type rules overrides this to call its
    /// specific `TypeChecker::check_*` method.
    fn typecheck(&self, _checker: &mut crate::typecheck::TypeChecker) {}

    /// Emit SystemVerilog for this construct. Default is a no-op,
    /// matching the original `Item::Function(_) | Item::Template(_) |
    /// Item::Bus(_) | Item::Use(_) => {}` arms — `function` is emitted
    /// inside each module body, `template` is compile-time-only,
    /// `bus` flattens at port sites, and `use` is an import directive.
    fn emit_sv(&self, _codegen: &mut crate::codegen::Codegen) {}

    /// Emit a C++ simulation model for this construct. Returns
    /// `Some(model)` for constructs that have a sim emitter (counter,
    /// fsm, regfile, ram, cam, fifo, synchronizer, clkgate, linklist,
    /// arbiter, pipeline). Returns `None` for everything else
    /// (Module is handled specially by the caller because it needs the
    /// debug-module set; Domain / Struct / Enum / Function / Template
    /// / Bus / Package / Use don't generate sim code).
    fn emit_sim(&self, _simgen: &crate::sim_codegen::SimCodegen) -> Option<crate::sim_codegen::SimModel> { None }
}

// ── Construct trait impls ────────────────────────────────────────────────────
// Every `*Decl` that appears as an `Item::*` variant impls `Construct` so
// the central `Item::as_construct` accessor can return `&dyn Construct`.
// Constructs that embed `ConstructCommon` (Module, Fsm, Fifo, Ram, Cam,
// Counter, Arbiter, Regfile, Pipeline, Linklist) get all five methods via
// the embedded common fields. Constructs without `ConstructCommon`
// (Domain, Struct, Enum, Function, Synchronizer, Clkgate, Bus, Package,
// Use, Template) carry their own `name` / `span` / `doc` / `inner_doc`
// fields directly.

/// Implement `Construct` for a `*Decl` that embeds `ConstructCommon`.
/// `iface = path::to::fn` and `check = method_name` are independently
/// optional in any order. Without them, the trait defaults apply
/// (`emit_interface` returns `None`, `typecheck` is a no-op).
macro_rules! impl_construct_via_common {
    ($ty:ty, $label:expr $(, iface = $iface:path)? $(, check = $check:ident)? $(, emit_sv = $emit_sv:ident)? $(, emit_sim = $emit_sim:ident)?) => {
        impl Construct for $ty {
            fn kind_label(&self) -> &'static str { $label }
            fn name(&self)      -> &Ident         { &self.common.name }
            fn span(&self)      -> Span           { self.common.span }
            fn doc(&self)       -> Option<&str>   { self.common.doc.as_deref() }
            fn inner_doc(&self) -> Option<&str>   { self.common.inner_doc.as_deref() }
            $(fn emit_interface(&self) -> Option<String> { Some($iface(self)) })?
            $(fn typecheck(&self, c: &mut crate::typecheck::TypeChecker) { c.$check(self); })?
            $(fn emit_sv(&self, c: &mut crate::codegen::Codegen) { c.$emit_sv(self); })?
            $(fn emit_sim(&self, c: &crate::sim_codegen::SimCodegen) -> Option<crate::sim_codegen::SimModel> { Some(c.$emit_sim(self)) })?
        }
    };
}

/// Implement `Construct` for a `*Decl` that carries `name` / `span` /
/// `doc` / `inner_doc` directly. Same optional-arg pattern as the
/// via-common variant.
macro_rules! impl_construct_direct {
    ($ty:ty, $label:expr $(, iface = $iface:path)? $(, check = $check:ident)? $(, emit_sv = $emit_sv:ident)? $(, emit_sim = $emit_sim:ident)?) => {
        impl Construct for $ty {
            fn kind_label(&self) -> &'static str { $label }
            fn name(&self)      -> &Ident         { &self.name }
            fn span(&self)      -> Span           { self.span }
            fn doc(&self)       -> Option<&str>   { self.doc.as_deref() }
            fn inner_doc(&self) -> Option<&str>   { self.inner_doc.as_deref() }
            $(fn emit_interface(&self) -> Option<String> { Some($iface(self)) })?
            $(fn typecheck(&self, c: &mut crate::typecheck::TypeChecker) { c.$check(self); })?
            $(fn emit_sv(&self, c: &mut crate::codegen::Codegen) { c.$emit_sv(self); })?
            $(fn emit_sim(&self, c: &crate::sim_codegen::SimCodegen) -> Option<crate::sim_codegen::SimModel> { Some(c.$emit_sim(self)) })?
        }
    };
}

impl_construct_direct!(ModuleDecl,           "module",       iface = crate::interface::emit_module_interface,       check = check_module,       emit_sv = emit_module);
impl_construct_via_common!(FsmDecl,          "fsm",          iface = crate::interface::emit_fsm_interface,          check = check_fsm,          emit_sv = emit_fsm,          emit_sim = gen_fsm);
impl_construct_via_common!(FifoDecl,         "fifo",         iface = crate::interface::emit_fifo_interface,         check = check_fifo,         emit_sv = emit_fifo,         emit_sim = gen_fifo);
impl_construct_via_common!(RamDecl,          "ram",          iface = crate::interface::emit_ram_interface,          check = check_ram,          emit_sv = emit_ram,          emit_sim = gen_ram);
impl_construct_via_common!(CamDecl,          "cam",                                                                 check = check_cam,          emit_sv = emit_cam,          emit_sim = gen_cam);
impl_construct_via_common!(CounterDecl,      "counter",      iface = crate::interface::emit_counter_interface,      check = check_counter,      emit_sv = emit_counter,      emit_sim = gen_counter);
impl_construct_via_common!(ArbiterDecl,      "arbiter",      iface = crate::interface::emit_arbiter_interface,      check = check_arbiter,      emit_sv = emit_arbiter,      emit_sim = gen_arbiter);
impl_construct_via_common!(RegfileDecl,      "regfile",      iface = crate::interface::emit_regfile_interface,      check = check_regfile,      emit_sv = emit_regfile,      emit_sim = gen_regfile);
impl_construct_via_common!(PipelineDecl,     "pipeline",     iface = crate::interface::emit_pipeline_interface,     check = check_pipeline,     emit_sv = emit_pipeline,     emit_sim = gen_pipeline);
impl_construct_via_common!(LinklistDecl,     "linklist",     iface = crate::interface::emit_linklist_interface,     check = check_linklist,     emit_sv = emit_linklist,     emit_sim = gen_linklist);

impl_construct_direct!(DomainDecl,           "domain",                                                              check = check_domain,       emit_sv = emit_domain);
impl_construct_direct!(StructDecl,           "struct",       iface = crate::interface::emit_struct,                 check = check_struct,       emit_sv = emit_struct);
impl_construct_direct!(EnumDecl,             "enum",         iface = crate::interface::emit_enum,                   check = check_enum,         emit_sv = emit_enum);
impl_construct_direct!(FunctionDecl,         "function",                                                            check = check_function);
impl_construct_direct!(SynchronizerDecl,     "synchronizer", iface = crate::interface::emit_synchronizer_interface, check = check_synchronizer, emit_sv = emit_synchronizer, emit_sim = gen_synchronizer);
impl_construct_direct!(ClkGateDecl,          "clkgate",      iface = crate::interface::emit_clkgate_interface,      check = check_clkgate,      emit_sv = emit_clkgate,      emit_sim = gen_clkgate);
impl_construct_direct!(BusDecl,              "bus",          iface = crate::interface::emit_bus_interface,          check = check_bus);
impl_construct_direct!(PackageDecl,          "package",      iface = crate::interface::emit_package_interface,      check = check_package,      emit_sv = emit_package);
impl_construct_direct!(TemplateDecl,         "template",                                                            check = check_template);

// `Use` has only `doc` — no inner doc (single-line decl).
impl Construct for UseDecl {
    fn kind_label(&self) -> &'static str { "use" }
    fn name(&self)      -> &Ident         { &self.name }
    fn span(&self)      -> Span           { self.span }
    fn doc(&self)       -> Option<&str>   { self.doc.as_deref() }
    fn inner_doc(&self) -> Option<&str>   { None }
}

// ── Function ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Ident,
    pub args: Vec<FunctionArg>,
    pub ret_ty: TypeExpr,
    pub body: Vec<FunctionBodyItem>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionArg {
    pub name: Ident,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone)]
pub enum FunctionBodyItem {
    Let(LetBinding),
    Return(Expr),
    IfElse(FunctionIfElse),
    For(FunctionFor),
    Assign(FunctionAssign),
}

/// If/elsif/else inside a function body.
#[derive(Debug, Clone)]
pub struct FunctionIfElse {
    pub cond: Expr,
    pub then_body: Vec<FunctionBodyItem>,
    pub else_body: Vec<FunctionBodyItem>,
    pub span: Span,
}

/// For loop inside a function body.
#[derive(Debug, Clone)]
pub struct FunctionFor {
    pub var: Ident,
    pub range: ForRange,
    pub body: Vec<FunctionBodyItem>,
    pub span: Span,
}

/// Assignment to a local variable inside a function (blocking =).
#[derive(Debug, Clone)]
pub struct FunctionAssign {
    pub target: Expr,
    pub value: Expr,
    pub span: Span,
}

// ── ConstructCommon — shared header for all first-class constructs ────────────

/// Fields present on every first-class construct (fsm, pipeline, fifo, ram,
/// counter, arbiter, regfile, linklist, op).  Extracted so that new shared
/// fields (e.g. `asserts`) require a single change here instead of one per
/// construct.  Each construct embeds this as `pub common: ConstructCommon` and
/// implements `Deref<Target = ConstructCommon>` so that existing code such as
/// `fsm.name`, `fsm.ports`, `fsm.asserts` continues to compile unchanged.
///
/// See `impl ConstructCommon` below for shared param-resolution helpers
/// (`param_int`, `resolve_count_expr`).
#[derive(Debug, Clone)]
pub struct ConstructCommon {
    pub name:    Ident,
    pub params:  Vec<ParamDecl>,
    pub ports:   Vec<PortDecl>,
    pub asserts: Vec<AssertDecl>,
    pub span:    Span,
    /// Outer doc comment from immediately-preceding `///` lines. None when
    /// the construct has no doc-comment block above it. See
    /// `doc/plan_arch_doc_comments.md` for the V1 surface.
    pub doc:     Option<String>,
    /// Inner doc comment from `//!` lines that appear between the opening
    /// keyword + name and any other body item. Distinct from `doc` so
    /// downstream tooling can tell "from the outside" prose apart from
    /// "from the inside" prose.
    pub inner_doc: Option<String>,
    /// True when this construct was loaded from a `.archi` interface
    /// stub (port-only, no body). Set post-parse from the source-file
    /// extension (see `main.rs` post-parse tagger). Body-only passes
    /// (output-driven check, codegen, .archi re-emit, sim model
    /// emission) skip these to avoid spurious diagnostics and duplicate
    /// output. Mirrors the same flag on `ModuleDecl`. Module isn't
    /// folded into `ConstructCommon` yet — see `feedback_*` for
    /// background — so that flag is duplicated; both feed the same
    /// post-parse tagger and downstream-skip pattern.
    pub is_interface: bool,
}

impl ConstructCommon {
    /// Resolve a param by name to its default integer literal value.
    /// Returns `default` if the param is missing, has no default, or its
    /// default isn't a `LitKind::Dec` (the only literal form param defaults
    /// take in practice — derived expressions like `XLEN/8` are out of
    /// scope; full const-eval is its own future refactor item).
    ///
    /// Replaces the same 5-line `let param_int = |...|` closure that was
    /// duplicated in `codegen::emit_regfile`, `sim_codegen::gen_regfile`,
    /// and `sim_codegen/linklist.rs`.
    pub fn param_int(&self, name: &str, default: u64) -> u64 {
        self.params.iter()
            .find(|p| p.name.name == name)
            .and_then(|p| p.default.as_ref())
            .and_then(|e| if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind { Some(*v) } else { None })
            .unwrap_or(default)
    }

    /// Resolve a port-array `count_expr` (`ports[N]` / `ports[PARAM]`):
    /// integer literal returns directly; bare param-name reference falls
    /// back to that param's default via [`Self::param_int`] with a default
    /// of 1 (a port array of length 0 makes no sense and shouldn't reach
    /// here). Anything more complex (arithmetic on params, etc.) returns
    /// 1 — same conservative fallback the duplicated closures used.
    pub fn resolve_count_expr(&self, expr: &Expr) -> u64 {
        match &expr.kind {
            ExprKind::Literal(LitKind::Dec(v)) => *v,
            ExprKind::Ident(name) => self.param_int(name, 1),
            _ => 1,
        }
    }
}

// ── FSM ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FsmDecl {
    pub common: ConstructCommon,
    /// Register declarations (datapath registers alongside FSM state)
    pub regs: Vec<RegDecl>,
    /// Combinational let bindings at FSM scope
    pub lets: Vec<LetBinding>,
    /// Wire declarations (combinational nets driven in comb blocks)
    pub wires: Vec<WireDecl>,
    /// Flat list of declared state names (`state A, B, C;`)
    pub state_names: Vec<Ident>,
    /// The reset / default state. `None` is only valid for an interface
    /// stub loaded from a `.archi` file (`common.is_interface == true`);
    /// real `fsm` declarations require an explicit `default state Name;`
    /// — that requirement is now enforced in `resolve.rs` rather than
    /// `parser.rs`, so the parser can accept body-less stubs that the
    /// post-parse tagger flips to `is_interface = true`.
    pub default_state: Option<Ident>,
    /// Default block: comb and seq statements applied before the state case
    pub default_comb: Vec<Stmt>,
    pub default_seq: Vec<Stmt>,
    /// State bodies (`state Foo ... end state Foo`)
    pub states: Vec<StateBody>,
}
impl std::ops::Deref for FsmDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for FsmDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone)]
pub struct StateBody {
    pub name: Ident,
    /// Combinational output assignments for this state
    pub comb_stmts: Vec<Stmt>,
    /// Sequential register assignments for this state
    pub seq_stmts: Vec<Stmt>,
    pub transitions: Vec<Transition>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub target: Ident,
    pub condition: Expr,
    pub span: Span,
}

// ── FIFO ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FifoKind {
    Fifo,
    Lifo,
}

#[derive(Debug, Clone)]
pub struct FifoDecl {
    pub common: ConstructCommon,
    pub kind: FifoKind,
}
impl std::ops::Deref for FifoDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for FifoDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

// ── Synchronizer (CDC) ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// N-stage flip-flop chain (default, best for 1-bit signals)
    Ff,
    /// Gray-code encode → FF chain → decode (safe for multi-bit counters/pointers)
    Gray,
    /// Req/ack handshake protocol (safe for arbitrary multi-bit data)
    Handshake,
    /// Reset synchronizer: assert immediate (async), deassert through FF chain (sync)
    Reset,
    /// Pulse synchronizer: single-cycle pulse across clock domains via toggle + edge detect
    Pulse,
}

#[derive(Debug, Clone)]
pub struct SynchronizerDecl {
    pub name: Ident,
    pub kind: SyncKind,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

// ── Clock Gate ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ClkGateKind {
    Latch,
    And,
}

#[derive(Debug, Clone)]
pub struct ClkGateDecl {
    pub name: Ident,
    pub kind: ClkGateKind,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

// ── RAM ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RamDecl {
    pub common: ConstructCommon,
    pub kind: RamKind,
    pub latency: u32,
    pub write_mode: Option<RamWriteMode>,
    pub collision: Option<RamCollision>,
    pub store_vars: Vec<RamStoreVar>,
    pub port_groups: Vec<RamPortGroup>,
    pub init: Option<RamInit>,
}
impl std::ops::Deref for RamDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for RamDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamKind {
    Single,
    SimpleDual,
    TrueDual,
    Rom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamWriteMode {
    WriteFirst,
    ReadFirst,
    NoChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamCollision {
    PortAWins,
    PortBWins,
    Undefined,
}

#[derive(Debug, Clone)]
pub struct RamStoreVar {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RamPortGroup {
    pub name: Ident,
    /// Signals inside the port group (no `port` keyword)
    pub signals: Vec<PortDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Hex,
    Bin,
}

#[derive(Debug, Clone)]
pub enum RamInit {
    Zero,
    None,
    File(String, FileFormat),
    Value(Expr),
    Array(Vec<u64>),
}

// ── CAM ──────────────────────────────────────────────────────────────────────

/// Content-addressable memory: combinational match of a search key against
/// a vector of (valid, key) entries. Single write port (set/clear by index).
/// See doc/plan_cam.md for full semantics.
#[derive(Debug, Clone)]
pub struct CamDecl {
    pub common: ConstructCommon,
}
impl std::ops::Deref for CamDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for CamDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

// ── Counter ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CounterDecl {
    pub common: ConstructCommon,
    pub mode: CounterMode,
    pub direction: CounterDirection,
    pub init: Option<Expr>,
}
impl std::ops::Deref for CounterDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for CounterDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterMode {
    Wrap,
    Saturate,
    Gray,
    OneHot,
    Johnson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterDirection {
    Up,
    Down,
    UpDown,
}

// ── Arbiter ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ArbiterDecl {
    pub common: ConstructCommon,
    pub port_arrays: Vec<PortArrayDecl>,
    pub policy: ArbiterPolicy,
    pub hook: Option<ArbiterHookDecl>,
    pub latency: u32,
}
impl std::ops::Deref for ArbiterDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for ArbiterDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone)]
pub enum ArbiterPolicy {
    RoundRobin,
    Priority,
    Lru,
    Weighted(Expr),  // weight expression (param reference)
    Custom(Ident),   // user function name as policy
}

/// `hook grant_select(req_mask: UInt<N>, ...) -> UInt<N> = FnName(arg1, arg2, ...);`
#[derive(Debug, Clone)]
pub struct ArbiterHookDecl {
    pub hook_name: Ident,          // e.g. "grant_select"
    pub params: Vec<FunctionArg>,  // formal parameters with types
    pub ret_ty: TypeExpr,          // return type
    pub fn_name: Ident,            // bound function name
    pub fn_args: Vec<Ident>,       // bound arguments
    pub span: Span,
}

/// A `ports[N] name ... end ports name` block (used by arbiter and regfile)
#[derive(Debug, Clone)]
pub struct PortArrayDecl {
    pub count_expr: Expr,
    pub name: Ident,
    pub signals: Vec<PortDecl>,
    pub span: Span,
}

// ── Regfile ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RegfileDecl {
    pub common: ConstructCommon,
    pub read_ports: Option<PortArrayDecl>,
    pub write_ports: Option<PortArrayDecl>,
    pub inits: Vec<RegfileInit>,
    pub forward_write_before_read: bool,
    /// Storage cell type. Default `Flop` matches v0.49.x and earlier
    /// behavior (a flop array). `Latch` emits one transparent latch per
    /// row with one-hot write-enable decoding — typically 30–50% smaller
    /// area than the flop form on ASIC, with most rows clock-gated when
    /// no write fires. See spec §regfile and `doc/plan_regfile_latch.md`.
    pub kind: RegfileKind,
    /// Where the write-port flops live (only meaningful when `kind: latch`):
    /// - `External` (default): caller flops `we` / `waddr` / `wdata` in
    ///   their logic; the typecheck pass enforces this at every inst site
    ///   (see `check_latch_regfile_writes`). Emitted SV is leaner — no
    ///   internal flop, no ICG cell.
    /// - `Internal` (Ibex-style): RF auto-emits `wdata_q` / `waddr_q`
    ///   flops + a per-row ICG cell (`mem_clk[i] = clk && (waddr_q==i)`,
    ///   gated through a transparent latch on `we` to suppress glitches).
    ///   Caller can drive `we` / `waddr` / `wdata` combinationally — the
    ///   static check is skipped.
    pub flops: RegfileFlops,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegfileKind {
    Flop,
    Latch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegfileFlops {
    External,
    Internal,
}
impl std::ops::Deref for RegfileDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for RegfileDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone)]
pub struct RegfileInit {
    pub index: Expr,
    pub value: Expr,
    pub span: Span,
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PipelineDecl {
    pub common: ConstructCommon,
    pub stages: Vec<StageDecl>,
    pub stall_conds: Vec<StallDecl>,
    pub flush_directives: Vec<FlushDecl>,
    pub forward_directives: Vec<ForwardDecl>,
}
impl std::ops::Deref for PipelineDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for PipelineDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone)]
pub struct StageDecl {
    pub name: Ident,
    pub stall_cond: Option<Expr>,
    pub body: Vec<ModuleBodyItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StallDecl {
    pub condition: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FlushDecl {
    pub target_stage: Ident,
    pub condition: Expr,
    /// When true, `flush <Stage> when <cond> clear;` also resets every
    /// data register in the target stage to its declared reset value
    /// in addition to the default `valid_r <= 0` bubble. Use for
    /// security / speculative-data-leakage scenarios where stale data
    /// in flushed registers is a hazard. Default false (bubble-only).
    pub clear: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForwardDecl {
    pub dest: Expr,
    pub source: Expr,
    pub condition: Expr,
    pub span: Span,
}

// ── Linklist ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LinklistKind {
    Singly,
    Doubly,
    CircularSingly,
    CircularDoubly,
}

#[derive(Debug, Clone)]
pub struct LinklistDecl {
    pub common: ConstructCommon,
    pub kind: LinklistKind,
    pub track_tail: bool,
    pub track_length: bool,
    pub ops: Vec<OpDecl>,
}
impl std::ops::Deref for LinklistDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for LinklistDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

#[derive(Debug, Clone)]
pub struct OpDecl {
    pub common: ConstructCommon,
    pub latency: u32,
    pub pipelined: bool,
}
impl std::ops::Deref for OpDecl {
    type Target = ConstructCommon;
    fn deref(&self) -> &ConstructCommon { &self.common }
}
impl std::ops::DerefMut for OpDecl {
    fn deref_mut(&mut self) -> &mut ConstructCommon { &mut self.common }
}

// ── Template ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TemplateDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub port_arrays: Vec<PortArrayDecl>,
    pub hooks: Vec<TemplateHookDecl>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

/// Hook signature in a template (no binding — just the contract)
#[derive(Debug, Clone)]
pub struct TemplateHookDecl {
    pub name: Ident,
    pub params: Vec<FunctionArg>,
    pub ret_ty: TypeExpr,
    pub span: Span,
}

// ── Package ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PackageDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub domains: Vec<DomainDecl>,
    pub enums: Vec<EnumDecl>,
    pub structs: Vec<StructDecl>,
    pub buses: Vec<BusDecl>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
    pub doc: Option<String>,
    pub inner_doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub name: Ident,
    pub span: Span,
    pub doc: Option<String>,
}

/// Hook binding in a module that `implements` a template
#[derive(Debug, Clone)]
pub struct ModuleHookDecl {
    pub hook_name: Ident,
    pub params: Vec<FunctionArg>,
    pub ret_ty: TypeExpr,
    pub fn_name: Ident,
    pub fn_args: Vec<Ident>,
    pub span: Span,
}

// ── Shared port utilities (used by both codegen.rs and sim_codegen.rs) ────

/// Find the reset port and return (name, is_async, is_low).
/// Defaults to ("rst", false, false) if no reset port is present (sync, active-high).
pub fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
    for p in ports {
        if let TypeExpr::Reset(kind, level) = &p.ty {
            return (
                p.name.name.clone(),
                *kind == ResetKind::Async,
                *level == ResetLevel::Low,
            );
        }
    }
    ("rst".to_string(), false, false)
}
