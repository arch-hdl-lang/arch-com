use crate::lexer::Span;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub items: Vec<Item>,
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
    /// Concurrency mode — v1 only accepts `blocking`.
    pub mode: Ident,
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
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: Ident,
    pub kind: ParamKind,
    pub default: Option<Expr>,
    /// `local param` → emits SV `localparam` (not overridable at inst site)
    pub is_local: bool,
    pub span: Span,
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
    /// Argument names bound as thread-local values for the body.
    /// Types come from the bus's `TlmMethodMeta.args` at lowering time.
    pub args: Vec<Ident>,
}

/// A statement inside a thread block.
#[derive(Debug, Clone)]
pub enum ThreadStmt {
    /// Combinational assign: `target = expr;`
    CombAssign(CombAssign),
    /// Sequential assign: `target <= expr;`
    SeqAssign(RegAssign),
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

/// `resource name : mutex<policy>;` — shared bus arbitration declaration
#[derive(Debug, Clone)]
pub struct ResourceDecl {
    pub name: Ident,
    pub policy: ArbiterPolicy,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CombBlock {
    pub stmts: Vec<CombStmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum CombStmt {
    Assign(CombAssign),
    IfElse(CombIfElse),
    MatchExpr(CombMatch),
    Log(LogStmt),
    For(ForLoop),
}

/// Assignment statement: `target = expr;` (combinational, in `comb` blocks)
/// or `target <= expr;` (sequential, in `seq` blocks / thread seq-assigns).
/// The assignment kind (blocking vs non-blocking) is determined by which
/// enum wraps it: `CombStmt::Assign` is blocking, `Stmt::Assign` and
/// `ThreadStmt::SeqAssign` are non-blocking.
#[derive(Debug, Clone)]
pub struct Assign {
    pub target: Expr,
    pub value: Expr,
    pub span: Span,
}

// CombAssign and RegAssign are aliases for Assign — previously three
// identical structs; now unified. Both names kept for readability at
// call sites (CombAssign for blocking `=`, RegAssign for non-blocking `<=`).
pub type CombAssign = Assign;

pub type CombIfElse = IfElseOf<CombStmt>;

#[derive(Debug, Clone)]
pub struct CombMatch {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub unique: bool,
    pub span: Span,
}

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
pub struct ForLoop {
    pub var: Ident,
    pub range: ForRange,
    pub body: Vec<Stmt>,
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

#[derive(Debug, Clone)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub unique: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
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
        match self {
            Item::Domain(d) => d.span,
            Item::Struct(s) => s.span,
            Item::Enum(e) => e.span,
            Item::Module(m) => m.span,
            Item::Fsm(f) => f.span,
            Item::Fifo(f) => f.span,
            Item::Ram(r) => r.span,
            Item::Counter(c) => c.span,
            Item::Arbiter(a) => a.span,
            Item::Regfile(r) => r.span,
            Item::Pipeline(p) => p.span,
            Item::Function(f) => f.span,
            Item::Linklist(l) => l.span,
            Item::Template(t) => t.span,
            Item::Synchronizer(s) => s.span,
            Item::Clkgate(c) => c.span,
            Item::Bus(b) => b.span,
            Item::Package(p) => p.span,
            Item::Use(u) => u.span,
        }
    }
}

// ── Function ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Ident,
    pub args: Vec<FunctionArg>,
    pub ret_ty: TypeExpr,
    pub body: Vec<FunctionBodyItem>,
    pub span: Span,
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
#[derive(Debug, Clone)]
pub struct ConstructCommon {
    pub name:    Ident,
    pub params:  Vec<ParamDecl>,
    pub ports:   Vec<PortDecl>,
    pub asserts: Vec<AssertDecl>,
    pub span:    Span,
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
    /// The reset / default state
    pub default_state: Ident,
    /// Default block: comb and seq statements applied before the state case
    pub default_comb: Vec<CombStmt>,
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
    pub comb_stmts: Vec<CombStmt>,
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
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub name: Ident,
    pub span: Span,
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
