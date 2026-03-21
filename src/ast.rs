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

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<Ident>,
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
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: Ident,
    pub kind: ParamKind,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Const,
    Type(TypeExpr),
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
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone)]
pub enum ModuleBodyItem {
    RegDecl(RegDecl),
    RegBlock(RegBlock),
    CombBlock(CombBlock),
    LetBinding(LetBinding),
    Inst(InstDecl),
    Generate(GenerateDecl),
    PipeRegDecl(PipeRegDecl),
}

impl ModuleBodyItem {
    pub fn span(&self) -> Span {
        match self {
            ModuleBodyItem::RegDecl(r)    => r.span,
            ModuleBodyItem::RegBlock(r)   => r.span,
            ModuleBodyItem::CombBlock(c)  => c.span,
            ModuleBodyItem::LetBinding(l) => l.span,
            ModuleBodyItem::Inst(i)       => i.span,
            ModuleBodyItem::Generate(g)   => match g {
                GenerateDecl::For(f) => f.span,
                GenerateDecl::If(i)  => i.span,
            },
            ModuleBodyItem::PipeRegDecl(p) => p.span,
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

// ── Generate ──────────────────────────────────────────────────────────────────

/// An item inside a generate block: either a port declaration or an instance.
#[derive(Debug, Clone)]
pub enum GenItem {
    Port(PortDecl),
    Inst(InstDecl),
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
    pub init: Expr,
    pub reset: RegReset,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum RegReset {
    /// No reset for this register
    None,
    /// Inherit sync/async and polarity from the named reset port declaration
    Inherit(Ident),
    /// Explicit override: reset signal, sync/async, high/low
    Explicit(Ident, ResetKind, ResetLevel),
}

#[derive(Debug, Clone)]
pub struct RegBlock {
    pub clock: Ident,
    pub clock_edge: ClockEdge,
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
}

#[derive(Debug, Clone)]
pub struct CombAssign {
    pub target: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CombIfElse {
    pub cond: Expr,
    pub then_stmts: Vec<CombStmt>,
    pub else_stmts: Vec<CombStmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CombMatch {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LetBinding {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
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
}

#[derive(Debug, Clone)]
pub struct LogStmt {
    pub level: LogLevel,
    pub tag: String,
    pub fmt: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RegAssign {
    pub target: Expr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfElse {
    pub cond: Expr,
    pub then_stmts: Vec<Stmt>,
    pub else_stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
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
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(LitKind),
    Ident(String),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    FieldAccess(Box<Expr>, Ident),
    MethodCall(Box<Expr>, Ident, Vec<Expr>),  // receiver, method, type_args encoded as exprs
    Cast(Box<Expr>, Box<TypeExpr>),
    Index(Box<Expr>, Box<Expr>),
    StructLiteral(Ident, Vec<FieldInit>),
    EnumVariant(Ident, Ident), // EnumName::Variant
    Todo,
    Bool(bool),
    Match(Box<Expr>, Vec<MatchArm>),
    /// Expression-level match: each arm produces a value (emitted as nested ternary)
    ExprMatch(Box<Expr>, Vec<ExprMatchArm>),
    /// Bit concatenation: {a, b, c} → {a, b, c} in SV; MSB first.
    Concat(Vec<Expr>),
    /// $clog2(expr) — ceiling log2, used in type width expressions.
    Clog2(Box<Expr>),
    /// Pure combinational function call: Name(arg, ...)
    FunctionCall(String, Vec<Expr>),
    /// Ternary conditional: cond ? then_expr : else_expr
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    BitNot,
    Neg,
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
}

// ── FSM ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FsmDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    /// Flat list of declared state names (`state A, B, C;`)
    pub state_names: Vec<Ident>,
    /// The reset / default state
    pub default_state: Ident,
    /// State bodies (`state Foo ... end state Foo`)
    pub states: Vec<StateBody>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StateBody {
    pub name: Ident,
    /// Combinational output assignments for this state
    pub comb_stmts: Vec<CombStmt>,
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

#[derive(Debug, Clone)]
pub struct FifoDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub span: Span,
}

// ── Synchronizer (CDC) ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SynchronizerDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub span: Span,
}

// ── RAM ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RamDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    /// Top-level ports: clk, optional rst
    pub ports: Vec<PortDecl>,
    pub kind: RamKind,
    pub latency: u32,
    pub write_mode: Option<RamWriteMode>,
    pub collision: Option<RamCollision>,
    pub store_vars: Vec<RamStoreVar>,
    pub port_groups: Vec<RamPortGroup>,
    pub init: Option<RamInit>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamKind {
    Single,
    SimpleDual,
    TrueDual,
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

#[derive(Debug, Clone)]
pub enum RamInit {
    Zero,
    None,
    File(String),
    Value(Expr),
}

// ── Counter ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CounterDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub mode: CounterMode,
    pub direction: CounterDirection,
    pub init: Option<Expr>,
    pub span: Span,
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
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub port_arrays: Vec<PortArrayDecl>,
    pub policy: ArbiterPolicy,
    pub hook: Option<ArbiterHookDecl>,
    pub latency: u32,
    pub span: Span,
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
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub read_ports: Option<PortArrayDecl>,
    pub write_ports: Option<PortArrayDecl>,
    pub inits: Vec<RegfileInit>,
    pub forward_write_before_read: bool,
    pub span: Span,
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
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub stages: Vec<StageDecl>,
    pub stall_conds: Vec<StallDecl>,
    pub flush_directives: Vec<FlushDecl>,
    pub forward_directives: Vec<ForwardDecl>,
    pub span: Span,
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
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    /// User-declared status ports (empty, full, length)
    pub ports: Vec<PortDecl>,
    pub kind: LinklistKind,
    pub track_tail: bool,
    pub track_length: bool,
    pub ops: Vec<OpDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct OpDecl {
    pub name: Ident,
    pub latency: u32,
    pub pipelined: bool,
    pub ports: Vec<PortDecl>,
    pub span: Span,
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
