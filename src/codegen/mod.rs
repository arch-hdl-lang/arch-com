use crate::ast::*;
use crate::diagnostics::CompileWarning;
use crate::lexer::Span;
use crate::resolve::{Symbol, SymbolTable};
use crate::typecheck::enum_width;
use method_call::MethodCallHost;

// Per-construct submodules. Each contributes `pub(super) fn emit_<name>`
// to `impl Codegen` and lives in its own file mirroring the layout of
// `sim_codegen/`. New constructs land in their own file rather than
// growing this `mod.rs`.
mod arbiter;
mod cam;
mod clkgate;
mod counter;
mod fifo;
mod fp;
mod fsm;
mod linklist;
mod method_call;
mod module;
mod pipeline;
mod ram;
mod regfile;
mod synchronizer;

/// SV assignment-operator context for the unified `emit_stmt` walker.
/// `Blocking` = `=` (comb / latch / reg-as-comb), `NonBlocking` = `<=` (seq).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum AssignCtx {
    Blocking,
    NonBlocking,
}

impl AssignCtx {
    fn op(&self) -> &'static str {
        match self {
            AssignCtx::Blocking => "=",
            AssignCtx::NonBlocking => "<=",
        }
    }
}

fn stmt_span_start(stmt: &Stmt) -> usize {
    match stmt {
        Stmt::Assign(a) => a.span.start,
        Stmt::IfElse(i) => i.span.start,
        Stmt::Match(m) => m.span.start,
        Stmt::Log(l) => l.span.start,
        Stmt::For(f) => f.span.start,
        Stmt::Init(ib) => ib.span.start,
        Stmt::WaitUntil(_, sp) => sp.start,
        Stmt::DoUntil { span, .. } => span.start,
    }
}

/// One `shared function` harness: one MAC (or whatever the function
/// computes), driven by per-state operand muxes. `sv_name` is the
/// emitted SV function name (post overload-mangling); `src_name` is
/// the original arch source name (used to look up the FunctionDecl
/// for arg names + types). Multiple harnesses for the same `src_name`
/// can exist if call sites are gated by different state regs (one
/// MAC per thread).
struct SharedHarness {
    src_name: String,
    sv_name: String,
    state_reg: String,
    /// Cached FunctionDecl (cloned). Pulled from either the module
    /// body or `pending_functions` at collection time so the
    /// later harness emission can read arg names + types without
    /// re-traversing.
    fn_decl: FunctionDecl,
    entries: Vec<SharedHarnessEntry>,
}

impl SharedHarness {
    fn new(src_name: String, sv_name: String, state_reg: String, fn_decl: FunctionDecl) -> Self {
        Self {
            src_name,
            sv_name,
            state_reg,
            fn_decl,
            entries: Vec::new(),
        }
    }
}

struct SharedHarnessEntry {
    state_lit: u64,
    arg_strs: Vec<String>,
    #[allow(dead_code)]
    args: Vec<Expr>,
}

/// One indexed-port group collected during `emit_inst`'s connection walk.
/// Both arbiter `port_arrays` and Vec-of-bus packed ports follow the same
/// "many per-element entries → one grouped connection" shape; the kind
/// discriminates the emit strategy.
struct IndexedGroup {
    /// Base port on the child (arbiter group name, or VOB port name).
    base_port: String,
    kind: GroupKind,
    /// Arbiter case: (idx, signal_str) pairs to drive the synthesized wire.
    arb_entries: Vec<(u32, String)>,
    /// VOB case: per-index parent expr (`ins[k] <- expr_k`).
    vob_entries: std::collections::HashMap<u32, Expr>,
}

enum GroupKind {
    /// Arbiter `port_arrays` — synthesize a temp wire array
    /// `__<inst>_<group>_<sig>[N]` + per-index drives + one whole-vector
    /// connection feeding the child's vector port.
    ArbiterPortArray {
        sig: String,
        dir: Direction,
        ty: TypeExpr,
        n: u64,
    },
    /// Vec-of-bus packed child port — emit a SV packed concat
    /// `.<port>_<sig>({elem_{N-1}, ..., elem_0})` per bus signal, no
    /// synthesized intermediate wires.
    VecOfBusPacked {
        n: u32,
        bus_name: String,
        bus_params: Vec<ParamAssign>,
    },
}

pub struct Codegen<'a> {
    pub symbols: &'a SymbolTable,
    pub source: &'a SourceFile,
    out: String,
    indent: usize,
    pub warnings: Vec<CompileWarning>,
    /// Comments extracted from the original source (byte span, text).
    comments: Vec<(Span, String)>,
    /// Cursor into `comments` — advanced as items are emitted.
    comment_idx: usize,
    /// Functions collected from the current file; emitted inside each module body.
    pending_functions: Vec<FunctionDecl>,
    /// Declared types of the function currently being emitted (params +
    /// typed locals). Consulted ahead of the module scope by
    /// `ident_decl_type`/`ident_float_fmt` so float-op dispatch works
    /// inside `function` bodies; empty outside `emit_function`.
    fn_local_types: std::collections::HashMap<String, TypeExpr>,
    /// Maps call-site span.start → overload index (for overloaded functions only).
    overload_map: std::collections::HashMap<usize, usize>,
    /// Bus port names in the current module → bus name (for FieldAccess rewriting).
    /// For Vec-of-bus ports, entries are keyed by the indexed names
    /// (`<port>_0`, `<port>_1`, ...); the parent port name itself is in
    /// `vec_of_bus_port_count`.
    bus_ports: std::collections::HashMap<String, String>,
    /// Map of Vec-of-bus port name → element count N. Used by
    /// `emit_for_loop_sv` to statically unroll loops whose body accesses
    /// the port via a non-literal index. Populated per-module from
    /// `BusPortInfo.count`.
    vec_of_bus_port_count: std::collections::HashMap<String, u32>,
    /// Same as `vec_of_bus_port_count` but for wires declared as
    /// `wire w: Vec<BusName, N>;`. Used to detect the unroll opportunity.
    vec_of_bus_wire_count: std::collections::HashMap<String, u32>,
    /// Cloned params of the module currently being emitted. Used by the
    /// for-loop static-unroll path to fold param-driven bounds like
    /// `for i in 0..N-1` where `N` is a module param. Populated at the
    /// top of `emit_module`; cleared between modules.
    current_module_params: Vec<ParamDecl>,
    /// Bus-typed wire names in the current module → bus name. Bus wires are
    /// flattened into individual SV signals `<wire>_<field>` at emission
    /// time (no SV interfaces or structs are generated for buses), so
    /// FieldAccess on a bus wire rewrites to the flat name just like a bus
    /// port does.
    bus_wires: std::collections::HashMap<String, String>,
    /// Reset port names in the current module → (kind, level), for `.asserted` emission.
    reset_ports: std::collections::HashMap<String, (ResetKind, ResetLevel)>,
    /// Set when any FP32/BF16 operation was emitted, so the `arch_f32_*` /
    /// `arch_bf16_*` SystemVerilog helper package is prepended to the output.
    fp_helpers_used: std::cell::Cell<bool>,
    /// `ScaledVec` block helpers referenced by this design, one per distinct
    /// (op, element, N, scale, policy, rounding). A `BTreeSet` so the emitted
    /// prefix is byte-stable run to run — `scripts/refactor_diff.sh` diffs
    /// emitted SV, so a `HashSet` here would show up as spurious churn.
    block_helpers: std::cell::RefCell<std::collections::BTreeSet<crate::fp_block::BlockHelper>>,
    /// Staged pipelined-operator sites (`arch build --staged-ops`,
    /// proposal phase 3.5) — see `pipelined_ops::StagedSite`. Empty in
    /// cascade mode. `staged_emitted` records that at least one instance
    /// was emitted so `generate()` prepends the staged module text(s).
    staged_sites: Vec<crate::pipelined_ops::StagedSite>,
    staged_emitted: std::cell::Cell<bool>,
    /// Floating-point special-value profile (doc/archive/plan_fp_types.md §6.2).
    /// Selects the emitted NaN-canonicalization / NaN→int constants.
    fp_compat: crate::FpCompat,
    /// `arch build --no-auto-asserts` (issue #649): when set, every
    /// compiler-generated `assert property` / `cover property` is skipped
    /// — bounds (`_auto_bound_*`), divide-by-zero (`_auto_div0_*`), FSM
    /// legal-state/reachability/transition, FIFO overflow/underflow,
    /// counter range, guard-contract, handshake/credit-channel/TLM protocol
    /// SVA. User-written `assert`/`cover` items (module/fsm/fifo/... body)
    /// are unaffected — narrower reading of the issue, called out in the
    /// PR description. Thread-lowering asserts are already off by default
    /// (`--auto-thread-asserts` opts in) and are additionally gated by this
    /// flag at the CLI layer so `--no-auto-asserts` wins if both are passed.
    suppress_auto_sva: bool,
    /// Name of the construct currently being emitted (for symbol lookups).
    current_construct: String,
    /// Context-sensitive identifier substitutions.
    /// Used during Vec method predicate emission to rebind `item` and
    /// `index` to per-iteration expressions (e.g. `vec[3]`, `2'd3`).
    /// Checked first in `emit_expr_str`'s Ident branch; empty otherwise.
    ident_subst: std::collections::HashMap<String, String>,
    /// Loop-variable → integer-value substitutions, pushed during static
    /// unrolling of `for` loops over Vec-of-bus indexed access. The
    /// bracket-dot resolver for `Index(Ident(arr), Ident(loopvar)).field`
    /// consults this when `loopvar` is in the map and treats the access
    /// like a literal-index one (so `chans[i].v` resolves to
    /// `chans_<value>_v`). Empty outside an unrolled body.
    loop_var_subst: std::collections::HashMap<String, u32>,
    /// Map of Vec-typed signal name → element count N.
    /// Populated per-module at emit time so Vec method lowerings
    /// (`any`/`all`/`count`/etc.) can unroll over N iterations.
    vec_sizes: std::collections::HashMap<String, u32>,
    /// Map of pipe_reg name → (source name, total stages N) for the
    /// current module being emitted. Used to lower `q@K` reads on RHS
    /// to the right SV intermediate signal: `q@0` → source, `q@K` for
    /// 1≤K<N → `q_stg{K}`, `q@N` → `q` (= bare q).
    pipe_regs: std::collections::HashMap<String, (String, u32)>,
    /// Vec-of-const param name → (element TypeExpr) for the current
    /// module. iverilog rejects unpacked-array params, so codegen emits
    /// the param packed and rewrites `B[i]` reads to `B[i*W +: W]`
    /// part-selects. Lookup populated per-module at emit time.
    vec_params: std::collections::HashMap<String, TypeExpr>,
    /// Set of index widths used by `.find_first(...)` calls in this file.
    /// Drives emission of one `typedef struct packed ... __ArchFindResult_<W>;`
    /// per unique W at the top of the generated SV. Interior-mutability
    /// so the `&self` emission path can record widths as it goes.
    find_first_widths: std::cell::RefCell<std::collections::BTreeSet<u32>>,
    /// `shared function` rewrite map: call-site span.start → SV output
    /// wire name (`__shared_<sv_fn_name>_out`). When `emit_expr_str`
    /// encounters a `FunctionCall` whose span.start is in this map, it
    /// emits the wire name instead of the per-call inline `FN(args)`
    /// form. Populated per-module by `collect_shared_calls`.
    shared_call_sites: std::collections::HashMap<usize, String>,
    /// Multicycle reg annotations collected from the items emitted by
    /// `generate` / `generate_items`. Populated by `collect_multicycle_regs`
    /// (called at the start of each `generate*` invocation). Phase A only
    /// affects SDC output — the SV `always_ff` for a multicycle reg is
    /// byte-identical to a plain reg.
    multicycle_regs: Vec<MulticycleReg>,
    /// SV-frontend portability (arch#650, extended by arch#807/#810): pending
    /// `logic [W-1:0] name;` / `assign name = <expr>;` line pairs
    /// synthesized by two call sites when a base expression is not
    /// portable to emit bare/parenthesized on Icarus:
    /// - The `Index` (`expr[i]`) emitter, when its base is not a
    ///   "portable" SV bit-select base (the same allowlist
    ///   `is_portable_bit_slice_base` enforces for `[hi:lo]`/`[start +:
    ///   w]`, spec §3.2.1) — e.g. an arithmetic expression `(a - b)[i]`.
    /// - `hoist_slice_base`, used by the `BitSlice`/`PartSelect` emitters
    ///   for a `Concat`/`Repeat` base (arch#807) or a
    ///   `FunctionCall`/`MethodCall` base (arch#810) — all portable per
    ///   `is_portable_bit_slice_base` but rejected by Icarus 12.0 bare or
    ///   parenthesized, and the `MethodCall` size-cast shape
    ///   (`8'(x)[hi:lo]`) rejected by Verilator too. `codegen/pipeline.rs`
    ///   reaches the same hoist through `hoist_slice_base_in` (arch#845),
    ///   supplying its own stage-prefix-rewriting emitter.
    /// Drained by `line()` so the hoisted temp's declaration+assignment
    /// land in a *legal* position for the scope currently being emitted —
    /// see `hoist_scope` for the three cases. That placement is scope-based,
    /// not caller-based, so `codegen/pipeline.rs` needs nothing of its own:
    /// its stage-update `always_ff` and its `always_comb` openers go through
    /// `line()` like any other, and `HoistScope::Procedural` splices their
    /// temps out to module scope automatically. `RefCell` because both call
    /// sites live inside `&self` expression emission; the actual
    /// `self.out` mutation only ever happens later from `line()`'s
    /// `&mut self`, so this needs no unsafe code (contrast `ident_subst`'s
    /// unsafe interior mutation in `emit_vec_method`).
    index_hoist_temps: std::cell::RefCell<Vec<HoistTemp>>,
    /// Byte offset in `out` just past the innermost runtime `for`-loop
    /// body's `begin` line, plus the indent to declare at. `None` outside a
    /// loop body.
    ///
    /// A hoist temp whose RHS reads the loop iterator (arch#861) can be
    /// declared neither at module scope (the iterator is a loop-local
    /// `int`, invisible there — and its `$bits(...)` width expression reads
    /// the iterator too) nor immediately before the reading statement (a
    /// declaration may not follow a statement inside `begin`/`end`; that is
    /// the arch#846 hard error). It goes at the *top of the loop body*,
    /// which both Icarus 12.0 and Verilator 5.048 accept and where the
    /// iterator is in scope.
    loop_body_anchor: Option<(usize, usize)>,
    /// Monotonic counter for `index_hoist_temps` temp names
    /// (`arch_idx_base_<n>`). Never reset per-module — names only need
    /// to be unique within the emitted file, and a file-wide counter is
    /// simplest and avoids any cross-module collision risk.
    index_hoist_counter: std::cell::Cell<u32>,
    /// Which SV scope `line()` is currently emitting into, so a drained
    /// `index_hoist_temps` entry can be placed somewhere the language
    /// actually allows it (arch#846). Maintained by `line()` itself
    /// (`track_hoist_scope`) plus an explicit push/pop around function
    /// bodies in `emit_function`.
    hoist_scope: HoistScope,
    /// Names of `for i in <range> ... end for` loop variables whose SV
    /// `for` loop body is currently being emitted (module.rs / mod.rs
    /// `emit_for_loop_sv`). A hoisted `Index`-base temp's continuous
    /// `assign` is emitted at module scope (see `hoist_scope` above),
    /// which cannot see a loop-local `int` iterator. So when a hoist
    /// candidate's base expression references an active loop variable,
    /// the hoist is skipped (falls back to prior bare-emission behavior)
    /// rather than emitting SV that references an out-of-scope
    /// identifier. arch#846 did NOT relax this: relocating the `assign`
    /// out of the enclosing `always_*` block is exactly what makes the
    /// iterator invisible, so the bail is still required for every
    /// non-function scope. (A function body is the one scope where the
    /// assignment stays in place — see `HoistScope::Function` — and the
    /// bail is harmless there because `emit_function_for` does not
    /// register its iterator in this set to begin with.)
    runtime_for_loop_vars: std::collections::HashSet<String>,
}

/// One pending portability hoist temp (see `Codegen::index_hoist_temps`).
///
/// Kept as its three parts rather than as pre-rendered SV lines because
/// the legal rendering differs by scope: module/`generate` and `always_*`
/// scopes want `logic [W-1:0] n;` + a continuous `assign n = rhs;`, while
/// a `function automatic` body wants the declaration hoisted to the top of
/// the body and a *blocking* `n = rhs;` left in place (a continuous assign
/// to an automatic-lifetime variable is illegal SV — rejected outright by
/// both Icarus 12.0 and Verilator 5.048).
#[derive(Debug, Clone)]
struct HoistTemp {
    /// Width expression, already paren-normalized (e.g. `8`, `($bits(x))`).
    width: String,
    /// Temp identifier (`arch_idx_base_<n>`).
    name: String,
    /// Emitted SV for the base expression the temp stands in for.
    rhs: String,
    /// The RHS references a live runtime `for`-loop iterator, so the value
    /// must be computed *inside* the loop body where that iterator is in
    /// scope — only the declaration may move to module scope, and the
    /// assignment has to be a blocking one (arch#861). Same split
    /// `HoistScope::Function` applies for function arguments, but decided
    /// per-temp rather than per-scope: one `always_*` block can mix
    /// loop-var temps with ordinary ones.
    in_loop: bool,
}

impl HoistTemp {
    fn decl_line(&self) -> String {
        format!("logic [{}-1:0] {};", self.width, self.name)
    }
    fn continuous_assign_line(&self) -> String {
        format!("assign {} = {};", self.name, self.rhs)
    }
    fn blocking_assign_line(&self) -> String {
        format!("{} = {};", self.name, self.rhs)
    }
}

/// Where `line()` is currently emitting, for the purposes of placing a
/// drained `HoistTemp` (arch#846).
#[derive(Debug, Clone, Copy)]
enum HoistScope {
    /// Module body or a `generate` block — both a `logic` declaration and
    /// a continuous `assign` are legal right where the reading statement
    /// goes, so the temp is emitted inline immediately before it.
    Module,
    /// Inside an `always_ff` / `always_comb` / `always_latch` / `initial` /
    /// `final` block. Neither a declaration nor a continuous `assign` is
    /// legal there (a mid-block declaration is a hard syntax error on both
    /// frontends once a statement precedes it, and `assign` inside a
    /// procedural block is a *procedural continuous assignment*, which
    /// Icarus rejects as unsynthesizable). Both lines are spliced into
    /// `out` at `at` — the byte offset of the block's opening line, i.e.
    /// module scope — indented to `indent`. `depth` tracks `begin`/`end`
    /// nesting so the scope pops when the block closes.
    Procedural {
        at: usize,
        indent: usize,
        depth: i32,
    },
    /// Inside a `function automatic ... endfunction` body. Module scope is
    /// *wrong* here (the base expression may reference function
    /// arguments/locals, which don't exist outside the body), so only the
    /// declaration moves — spliced to `at`, the offset just past the
    /// function header, which is where SV requires body declarations — and
    /// the assignment stays inline as a blocking assignment.
    Function { at: usize, indent: usize },
}

/// One `multicycle <N>` reg discovered during SV emission. Captures
/// enough to emit a `set_multicycle_path` constraint in the companion
/// `.sdc` file. `module_name` is the enclosing module/fsm name; `reg_name`
/// is the reg identifier exactly as it appears in the emitted SV (no
/// transformation — see codegen/module.rs:368 where reg decls emit as
/// `r.name.name` verbatim).
#[derive(Debug, Clone)]
pub struct MulticycleReg {
    pub module_name: String,
    pub reg_name: String,
    pub latency: u32,
}

impl<'a> Codegen<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        source: &'a SourceFile,
        overload_map: std::collections::HashMap<usize, usize>,
    ) -> Self {
        Self {
            symbols,
            source,
            out: String::new(),
            indent: 0,
            warnings: Vec::new(),
            comments: Vec::new(),
            comment_idx: 0,
            pending_functions: Vec::new(),
            fn_local_types: std::collections::HashMap::new(),
            overload_map,
            bus_ports: std::collections::HashMap::new(),
            vec_of_bus_port_count: std::collections::HashMap::new(),
            vec_of_bus_wire_count: std::collections::HashMap::new(),
            current_module_params: Vec::new(),
            bus_wires: std::collections::HashMap::new(),
            reset_ports: std::collections::HashMap::new(),
            fp_helpers_used: std::cell::Cell::new(false),
            block_helpers: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            staged_sites: Vec::new(),
            staged_emitted: std::cell::Cell::new(false),
            fp_compat: crate::FpCompat::default(),
            suppress_auto_sva: false,
            current_construct: String::new(),
            ident_subst: std::collections::HashMap::new(),
            loop_var_subst: std::collections::HashMap::new(),
            vec_sizes: std::collections::HashMap::new(),
            pipe_regs: std::collections::HashMap::new(),
            vec_params: std::collections::HashMap::new(),
            find_first_widths: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            shared_call_sites: std::collections::HashMap::new(),
            multicycle_regs: Vec::new(),
            index_hoist_temps: std::cell::RefCell::new(Vec::new()),
            loop_body_anchor: None,
            index_hoist_counter: std::cell::Cell::new(0),
            hoist_scope: HoistScope::Module,
            runtime_for_loop_vars: std::collections::HashSet::new(),
        }
    }

    /// Multicycle regs collected from the most recent `generate*` call.
    /// Empty if no `.arch` source declared `multicycle <N>`. Callers (the
    /// `arch build` driver, integration tests) inspect this to decide
    /// whether to write a `.sdc` file alongside the `.sv` output.
    pub fn multicycle_regs(&self) -> &[MulticycleReg] {
        &self.multicycle_regs
    }

    /// Emit the SDC-constraint text matching the multicycle regs collected
    /// during the last `generate*` call. Returns `None` when no multicycle
    /// regs were seen — callers should skip writing the `.sdc` file in
    /// that case. `source_filename` is recorded in the header for trace
    /// purposes (the synth tool ignores it).
    ///
    /// SDC convention: a `multicycle N` reg has a setup budget of N cycles
    /// and a hold budget of N-1 cycles. The canonical Synopsys SDC idiom
    /// is `set_multicycle_path N -setup -to {<path>}` paired with
    /// `set_multicycle_path N-1 -hold -to {<path>}` (both relative to the
    /// destination flop). Without the matched -hold relaxation the tool
    /// would tighten the hold check to the new setup window's last cycle
    /// and report false hold violations.
    pub fn emit_sdc(&self, source_filename: &str) -> Option<String> {
        if self.multicycle_regs.is_empty() {
            return None;
        }
        let mut s = String::new();
        s.push_str("# Auto-generated SDC constraints from arch HDL multicycle reg annotations.\n");
        s.push_str(&format!("# Source: {}\n", source_filename));
        s.push_str("# Each `multicycle <N>` reg becomes one matched setup/hold pair.\n");
        s.push_str("# Setup budget = N cycles, hold budget = N-1 cycles (Synopsys SDC idiom).\n");
        s.push('\n');
        for mc in &self.multicycle_regs {
            s.push_str(&format!(
                "# Module {}: multicycle reg {}\n",
                mc.module_name, mc.reg_name
            ));
            // `[get_cells -hierarchical {*<reg>_reg*}]` is the largest
            // common subset across OpenSTA, DC, Genus, Vivado, and Quartus.
            // Bare-path `[*]` only works on DC/Genus; `get_registers` is
            // missing from OpenSTA. The leading `*` glob handles both flat
            // synth (reg at top level, no module instance prefix) and
            // hierarchical synth (the wildcard absorbs `top/.../<module>/`).
            // The `-hierarchical` flag is required for hierarchical netlists
            // under OpenSTA — without it, `get_cells` is non-recursive and
            // the `*` glob does not descend into instance subhierarchies,
            // so the multicycle constraint silently attaches to zero cells
            // and the path is treated as single-cycle. `-hierarchical` is
            // harmless for flat netlists (returns the same cells either
            // way) and is standard SDC across DC/Genus/Vivado/Quartus.
            // The module name remains in the header comment above for
            // human readers.
            s.push_str(&format!(
                "set_multicycle_path {} -setup -to [get_cells -hierarchical {{*{}_reg*}}]\n",
                mc.latency, mc.reg_name
            ));
            s.push_str(&format!(
                "set_multicycle_path {} -hold -to [get_cells -hierarchical {{*{}_reg*}}]\n",
                mc.latency.saturating_sub(1),
                mc.reg_name
            ));
            s.push('\n');
        }
        Some(s)
    }

    /// Walk the given items and record every `multicycle` reg into
    /// `self.multicycle_regs`. Cleared at the start of each call so
    /// repeated `generate_items` invocations on a single Codegen produce
    /// the right set per output file.
    fn collect_multicycle_regs(&mut self, items: &[Item]) {
        self.multicycle_regs.clear();
        for item in items {
            match item {
                Item::Module(m) => {
                    for bi in &m.body {
                        if let ModuleBodyItem::RegDecl(r) = bi {
                            if let Some(n) = r.multicycle {
                                self.multicycle_regs.push(MulticycleReg {
                                    module_name: m.name.name.clone(),
                                    reg_name: r.name.name.clone(),
                                    latency: n,
                                });
                            }
                        }
                    }
                }
                Item::Fsm(f) => {
                    for r in &f.regs {
                        if let Some(n) = r.multicycle {
                            self.multicycle_regs.push(MulticycleReg {
                                module_name: f.common.name.name.clone(),
                                reg_name: r.name.name.clone(),
                                latency: n,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Emit all pending comments whose byte offset is before `pos`.
    fn emit_comments_before(&mut self, pos: usize) {
        while self.comment_idx < self.comments.len()
            && self.comments[self.comment_idx].0.start < pos
        {
            let text = self.comments[self.comment_idx].1.clone();
            self.line(&text);
            self.comment_idx += 1;
        }
    }

    /// Attach extracted source comments so they are interleaved with the output.
    pub fn with_comments(mut self, comments: Vec<(Span, String)>) -> Self {
        self.comments = comments;
        self
    }

    /// Select the floating-point special-value profile (§6.2). Default `Riscv`.
    pub fn with_fp_compat(mut self, profile: crate::FpCompat) -> Self {
        self.fp_compat = profile;
        self
    }

    /// Install the staged pipelined-operator sites collected by
    /// `pipelined_ops::lower_pipelined_calls_mode` (phase 3.5,
    /// `arch build --staged-ops`).
    pub fn set_staged_sites(&mut self, sites: Vec<crate::pipelined_ops::StagedSite>) {
        self.staged_sites = sites;
    }

    /// `arch build --no-auto-asserts` (issue #649). Default `false`
    /// (unchanged behavior — all generated SVA emitted as before).
    pub fn with_suppress_auto_sva(mut self, suppress: bool) -> Self {
        self.suppress_auto_sva = suppress;
        self
    }

    pub fn generate(&mut self) -> String {
        // `source.items` is borrowed for the whole call but `generate_items`
        // only needs an `&[Item]` slice — clone the Vec into a local so we
        // can pass an owned-borrow without colliding with `&mut self`. The
        // clone cost is negligible vs the SV emit it precedes.
        let items: Vec<Item> = self.source.items.clone();
        self.generate_items(&items)
    }

    /// Generate SV for a specific subset of items (used for per-file output).
    pub fn generate_items(&mut self, items: &[Item]) -> String {
        self.out.clear();
        self.comment_idx = 0;
        // `HoistScope`'s splice offsets index into `out`; clearing it
        // invalidates them, and emission always restarts at module scope.
        self.hoist_scope = HoistScope::Module;
        self.collect_multicycle_regs(items);
        // Pre-collect all functions so they can be emitted inside each module.
        self.pending_functions = items
            .iter()
            .flat_map(|i| match i {
                Item::Function(f) => vec![f.clone()],
                Item::Package(p) => p.functions.clone(),
                _ => vec![],
            })
            .collect();
        let mut trimmed_threads_helper_boundary = false;
        for (idx, item) in items.iter().enumerate() {
            self.emit_comments_before(item.span().start);
            // Function / Template / Bus / Use have no top-level SV emit
            // (Function is emitted inside each module body, Template is
            // compile-time only, Bus is flattened at port sites, Use is
            // an import emitted inside modules) — their `Construct::emit_sv`
            // impl is the trait default no-op.
            item.as_construct().emit_sv(self);
            if Self::is_threads_helper_for_next_public_module(item, items.get(idx + 1)) {
                self.trim_trailing_blank_line();
                trimmed_threads_helper_boundary = true;
            }
        }
        // Flush any trailing comments after the last item.
        let end = usize::MAX;
        self.emit_comments_before(end);
        if trimmed_threads_helper_boundary {
            self.trim_trailing_blank_line();
        }

        // Prepend typedefs for any synthesized find_first result structs.
        // One packed struct per unique index width used in the source.
        let widths = self.find_first_widths.borrow();
        if !widths.is_empty() {
            let mut prefix = String::new();
            prefix.push_str("// Auto-generated result struct(s) for Vec.find_first\n");
            for w in widths.iter() {
                prefix.push_str(&format!(
                    "typedef struct packed {{ logic found; logic [{}:0] index; }} __ArchFindResult_{};\n",
                    w.saturating_sub(1),
                    w
                ));
            }
            prefix.push('\n');
            prefix.push_str(&self.out);
            self.out = prefix;
        }

        // Prepend the staged pipelined-operator module(s) if any staged
        // instance was emitted (deduped by module name — several sites may
        // share one staged datapath definition).
        if self.staged_emitted.get() {
            let mut seen = std::collections::BTreeSet::new();
            let mut prefix = String::new();
            for site in &self.staged_sites {
                if seen.insert(site.sv_module) {
                    prefix.push_str(&site.sv_text);
                    prefix.push('\n');
                }
            }
            prefix.push_str(&self.out);
            self.out = prefix;
        }

        // Prepend the `ScaledVec` block helpers, then the FP helpers they
        // call. Order matters: SV requires a function to be declared before
        // use at `$unit` scope, and the block helpers call `arch_f32_mul` and
        // friends, so the FP block must end up FIRST in the file. Prepending
        // the blocks before the FP helpers achieves that.
        if !self.block_helpers.borrow().is_empty() {
            let mut prefix = String::from(
                "// ── ScaledVec block helpers — generated from src/fp_block.rs, which\n\
                 // emits this SystemVerilog and the matching arch-sim C++ from ONE\n\
                 // descriptor. Do not edit by hand. ──\n",
            );
            for h in self.block_helpers.borrow().iter() {
                prefix.push_str(&crate::fp_block::sv_definition(*h));
                prefix.push('\n');
            }
            prefix.push_str(&self.out);
            self.out = prefix;
        }
        // Prepend the floating-point helper functions if any FP op was emitted.
        if self.fp_helpers_used.get() {
            let mut prefix = fp::fp_sv_helpers(self.fp_compat);
            prefix.push_str(&self.out);
            self.out = prefix;
        }
        std::mem::take(&mut self.out)
    }

    fn is_threads_helper_for_next_public_module(item: &Item, next: Option<&Item>) -> bool {
        let Item::Module(m) = item else {
            return false;
        };
        let Some(Item::Module(next_m)) = next else {
            return false;
        };
        m.name.name == format!("_{}_threads", next_m.name.name)
    }

    fn trim_trailing_blank_line(&mut self) {
        while self.out.ends_with("\n\n") {
            self.out.pop();
        }
    }

    fn line(&mut self, s: &str) {
        // Drain any SV-portability index-hoist temporaries synthesized
        // while building `s` (see `index_hoist_temps`), placing them
        // wherever the enclosing SV scope actually permits a declaration
        // and an assignment (see `HoistScope`). Empty in the overwhelming
        // common case (no hoist was needed), so this is a no-op
        // borrow-and-check for ordinary lines.
        let pending: Vec<HoistTemp> = {
            let mut hoists = self.index_hoist_temps.borrow_mut();
            if hoists.is_empty() {
                Vec::new()
            } else {
                std::mem::take(&mut *hoists)
            }
        };
        if !pending.is_empty() {
            self.place_hoist_temps(&pending);
        }
        let line_start = self.out.len();
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
        self.track_hoist_scope(s, line_start);
    }

    /// Append one already-rendered line at the current indent, without
    /// any hoist drain or scope tracking (`line()`'s inner half). Used to
    /// emit hoist temps themselves — they can never contain a `begin`, an
    /// `always_*` opener, or a further hoist.
    fn raw_line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Render `pending` hoist temps into the right place for the scope
    /// `line()` is currently emitting into (arch#846).
    fn place_hoist_temps(&mut self, pending: &[HoistTemp]) {
        fn push_at(buf: &mut String, indent: usize, s: &str) {
            for _ in 0..indent {
                buf.push_str("  ");
            }
            buf.push_str(s);
            buf.push('\n');
        }
        // arch#861: a temp reading the loop iterator declares at the top of
        // the loop body and is assigned in place, whatever the enclosing
        // scope. Split it out first — one `always_*` block can mix these
        // with ordinary temps, and the two go to different places. Done
        // before the `hoist_scope` match because the destination is the
        // loop body, not the block.
        let (in_loop, pending): (Vec<&HoistTemp>, Vec<&HoistTemp>) = pending
            .iter()
            .partition(|t| t.in_loop && self.loop_body_anchor.is_some());
        if let Some((at, indent)) = self.loop_body_anchor {
            if !in_loop.is_empty() {
                let mut text = String::new();
                for t in &in_loop {
                    push_at(&mut text, indent, &t.decl_line());
                }
                self.out.insert_str(at, &text);
                let grew = text.len();
                self.loop_body_anchor = Some((at + grew, indent));
                // A splice at the loop-body top sits *after* an enclosing
                // block's opener, so `Procedural`'s module-scope anchor is
                // unaffected; the reverse is not true and is fixed up below.
            }
        }
        match self.hoist_scope {
            HoistScope::Module => {
                for t in &pending {
                    let decl = t.decl_line();
                    let asg = t.continuous_assign_line();
                    self.raw_line(&decl);
                    self.raw_line(&asg);
                }
            }
            HoistScope::Procedural { at, indent, depth } => {
                // Splice declaration + continuous assign out to module
                // scope, immediately above the `always_*` opener. `at` is
                // advanced so a second hoist from the same block lands
                // after the first, preserving generation order (an outer
                // hoist's RHS can reference an inner one).
                let mut text = String::new();
                for t in &pending {
                    push_at(&mut text, indent, &t.decl_line());
                    push_at(&mut text, indent, &t.continuous_assign_line());
                }
                self.out.insert_str(at, &text);
                let grew = text.len();
                self.hoist_scope = HoistScope::Procedural {
                    at: at + grew,
                    indent,
                    depth,
                };
                // This splice landed *before* the loop-body anchor, so
                // shift it by what was inserted or the next in-loop
                // declaration lands mid-statement.
                if let Some((loop_at, loop_indent)) = self.loop_body_anchor {
                    if loop_at >= at {
                        self.loop_body_anchor = Some((loop_at + grew, loop_indent));
                    }
                }
            }
            HoistScope::Function { at, indent } => {
                // Only the declaration moves (to the top of the function
                // body, where SV requires declarations); the value must be
                // computed in place, and as a blocking assignment because a
                // continuous `assign` to an automatic-lifetime variable is
                // illegal.
                let mut text = String::new();
                for t in &pending {
                    push_at(&mut text, indent, &t.decl_line());
                }
                self.out.insert_str(at, &text);
                let grew = text.len();
                self.hoist_scope = HoistScope::Function {
                    at: at + grew,
                    indent,
                };
                if let Some((loop_at, loop_indent)) = self.loop_body_anchor {
                    if loop_at >= at {
                        self.loop_body_anchor = Some((loop_at + grew, loop_indent));
                    }
                }
                for t in &pending {
                    let asg = t.blocking_assign_line();
                    self.raw_line(&asg);
                }
            }
        }
        // arch#861 in-loop temps: declaration was spliced to the loop-body
        // top above; the value is computed here, inline, where the iterator
        // is in scope. Emitted after the scope match so the assignment
        // follows any temp this same drain moved to module scope (an
        // in-loop RHS may read one).
        for t in &in_loop {
            let asg = t.blocking_assign_line();
            self.raw_line(&asg);
        }
    }

    /// Update `hoist_scope` after `line()` emitted `s` starting at byte
    /// offset `line_start` in `out`.
    ///
    /// Only `Module` <-> `Procedural` is tracked here; `Function` is
    /// entered and left explicitly by `emit_function`, and a function body
    /// can never contain an `always_*` block, so it is left alone.
    fn track_hoist_scope(&mut self, s: &str, line_start: usize) {
        if matches!(self.hoist_scope, HoistScope::Function { .. }) {
            return;
        }
        let code = Self::strip_sv_comments_and_strings(s);
        let delta = Self::begin_end_delta(&code);
        match self.hoist_scope {
            HoistScope::Module => {
                // A procedural block that opens without a `begin` is a
                // single-statement `always_* <stmt>;` — nothing follows it
                // inside the block, and any hoist for that statement was
                // already drained (at module scope) before this line.
                if delta > 0 && Self::opens_procedural_block(&code) {
                    self.hoist_scope = HoistScope::Procedural {
                        at: line_start,
                        indent: self.indent,
                        depth: delta,
                    };
                }
            }
            HoistScope::Procedural { at, indent, depth } => {
                let d = depth + delta;
                self.hoist_scope = if d <= 0 {
                    HoistScope::Module
                } else {
                    HoistScope::Procedural {
                        at,
                        indent,
                        depth: d,
                    }
                };
            }
            HoistScope::Function { .. } => unreachable!("filtered above"),
        }
    }

    /// Blank out `"..."` string literals and `// ...` trailing comments so
    /// `begin`/`end` counting can't be thrown off by prose inside an
    /// `$error`/`$fatal` message or a generated comment.
    fn strip_sv_comments_and_strings(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        let mut in_str = false;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if in_str {
                if c == '\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if c == '"' {
                    in_str = false;
                }
                out.push(' ');
                i += 1;
                continue;
            }
            if c == '"' {
                in_str = true;
                out.push(' ');
                i += 1;
                continue;
            }
            if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                break;
            }
            out.push(c);
            i += 1;
        }
        out
    }

    /// `+1` per `begin` keyword, `-1` per `end` keyword, matching whole
    /// identifiers only — so `endcase`/`endfunction`/`endgenerate`/
    /// `endmodule` and any identifier merely *containing* "end" are not
    /// miscounted, and `end else begin` nets out to zero.
    fn begin_end_delta(code: &str) -> i32 {
        let mut delta = 0;
        for tok in code.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')) {
            match tok {
                "begin" => delta += 1,
                "end" => delta -= 1,
                _ => {}
            }
        }
        delta
    }

    /// Does `code` open an SV procedural block? Keyed on the first token,
    /// which is how every emitter in `src/codegen/` writes these.
    fn opens_procedural_block(code: &str) -> bool {
        let first = code
            .trim_start()
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("");
        matches!(
            first,
            "always" | "always_ff" | "always_comb" | "always_latch" | "initial" | "final"
        )
    }

    /// Emit one inst-site param override `.NAME(...)`. Handles two cases:
    ///
    /// 1. **Value override** (`pa.ty == None`) — emit `.NAME(<expr>)`.
    /// 2. **Type override** (`pa.ty == Some(te)`) — the override targets
    ///    a child param declared as `param NAME: type = ...`. SV codegen
    ///    has two conventions for these:
    ///    - `fifo` synthesizes an int `parameter DATA_WIDTH` from the
    ///      type-param's bit width. So a type override translates to
    ///      `.DATA_WIDTH(<bits-of-new-type>)` at the inst site.
    ///    - User modules emit type-typed params as `parameter int NAME`
    ///      (legacy quirk; type params on user modules aren't fully
    ///      supported at SV level today). Type overrides for those emit
    ///      `.NAME(<bits-of-new-type>)` as a best-effort.
    fn emit_param_override(&self, child: &str, pa: &ParamAssign) -> String {
        let Some(te) = &pa.ty else {
            return format!(".{}({})", pa.name.name, self.emit_expr_str(&pa.value));
        };
        let width = self
            .type_expr_data_width(te)
            .unwrap_or_else(|| "0".to_string());
        // Map T → DATA_WIDTH for constructs that erase type params into a
        // synthesized packed payload width.
        let is_fifo_type_param = self.source.items.iter().any(|it| match it {
            Item::Fifo(f) if f.name.name == child => f.params.iter().any(|p| {
                p.name.name == pa.name.name && matches!(p.kind, crate::ast::ParamKind::Type(_))
            }),
            _ => false,
        });
        let is_ram_type_param = self.source.items.iter().any(|it| match it {
            Item::Ram(r) if r.name.name == child => r.params.iter().any(|p| {
                p.name.name == pa.name.name && matches!(p.kind, crate::ast::ParamKind::Type(_))
            }),
            _ => false,
        });
        if is_fifo_type_param || is_ram_type_param {
            format!(".DATA_WIDTH({width})")
        } else {
            format!(".{}({width})", pa.name.name)
        }
    }

    fn emit_param_decl(&mut self, p: &ParamDecl, comma: &str) {
        let default_str = if let Some(d) = &p.default {
            format!(" = {}", self.emit_expr_str(d))
        } else {
            String::new()
        };
        let kw = if p.is_local {
            "localparam"
        } else {
            "parameter"
        };
        // Optional post-name unpacked dim: `param NAME: T [N]` →
        // SV `parameter T NAME [N]`. Goes after the param name and
        // before the `=` default. Used to forward upstream-SV
        // unpacked-array params like `pmp_cfg_t [PMP_MAX_REGIONS]`.
        let unpacked_str = if let Some(sz) = &p.unpacked_size {
            format!(" [{}]", self.emit_expr_str(sz))
        } else {
            String::new()
        };
        match &p.kind {
            ParamKind::WidthConst(hi, lo) => {
                let hi_s = self.emit_expr_str(hi);
                let lo_s = self.emit_expr_str(lo);
                self.line(&format!(
                    "{kw} [{}:{}] {}{}{}{}",
                    hi_s, lo_s, p.name.name, unpacked_str, default_str, comma
                ));
            }
            ParamKind::EnumConst(enum_name) => {
                self.line(&format!(
                    "{kw} {} {}{}{}{}",
                    enum_name, p.name.name, unpacked_str, default_str, comma
                ));
            }
            ParamKind::Logic(ty) => {
                // Emit as `parameter <packed-bits> NAME [unpacked]? = ...`.
                // emit_port_type_str returns "logic [W-1:0]" for UInt/SInt
                // and just "logic" for Bool; we want the bit-range form
                // without the leading `logic` keyword (SV `parameter`
                // doesn't take `logic` as the type qualifier in the
                // same way `input/output` does).
                let ty_str = self.emit_port_type_str(ty);
                let ty_qual = ty_str
                    .strip_prefix("logic")
                    .map(|r| r.trim_start())
                    .unwrap_or(&ty_str);
                if ty_qual.is_empty() {
                    self.line(&format!(
                        "{kw} {}{}{}{}",
                        p.name.name, unpacked_str, default_str, comma
                    ));
                } else {
                    self.line(&format!(
                        "{kw} {} {}{}{}{}",
                        ty_qual, p.name.name, unpacked_str, default_str, comma
                    ));
                }
            }
            ParamKind::ConstVec(ty) => {
                // Vec<T, N> param. iverilog rejects unpacked-array parameters,
                // so emit a packed `parameter logic [N*W-1:0] NAME = {…}` and
                // expose a sibling `wire NAME_arr [0:N-1]` (driven elsewhere
                // in the module body) for `NAME[i]` indexing.
                //
                // Default `{a, b, c, …}` (parsed as ExprKind::Concat) packs
                // with reversed ordering so `NAME[0]` lands at the LSB and
                // matches the user's literal index — `parts[0]` = LSB chunk.
                let (elem_ty, size_expr) = match ty {
                    TypeExpr::Vec(elem, size) => (elem.as_ref().clone(), (**size).clone()),
                    _ => {
                        self.line(&format!("{kw} int {}{}{}", p.name.name, default_str, comma));
                        return;
                    }
                };
                let elem_w_expr = match &elem_ty {
                    TypeExpr::UInt(w) | TypeExpr::SInt(w) => (**w).clone(),
                    _ => Expr::new(ExprKind::Literal(LitKind::Dec(1)), p.span),
                };
                let elem_w_s = self.emit_expr_str(&elem_w_expr);
                let signed = matches!(&elem_ty, TypeExpr::SInt(_));
                let signed_kw = if signed { "signed " } else { "" };
                let size_s = self.emit_expr_str(&size_expr);
                let default_packed = if let Some(d) = &p.default {
                    if let ExprKind::Concat(parts) = &d.kind {
                        // Reverse so parts[0] is the LSB chunk → NAME[0] reads parts[0].
                        let mut rev: Vec<&Expr> = parts.iter().collect();
                        rev.reverse();
                        let chunks: Vec<String> = rev
                            .iter()
                            .map(|e| format!("({})'({})", elem_w_s, self.emit_expr_str(e)))
                            .collect();
                        format!(" = {{{}}}", chunks.join(", "))
                    } else {
                        format!(" = {}", self.emit_expr_str(d))
                    }
                } else {
                    String::new()
                };
                self.line(&format!(
                    "{kw} logic {signed_kw}[({size_s})*({elem_w_s})-1:0] {}{default_packed}{comma}",
                    p.name.name
                ));
            }
            _ => {
                self.line(&format!(
                    "{kw} int {}{}{}{}",
                    p.name.name, unpacked_str, default_str, comma
                ));
            }
        }
    }

    pub(crate) fn emit_domain(&mut self, d: &DomainDecl) {
        self.line(&format!("// domain {}", d.name.name));
        for field in &d.fields {
            self.line(&format!(
                "//   {}: {}",
                field.name.name,
                self.emit_expr_str(&field.value)
            ));
        }
        self.line("");
    }

    /// Compute a short tag string for a TypeExpr used in mangled function names.
    /// `UInt<8>` → "8", `SInt<16>` → "s16", `Bool` → "b", etc.
    fn type_mangle_tag(te: &TypeExpr) -> String {
        match te {
            TypeExpr::UInt(e) => Self::expr_simple_str(e),
            TypeExpr::SInt(e) => format!("s{}", Self::expr_simple_str(e)),
            TypeExpr::Bool => "b".to_string(),
            TypeExpr::Bit => "1".to_string(),
            TypeExpr::Named(n) => n.name.clone(),
            _ => "x".to_string(),
        }
    }

    fn expr_simple_str(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(n)) => n.to_string(),
            ExprKind::Literal(LitKind::Hex(n)) => n.to_string(),
            _ => "x".to_string(),
        }
    }

    /// Return the SV name for a function overload.  When a name has multiple overloads,
    /// mangle as `Name_W1_W2` using the declared arg type widths (e.g. `Xtime_8`).
    fn sv_function_name(&self, f: &FunctionDecl) -> String {
        if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(&f.name.name) {
            if overloads.len() > 1 {
                let suffix: String = f
                    .args
                    .iter()
                    .map(|a| Self::type_mangle_tag(&a.ty))
                    .collect::<Vec<_>>()
                    .join("_");
                return format!("{}_{}", f.name.name, suffix);
            }
        }
        f.name.name.clone()
    }

    /// Collect declared types visible inside a function body: params plus
    /// every explicitly-typed `let`, recursively through if/for bodies.
    fn collect_fn_local_types(f: &FunctionDecl) -> std::collections::HashMap<String, TypeExpr> {
        fn walk(items: &[FunctionBodyItem], out: &mut std::collections::HashMap<String, TypeExpr>) {
            for item in items {
                match item {
                    FunctionBodyItem::Let(l) => {
                        if let Some(t) = &l.ty {
                            out.insert(l.name.name.clone(), t.clone());
                        }
                    }
                    FunctionBodyItem::IfElse(ie) => {
                        walk(&ie.then_body, out);
                        walk(&ie.else_body, out);
                    }
                    FunctionBodyItem::For(fl) => walk(&fl.body, out),
                    _ => {}
                }
            }
        }
        let mut out = std::collections::HashMap::new();
        for a in &f.args {
            out.insert(a.name.name.clone(), a.ty.clone());
        }
        walk(&f.body, &mut out);
        out
    }

    /// Emit every top-level (`Item::Function`) and `package` function
    /// collected into `pending_functions` as a local `function automatic`
    /// declaration inside the construct currently being emitted.
    ///
    /// SystemVerilog has no free functions: a `function automatic` must
    /// live inside a module (or package/`$unit`), so ARCH copies the whole
    /// set into each emitted module. Every construct that can emit a call
    /// to a user function must therefore run this at the top of its module
    /// body — which for a long time only `emit_module` did, so a `pipeline`
    /// / `fsm` / `fifo` / … that called one emitted SV referencing an
    /// undeclared function and no frontend would accept it (arch#852).
    /// Emitting the full set (rather than only the ones this construct
    /// references) matches `emit_module`'s long-standing behavior; an
    /// unused `function automatic` is legal, unused SV.
    ///
    /// `pending_functions` is taken and restored because `emit_function`
    /// needs `&mut self` and the same set is re-emitted into every
    /// following construct in the file.
    pub(crate) fn emit_pending_functions(&mut self) {
        let fns = std::mem::take(&mut self.pending_functions);
        for f in &fns {
            self.emit_function(f);
        }
        self.pending_functions = fns;
    }

    fn emit_function(&mut self, f: &FunctionDecl) {
        self.fn_local_types = Self::collect_fn_local_types(f);
        let sv_name = self.sv_function_name(f);
        let ret_str = self.emit_type_str(&f.ret_ty);
        let args_str: Vec<String> = f
            .args
            .iter()
            .map(|a| format!("input {} {}", self.emit_type_str(&a.ty), a.name.name))
            .collect();
        self.line(&format!(
            "function automatic {} {}({});",
            ret_str,
            sv_name,
            args_str.join(", ")
        ));
        self.indent += 1;
        // Hoist temps synthesized inside this body get their declaration
        // spliced here — just past the header, the only place SV allows a
        // body declaration — while the assignment stays in place (arch#846).
        // Module scope would be wrong: the RHS may reference `f`'s
        // arguments or locals.
        let saved_scope = self.hoist_scope;
        self.hoist_scope = HoistScope::Function {
            at: self.out.len(),
            indent: self.indent,
        };
        for item in &f.body {
            match item {
                FunctionBodyItem::Let(l) => {
                    let ty_str = if let Some(ann) = &l.ty {
                        self.emit_type_str(ann)
                    } else {
                        "logic".to_string()
                    };
                    let val = self.emit_expr_str(&l.value);
                    self.line(&format!("{} {} = {};", ty_str, l.name.name, val));
                }
                FunctionBodyItem::Return(expr) => {
                    let val = self.emit_expr_str(expr);
                    self.line(&format!("return {};", val));
                }
                FunctionBodyItem::IfElse(ie) => {
                    self.emit_function_if(ie);
                }
                FunctionBodyItem::For(fl) => {
                    self.emit_function_for(fl);
                }
                FunctionBodyItem::Assign(a) => {
                    let target = self.emit_expr_str(&a.target);
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("{target} = {val};"));
                }
            }
        }
        self.hoist_scope = saved_scope;
        self.indent -= 1;
        self.line("endfunction");
        self.line("");
        self.fn_local_types.clear();
    }

    // ── `shared function` support ────────────────────────────────────
    //
    // A `shared function NAME(...)` (declared with the `shared`
    // contextual keyword) avoids per-call-site body inlining when
    // called from multiple states of the same thread. yosys CSE does
    // not merge identical calls across thread states because each
    // call appears under a different `if (_tN_state == M)` arm — the
    // synth tool sees them as distinct cones. With `shared`, codegen
    // emits ONE module-scope `assign __shared_FN_out = FN(<muxed>)`
    // wired through a per-state operand mux, mirroring upstream
    // hand-written shared-MAC FSM patterns.
    //
    // Lifecycle in `emit_module`:
    //   1. `collect_shared_calls(m)` walks the module body, recording
    //      every FunctionCall whose source-level fn is `shared` AND
    //      whose enclosing state predicate is unambiguous. Returns
    //      one `SharedHarness` per (sv_fn_name, state_reg) pair, with
    //      one mux entry per state literal. Also populates
    //      `self.shared_call_sites` so `emit_expr_str` rewrites the
    //      call to `__shared_<sv_fn_name>_out`.
    //   2. `emit_shared_harnesses(...)` emits the wire decls and the
    //      per-state always_comb mux + the single `assign __shared_FN_out
    //      = FN(<wires>);` continuous-assign. Called BEFORE the body
    //      bucket loop so the wires are visible to all later
    //      references.
    //
    // Conservative on ambiguity:
    //   - Call site outside any `_tN_state == LIT` predicate → no rewrite
    //     (inline as before).
    //   - Two call sites with the same state predicate but different args
    //     → typecheck error (`shared` would silently duplicate the body,
    //     defeating the area saving). User is told to either match the
    //     args or drop the `shared` keyword.

    /// One shared-function harness instance. Identified by (sv_fn_name,
    /// state_reg) — multiple state-reg keys would produce multiple
    /// harnesses (one MAC per thread). Each `entries` row is one state
    /// literal → operand-args mapping; the always_comb mux ORs them by
    /// `if (state_reg == lit)` arms.
    fn collect_shared_calls(&mut self, m: &ModuleDecl) -> Vec<SharedHarness> {
        // (sv_fn_name, state_reg) → harness builder.
        let mut harnesses: std::collections::BTreeMap<(String, String), SharedHarness> =
            std::collections::BTreeMap::new();
        // Track call sites we successfully bind to a harness so
        // emit_expr_str can rewrite them.
        let mut errors: Vec<CompileWarning> = Vec::new();
        // Build name→FunctionDecl map from BOTH the current module's
        // body AND `pending_functions` (top-level / package fns).
        // Threads-submodule lowerings copy the function into the
        // submodule body, so `m.body` is the authoritative source for
        // the threads case; top-level Item::Function and Package fns
        // come from `pending_functions`.
        let mut fn_decls: std::collections::HashMap<String, FunctionDecl> =
            std::collections::HashMap::new();
        for f in &self.pending_functions {
            fn_decls.insert(f.name.name.clone(), f.clone());
        }
        for it in &m.body {
            if let ModuleBodyItem::Function(f) = it {
                fn_decls.insert(f.name.name.clone(), f.clone());
            }
        }
        for item in &m.body {
            self.walk_item_for_shared(item, None, &mut harnesses, &mut errors, &fn_decls);
        }
        for w in errors {
            self.warnings.push(w);
        }
        harnesses.into_iter().map(|(_, h)| h).collect()
    }

    fn walk_item_for_shared(
        &mut self,
        item: &ModuleBodyItem,
        state_pred: Option<(&str, u64)>,
        harnesses: &mut std::collections::BTreeMap<(String, String), SharedHarness>,
        errors: &mut Vec<CompileWarning>,
        fn_decls: &std::collections::HashMap<String, FunctionDecl>,
    ) {
        match item {
            ModuleBodyItem::CombBlock(cb) => {
                for s in &cb.stmts {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            ModuleBodyItem::RegBlock(rb) => {
                for s in &rb.stmts {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            ModuleBodyItem::LatchBlock(lb) => {
                for s in &lb.stmts {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            ModuleBodyItem::LetBinding(l) => {
                self.walk_expr_for_shared(&l.value, state_pred, harnesses, errors, fn_decls);
            }
            _ => {}
        }
    }

    fn walk_stmt_for_shared(
        &mut self,
        stmt: &Stmt,
        state_pred: Option<(&str, u64)>,
        harnesses: &mut std::collections::BTreeMap<(String, String), SharedHarness>,
        errors: &mut Vec<CompileWarning>,
        fn_decls: &std::collections::HashMap<String, FunctionDecl>,
    ) {
        match stmt {
            Stmt::Assign(a) => {
                self.walk_expr_for_shared(&a.target, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(&a.value, state_pred, harnesses, errors, fn_decls);
            }
            Stmt::IfElse(ie) => {
                // Detect `_tN_state == LIT` shape and push the new
                // predicate while walking the then-branch. The else-
                // branch is walked under the OUTER predicate (an `else`
                // arm doesn't refine the state to a single value).
                let new_pred = Self::extract_state_predicate(&ie.cond);
                let then_pred = match (&new_pred, state_pred) {
                    (Some((reg, lit)), None) => Some((reg.as_str(), *lit)),
                    // Already inside a state arm: nested `_tN_state == K`
                    // would only be reachable when state == K AND state
                    // == OUTER, which is unsatisfiable unless K == OUTER.
                    // Safer: keep the outer predicate (don't refine).
                    (Some(_), Some(p)) => Some(p),
                    (None, p) => p,
                };
                // Walk cond under outer predicate (cond is evaluated to
                // pick the branch — its FunctionCall children, if any,
                // aren't gated by the inner predicate).
                self.walk_expr_for_shared(&ie.cond, state_pred, harnesses, errors, fn_decls);
                // Cannot pass &str borrow + recurse if we restructured
                // owned Strings; clone to a local owned String so the
                // borrow checker is happy.
                let then_pred_owned = then_pred.map(|(r, l)| (r.to_string(), l));
                let then_pred_ref = then_pred_owned.as_ref().map(|(r, l)| (r.as_str(), *l));
                for s in &ie.then_stmts {
                    self.walk_stmt_for_shared(s, then_pred_ref, harnesses, errors, fn_decls);
                }
                for s in &ie.else_stmts {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            Stmt::Match(m) => {
                self.walk_expr_for_shared(&m.scrutinee, state_pred, harnesses, errors, fn_decls);
                for arm in &m.arms {
                    for s in &arm.body {
                        self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                    }
                }
            }
            Stmt::For(fl) => {
                for s in &fl.body {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            Stmt::Init(ib) => {
                for s in &ib.body {
                    self.walk_stmt_for_shared(s, state_pred, harnesses, errors, fn_decls);
                }
            }
            Stmt::Log(_) | Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {}
        }
    }

    fn walk_expr_for_shared(
        &mut self,
        expr: &Expr,
        state_pred: Option<(&str, u64)>,
        harnesses: &mut std::collections::BTreeMap<(String, String), SharedHarness>,
        errors: &mut Vec<CompileWarning>,
        fn_decls: &std::collections::HashMap<String, FunctionDecl>,
    ) {
        match &expr.kind {
            ExprKind::FunctionCall(name, args) => {
                // Recurse into args first so nested shared calls inside
                // an outer shared call's arg also get rewritten.
                for a in args {
                    self.walk_expr_for_shared(a, state_pred, harnesses, errors, fn_decls);
                }
                // Is this a shared function?
                let is_shared = matches!(
                    self.symbols.globals.get(name),
                    Some((Symbol::Function(ovs), _)) if ovs.iter().any(|o| o.shared)
                );
                if !is_shared {
                    return;
                }
                let Some((reg, lit)) = state_pred else {
                    // Outside a state predicate: fall back to inline.
                    return;
                };
                // Resolve the FunctionDecl (for arg names + types at
                // harness emission). If the decl isn't visible in this
                // module, we can't synthesize the harness — fall back
                // to inline.
                let Some(fd) = fn_decls.get(name).cloned() else {
                    return;
                };
                // Resolve mangled SV name (mirrors emit_expr_str's
                // FunctionCall arm).
                let sv_name = self.fn_call_sv_name(name, expr);
                let key = (sv_name.clone(), reg.to_string());
                let arg_strs: Vec<String> = args.iter().map(|a| self.emit_expr_str(a)).collect();

                let entry = harnesses.entry(key).or_insert_with(|| {
                    SharedHarness::new(name.clone(), sv_name.clone(), reg.to_string(), fd)
                });
                // Look for existing entry under this state literal.
                if let Some(existing) = entry.entries.iter().find(|e| e.state_lit == lit) {
                    if existing.arg_strs != arg_strs {
                        // Same state, different args — would need two
                        // MACs. Typecheck error: surface as a warning
                        // for now (CompileError plumbing through the
                        // codegen path is heavier; the build will fail
                        // visibly anyway because the rewrite produces
                        // the wrong result if we proceeded).
                        errors.push(CompileWarning {
                            message: format!(
                                "shared function {} called with different args in same state {}; \
                                 this would require multiple instances. Either change the args to \
                                 match, or hand-rewrite without `shared`.",
                                name, lit
                            ),
                            span: expr.span,
                        });
                        return;
                    }
                    // Identical args — merge by recording this call
                    // site for rewrite without adding another mux entry.
                    self.shared_call_sites
                        .insert(expr.span.start, format!("__shared_{sv_name}_out"));
                    return;
                }
                // New entry: record args + rewrite this call site.
                entry.entries.push(SharedHarnessEntry {
                    state_lit: lit,
                    arg_strs,
                    args: args.clone(),
                });
                self.shared_call_sites
                    .insert(expr.span.start, format!("__shared_{sv_name}_out"));
            }
            ExprKind::Binary(_, a, b) => {
                self.walk_expr_for_shared(a, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(b, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::Unary(_, a) => {
                self.walk_expr_for_shared(a, state_pred, harnesses, errors, fn_decls)
            }
            ExprKind::FieldAccess(e, _) => {
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls)
            }
            ExprKind::MethodCall(recv, _, margs) => {
                self.walk_expr_for_shared(recv, state_pred, harnesses, errors, fn_decls);
                for a in margs {
                    self.walk_expr_for_shared(a, state_pred, harnesses, errors, fn_decls);
                }
            }
            ExprKind::Cast(e, _) => {
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls)
            }
            ExprKind::Index(b, i) => {
                self.walk_expr_for_shared(b, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(i, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::BitSlice(b, h, l) => {
                self.walk_expr_for_shared(b, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(h, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(l, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::PartSelect(b, s, w, _) => {
                self.walk_expr_for_shared(b, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(s, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(w, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::Concat(es) => {
                for e in es {
                    self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls);
                }
            }
            ExprKind::Repeat(n, e) => {
                self.walk_expr_for_shared(n, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::Clog2(e)
            | ExprKind::Onehot(e)
            | ExprKind::Signed(e)
            | ExprKind::Unsigned(e) => {
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::Ternary(c, t, f) => {
                self.walk_expr_for_shared(c, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(t, state_pred, harnesses, errors, fn_decls);
                self.walk_expr_for_shared(f, state_pred, harnesses, errors, fn_decls);
            }
            ExprKind::Inside(s, members) => {
                self.walk_expr_for_shared(s, state_pred, harnesses, errors, fn_decls);
                for m in members {
                    match m {
                        InsideMember::Single(e) => {
                            self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls)
                        }
                        InsideMember::Range(a, b) => {
                            self.walk_expr_for_shared(a, state_pred, harnesses, errors, fn_decls);
                            self.walk_expr_for_shared(b, state_pred, harnesses, errors, fn_decls);
                        }
                    }
                }
            }
            ExprKind::ExprMatch(s, arms) => {
                self.walk_expr_for_shared(s, state_pred, harnesses, errors, fn_decls);
                for arm in arms {
                    self.walk_expr_for_shared(&arm.value, state_pred, harnesses, errors, fn_decls);
                }
            }
            ExprKind::SvaNext(_, e) => {
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls)
            }
            ExprKind::LatencyAt(e, _) => {
                self.walk_expr_for_shared(e, state_pred, harnesses, errors, fn_decls)
            }
            ExprKind::StructLiteral(_, fields) => {
                for fi in fields {
                    self.walk_expr_for_shared(&fi.value, state_pred, harnesses, errors, fn_decls);
                }
            }
            // Leaf nodes: literals, idents, enum variants, todo, bool.
            _ => {}
        }
    }

    /// Match `_tN_state == LIT` exactly. Returns `(state_reg_name, lit_value)`.
    /// Unrecognized shapes return None — the caller falls back to
    /// inline (no shared rewrite) so we never silently mis-gate a call.
    fn extract_state_predicate(cond: &Expr) -> Option<(String, u64)> {
        if let ExprKind::Binary(BinOp::Eq, lhs, rhs) = &cond.kind {
            // _tN_state on the LHS, literal on the RHS (the shape
            // emitted by elaborate.rs).
            if let (ExprKind::Ident(name), ExprKind::Literal(LitKind::Dec(n))) =
                (&lhs.kind, &rhs.kind)
            {
                if name.starts_with("_t") && name.ends_with("_state") {
                    return Some((name.clone(), *n));
                }
            }
        }
        None
    }

    /// Resolve the SV emission name a `FunctionCall(name, args)` would
    /// produce. Mirrors the overload-mangling logic in `emit_expr_str`.
    fn fn_call_sv_name(&self, name: &str, expr: &Expr) -> String {
        if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(name) {
            if overloads.len() > 1 {
                let idx = self
                    .overload_map
                    .get(&expr.span.start)
                    .copied()
                    .unwrap_or(0);
                let ov = &overloads[idx];
                let suffix: String = ov
                    .arg_types
                    .iter()
                    .map(|t| Self::type_mangle_tag(t))
                    .collect::<Vec<_>>()
                    .join("_");
                return format!("{name}_{suffix}");
            }
        }
        name.to_string()
    }

    /// Emit the per-shared-function harness: input wires, per-state
    /// operand mux (always_comb), and the single continuous-assign
    /// that calls FN once. Called from `emit_module` BEFORE the main
    /// body emission loop so the wires precede every reference.
    fn emit_shared_harnesses(&mut self, harnesses: &[SharedHarness]) {
        for h in harnesses {
            // FunctionDecl was cached at collection time so we can
            // borrow self mutably here without re-traversing the
            // module body.
            let fd: &FunctionDecl = &h.fn_decl;

            // 1. Wire decls.
            self.line(&format!(
                "// shared function harness — single instance of {} muxed by {}",
                h.src_name, h.state_reg
            ));
            for arg in &fd.args {
                let ty_str = self.emit_type_str(&arg.ty);
                self.line(&format!(
                    "{} __shared_{}_in_{};",
                    ty_str, h.sv_name, arg.name.name
                ));
            }
            let ret_ty_str = self.emit_type_str(&fd.ret_ty);
            self.line(&format!("{} __shared_{}_out;", ret_ty_str, h.sv_name));

            // 2. Per-state operand mux. Defaults zero each input, then
            //    each state arm overrides the inputs with that state's
            //    args. SV `always_comb` ensures no latch. `unique case`
            //    on the state register tells the synthesizer the arms
            //    are mutually exclusive (one entry per thread state),
            //    so it produces a parallel mux instead of inferring
            //    priority across N independent `if (state == K)` tests.
            self.line("always_comb begin");
            self.indent += 1;
            for arg in &fd.args {
                self.line(&format!(
                    "__shared_{}_in_{} = '0;",
                    h.sv_name, arg.name.name
                ));
            }
            self.line(&format!("unique case ({})", h.state_reg));
            self.indent += 1;
            for entry in &h.entries {
                self.line(&format!("{}: begin", entry.state_lit));
                self.indent += 1;
                for (arg_decl, arg_str) in fd.args.iter().zip(entry.arg_strs.iter()) {
                    self.line(&format!(
                        "__shared_{}_in_{} = {};",
                        h.sv_name, arg_decl.name.name, arg_str
                    ));
                }
                self.indent -= 1;
                self.line("end");
            }
            self.line("default: ;");
            self.indent -= 1;
            self.line("endcase");
            self.indent -= 1;
            self.line("end");

            // 3. Single function call. The args are fed by the wires
            //    above, so yosys synthesizes ONE evaluation of the
            //    function body.
            let arg_wires: Vec<String> = fd
                .args
                .iter()
                .map(|a| format!("__shared_{}_in_{}", h.sv_name, a.name.name))
                .collect();
            self.line(&format!(
                "assign __shared_{}_out = {}({});",
                h.sv_name,
                h.sv_name,
                arg_wires.join(", ")
            ));
            self.line("");
        }
    }

    fn emit_function_body_items(&mut self, items: &[FunctionBodyItem]) {
        for item in items {
            match item {
                FunctionBodyItem::Let(l) => {
                    let ty_str = if let Some(ann) = &l.ty {
                        self.emit_type_str(ann)
                    } else {
                        "logic".to_string()
                    };
                    let val = self.emit_expr_str(&l.value);
                    self.line(&format!("{} {} = {};", ty_str, l.name.name, val));
                }
                FunctionBodyItem::Return(expr) => {
                    let val = self.emit_expr_str(expr);
                    self.line(&format!("return {};", val));
                }
                FunctionBodyItem::IfElse(ie) => self.emit_function_if(ie),
                FunctionBodyItem::For(fl) => self.emit_function_for(fl),
                FunctionBodyItem::Assign(a) => {
                    let target = self.emit_expr_str(&a.target);
                    let val = self.emit_expr_str(&a.value);
                    self.line(&format!("{target} = {val};"));
                }
            }
        }
    }

    fn emit_function_if(&mut self, ie: &FunctionIfElse) {
        let cond = self.emit_expr_str(&ie.cond);
        self.line(&format!("if ({cond}) begin"));
        self.indent += 1;
        self.emit_function_body_items(&ie.then_body);
        self.indent -= 1;
        if !ie.else_body.is_empty() {
            // Check if else body is a single elsif (nested IfElse)
            if ie.else_body.len() == 1 {
                if let FunctionBodyItem::IfElse(nested) = &ie.else_body[0] {
                    let ncond = self.emit_expr_str(&nested.cond);
                    self.line(&format!("end else if ({ncond}) begin"));
                    self.indent += 1;
                    self.emit_function_body_items(&nested.then_body);
                    self.indent -= 1;
                    if !nested.else_body.is_empty() {
                        self.line("end else begin");
                        self.indent += 1;
                        self.emit_function_body_items(&nested.else_body);
                        self.indent -= 1;
                    }
                    self.line("end");
                    return;
                }
            }
            self.line("end else begin");
            self.indent += 1;
            self.emit_function_body_items(&ie.else_body);
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_function_for(&mut self, fl: &FunctionFor) {
        let var = &fl.var.name;
        match &fl.range {
            ForRange::Range(lo, hi) => {
                let lo_s = self.emit_expr_str(lo);
                let hi_s = self.emit_expr_str(hi);
                self.line(&format!(
                    "for (int {var} = {lo_s}; {var} <= {hi_s}; {var}++) begin"
                ));
            }
            ForRange::ValueList(_vals) => {
                if let ForRange::ValueList(vals) = &fl.range {
                    for val in vals {
                        if let Some(v) =
                            self.eval_const_u32(val, &self.current_module_params.clone())
                        {
                            let old = self.loop_var_subst.insert(var.clone(), v);
                            self.emit_function_body_items(&fl.body);
                            if let Some(prev) = old {
                                self.loop_var_subst.insert(var.clone(), prev);
                            } else {
                                self.loop_var_subst.remove(var);
                            }
                        } else {
                            let v = self.emit_expr_str(val);
                            self.line(&format!(
                                "for (int {var} = {v}; {var} == {v}; {var}++) begin"
                            ));
                            self.indent += 1;
                            self.emit_function_body_items(&fl.body);
                            self.indent -= 1;
                            self.line("end");
                        }
                    }
                }
                return;
            }
        }
        self.indent += 1;
        self.emit_function_body_items(&fl.body);
        self.indent -= 1;
        self.line("end");
    }

    pub(crate) fn emit_package(&mut self, pkg: &PackageDecl) {
        self.line(&format!("package {};", pkg.name.name));
        self.indent += 1;

        // Typedefs must precede params: an `EnumConst` param references its
        // enum type, which SV requires forward-declared.
        for e in &pkg.enums {
            self.emit_enum(e);
        }
        for s in &pkg.structs {
            self.emit_struct(s);
        }

        // Dispatch on ParamKind: width-qualified params must emit
        // `localparam [hi:lo]`, not `int` (truncates >32-bit values).
        for p in &pkg.params {
            if let Some(d) = &p.default {
                let val = self.emit_expr_str(d);
                match &p.kind {
                    ParamKind::WidthConst(hi, lo) => {
                        let hi_s = self.emit_expr_str(hi);
                        let lo_s = self.emit_expr_str(lo);
                        self.line(&format!(
                            "localparam [{}:{}] {} = {};",
                            hi_s, lo_s, p.name.name, val
                        ));
                    }
                    ParamKind::EnumConst(enum_name) => {
                        self.line(&format!(
                            "localparam {} {} = {};",
                            enum_name, p.name.name, val
                        ));
                    }
                    ParamKind::Logic(ty) => {
                        let ty_str = self.emit_port_type_str(ty);
                        let ty_qual = ty_str
                            .strip_prefix("logic")
                            .map(|r| r.trim_start())
                            .unwrap_or(&ty_str);
                        if ty_qual.is_empty() {
                            self.line(&format!("localparam {} = {};", p.name.name, val));
                        } else {
                            self.line(&format!(
                                "localparam {} {} = {};",
                                ty_qual, p.name.name, val
                            ));
                        }
                    }
                    ParamKind::Const | ParamKind::Type(_) | ParamKind::ConstVec(_) => {
                        self.line(&format!("localparam int {} = {};", p.name.name, val));
                    }
                }
            }
        }

        // functions
        for f in &pkg.functions {
            self.emit_function(f);
        }

        self.indent -= 1;
        self.line("endpackage");
        self.line("");
    }

    pub(crate) fn emit_struct(&mut self, s: &StructDecl) {
        // Canonical ARCH packed-struct bit layout: first-declared field = MSB,
        // last-declared field = LSB — matching SV's `struct packed` convention
        // (fields listed top-to-bottom inside `struct packed { ... }` are emitted
        // MSB-first by the SV standard). Emit fields in declaration order.
        self.line("typedef struct packed {");
        self.indent += 1;
        for field in s.fields.iter() {
            let ty_str = self.emit_type_str(&field.ty);
            self.line(&format!("{} {};", ty_str, field.name.name));
        }
        self.indent -= 1;
        self.line(&format!("}} {};", s.name.name));
        self.line("");
    }

    pub(crate) fn emit_enum(&mut self, e: &EnumDecl) {
        // Compute effective values: explicit where provided, auto-sequential otherwise
        let effective_values: Vec<u64> = e
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_ref()
                    .and_then(|expr| match &expr.kind {
                        ExprKind::Literal(LitKind::Dec(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Hex(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Bin(n)) => Some(*n),
                        ExprKind::Literal(LitKind::Sized(_, n)) => Some(*n),
                        _ => None,
                    })
                    .unwrap_or(i as u64)
            })
            .collect();
        // Width: from max value (covers explicit encodings like one-hot)
        let max_val = effective_values.iter().copied().max().unwrap_or(0);
        let width = if max_val == 0 {
            1
        } else {
            64 - max_val.leading_zeros()
        };
        let width = std::cmp::max(width, enum_width(e.variants.len()));
        let variants: Vec<String> = e
            .variants
            .iter()
            .zip(effective_values.iter())
            .map(|(v, val)| format!("{} = {}'d{}", v.name.to_uppercase(), width, val))
            .collect();
        self.line(&format!(
            "typedef enum logic [{}:0] {{",
            width.saturating_sub(1)
        ));
        self.indent += 1;
        for (i, v) in variants.iter().enumerate() {
            if i < variants.len() - 1 {
                self.line(&format!("{v},"));
            } else {
                self.line(v);
            }
        }
        self.indent -= 1;
        self.line(&format!("}} {};", e.name.name));
        self.line("");
    }

    fn emit_for_loop_sv(
        &mut self,
        f: &ForLoop<Stmt>,
        mut emit_body_stmt: impl FnMut(&mut Self, &Stmt),
    ) {
        let var = &f.var.name;
        // Static unrolling for Vec-of-bus indexed access: the SV signature
        // exposes only the flattened `<port>_<i>_<sig>` names, with no
        // SV-level array of buses. A behavioral `for` loop indexing the
        // bus by the loop variable would emit `chans[i].sig`, which is
        // not legal SV. Detect the case and inline the body N times
        // with the loop variable bound to each literal index via
        // `loop_var_subst`.
        if let ForRange::Range(rs, re) = &f.range {
            // Param-driven bounds (e.g. `for i in 0..NUM-1`) fold against the
            // current module's params here so the unroll can fire on
            // `Vec<Bus, NUM>` ports/wires with a param-driven N.
            let start_lit = self.eval_const_u32(rs, &self.current_module_params.clone());
            let end_lit = self.eval_const_u32(re, &self.current_module_params.clone());
            if let (Some(start_lit), Some(end_lit)) = (start_lit, end_lit) {
                let body_touches_vob = f.body.iter().any(|s| {
                    Self::stmt_indexes_vob_with_var(
                        s,
                        var,
                        &self.vec_of_bus_port_count,
                        &self.vec_of_bus_wire_count,
                    )
                });
                if body_touches_vob {
                    for i in start_lit..=end_lit {
                        self.loop_var_subst.insert(var.clone(), i);
                        for s in &f.body {
                            emit_body_stmt(self, s);
                        }
                    }
                    self.loop_var_subst.remove(var);
                    return;
                }
            }
        }
        match &f.range {
            ForRange::Range(rs, re) => {
                let start = self.emit_expr_str(rs);
                let end = self.emit_expr_str(re);
                self.line(&format!(
                    "for (int {var} = {start}; {var} <= {end}; {var}++) begin"
                ));
                self.indent += 1;
                // `var` is a real SV loop-local `int`, not visible at module
                // scope — a hoist temp reading it must declare inside this
                // body (arch#861), so record both the iterator name and the
                // body's top-of-block offset for the body's duration.
                let newly_inserted = self.runtime_for_loop_vars.insert(var.clone());
                let prev_anchor = self.loop_body_anchor;
                self.loop_body_anchor = Some((self.out.len(), self.indent));
                for s in &f.body {
                    emit_body_stmt(self, s);
                }
                self.loop_body_anchor = prev_anchor;
                if newly_inserted {
                    self.runtime_for_loop_vars.remove(var);
                }
                self.indent -= 1;
                self.line("end");
            }
            ForRange::ValueList(vals) => {
                for v in vals {
                    let val = self.emit_expr_str(v);
                    // Emit as a for-loop with a single iteration for Icarus compatibility
                    // (Icarus doesn't support variable declarations inside always_* blocks)
                    self.line(&format!(
                        "for (int {var} = {val}; {var} == {val}; {var}++) begin"
                    ));
                    self.indent += 1;
                    let newly_inserted = self.runtime_for_loop_vars.insert(var.clone());
                    let prev_anchor = self.loop_body_anchor;
                    self.loop_body_anchor = Some((self.out.len(), self.indent));
                    for s in &f.body {
                        emit_body_stmt(self, s);
                    }
                    self.loop_body_anchor = prev_anchor;
                    if newly_inserted {
                        self.runtime_for_loop_vars.remove(var);
                    }
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// Does `stmt` contain any expression of the form
    /// `Index(Ident(name), Ident(var))` where `name` is a known Vec-of-bus
    /// port or wire? Used by `emit_for_loop_sv` to decide whether to
    /// statically unroll the loop. Recurses into nested ifs/matches/fors
    /// and into LHS-of-assign targets as well as RHS values.
    fn stmt_indexes_vob_with_var(
        stmt: &Stmt,
        var: &str,
        ports: &std::collections::HashMap<String, u32>,
        wires: &std::collections::HashMap<String, u32>,
    ) -> bool {
        fn walk_expr(
            e: &Expr,
            var: &str,
            ports: &std::collections::HashMap<String, u32>,
            wires: &std::collections::HashMap<String, u32>,
        ) -> bool {
            if let ExprKind::Index(arr, idx) = &e.kind {
                if let (ExprKind::Ident(arr_name), ExprKind::Ident(idx_name)) =
                    (&arr.kind, &idx.kind)
                {
                    if idx_name == var
                        && (ports.contains_key(arr_name) || wires.contains_key(arr_name))
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
                        .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires))
                    || ie
                        .else_stmts
                        .iter()
                        .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires))
            }
            Stmt::Match(m) => {
                walk_expr(&m.scrutinee, var, ports, wires)
                    || m.arms.iter().any(|arm| {
                        arm.body
                            .iter()
                            .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires))
                    })
            }
            Stmt::For(f) => f
                .body
                .iter()
                .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires)),
            Stmt::Init(ib) => ib
                .body
                .iter()
                .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires)),
            Stmt::DoUntil { body, cond, .. } => {
                walk_expr(cond, var, ports, wires)
                    || body
                        .iter()
                        .any(|s| Self::stmt_indexes_vob_with_var(s, var, ports, wires))
            }
            Stmt::WaitUntil(e, _) => walk_expr(e, var, ports, wires),
            Stmt::Log(l) => l.args.iter().any(|a| walk_expr(a, var, ports, wires)),
        }
    }

    /// Emit a `log(...)` statement as an `if`-guarded `$display` or `$fwrite`.
    /// Wrapped in translate_off/on so synthesis tools ignore it.
    fn emit_log_stmt(&mut self, l: &LogStmt) {
        let args_str: String = l
            .args
            .iter()
            .map(|a| format!(", {}", self.emit_expr_str(a)))
            .collect();
        let stmt = if let Some(ref path) = l.file {
            let fd_name = Self::log_fd_name(path);
            format!(
                "$fwrite({}, \"[%0t][{}][{}] {}\\n\", $time{});",
                fd_name,
                l.level.name(),
                l.tag,
                l.fmt,
                args_str
            )
        } else {
            format!(
                "$display(\"[%0t][{}][{}] {}\", $time{});",
                l.level.name(),
                l.tag,
                l.fmt,
                args_str
            )
        };
        self.line("// synopsys translate_off");
        if l.level == LogLevel::Always {
            self.line(&stmt);
        } else {
            self.line(&format!(
                "if (_arch_verbosity >= {}) {}",
                l.level.value(),
                stmt
            ));
        }
        self.line("// synopsys translate_on");
    }

    /// Generate a deterministic SV file descriptor name from a log file path.
    fn log_fd_name(path: &str) -> String {
        let clean: String = path
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("_log_fd_{clean}")
    }

    /// Unified `Stmt` emitter for `comb` and `seq` (and `latch`-as-comb)
    /// contexts. Phase 5b consolidation: the only thing the wrapping block
    /// decides is the assignment operator (`=` for blocking comb, `<=` for
    /// non-blocking seq). All other stmt-shape codegen is identical.
    ///
    /// `Blocking` is also used for the latch-block emitter and for emitting
    /// register-shaped FSM/state bodies as combinational logic (e.g. inside
    /// always_comb when the FSM lowering pulls the body out of seq).
    fn emit_stmt(&mut self, stmt: &Stmt, ctx: AssignCtx) {
        self.emit_comments_before(stmt_span_start(stmt));
        match stmt {
            Stmt::Assign(a) => {
                // Comb-context special case: `target = match scrut { ... }`
                // expands to a case block so the RHS can branch per pattern.
                // Seq context drives the same RHS through emit_expr_str, which
                // pretty-prints it as a ternary chain — no expansion needed.
                if ctx == AssignCtx::Blocking {
                    if let ExprKind::ExprMatch(scrutinee, arms) = &a.value.kind {
                        let s = self.emit_expr_str(scrutinee);
                        let target = self.emit_expr_str(&a.target);
                        self.line(&format!("case ({s})"));
                        self.indent += 1;
                        for arm in arms {
                            let pat = match &arm.pattern {
                                Pattern::Wildcard => "default".to_string(),
                                Pattern::Ident(id) if id.name == "_" => "default".to_string(),
                                Pattern::Literal(e) => self.emit_expr_str(e),
                                Pattern::Ident(id) => id.name.clone(),
                                Pattern::EnumVariant(en, vr) => {
                                    format!(
                                        "{}__{}",
                                        en.name.to_uppercase(),
                                        vr.name.to_uppercase()
                                    )
                                }
                            };
                            let val = self.emit_expr_str(&arm.value);
                            self.line(&format!("{pat}: {target} = {val};"));
                        }
                        self.indent -= 1;
                        self.line("endcase");
                        return;
                    }
                }
                let val = self.emit_expr_str(&a.value);
                let tgt = self.emit_expr_str(&a.target);
                self.line(&format!("{} {} {};", tgt, ctx.op(), val));
            }
            Stmt::IfElse(ie) => {
                self.emit_if_else(ie, ctx, false);
            }
            Stmt::Match(m) => {
                let scrut = self.emit_expr_str(&m.scrutinee);
                let u = if m.unique { "unique " } else { "" };
                self.line(&format!("{}case ({})", u, scrut));
                self.indent += 1;
                for arm in &m.arms {
                    let pat = self.emit_pattern(&arm.pattern);
                    self.line(&format!("{}: begin", pat));
                    self.indent += 1;
                    for s in &arm.body {
                        self.emit_stmt(s, ctx);
                    }
                    self.indent -= 1;
                    self.line("end");
                }
                self.indent -= 1;
                self.line("endcase");
            }
            Stmt::Log(l) => self.emit_log_stmt(l),
            Stmt::For(f) => {
                self.emit_for_loop_sv(f, |s, stmt| s.emit_stmt(stmt, ctx));
            }
            Stmt::Init(_) => {
                // `init on RST.asserted` blocks are extracted by emit_reg_block
                // before this walker runs; reaching here is a compiler bug.
                unreachable!("Stmt::Init reached emit_stmt; should be handled by emit_reg_block");
            }
            Stmt::WaitUntil(..) | Stmt::DoUntil { .. } => {
                unreachable!("Stmt::WaitUntil / DoUntil are pipeline-stage-seq only");
            }
        }
    }

    fn emit_if_else(&mut self, ie: &IfElse, ctx: AssignCtx, is_chain: bool) {
        let cond = self.emit_expr_str(&ie.cond);
        let u = if ie.unique && !is_chain {
            "unique "
        } else {
            ""
        };
        if is_chain {
            self.line(&format!("end else if ({}) begin", cond));
        } else {
            self.line(&format!("{}if ({}) begin", u, cond));
        }
        self.indent += 1;
        for s in &ie.then_stmts {
            self.emit_stmt(s, ctx);
        }
        self.indent -= 1;
        if ie.else_stmts.len() == 1 {
            if let Stmt::IfElse(nested) = &ie.else_stmts[0] {
                self.emit_if_else(nested, ctx, true);
                return;
            }
        }
        if !ie.else_stmts.is_empty() {
            self.line("end else begin");
            self.indent += 1;
            for s in &ie.else_stmts {
                self.emit_stmt(s, ctx);
            }
            self.indent -= 1;
        }
        self.line("end");
    }

    fn emit_comb_stmt(&mut self, stmt: &Stmt) {
        self.emit_stmt(stmt, AssignCtx::Blocking);
    }

    fn reset_value_expr(reset: &RegReset) -> Option<&Expr> {
        match reset {
            RegReset::None => None,
            RegReset::Inherit(_, val) | RegReset::Explicit(_, _, _, val) => Some(val),
        }
    }

    fn resolve_reg_reset(&self, reset: &RegReset, m: &ModuleDecl) -> Option<(String, bool, bool)> {
        match reset {
            RegReset::None => Option::None,
            RegReset::Explicit(signal, kind, level, _) => Some((
                signal.name.clone(),
                *kind == ResetKind::Async,
                *level == ResetLevel::Low,
            )),
            RegReset::Inherit(signal, _) => {
                // Look up the port declaration to get sync/async and polarity
                let port = m.ports.iter().find(|p| p.name.name == signal.name);
                if let Some(port) = port {
                    if let TypeExpr::Reset(kind, level) = &port.ty {
                        Some((
                            signal.name.clone(),
                            *kind == ResetKind::Async,
                            *level == ResetLevel::Low,
                        ))
                    } else {
                        // Port exists but isn't a Reset type — treat as no reset
                        Option::None
                    }
                } else {
                    // Signal not found as port — shouldn't happen after typecheck
                    Option::None
                }
            }
        }
    }

    /// Collect root signal names from all LHS assignments in a statement list.
    fn collect_assigned_roots(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    out.insert(Self::expr_root_name(&a.target));
                }
                Stmt::IfElse(ie) => {
                    Self::collect_assigned_roots(&ie.then_stmts, out);
                    Self::collect_assigned_roots(&ie.else_stmts, out);
                }
                Stmt::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_assigned_roots(&arm.body, out);
                    }
                }
                Stmt::Log(_) => {}
                Stmt::For(f) => {
                    Self::collect_assigned_roots(&f.body, out);
                }
                Stmt::Init(ib) => {
                    Self::collect_assigned_roots(&ib.body, out);
                }
                Stmt::WaitUntil(_, _) => {}
                Stmt::DoUntil { body, .. } => {
                    Self::collect_assigned_roots(body, out);
                }
            }
        }
    }

    /// Clone a statement, keeping only assignments whose LHS root reg name
    /// satisfies the inclusion test against `target_set`. When
    /// `keep_in_set=true`, an Assign survives iff its root is IN the set;
    /// when `keep_in_set=false`, an Assign survives iff its root is NOT in
    /// the set. Used by `emit_reg_block` to split a mixed-reset-kind seq
    /// block into two always_ff bodies — one with reset-edge sensitivity
    /// for reset-bearing regs and one clock-only for `reset none` regs —
    /// without losing any assignments.
    ///
    /// Returns `None` when the filtered statement has nothing to emit
    /// (all its assigns were dropped). Container statements (IfElse, For,
    /// Match, Init, DoUntil) survive only if they have at least one
    /// surviving inner assign. Log/WaitUntil pass through unchanged when
    /// the parent's `Some(_)` branch survives — they are conservatively
    /// kept where their parent body is non-empty.
    pub(crate) fn filter_stmt_by_assigned_set(
        stmt: &Stmt,
        target_set: &std::collections::BTreeSet<String>,
        keep_in_set: bool,
    ) -> Option<Stmt> {
        match stmt {
            Stmt::Assign(a) => {
                let root = Self::expr_root_name(&a.target);
                let in_set = target_set.contains(&root);
                if (keep_in_set && in_set) || (!keep_in_set && !in_set) {
                    Some(Stmt::Assign(a.clone()))
                } else {
                    None
                }
            }
            Stmt::IfElse(ie) => {
                let then_filt: Vec<Stmt> = ie
                    .then_stmts
                    .iter()
                    .filter_map(|s| Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set))
                    .collect();
                let else_filt: Vec<Stmt> = ie
                    .else_stmts
                    .iter()
                    .filter_map(|s| Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set))
                    .collect();
                if then_filt.is_empty() && else_filt.is_empty() {
                    None
                } else {
                    let mut clone = ie.clone();
                    clone.then_stmts = then_filt;
                    clone.else_stmts = else_filt;
                    Some(Stmt::IfElse(clone))
                }
            }
            Stmt::Match(m) => {
                let mut clone = m.clone();
                let mut any = false;
                for arm in &mut clone.arms {
                    arm.body = arm
                        .body
                        .iter()
                        .filter_map(|s| {
                            Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set)
                        })
                        .collect();
                    if !arm.body.is_empty() {
                        any = true;
                    }
                }
                if any {
                    Some(Stmt::Match(clone))
                } else {
                    None
                }
            }
            Stmt::For(f) => {
                let body_filt: Vec<Stmt> = f
                    .body
                    .iter()
                    .filter_map(|s| Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set))
                    .collect();
                if body_filt.is_empty() {
                    None
                } else {
                    let mut clone = f.clone();
                    clone.body = body_filt;
                    Some(Stmt::For(clone))
                }
            }
            Stmt::Init(ib) => {
                let body_filt: Vec<Stmt> = ib
                    .body
                    .iter()
                    .filter_map(|s| Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set))
                    .collect();
                if body_filt.is_empty() {
                    None
                } else {
                    let mut clone = ib.clone();
                    clone.body = body_filt;
                    Some(Stmt::Init(clone))
                }
            }
            Stmt::DoUntil { body, cond, span } => {
                let body_filt: Vec<Stmt> = body
                    .iter()
                    .filter_map(|s| Self::filter_stmt_by_assigned_set(s, target_set, keep_in_set))
                    .collect();
                if body_filt.is_empty() {
                    None
                } else {
                    Some(Stmt::DoUntil {
                        body: body_filt,
                        cond: cond.clone(),
                        span: *span,
                    })
                }
            }
            // Log + WaitUntil don't have assignments themselves; if they
            // appear at the top level of a partition, they only survive
            // when at least one inner assign survives — but at top-level
            // they can be dropped (no inner). The recursive walk handles
            // them only by keeping their parent container; standalone
            // top-level Log/WaitUntil get dropped. (In practice these
            // sit inside For/IfElse/Match bodies and the parent's empty-
            // body check decides their fate.)
            Stmt::Log(_) | Stmt::WaitUntil(_, _) => None,
        }
    }

    /// Check if an expression produces a signed (SInt) value.
    fn expr_is_signed(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Cast(_, ty) => matches!(&**ty, TypeExpr::SInt(_)),
            ExprKind::Ident(name) => self.ident_is_sint(name),
            ExprKind::MethodCall(recv, method, _) => {
                // sext always produces signed; trunc preserves signedness
                match method.name.as_str() {
                    "sext" => true,
                    "trunc" | "resize" => self.expr_is_signed(recv),
                    _ => false,
                }
            }
            ExprKind::Signed(_) => true,
            ExprKind::Unsigned(_) => false,
            ExprKind::Binary(_, lhs, _) => self.expr_is_signed(lhs),
            _ => false,
        }
    }

    /// Floating-point format of an expression in the current scope, if any.
    /// Drives dispatch of `+ - *` / comparisons / conversions to the emitted
    /// `arch_f32_*` / `arch_bf16_*` SystemVerilog helper functions.
    fn expr_float_fmt(&self, expr: &Expr) -> Option<&'static str> {
        match &expr.kind {
            ExprKind::Cast(_, ty) => crate::fp_format::by_type_expr(&**ty).map(|f| f.tag),
            ExprKind::Ident(name) => self.ident_float_fmt(name),
            ExprKind::Literal(LitKind::Float(_)) => Some("f32"),
            ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::Fp32, _)) => Some("f32"),
            ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::Bf16, _)) => Some("bf16"),
            ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::E4m3, _)) => Some("e4m3"),
            ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::E5m2, _)) => Some("e5m2"),
            ExprKind::MethodCall(_, method, _) => match method.name.as_str() {
                "to_fp32" => Some("f32"),
                "to_bf16" => Some("bf16"),
                "to_fp8e4m3" => Some("e4m3"),
                "to_fp8e5m2" => Some("e5m2"),
                "to_fp4e2m1" => Some("e2m1"),
                "to_fp6e2m3" => Some("e2m3"),
                "to_fp6e3m2" => Some("e3m2"),
                // NOTE: `to_e8m0` is deliberately absent — E8M0 is a scale
                // type, not a float format, so it must not enter float
                // operator dispatch.
                _ => None,
            },
            ExprKind::FunctionCall(name, args) if name == "fma" => {
                args.first().and_then(|a| self.expr_float_fmt(a))
            }
            ExprKind::Binary(op, lhs, rhs) => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => self
                    .expr_float_fmt(lhs)
                    .or_else(|| self.expr_float_fmt(rhs)),
                _ => None,
            },
            ExprKind::Ternary(_, t, e) => self.expr_float_fmt(t).or_else(|| self.expr_float_fmt(e)),
            // Composite accesses: Vec element and struct field reads carry
            // their declared element/field float type.
            ExprKind::Index(..) | ExprKind::FieldAccess(..) => match self.expr_decl_type(expr) {
                Some(TypeExpr::FP32) => Some("f32"),
                Some(TypeExpr::BF16) => Some("bf16"),
                Some(TypeExpr::FP8E4M3) => Some("e4m3"),
                Some(TypeExpr::FP8E5M2) => Some("e5m2"),
                _ => None,
            },
            _ => None,
        }
    }

    /// Declared TypeExpr of a scope identifier (ports, regs, typed lets,
    /// wires) — the basis for composite float resolution (`Vec<FP32,N>[i]`,
    /// `s.field`).
    fn ident_decl_type(&self, name: &str) -> Option<TypeExpr> {
        // Function-body scope shadows the module scope while emitting a
        // `function` (params + typed locals).
        if let Some(t) = self.fn_local_types.get(name) {
            return Some(t.clone());
        }
        let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) else {
            // Pipelines (and other scope-less constructs) fall back to a
            // direct AST walk.
            return self.pipeline_ident_decl_type(name);
        };
        match scope.get(name).map(|(sym, _)| sym)? {
            Symbol::Port(p) => Some(p.ty.clone()),
            Symbol::Reg(r) => Some(r.ty.clone()),
            Symbol::Let(_) => {
                for item in &self.source.items {
                    match item {
                        Item::Module(m) if m.name.name == self.current_construct => {
                            for bi in &m.body {
                                match bi {
                                    ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                                        return l.ty.clone();
                                    }
                                    ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                                        return Some(w.ty.clone());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Item::Fsm(f) if f.name.name == self.current_construct => {
                            for l in &f.lets {
                                if l.name.name == name {
                                    return l.ty.clone();
                                }
                            }
                            for w in &f.wires {
                                if w.name.name == name {
                                    return Some(w.ty.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Pipeline-scope identifier types: pipelines have no `module_scopes`
    /// entry, so resolve ports and stage-body decls straight from the AST.
    fn pipeline_ident_decl_type(&self, name: &str) -> Option<TypeExpr> {
        for item in &self.source.items {
            let Item::Pipeline(p) = item else { continue };
            if p.name.name != self.current_construct {
                continue;
            }
            for port in &p.common.ports {
                if port.name.name == name {
                    return Some(port.ty.clone());
                }
            }
            for stage in &p.stages {
                for bi in &stage.body {
                    match bi {
                        ModuleBodyItem::RegDecl(r) if r.name.name == name => {
                            return Some(r.ty.clone());
                        }
                        ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                            return Some(w.ty.clone());
                        }
                        ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                            return l.ty.clone();
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }

    /// Declared TypeExpr of an lvalue-shaped expression: identifiers,
    /// Vec-element selects, and struct field accesses (recursively, so
    /// `v[i].f` and `s.f[i]` both resolve). Used to give composite float
    /// accesses a float format for operator dispatch.
    /// The block-scale type an expression *produces*, if any.
    ///
    /// Wider than `expr_decl_type` on purpose: a scale type carries no float
    /// dispatch tag (it is not a float), so any site that routes by tag falls
    /// through to the INTEGER path on a scale value. That is fine while the
    /// receiver is a declared signal, and silently wrong the moment it is a
    /// chained conversion — `v.to_e8m0().to_fp32()` emitted an integer widen
    /// and reinterpreted the 8-bit code as a whole number (arch#904).
    fn scale_type_of(&self, e: &Expr) -> Option<TypeExpr> {
        if let ExprKind::MethodCall(_, m, _) = &e.kind {
            match m.name.as_str() {
                "to_e8m0" => return Some(TypeExpr::E8M0),
                "to_ue4m3" => return Some(TypeExpr::UE4M3),
                _ => {}
            }
        }
        match self.expr_decl_type(e) {
            Some(t @ (TypeExpr::E8M0 | TypeExpr::UE4M3)) => Some(t),
            _ => None,
        }
    }

    fn expr_decl_type(&self, e: &Expr) -> Option<TypeExpr> {
        match &e.kind {
            ExprKind::Ident(name) => self.ident_decl_type(name),
            ExprKind::Index(base, _) => match self.expr_decl_type(base)? {
                TypeExpr::Vec(elem, _) => Some(*elem),
                _ => None,
            },
            ExprKind::FieldAccess(base, field) => {
                // Bus port signal (`m.data`): resolve through the bus def's
                // signal types. Bus ports carry their bus in PortDecl
                // bus_info (AST), not in the TypeExpr.
                if let ExprKind::Ident(root) = &base.kind {
                    if let Some(bus_name) = self.port_bus_name(root) {
                        if let Some((Symbol::Bus(bi), _)) = self.symbols.globals.get(&bus_name) {
                            for (sname, _dir, sty) in &bi.signals {
                                if sname == &field.name {
                                    return Some(sty.clone());
                                }
                            }
                        }
                        return None;
                    }
                    // Cross-stage pipeline reg reference (`Mul.p`).
                    if let Some(t) = self.pipeline_stage_reg_type(root, &field.name) {
                        return Some(t);
                    }
                }
                let base_ty = self.expr_decl_type(base)?;
                let TypeExpr::Named(sname) = base_ty else {
                    return None;
                };
                if let Some((Symbol::Struct(si), _)) = self.symbols.globals.get(&sname.name) {
                    for (fname, fty) in &si.fields {
                        if fname == &field.name {
                            return Some(fty.clone());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Bus name of a bus-typed port in the current construct, if any.
    fn port_bus_name(&self, port: &str) -> Option<String> {
        for item in &self.source.items {
            let ports = match item {
                Item::Module(m) if m.name.name == self.current_construct => &m.ports,
                Item::Fsm(f) if f.name.name == self.current_construct => &f.common.ports,
                Item::Pipeline(p) if p.name.name == self.current_construct => &p.common.ports,
                _ => continue,
            };
            for pd in ports {
                if pd.name.name == port {
                    return pd.bus_info.as_ref().map(|bi| bi.bus_name.name.clone());
                }
            }
        }
        None
    }

    /// Type of `Stage.reg` when `Stage` is a stage of the current pipeline.
    fn pipeline_stage_reg_type(&self, stage: &str, reg: &str) -> Option<TypeExpr> {
        for item in &self.source.items {
            let Item::Pipeline(p) = item else { continue };
            if p.name.name != self.current_construct {
                continue;
            }
            for st in &p.stages {
                if st.name.name != stage {
                    continue;
                }
                for bi in &st.body {
                    if let ModuleBodyItem::RegDecl(r) = bi {
                        if r.name.name == reg {
                            return Some(r.ty.clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Float format of an identifier declared in the current construct's scope.
    fn ident_float_fmt(&self, name: &str) -> Option<&'static str> {
        if let Some(t) = self.fn_local_types.get(name) {
            return crate::fp_format::by_type_expr(t).map(|f| f.tag);
        }
        let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) else {
            return match self.pipeline_ident_decl_type(name) {
                Some(TypeExpr::FP32) => Some("f32"),
                Some(TypeExpr::BF16) => Some("bf16"),
                Some(TypeExpr::FP8E4M3) => Some("e4m3"),
                Some(TypeExpr::FP8E5M2) => Some("e5m2"),
                _ => None,
            };
        };
        {
            if let Some((sym, _)) = scope.get(name) {
                let ty = match sym {
                    Symbol::Port(p) => Some(&p.ty),
                    Symbol::Reg(r) => Some(&r.ty),
                    _ => None,
                };
                if let Some(ty) = ty {
                    return crate::fp_format::by_type_expr(ty).map(|f| f.tag);
                }
                if matches!(sym, Symbol::Let(_)) {
                    return self.let_binding_float_fmt(name);
                }
                if matches!(sym, Symbol::Param(_)) {
                    return self.param_float_fmt(name);
                }
            }
        }
        None
    }

    fn param_float_fmt(&self, name: &str) -> Option<&'static str> {
        for item in &self.source.items {
            match item {
                Item::Module(m) if m.name.name == self.current_construct => {
                    for p in &m.params {
                        if p.name.name == name {
                            match &p.kind {
                                ParamKind::Logic(ty) | ParamKind::Type(ty) => {
                                    return crate::fp_format::by_type_expr(ty).map(|f| f.tag)
                                }
                                _ => return None,
                            }
                        }
                    }
                }
                Item::Fsm(f) if f.name.name == self.current_construct => {
                    for p in &f.params {
                        if p.name.name == name {
                            match &p.kind {
                                ParamKind::Logic(ty) | ParamKind::Type(ty) => {
                                    return crate::fp_format::by_type_expr(ty).map(|f| f.tag)
                                }
                                _ => return None,
                            }
                        }
                    }
                }
                Item::Package(pkg) => {
                    for p in &pkg.params {
                        if p.name.name == name {
                            match &p.kind {
                                ParamKind::Logic(ty) | ParamKind::Type(ty) => {
                                    return crate::fp_format::by_type_expr(ty).map(|f| f.tag)
                                }
                                _ => return None,
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Float format of a `let`/`wire` binding by AST lookup (modules + fsms).
    fn let_binding_float_fmt(&self, name: &str) -> Option<&'static str> {
        let fmt_of = |t: &TypeExpr| crate::fp_format::by_type_expr(t).map(|f| f.tag);
        for item in &self.source.items {
            match item {
                Item::Module(m) if m.name.name == self.current_construct => {
                    for bi in &m.body {
                        match bi {
                            ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                                return l.ty.as_ref().and_then(|t| fmt_of(t));
                            }
                            ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                                return fmt_of(&w.ty);
                            }
                            _ => {}
                        }
                    }
                }
                Item::Fsm(f) if f.name.name == self.current_construct => {
                    for l in &f.lets {
                        if l.name.name == name {
                            return l.ty.as_ref().and_then(|t| fmt_of(t));
                        }
                    }
                    for w in &f.wires {
                        if w.name.name == name {
                            return fmt_of(&w.ty);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Check if an identifier is declared as SInt in the current construct's scope.
    fn ident_is_sint(&self, name: &str) -> bool {
        if let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) {
            if let Some((sym, _)) = scope.get(name) {
                return match sym {
                    Symbol::Port(p) => matches!(&p.ty, TypeExpr::SInt(_)),
                    Symbol::Reg(r) => matches!(&r.ty, TypeExpr::SInt(_)),
                    Symbol::Let(_) => self.let_binding_is_sint(name),
                    _ => false,
                };
            }
        }
        false
    }

    /// Check if a let binding is typed as SInt by looking up the AST.
    /// Searches modules and fsms (which carry their own `lets` field).
    fn let_binding_is_sint(&self, name: &str) -> bool {
        for item in &self.source.items {
            match item {
                Item::Module(m) if m.name.name == self.current_construct => {
                    for bi in &m.body {
                        match bi {
                            ModuleBodyItem::LetBinding(l) if l.name.name == name => {
                                return l
                                    .ty
                                    .as_ref()
                                    .map_or(false, |t| matches!(t, TypeExpr::SInt(_)));
                            }
                            ModuleBodyItem::WireDecl(w) if w.name.name == name => {
                                return matches!(w.ty, TypeExpr::SInt(_));
                            }
                            _ => {}
                        }
                    }
                }
                Item::Fsm(f) if f.name.name == self.current_construct => {
                    for l in &f.lets {
                        if l.name.name == name {
                            return l
                                .ty
                                .as_ref()
                                .map_or(false, |t| matches!(t, TypeExpr::SInt(_)));
                        }
                    }
                    for w in &f.wires {
                        if w.name.name == name {
                            return matches!(w.ty, TypeExpr::SInt(_));
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Try to detect indexed part-select pattern: hi = lo + (width - 1).
    /// Returns Some(width) if the width is a compile-time constant,
    /// enabling emission of `base[lo +: width]` instead of `base[hi:lo]`.
    fn try_indexed_part_select(hi: &Expr, lo: &Expr) -> Option<String> {
        // Try to check if hi == lo + (W - 1) structurally.
        // Strategy: subtract lo from hi symbolically, add 1, and see if we get a constant.
        // We do this by collecting all terms as (coefficient, variable_or_empty) pairs.

        // Simpler approach: check the common pattern where
        // hi = Binary(Add, Binary(Mul, var, W), Binary(Sub, W, 1))
        // or hi = Binary(Sub, Binary(Add, Binary(Mul, var, W), W), 1)
        // and lo = Binary(Mul, var, W)
        //
        // Most robust: check if hi and lo both contain a non-constant sub-expression,
        // and if (hi - lo) const-evaluates to a constant.
        fn try_const_eval(expr: &Expr) -> Option<i64> {
            match &expr.kind {
                ExprKind::Literal(lit) => {
                    let val = match lit {
                        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => *v as i64,
                        LitKind::Sized(_, v) | LitKind::ParamSized(_, v) => *v as i64,
                        LitKind::Float(_) | LitKind::TypedFloat(..) => return None, // not an integer constant
                    };
                    Some(val)
                }
                ExprKind::Binary(op, lhs, rhs) => {
                    let l = try_const_eval(lhs)?;
                    let r = try_const_eval(rhs)?;
                    match op {
                        BinOp::Add => Some(l + r),
                        BinOp::Sub => Some(l - r),
                        BinOp::Mul => Some(l * r),
                        _ => None,
                    }
                }
                _ => None, // Contains variable — not a constant
            }
        }

        // Check if lo contains any non-constant part (otherwise static slice is fine)
        if try_const_eval(lo).is_some() {
            return None; // Both constant — normal [hi:lo] is fine
        }

        // Collect additive terms from an expression: returns Vec<(sign, term)>
        // where term is either a constant or an opaque expression.
        // Produce a span-independent string key for an expression
        fn expr_key(expr: &Expr) -> String {
            match &expr.kind {
                ExprKind::Ident(name) => name.clone(),
                ExprKind::Literal(lit) => match lit {
                    LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => format!("{v}"),
                    LitKind::Sized(w, v) => format!("{w}'{v}"),
                    LitKind::ParamSized(name, v) => format!("{name}'{v}"),
                    LitKind::Float(bits) => format!("f{bits}"),
                    LitKind::TypedFloat(fmt, bits) => format!("tf{fmt:?}{bits}"),
                },
                ExprKind::Binary(op, lhs, rhs) => {
                    format!("({} {:?} {})", expr_key(lhs), op, expr_key(rhs))
                }
                ExprKind::Unary(op, inner) => format!("{:?}({})", op, expr_key(inner)),
                ExprKind::Index(base, idx) => format!("{}[{}]", expr_key(base), expr_key(idx)),
                ExprKind::FieldAccess(base, field) => format!("{}.{}", expr_key(base), field.name),
                _ => format!("{:?}", std::mem::discriminant(&expr.kind)),
            }
        }

        fn collect_terms(expr: &Expr, sign: i64, terms: &mut Vec<(i64, Option<i64>, String)>) {
            match &expr.kind {
                ExprKind::Literal(lit) => {
                    let val = match lit {
                        LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v) => *v as i64,
                        LitKind::Sized(_, v) | LitKind::ParamSized(_, v) => *v as i64,
                        LitKind::Float(_) | LitKind::TypedFloat(..) => {
                            terms.push((sign, None, expr_key(expr)));
                            return;
                        }
                    };
                    terms.push((sign, Some(val), String::new()));
                }
                ExprKind::Binary(BinOp::Add, lhs, rhs) => {
                    collect_terms(lhs, sign, terms);
                    collect_terms(rhs, sign, terms);
                }
                ExprKind::Binary(BinOp::Sub, lhs, rhs) => {
                    collect_terms(lhs, sign, terms);
                    collect_terms(rhs, -sign, terms);
                }
                _ => {
                    // Opaque expression — use span-free representation as key
                    terms.push((sign, None, expr_key(expr)));
                }
            }
        }

        let mut hi_terms = Vec::new();
        let mut lo_terms = Vec::new();
        collect_terms(hi, 1, &mut hi_terms);
        collect_terms(lo, -1, &mut lo_terms);

        // Compute (hi - lo + 1): cancel non-constant terms, sum constants
        let mut all_terms = hi_terms;
        all_terms.extend(lo_terms);

        // Separate constants and variable terms
        let mut const_sum: i64 = 1; // the +1
        let mut var_terms: Vec<(i64, String)> = Vec::new();

        for (sign, val, key) in &all_terms {
            if let Some(v) = val {
                const_sum += sign * v;
            } else {
                var_terms.push((*sign, key.clone()));
            }
        }

        // Check if variable terms cancel out
        let mut var_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for (sign, key) in &var_terms {
            *var_map.entry(key.clone()).or_insert(0) += sign;
        }

        // Collect remaining (non-cancelled) variable terms
        let remaining_vars: Vec<(&String, &i64)> =
            var_map.iter().filter(|(_, &v)| v != 0).collect();

        if remaining_vars.is_empty() && const_sum > 0 {
            // Pure constant width
            Some(const_sum.to_string())
        } else if remaining_vars.len() == 1 {
            let (key, &coeff) = remaining_vars[0];
            if coeff == 1 {
                // Width = key + const_sum (key is an expression like a param name)
                // Only emit if key looks like a simple identifier or expression
                // (not something with parentheses that would be ambiguous)
                if const_sum == 0 {
                    Some(key.clone())
                } else if const_sum > 0 {
                    Some(format!("{key} + {const_sum}"))
                } else {
                    Some(format!("{key} - {}", -const_sum))
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn expr_root_name(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::FieldAccess(base, _) => Self::expr_root_name(base),
            ExprKind::Index(base, _)
            | ExprKind::BitSlice(base, _, _)
            | ExprKind::PartSelect(base, _, _, _) => Self::expr_root_name(base),
            ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => {
                Self::expr_root_name(inner)
            }
            _ => String::new(),
        }
    }

    /// Extract reset info from a port list: (name, is_async, is_low).
    /// Returns ("rst", false, false) as defaults if no Reset port found.
    fn extract_reset_info(ports: &[PortDecl]) -> (String, bool, bool) {
        crate::ast::extract_reset_info(ports)
    }

    /// Compute the total bit-width of a TypeExpr (for FIFO `DATA_WIDTH`,
    /// RAM `WIDTH`, and similar param-derived widths). Recurses through
    /// `Vec<T, N>`, struct fields, and enum variants. Used by the fifo /
    /// ram codegen submodules; lives here as a shared helper because
    /// nothing about it is fifo-specific.
    fn type_expr_data_width(&self, ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => Some(self.emit_expr_str(w)),
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => {
                Some("1".to_string())
            }
            TypeExpr::FP32 => Some("32".to_string()),
            TypeExpr::BF16 => Some("16".to_string()),
            TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => Some("8".to_string()),
            TypeExpr::FP4E2M1 => Some("4".to_string()),
            TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => Some("6".to_string()),
            TypeExpr::E8M0 | TypeExpr::UE4M3 => Some("8".to_string()),
            TypeExpr::Vec(inner, size) => {
                let iw = self.type_expr_data_width(inner)?;
                let n = self.emit_expr_str(size);
                Some(format!("({iw}) * ({n})"))
            }
            // Packed block: `scale_w + N*elem_w`. Folded to a literal when `N`
            // is constant (the common case — MXFP4 becomes a plain `136`);
            // falls back to an expression so a param-sized `N` survives.
            //
            // The member types are gated by the same predicates the type
            // checker uses. Falling back to the generic recursive width walk
            // here is what let `ScaledVec<UInt<8>,4,E8M0>` emit a 40-bit port
            // while the sim emitted a `uint8_t`.
            TypeExpr::ScaledVec(elem, size, scale) => {
                if !crate::fp_format::is_block_element(elem)
                    || !crate::fp_format::is_block_scale(scale)
                {
                    return None;
                }
                if let Some(w) = self.type_expr_width(ty) {
                    return Some(w.to_string());
                }
                let ew = self.type_expr_data_width(elem)?;
                let sw = self.type_expr_data_width(scale)?;
                let n = self.emit_expr_str(size);
                Some(format!("({sw}) + ({ew}) * ({n})"))
            }
            TypeExpr::Named(ident) => {
                if let Some((crate::resolve::Symbol::Struct(info), _)) =
                    self.symbols.globals.get(&ident.name)
                {
                    let mut parts = Vec::new();
                    for (_, field_ty) in &info.fields {
                        parts.push(self.type_expr_data_width(field_ty)?);
                    }
                    if parts.len() == 1 {
                        Some(parts.into_iter().next().unwrap())
                    } else {
                        Some(parts.join(" + "))
                    }
                } else if let Some((crate::resolve::Symbol::Enum(info), _)) =
                    self.symbols.globals.get(&ident.name)
                {
                    let n = info.variants.len();
                    let bits = crate::width::index_width(n as u64);
                    Some(bits.to_string())
                } else {
                    None
                }
            }
        }
    }

    /// Build the sensitivity list string for an always_ff block.
    fn ff_sensitivity(clk: &str, rst: &str, is_async: bool, is_low: bool) -> String {
        if is_async {
            let rst_edge = if is_low { "negedge" } else { "posedge" };
            format!("posedge {clk} or {rst_edge} {rst}")
        } else {
            format!("posedge {clk}")
        }
    }

    /// Build the reset condition string (e.g. "rst" or "(!rst_n)").
    fn rst_condition(rst: &str, is_low: bool) -> String {
        if is_low {
            format!("(!{rst})")
        } else {
            rst.to_string()
        }
    }

    fn emit_reg_stmt(&mut self, stmt: &Stmt) {
        self.emit_stmt(stmt, AssignCtx::NonBlocking);
    }

    /// Auto-declare `logic` wires for inst output connections that reference
    /// names not already declared as ports, regs, or lets in the current module.
    /// The wire type is resolved from the source module's port definition.
    fn emit_inst_output_wire_decls(
        &mut self,
        inst: &InstDecl,
        declared: &std::collections::HashSet<String>,
    ) {
        // Look up the instantiated module's port info
        let module_ports = if let Some((Symbol::Module(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else if let Some((Symbol::Pipeline(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else if let Some((Symbol::Fsm(info), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            info.ports.clone()
        } else if let Some((Symbol::Ram(_), _)) = self.symbols.globals.get(&inst.module_name.name) {
            // RAM uses port groups — handle separately below
            Vec::new()
        } else if let Some((Symbol::Regfile(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            // Regfile uses port arrays — handle separately below
            Vec::new()
        } else {
            return;
        };

        // For RAM instances, build a flattened port map from port groups
        // Resolve type params (e.g. WIDTH → UInt<32>) from the RAM's param list.
        let ram_flat_ports: Vec<(String, TypeExpr)> =
            if let Some((Symbol::Ram(_), _)) = self.symbols.globals.get(&inst.module_name.name) {
                let mut flat = Vec::new();
                for item in &self.source.items {
                    if let Item::Ram(r) = item {
                        if r.name.name == inst.module_name.name {
                            // Build type param map: param name → resolved TypeExpr
                            let type_params: std::collections::HashMap<String, TypeExpr> = r
                                .params
                                .iter()
                                .filter_map(|p| match &p.kind {
                                    crate::ast::ParamKind::Type(ty) => {
                                        Some((p.name.name.clone(), ty.clone()))
                                    }
                                    _ => None,
                                })
                                .collect();
                            for pg in &r.port_groups {
                                for s in &pg.signals {
                                    // Resolve Named type params to their actual types
                                    let resolved_ty = match &s.ty {
                                        TypeExpr::Named(ident) => type_params
                                            .get(&ident.name)
                                            .cloned()
                                            .unwrap_or_else(|| s.ty.clone()),
                                        other => other.clone(),
                                    };
                                    flat.push((
                                        format!("{}_{}", pg.name.name, s.name.name),
                                        resolved_ty,
                                    ));
                                }
                            }
                            break;
                        }
                    }
                }
                flat
            } else {
                Vec::new()
            };

        // For Regfile instances, build a flattened port map from port arrays
        let regfile_flat_ports: Vec<(String, TypeExpr)> = if let Some((Symbol::Regfile(_), _)) =
            self.symbols.globals.get(&inst.module_name.name)
        {
            let mut flat = Vec::new();
            for item in &self.source.items {
                if let Item::Regfile(r) = item {
                    if r.name.name == inst.module_name.name {
                        // Scalar ports
                        for p in &r.ports {
                            flat.push((p.name.name.clone(), p.ty.clone()));
                        }
                        // Read port array: read{i}_signal
                        if let Some(rp) = &r.read_ports {
                            let count = self.resolve_regfile_count(&rp.count_expr, r);
                            for i in 0..count {
                                for s in &rp.signals {
                                    flat.push((
                                        format!("{}{i}_{}", rp.name.name, s.name.name),
                                        s.ty.clone(),
                                    ));
                                }
                            }
                        }
                        // Write port array: write{i}_signal
                        if let Some(wp) = &r.write_ports {
                            let count = self.resolve_regfile_count(&wp.count_expr, r);
                            for i in 0..count {
                                for s in &wp.signals {
                                    flat.push((
                                        format!("{}{i}_{}", wp.name.name, s.name.name),
                                        s.ty.clone(),
                                    ));
                                }
                            }
                        }
                        break;
                    }
                }
            }
            flat
        } else {
            Vec::new()
        };

        // Implicit bus wires: for any inst connection on a bus port
        // whose parent-side signal is an undeclared Ident, declare each
        // flattened bus signal as a wire on the parent. Mirrors the
        // sim_codegen fix from PRs #110+#112. Without this, Verilator
        // creates 1-bit IMPLICIT wires that silently truncate wider
        // signals like `_flits_send_data` and the design appears dead.
        let mut bus_emitted: std::collections::HashSet<String> = std::collections::HashSet::new();
        for conn in &inst.connections {
            let Some(port) = module_ports
                .iter()
                .find(|p| p.name.name == conn.port_name.name)
            else {
                continue;
            };
            let Some(bi) = &port.bus_info else {
                continue;
            };
            let ExprKind::Ident(parent_name) = &conn.signal.kind else {
                continue;
            };
            let Some((Symbol::Bus(bus_info), _)) = self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            let mut pm = bus_info.default_param_map();
            for pa in &bi.params {
                pm.insert(pa.name.name.clone(), &pa.value);
            }
            for (sname, _sdir, ty) in bus_info.effective_signals(&pm) {
                let flat = format!("{parent_name}_{sname}");
                if declared.contains(&flat) || !bus_emitted.insert(flat.clone()) {
                    continue;
                }
                let subst_ty = Self::subst_type_expr(&ty, &pm);
                let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&subst_ty);
                self.line(&format!("{} {}{};", ty_str, flat, arr_suffix));
            }
        }

        for conn in &inst.connections {
            if conn.direction != ConnectDir::Output {
                continue;
            }
            if let ExprKind::Ident(target) = &conn.signal.kind {
                if declared.contains(target) {
                    continue;
                }
                // Bus ports are handled above as a separate pass; skip.
                if let Some(port) = module_ports
                    .iter()
                    .find(|p| p.name.name == conn.port_name.name)
                {
                    if port.bus_info.is_some() {
                        continue;
                    }
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(&port.ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else if let Some((_, ty)) = ram_flat_ports
                    .iter()
                    .find(|(n, _)| *n == conn.port_name.name)
                {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else if let Some((_, ty)) = regfile_flat_ports
                    .iter()
                    .find(|(n, _)| *n == conn.port_name.name)
                {
                    let (ty_str, arr_suffix) = self.emit_type_and_array_suffix(ty);
                    self.line(&format!("{} {}{};", ty_str, target, arr_suffix));
                } else {
                    self.line(&format!("logic {};", target));
                }
            }
        }
    }

    fn resolve_regfile_count(&self, expr: &crate::ast::Expr, r: &crate::ast::RegfileDecl) -> u64 {
        use crate::ast::{ExprKind, LitKind, ParamKind};
        match &expr.kind {
            ExprKind::Literal(LitKind::Dec(v)) => *v,
            ExprKind::Ident(name) => r
                .params
                .iter()
                .find(|p| p.name.name == *name)
                .and_then(|p| match &p.kind {
                    ParamKind::Const | ParamKind::WidthConst(..) => p.default.as_ref(),
                    _ => None,
                })
                .and_then(|e| {
                    if let ExprKind::Literal(LitKind::Dec(v)) = &e.kind {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .unwrap_or(1),
            _ => 1,
        }
    }

    fn emit_inst(&mut self, inst: &InstDecl) {
        let mut parts = Vec::new();

        // Module name with params
        if inst.param_assigns.is_empty() {
            parts.push(format!("{} {} (", inst.module_name.name, inst.name.name));
        } else {
            let params: Vec<String> = inst
                .param_assigns
                .iter()
                .map(|p| self.emit_param_override(&inst.module_name.name, p))
                .collect();
            parts.push(format!(
                "{} #({}) {} (",
                inst.module_name.name,
                params.join(", "),
                inst.name.name,
            ));
        }

        // Expand bus port connections: one bus connect → N signal connects
        let mut connections: Vec<String> = Vec::new();
        // Find the target construct's ports to detect bus ports (modules and FSMs)
        let target_ports_ref: Option<&[PortDecl]> =
            self.source.items.iter().find_map(|item| match item {
                Item::Module(m) if m.name.name == inst.module_name.name => Some(m.ports.as_slice()),
                Item::Fsm(f) if f.name.name == inst.module_name.name => Some(f.ports.as_slice()),
                _ => None,
            });
        // For each bus port, expose either the scalar name (count=None) or
        // every indexed name `port_0`, ..., `port_{N-1}` (count=Some(N)) so
        // inst-site connections like `chans[i] -> w[i]` match a Vec<Bus,N>
        // element of the child. Vec count is resolved against the child
        // module's params (with the inst-site `param NAME = ...` overrides
        // applied on top).
        let child_params: Vec<ParamDecl> = self
            .source
            .items
            .iter()
            .find_map(|item| {
                if let Item::Module(m) = item {
                    if m.name.name == inst.module_name.name {
                        Some(m.params.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let mut child_params_overridden = child_params.clone();
        for pa in &inst.param_assigns {
            if let Some(p) = child_params_overridden
                .iter_mut()
                .find(|p| p.name.name == pa.name.name)
            {
                // The override RHS is written in the *parent* param scope, so
                // resolve it there before substituting into the child param
                // list. Without this, `param NUM = NUM` (parent NUM forwarded
                // into the child's same-named NUM) yields a self-referential
                // child default `NUM => NUM`, and a later eval_const_u32 against
                // the child params recurses forever (stack overflow). Folding to
                // a literal in the parent scope severs that cycle; if the parent
                // value isn't reducible at codegen we keep the original expr
                // (eval_const_u32's visited-guard then safely bails to None).
                let resolved = self
                    .eval_const_u32(&pa.value, &self.current_module_params.clone())
                    .map(|n| Expr {
                        kind: ExprKind::Literal(LitKind::Dec(n as u64)),
                        span: pa.value.span,
                        parenthesized: false,
                    })
                    .unwrap_or_else(|| pa.value.clone());
                p.default = Some(resolved);
            }
        }
        let target_bus_ports: Vec<(String, String, Vec<ParamAssign>)> = target_ports_ref
            .map(|ports| {
                let mut v = Vec::new();
                for p in ports {
                    if let Some(bi) = p.bus_info.as_ref() {
                        match bi.count.as_ref() {
                            None => {
                                v.push((
                                    p.name.name.clone(),
                                    bi.bus_name.name.clone(),
                                    bi.params.clone(),
                                ));
                            }
                            Some(count_expr) => {
                                if let Some(n) =
                                    self.eval_const_u32(count_expr, &child_params_overridden)
                                {
                                    for i in 0..n {
                                        v.push((
                                            format!("{}_{}", p.name.name, i),
                                            bi.bus_name.name.clone(),
                                            bi.params.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                v
            })
            .unwrap_or_default();
        // Separate index: Vec-of-bus port → (count, bus, params). Lets a
        // whole-vec inst connection `chans -> w` (where both child port
        // and parent signal are `Vec<Bus, N>`) expand to N per-element
        // per-signal named-port connections without requiring the user
        // to enumerate `chans[0] -> w[0]; chans[1] -> w[1]; ...`.
        let target_vec_of_bus_ports: Vec<(String, u32, String, Vec<ParamAssign>)> =
            target_ports_ref
                .map(|ports| {
                    let mut v = Vec::new();
                    for p in ports {
                        if let Some(bi) = p.bus_info.as_ref() {
                            if let Some(count_expr) = bi.count.as_ref() {
                                if let Some(n) =
                                    self.eval_const_u32(count_expr, &child_params_overridden)
                                {
                                    v.push((
                                        p.name.name.clone(),
                                        n,
                                        bi.bus_name.name.clone(),
                                        bi.params.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    v
                })
                .unwrap_or_default();

        // Per-port enum-cast lookup. For each input port whose type is
        // an extern-package enum (declared via `extern package Pkg ...
        // type enum_t; ...`), record the package + type name so the
        // inst-connect emission below can wrap the connected signal in
        // an explicit `Pkg::enum_t'(sig)` cast. yosys-slang rejects
        // implicit `logic[N] -> enum_t` conversion at port boundaries
        // (the source-level `wire NAME: UInt<N>;` declaration in the
        // arch source means the connected signal is `logic[N]`, not the
        // enum); the cast keeps strict elaborators happy.
        // Verilator and iverilog accept the implicit conversion either
        // way.
        let port_enum_casts: std::collections::HashMap<String, (String, String)> = {
            let mut m = std::collections::HashMap::new();
            if let Some(ports) = target_ports_ref {
                for p in ports {
                    if let TypeExpr::Named(id) = &p.ty {
                        if let Some((Symbol::ExternEnum(_), _)) = self.symbols.globals.get(&id.name)
                        {
                            // Find the owning extern-package by scanning
                            // Item::ExternPackage entries — the resolve
                            // table records only the type name, not its
                            // owning package.
                            let pkg_name = self
                                .source
                                .items
                                .iter()
                                .find_map(|it| match it {
                                    Item::ExternPackage(ep)
                                        if ep.types.iter().any(|t| t.name == id.name) =>
                                    {
                                        Some(ep.name.name.clone())
                                    }
                                    _ => None,
                                })
                                .unwrap_or_else(|| id.name.clone());
                            m.insert(p.name.name.clone(), (pkg_name, id.name.clone()));
                        }
                    }
                }
            }
            m
        };

        // ── Collect target's per-requester ports[N] groups (arbiter / regfile) ──
        // The parser flattens `request[0].valid` into a single port name
        // `request0_valid`; on the inst-target side, the SV port is a vector
        // (`request_valid [N-1:0]`). We need to:
        //   - synthesize a hidden wire of width N,
        //   - drive each bit from the user's per-index expression (or split
        //     the inst's output back to user wires for output signals),
        //   - replace the N flat connections with one whole-vector
        //     `.<group>_<sig>(<wire>)` connection.
        // Without this, the inst-site SV references non-existent port
        // names like `.request0_valid()`. Issue #296.
        // Arbiter emits its `ports[N] request { valid; ready; }` group
        // as vector ports (`request_valid [N-1:0]`, `request_ready [N-1:0]`)
        // — so individual `request[i].valid <- expr` connections need to
        // be merged into a synthesized vector wire + per-bit drives, then
        // connected as a single whole-vector `.request_valid(wire)` pin.
        //
        // Regfile uses a different convention — its `ports[N] read { addr;
        // data; }` emits as per-index scalar ports (`read0_addr`,
        // `read1_addr`, ...) so the existing flattened-port-name behavior
        // is correct there. Regfile is intentionally excluded from the
        // synthesis here.
        let target_port_arrays: Vec<(String, u64, Vec<(String, Direction, TypeExpr)>)> = self
            .source
            .items
            .iter()
            .find_map(|item| match item {
                Item::Arbiter(a) if a.name.name == inst.module_name.name => Some(
                    a.port_arrays
                        .iter()
                        .map(|pa| {
                            let n = self.eval_count_with_inst_params(&pa.count_expr, inst);
                            let signals = pa
                                .signals
                                .iter()
                                .map(|s| (s.name.name.clone(), s.direction, s.ty.clone()))
                                .collect();
                            (pa.name.name.clone(), n, signals)
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();

        // Unified per-element "indexed-port-group" gather state. Both the
        // arbiter `port_arrays` flattening (`.req0_valid` → packed wire +
        // per-bit drives) and the Vec-of-bus packed-port flattening
        // (`ins[k] <- expr_k` → `{...}` concat per bus signal) follow the
        // same shape: walk inst connections, bucket per-element entries by
        // base port, then emit one grouped output per (port[, signal]).
        // See `IndexedGroup` / `GroupKind` for the data layout.
        let mut indexed_groups: std::collections::HashMap<String, IndexedGroup> =
            std::collections::HashMap::new();
        // (base_port_name, N) → bus_name, bus_params for VOB packed emit.
        let vob_port_meta: std::collections::HashMap<String, (u32, String, Vec<ParamAssign>)> =
            target_vec_of_bus_ports
                .iter()
                .map(|(name, n, bus_name, bus_params)| {
                    (name.clone(), (*n, bus_name.clone(), bus_params.clone()))
                })
                .collect();

        for c in &inst.connections {
            // Try to match `<group>{idx}_<sig>` against any known port array.
            let matched = target_port_arrays.iter().find_map(|(group, _n, sigs)| {
                let pname = &c.port_name.name;
                let prefix = format!("{group}");
                let rest = pname.strip_prefix(&prefix)?;
                // rest looks like "0_valid" — split on first underscore
                let und = rest.find('_')?;
                let idx_str = &rest[..und];
                let sig = &rest[und + 1..];
                let idx: u32 = idx_str.parse().ok()?;
                let (sname, dir, ty) = sigs.iter().find(|(sn, _, _)| sn == sig)?;
                Some((group.clone(), idx, sname.clone(), *dir, ty.clone()))
            });
            if let Some((group, idx, sig, dir, ty)) = matched {
                let sig_str = self.emit_expr_str(&c.signal);
                // Arbiter case is keyed by (group, sig) — we synthesize a
                // separate temp wire per (group, sig) pair, so use the
                // concatenated key as the map slot.
                let key = format!("{group}.{sig}");
                let n = target_port_arrays
                    .iter()
                    .find(|(g, _, _)| *g == group)
                    .map(|(_, n, _)| *n)
                    .unwrap_or(0);
                indexed_groups
                    .entry(key)
                    .or_insert_with(|| IndexedGroup {
                        base_port: group.clone(),
                        kind: GroupKind::ArbiterPortArray {
                            sig: sig.clone(),
                            dir,
                            ty: ty.clone(),
                            n,
                        },
                        arb_entries: Vec::new(),
                        vob_entries: std::collections::HashMap::new(),
                    })
                    .arb_entries
                    .push((idx, sig_str));
                continue;
            }
            // Whole-vec bus connection on a Vec-of-bus child port.
            // Packed SV emission: one connection per bus signal, parent
            // expr passed whole — works for all three shapes:
            //   `chans -> w`       (1D Vec-of-bus wire `w` → packed `w_<sig>`)
            //   `chans -> edges[i]` (row of 2D wire → packed slice `edges_<sig>[i]`)
            //   `chans -> p`       (parent Vec-of-bus port `p` → packed `p_<sig>`)
            if let Some((_, _n, bus_name, bus_params)) = target_vec_of_bus_ports
                .iter()
                .find(|(pn, _, _, _)| *pn == c.port_name.name)
            {
                let parent_expr_str = self.emit_expr_str(&c.signal);
                // For an Ident parent: name is the bus base; per-signal ref is `<name>_<sig>`.
                // For an Index parent: emit_expr_str already produced `<base>_<sig>[i]`
                // form via the FieldAccess(Index) path — but here there's no field,
                // so emit_expr_str gives just the indexed array (e.g. `edges[i]`),
                // which is wrong. Build the correct per-signal ref directly.
                let per_sig_emit = |sname: &str, self_: &Self| -> String {
                    match &c.signal.kind {
                        ExprKind::Ident(name) => format!("{}_{}", name, sname),
                        ExprKind::Index(arr, idx) => {
                            let idx_str = match &idx.kind {
                                ExprKind::Ident(loopvar) => self_
                                    .loop_var_subst
                                    .get(loopvar)
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| loopvar.clone()),
                                _ => self_.emit_expr_str(idx),
                            };
                            if let ExprKind::Ident(arr_name) = &arr.kind {
                                format!("{}_{}[{}]", arr_name, sname, idx_str)
                            } else {
                                // Fallback — shouldn't happen for legal whole-vec sources.
                                format!("{}_{}", parent_expr_str, sname)
                            }
                        }
                        _ => format!("{}_{}", parent_expr_str, sname),
                    }
                };
                if let Some((Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) {
                    let mut param_map: std::collections::HashMap<String, &Expr> = info
                        .params
                        .iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in bus_params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff = info.effective_signals(&param_map);
                    for (sname, _, _) in &eff {
                        connections.push(format!(
                            ".{}_{}({})",
                            c.port_name.name,
                            sname,
                            per_sig_emit(sname, self),
                        ));
                    }
                    continue;
                }
            }
            // Per-element Vec-of-bus connection: `ins[k] <- expr`. The parser
            // already lowered `ins[k]` to port_name `ins_<k>`. Detect the
            // `<base>_<k>` pattern against `target_vec_of_bus_ports` and
            // accumulate for post-loop concat emission.
            {
                let pname = &c.port_name.name;
                let matched_vob = target_vec_of_bus_ports.iter().find_map(|(base, n, _, _)| {
                    let prefix = format!("{base}_");
                    let rest = pname.strip_prefix(&prefix)?;
                    let idx: u32 = rest.parse().ok()?;
                    if idx < *n {
                        Some((base.clone(), idx))
                    } else {
                        None
                    }
                });
                if let Some((base, idx)) = matched_vob {
                    let Some((n, bus_name, bus_params)) = vob_port_meta.get(&base) else {
                        continue;
                    };
                    indexed_groups
                        .entry(base.clone())
                        .or_insert_with(|| IndexedGroup {
                            base_port: base.clone(),
                            kind: GroupKind::VecOfBusPacked {
                                n: *n,
                                bus_name: bus_name.clone(),
                                bus_params: bus_params.clone(),
                            },
                            arb_entries: Vec::new(),
                            vob_entries: std::collections::HashMap::new(),
                        })
                        .vob_entries
                        .insert(idx, c.signal.clone());
                    continue;
                }
            }
            if let Some((_, bus_name, bus_params)) = target_bus_ports
                .iter()
                .find(|(pn, _, _)| *pn == c.port_name.name)
            {
                // Bus connection — expand to individual signals. The parent-side
                // signal can be one of:
                //   * `Ident("w")`               — scalar bus port or bus wire        → `w_<sig>`
                //   * `Index(Ident("w"), N)`     — element N of a `Vec<Bus,M>` wire   → `w_N_<sig>`
                //   * `Index(Ident("p"), <e>)`   — element of a `Vec<Bus,M>` port (D2) → `p_<sig>[<e>]`
                //                                  where <e> is a literal, a loop var
                //                                  (left as-is for SV genvar), or
                //                                  a static-unrolled loop var.
                if let Some((crate::resolve::Symbol::Bus(info), _)) =
                    self.symbols.globals.get(bus_name)
                {
                    let mut param_map: std::collections::HashMap<String, &Expr> = info
                        .params
                        .iter()
                        .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
                        .collect();
                    for pa in bus_params {
                        param_map.insert(pa.name.name.clone(), &pa.value);
                    }
                    let eff_signals = info.effective_signals(&param_map);
                    // Vec-of-bus *port* OR 1-D Vec-of-bus *wire* on the parent:
                    // both are declared in packed `<base>_<sig> [N-1:0]` form, so
                    // an indexed element must emit `<base>_<sig>[<idx>]`, NOT the
                    // flattened `<base>_<idx>_<sig>` (which would reference a
                    // non-existent net — Verilator IMPLICIT, and a disconnected
                    // signal in sim). The 2-D `Vec<Vec<Bus>>` edge-wire case
                    // (`edges[m][n]`) is intentionally excluded — it is not in
                    // `vec_of_bus_wire_count` and keeps its `edges_<m>_<n>` form
                    // via the else branch below.
                    let vec_of_bus_port_ref: Option<(String, String)> = match &c.signal.kind {
                        ExprKind::Index(arr, idx) => {
                            if let ExprKind::Ident(arr_name) = &arr.kind {
                                if self.vec_of_bus_port_count.contains_key(arr_name)
                                    || self.vec_of_bus_wire_count.contains_key(arr_name)
                                {
                                    let idx_str = match &idx.kind {
                                        ExprKind::Ident(loopvar) => {
                                            if let Some(&v) = self.loop_var_subst.get(loopvar) {
                                                v.to_string()
                                            } else {
                                                loopvar.clone()
                                            }
                                        }
                                        _ => self.emit_expr_str(idx),
                                    };
                                    Some((arr_name.clone(), idx_str))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some((arr_name, idx_str)) = vec_of_bus_port_ref {
                        for (sname, _, _) in &eff_signals {
                            connections.push(format!(
                                ".{}_{}({}_{}[{}])",
                                c.port_name.name, sname, arr_name, sname, idx_str
                            ));
                        }
                    } else {
                        let sig_prefix = match &c.signal.kind {
                            // 2D bus wire element: `edges[m][n]`. Both indices
                            // resolve to literals (or static-unrolled loop
                            // vars); emit `edges_<m>_<n>` as the prefix.
                            ExprKind::Index(arr, idx) => {
                                let resolve = |e: &Expr| -> Option<u64> {
                                    match &e.kind {
                                        ExprKind::Literal(LitKind::Dec(i)) => Some(*i),
                                        ExprKind::Ident(loopvar) => {
                                            self.loop_var_subst.get(loopvar).map(|v| *v as u64)
                                        }
                                        _ => None,
                                    }
                                };
                                if let ExprKind::Index(inner_arr, inner_idx) = &arr.kind {
                                    if let ExprKind::Ident(arr_name) = &inner_arr.kind {
                                        if let (Some(m_v), Some(n_v)) =
                                            (resolve(inner_idx), resolve(idx))
                                        {
                                            format!("{}_{}_{}", arr_name, m_v, n_v)
                                        } else {
                                            self.emit_expr_str(&c.signal)
                                        }
                                    } else {
                                        self.emit_expr_str(&c.signal)
                                    }
                                } else if let (ExprKind::Ident(arr_name), Some(i)) =
                                    (&arr.kind, resolve(idx))
                                {
                                    format!("{}_{}", arr_name, i)
                                } else {
                                    self.emit_expr_str(&c.signal)
                                }
                            }
                            _ => self.emit_expr_str(&c.signal),
                        };
                        for (sname, _, _) in &eff_signals {
                            connections.push(format!(
                                ".{}_{}({}_{})",
                                c.port_name.name, sname, sig_prefix, sname
                            ));
                        }
                    }
                }
            } else {
                let sig_str = self.emit_expr_str(&c.signal);
                let conn_str = if let Some((pkg, ty_name)) = port_enum_casts.get(&c.port_name.name)
                {
                    // Wrap in explicit cast to the destination port's
                    // extern-enum type so yosys-slang accepts the
                    // boundary. No-op for verilator / iverilog.
                    format!(".{}({}::{}'({}))", c.port_name.name, pkg, ty_name, sig_str)
                } else {
                    format!(".{}({})", c.port_name.name, sig_str)
                };
                connections.push(conn_str);
            }
        }

        // Emit grouped connections for all indexed-port groups. To preserve
        // historical output ordering: VOB packed concats are appended to
        // `connections` first (sorted by base port), then arbiter port-array
        // connections (sorted by group.sig). The arbiter emit also writes
        // synthesized `logic` decl + per-bit `assign` lines via `self.line`
        // BEFORE the inst body.
        let mut group_keys: Vec<&String> = indexed_groups.keys().collect();
        group_keys.sort();
        // First pass: VOB packed concats.
        for key in &group_keys {
            let g = &indexed_groups[*key];
            if matches!(g.kind, GroupKind::VecOfBusPacked { .. }) {
                self.emit_indexed_group(g, &mut connections);
            }
        }
        // Second pass: arbiter port-array synthesized wires + drives.
        for key in &group_keys {
            let g = &indexed_groups[*key];
            if matches!(g.kind, GroupKind::ArbiterPortArray { .. }) {
                self.emit_indexed_group_with_decls(g, inst, &mut connections);
            }
        }

        self.line(&parts[0]);
        self.indent += 1;
        for (i, conn) in connections.iter().enumerate() {
            if i < connections.len() - 1 {
                self.line(&format!("{},", conn));
            } else {
                self.line(conn);
            }
        }
        self.indent -= 1;
        self.line(");");
    }

    /// Emit a VOB-packed-concat group: one `.<port>_<sig>({...})` connection
    /// per bus signal. Per-element parent exprs were captured at gather
    /// time; here we attach `.<sig>` and re-emit via `emit_expr_str`
    /// (which handles 2D-wire packed-slice lowering).
    fn emit_indexed_group(&self, g: &IndexedGroup, connections: &mut Vec<String>) {
        let GroupKind::VecOfBusPacked {
            n,
            bus_name,
            bus_params,
        } = &g.kind
        else {
            return;
        };
        let Some((Symbol::Bus(info), _)) = self.symbols.globals.get(bus_name) else {
            return;
        };
        let mut param_map: std::collections::HashMap<String, &Expr> = info
            .params
            .iter()
            .filter_map(|pd| pd.default.as_ref().map(|d| (pd.name.name.clone(), d)))
            .collect();
        for pa in bus_params {
            param_map.insert(pa.name.name.clone(), &pa.value);
        }
        let eff = info.effective_signals(&param_map);
        for (sname, _, _) in &eff {
            // Build {expr_{N-1}_<sig>, expr_{N-2}_<sig>, …, expr_0_<sig>}.
            let mut parts: Vec<String> = Vec::with_capacity(*n as usize);
            for k in (0..*n).rev() {
                if let Some(elem_expr) = g.vob_entries.get(&k) {
                    let fa = Expr::new(
                        ExprKind::FieldAccess(
                            Box::new(elem_expr.clone()),
                            Ident::new(sname.clone(), elem_expr.span),
                        ),
                        elem_expr.span,
                    );
                    parts.push(self.emit_expr_str(&fa));
                } else {
                    // Missing element — emit dummy so SV still parses (typecheck
                    // should have rejected this upstream).
                    parts.push(format!("'0"));
                }
            }
            connections.push(format!(
                ".{}_{}({{{}}})",
                g.base_port,
                sname,
                parts.join(", "),
            ));
        }
    }

    /// Emit an arbiter port-array group: synthesize a vector wire,
    /// per-index `assign` drives (direction-aware), and one whole-vector
    /// `.<group>_<sig>(<wire>)` connection. The `logic`/`assign` lines
    /// are written immediately so they appear BEFORE the inst body.
    fn emit_indexed_group_with_decls(
        &mut self,
        g: &IndexedGroup,
        inst: &InstDecl,
        connections: &mut Vec<String>,
    ) {
        let GroupKind::ArbiterPortArray { sig, dir, ty, n } = &g.kind else {
            return;
        };
        let group = &g.base_port;
        // If `n` is 0 (no port-array metadata found), fall back to the
        // entry count — preserves prior behavior.
        let n = if *n == 0 {
            g.arb_entries.len() as u64
        } else {
            *n
        };
        let wire_name = format!("__{}_{}_{}", inst.name.name, group, sig);
        // Synthesize the vector wire as `logic [<elem>][N-1:0]`.
        // Per-bit (`elem` == Bool) flattens to `logic [N-1:0]`.
        let elem_ty_str = match ty {
            TypeExpr::Bool => format!("[{}:0]", n - 1),
            _ => {
                let elem = self.emit_logic_type_str(ty);
                let body = elem.strip_prefix("logic").unwrap_or(&elem).trim_start();
                if body.is_empty() {
                    format!("[{}:0]", n - 1)
                } else {
                    format!("[{}:0]{body}", n - 1)
                }
            }
        };
        self.line(&format!("logic {elem_ty_str} {wire_name};"));
        // Drive direction depends on the inst's port direction.
        // Input (`in`) port: user assigns drive bits of the wire.
        // Output (`out`) port: bit-selects of the wire drive user wires.
        for (idx, sig_str) in &g.arb_entries {
            match dir {
                Direction::In => self.line(&format!("assign {wire_name}[{idx}] = {sig_str};")),
                Direction::Out => self.line(&format!("assign {sig_str} = {wire_name}[{idx}];")),
            }
        }
        connections.push(format!(".{group}_{sig}({wire_name})"));
    }

    /// Evaluate a `ports[N]` count expression in the context of an inst's
    /// param assignments. Falls back to evaluating against the source
    /// module's defaults if the inst doesn't override the param.
    fn eval_count_with_inst_params(&self, expr: &Expr, inst: &InstDecl) -> u64 {
        // Build a param map for the inst.
        let mut params: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        // Source defaults first.
        let target_params: Option<&[ParamDecl]> =
            self.source.items.iter().find_map(|item| match item {
                Item::Arbiter(a) if a.name.name == inst.module_name.name => {
                    Some(a.params.as_slice())
                }
                Item::Regfile(r) if r.name.name == inst.module_name.name => {
                    Some(r.params.as_slice())
                }
                _ => None,
            });
        if let Some(ps) = target_params {
            for p in ps {
                if let Some(d) = &p.default {
                    if let Some(v) = crate::elaborate::try_eval_i64(d, &params) {
                        params.insert(p.name.name.clone(), v);
                    }
                }
            }
        }
        // Inst overrides.
        for pa in &inst.param_assigns {
            if let Some(v) = crate::elaborate::try_eval_i64(&pa.value, &params) {
                params.insert(pa.name.name.clone(), v);
            }
        }
        crate::elaborate::try_eval_i64(expr, &params)
            .unwrap_or(0)
            .max(0) as u64
    }

    fn emit_generate(&mut self, gen: &GenerateDecl) {
        match gen {
            GenerateDecl::For(gf) => {
                let var = &gf.var.name;
                let start_str = self.emit_expr_str(&gf.start);
                let end_str = self.emit_expr_str(&gf.end);
                self.line(&format!("genvar {var};"));
                self.line(&format!(
                    "for ({var} = {start_str}; {var} <= {end_str}; {var} = {var} + 1) begin : gen_{var}",
                ));
                self.indent += 1;
                for item in &gf.items {
                    match item {
                        GenItem::Inst(inst) => self.emit_inst(inst),
                        GenItem::Port(_) => {
                            unreachable!("port GenItems should have been lifted by elaboration")
                        }
                        GenItem::TlmConnect(_) => unreachable!(
                            "TLM connect GenItems should have been lowered by elaboration"
                        ),
                        GenItem::Thread(_) => {
                            unreachable!("thread GenItems should have been lowered by elaboration")
                        }
                        GenItem::Seq(_) | GenItem::Comb(_) => unreachable!(
                            "seq/comb GenItems should have been unrolled by elaboration"
                        ),
                        GenItem::Wire(_) => {
                            unreachable!("wire GenItems should have been unrolled by elaboration")
                        }
                        GenItem::Assert(_) => {
                            // SVA inside generate for: not yet supported in SV codegen (SVA needs static clock ref)
                        }
                    }
                }
                self.indent -= 1;
                self.line("end");
            }
            GenerateDecl::If(gi) => {
                let cond_str = self.emit_expr_str(&gi.cond);
                self.line(&format!("if ({cond_str}) begin : gen_if"));
                self.indent += 1;
                for item in &gi.then_items {
                    match item {
                        GenItem::Inst(inst) => self.emit_inst(inst),
                        GenItem::Port(_) => {
                            unreachable!("port GenItems should have been lifted by elaboration")
                        }
                        GenItem::TlmConnect(_) => unreachable!(
                            "TLM connect GenItems should have been lowered by elaboration"
                        ),
                        GenItem::Thread(_) => {
                            unreachable!("thread GenItems should have been lowered by elaboration")
                        }
                        GenItem::Seq(_) | GenItem::Comb(_) => {
                            unreachable!("seq/comb GenItems should have been lifted by elaboration")
                        }
                        GenItem::Wire(_) => {
                            unreachable!("wire GenItems should have been unrolled by elaboration")
                        }
                        GenItem::Assert(_) => {}
                    }
                }
                self.indent -= 1;
                if !gi.else_items.is_empty() {
                    self.line("end else begin : gen_else");
                    self.indent += 1;
                    for item in &gi.else_items {
                        match item {
                            GenItem::Inst(inst) => self.emit_inst(inst),
                            GenItem::Port(_) => {
                                unreachable!("port GenItems should have been lifted by elaboration")
                            }
                            GenItem::TlmConnect(_) => unreachable!(
                                "TLM connect GenItems should have been lowered by elaboration"
                            ),
                            GenItem::Thread(_) => unreachable!(
                                "thread GenItems should have been lowered by elaboration"
                            ),
                            GenItem::Seq(_) | GenItem::Comb(_) => unreachable!(
                                "seq/comb GenItems should have been lifted by elaboration"
                            ),
                            GenItem::Wire(_) => unreachable!(
                                "wire GenItems should have been unrolled by elaboration"
                            ),
                            GenItem::Assert(_) => {}
                        }
                    }
                    self.indent -= 1;
                }
                self.line("end");
            }
        }
    }

    /// Resolve a construct's `Reset<Kind, Polarity>` port to the active-level
    /// SV expression used inside `disable iff (...)`. Returns `None` if the
    /// construct has no reset port (the SVA then has no `disable iff` clause).
    /// Active-low becomes `!rst`; active-high becomes the bare port name.
    /// Mirrors the inline pattern used by `_auto_bound_*` / `_auto_div0_*`
    /// emitters in this file.
    pub(crate) fn rst_active_from_ports(ports: &[PortDecl]) -> Option<String> {
        ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            })
    }

    fn emit_assert_sva(
        &mut self,
        a: &AssertDecl,
        construct_name: &str,
        clk: &str,
        rst_active: Option<&str>,
    ) {
        // `assert<bound_err>` properties are specification-only (their
        // exact()/abs()/ulp() builtins have no SV form) — discharged by
        // `arch formal --error-engine`, never emitted as SVA.
        if a.engine == crate::ast::AssertEngine::BoundErr {
            return;
        }
        let expr_str = self.emit_expr_str(&a.expr);
        let label = a
            .name
            .as_ref()
            .map(|n| n.name.as_str().to_string())
            .unwrap_or_else(|| match a.kind {
                AssertKind::Assert => "_assert_anon".to_string(),
                AssertKind::Cover => "_cover_anon".to_string(),
                AssertKind::Assume => "_assume_anon".to_string(),
            });
        let disable = rst_active
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();
        match a.kind {
            AssertKind::Assert => {
                self.line(&format!(
                    "{label}: assert property (@(posedge {clk}){disable} {expr_str})"
                ));
                self.line(&format!(
                    "  else $fatal(1, \"ASSERTION FAILED: {construct_name}.{label}\");"
                ));
            }
            AssertKind::Cover => {
                self.line(&format!(
                    "{label}: cover property (@(posedge {clk}){disable} {expr_str});"
                ));
            }
            AssertKind::Assume => {
                self.line(&format!(
                    "{label}: assume property (@(posedge {clk}){disable} {expr_str});"
                ));
            }
        }
    }

    /// Emit assert/cover SVA for construct-level assert declarations (FSM, FIFO, etc.)
    /// Wrapped in translate_off/on so synthesis tools and Yosys ignore the SVA.
    /// `rst_active` is the construct's active-level reset expression
    /// (`Some("!rst")` for active-low, `Some("rst")` for active-high, `None`
    /// for clockless / reset-less constructs).
    fn emit_asserts_for_construct(
        &mut self,
        asserts: &[AssertDecl],
        name: &str,
        clk: &str,
        rst_active: Option<&str>,
    ) {
        if asserts.is_empty() {
            return;
        }
        self.line("// synopsys translate_off");
        for a in asserts {
            self.emit_assert_sva(a, name, clk, rst_active);
        }
        self.line("// synopsys translate_on");
    }

    /// For each `reg ... guard <sig>` in the module, emit:
    /// 1. A shadow `_<reg>_written` flag, set on any seq-block commit for the reg.
    /// 2. An SVA contract `<sig> |-> _<reg>_written` (in translate_off).
    ///
    /// This catches the producer-bug pattern: `valid` asserts but data was never
    /// written. Verilator `--assert` and EBMC formal both consume this.
    ///
    /// v1 uses a coarse "written at least once after reset" approximation:
    /// the shadow flag is set whenever the ff block's reset branch is NOT taken
    /// (i.e. any non-reset cycle). This may over-approximate (flag goes high
    /// before the actual `<reg> <= ...` assignment), which is safe — it only
    /// misses some bug detections, never false-alarms.
    fn emit_guard_contracts(&mut self, m: &ModuleDecl) {
        let mut guarded: Vec<(String, String, crate::ast::RegReset)> = Vec::new();
        for item in &m.body {
            if let ModuleBodyItem::RegDecl(r) = item {
                if let Some(ref g) = r.guard {
                    guarded.push((r.name.name.clone(), g.name.clone(), r.reset.clone()));
                }
            }
        }
        for p in &m.ports {
            if let Some(ri) = &p.reg_info {
                if let Some(ref g) = ri.guard {
                    guarded.push((p.name.name.clone(), g.name.clone(), ri.reset.clone()));
                }
            }
        }
        if guarded.is_empty() {
            return;
        }

        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone())
            .unwrap_or_else(|| "clk".to_string());
        let (rst_name, _, is_low) = Self::extract_reset_info(&m.ports);
        let rst_active = if is_low {
            format!("!{rst_name}")
        } else {
            rst_name.clone()
        };

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Guard-contract shadow regs + SVA (one per `reg ... guard <sig>`)");
        for (reg_name, guard_sig, _) in &guarded {
            // Collect the disjunction of conditions under which `reg_name` is written.
            // If reg_name is never assigned anywhere, condition is just `false`.
            let write_conds = self.collect_write_conds(m, reg_name);
            let write_cond_expr = if write_conds.is_empty() {
                "1'b0".to_string()
            } else {
                // OR-reduce
                write_conds
                    .iter()
                    .map(|s| format!("({s})"))
                    .collect::<Vec<_>>()
                    .join(" || ")
            };

            // Shadow "written at least once" flag; goes high only when reg is actually assigned
            self.line(&format!("logic _{reg_name}_written;"));
            self.line(&format!("always_ff @(posedge {clk}) begin"));
            self.indent += 1;
            self.line(&format!("if ({rst_active}) _{reg_name}_written <= 1'b0;"));
            self.line(&format!(
                "else if ({write_cond_expr}) _{reg_name}_written <= 1'b1;"
            ));
            self.indent -= 1;
            self.line("end");
            // SVA contract (disable iff rst to exclude reset states from evaluation)
            self.line(&format!(
                "_{reg_name}_guard_contract: assert property \
                 (@(posedge {clk}) disable iff ({rst_active}) {guard_sig} |-> _{reg_name}_written)"
            ));
            self.line(&format!(
                "  else $fatal(1, \"GUARD VIOLATION: {mod}.{reg_name} — \
                 {guard_sig} asserted but {reg_name} never written\");",
                mod = m.name.name,
            ));
        }
        self.line("// synopsys translate_on");
    }

    /// Emit concurrent SVA safety checks for runtime-risky expressions in
    /// seq/latch blocks. Covers two classes:
    ///   * Bounds: Vec indexing, bit-select, variable part-select — mirrors
    ///     arch sim's `_ARCH_BCHK` runtime aborts.
    ///   * Divide-by-zero: `/` and `%` with non-const divisor — mirrors
    ///     arch sim's `_ARCH_DCHK`.
    ///
    /// **Scope** — seq/latch contexts only. Accesses that appear exclusively
    /// in comb blocks or `let` bindings are not covered here; concurrent
    /// assertions can't catch sub-cycle glitches, and wiring in immediate
    /// assertions inside generated `always_comb` is a future extension.
    /// The arch sim runtime checks (`_ARCH_BCHK`, `_ARCH_DCHK`) still fire
    /// for those paths.
    fn emit_bound_asserts(&mut self, m: &ModuleDecl) {
        // Collect const-param names — identifiers bound to compile-time constants.
        // `is_const_reducible_with` treats these as foldable so divisors named by
        // them do not produce spurious assertions.
        let const_params: std::collections::HashSet<String> = m
            .params
            .iter()
            .filter(|p| {
                matches!(
                    &p.kind,
                    ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::EnumConst(_)
                )
            })
            .map(|p| p.name.name.clone())
            .collect();

        // Build Vec<T,N> size and scalar-width lookups for accesses in this module.
        let mut vec_sizes: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut scalar_widths: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let record =
            |name: &str,
             ty: &TypeExpr,
             vec_sizes: &mut std::collections::HashMap<String, String>,
             scalar_widths: &mut std::collections::HashMap<String, String>| {
                match ty {
                    TypeExpr::Vec(_, count) => {
                        let s = Self::expr_to_sv_const(count);
                        vec_sizes.insert(name.to_string(), s);
                    }
                    TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
                        let s = Self::expr_to_sv_const(w);
                        scalar_widths.insert(name.to_string(), s);
                    }
                    TypeExpr::Bool | TypeExpr::Bit => {
                        scalar_widths.insert(name.to_string(), "1".to_string());
                    }
                    _ => {}
                }
            };
        for p in &m.ports {
            if p.bus_info.is_some() {
                continue;
            }
            record(&p.name.name, &p.ty, &mut vec_sizes, &mut scalar_widths);
        }
        for item in &m.body {
            match item {
                ModuleBodyItem::RegDecl(r) => {
                    record(&r.name.name, &r.ty, &mut vec_sizes, &mut scalar_widths)
                }
                ModuleBodyItem::WireDecl(w) => {
                    record(&w.name.name, &w.ty, &mut vec_sizes, &mut scalar_widths)
                }
                ModuleBodyItem::LetBinding(l) => {
                    if let Some(ty) = &l.ty {
                        record(&l.name.name, ty, &mut vec_sizes, &mut scalar_widths);
                    }
                }
                _ => {}
            }
        }

        // Walk seq + latch bodies, collect unique (predicate, label-tag) pairs.
        let mut sites: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in &m.body {
            match item {
                ModuleBodyItem::RegBlock(rb) => {
                    let empty_iters: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    for s in &rb.stmts {
                        self.collect_bound_stmt(
                            s,
                            &vec_sizes,
                            &scalar_widths,
                            &const_params,
                            &empty_iters,
                            None,
                            &mut sites,
                            &mut seen,
                        );
                    }
                }
                ModuleBodyItem::LatchBlock(lb) => {
                    let empty_iters: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    for s in &lb.stmts {
                        self.collect_bound_stmt(
                            s,
                            &vec_sizes,
                            &scalar_widths,
                            &const_params,
                            &empty_iters,
                            None,
                            &mut sites,
                            &mut seen,
                        );
                    }
                }
                _ => {}
            }
        }
        if sites.is_empty() {
            return;
        }

        // Pick the module's clock and reset (best-effort; use first of each).
        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let rst_active = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });

        // A module with no clock has no meaningful concurrent assertion — skip.
        let Some(clk) = clk else {
            return;
        };

        self.line("// synopsys translate_off");
        self.line("// Auto-generated safety assertions (bounds / divide-by-zero)");
        for (i, (predicate, tag)) in sites.iter().enumerate() {
            let is_div0 = tag == "div0" || tag == "mod0";
            let label_prefix = if is_div0 { "_auto_div0" } else { "_auto_bound" };
            let label = format!("{label_prefix}_{}_{}", tag, i);
            let violation_kind = if is_div0 { "DIV-BY-ZERO" } else { "BOUNDS" };
            let disable = rst_active
                .as_ref()
                .map(|r| format!(" disable iff ({r})"))
                .unwrap_or_default();
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {predicate})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"{violation_kind} VIOLATION: {mod}.{label}\");",
                mod = m.name.name
            ));
        }
        self.line("// synopsys translate_on");
    }

    /// Tier 2 of the handshake primitive: for every bus port on this module
    /// whose bus definition declares `handshake` channels, emit per-variant
    /// concurrent SVA protocol assertions, wrapped in `translate_off/on`.
    ///
    /// Labels follow `_auto_hs_<port>_<channel>_<rule>`, mirroring
    /// `_auto_bound_*` / `_auto_div0_*` for consistency with formal tools
    /// (EBMC, SymbiYosys) and simulator lint (`--assert`).
    ///
    /// The protocol rules are symmetric — they bind regardless of whether
    /// this module is the sender (initiator) or receiver (target), so
    /// perspective-flip on the bus port doesn't change which signals
    /// participate in the property.
    ///
    /// Current coverage: valid_ready → valid-stable-until-ready,
    /// valid_stall → valid-stable-while-stalled, req_ack_4phase →
    /// req-holds-until-ack, req_ack_2phase → req/ack toggle ordering.
    /// valid_only and ready_only have no backpressure/producer-valid signal
    /// pair to constrain, so they intentionally emit no protocol SVA.
    fn emit_handshake_asserts(&mut self, m: &ModuleDecl) {
        // Gather (port_name, HandshakeMeta) for each bus-typed port whose
        // bus declares one or more handshake channels.
        let mut emissions: Vec<(String, crate::ast::HandshakeMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for hs in &info.handshakes {
                emissions.push((p.name.name.clone(), hs.clone()));
            }
        }
        if emissions.is_empty() {
            return;
        }

        // Reuse the same clock/reset picking convention as emit_bound_asserts.
        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_active = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });
        let any_emits = emissions.iter().any(|(_, hs)| {
            matches!(
                hs.variant.name.as_str(),
                "valid_ready" | "valid_stall" | "req_ack_4phase" | "req_ack_2phase"
            )
        });
        if !any_emits {
            return;
        }

        self.line("// synopsys translate_off");
        self.line("// Auto-generated handshake protocol assertions (Tier 2)");
        let mod_name = m.name.name.clone();
        for (port_name, hs) in &emissions {
            let label_stem = format!("{}_{}", port_name, hs.name.name);
            let sig_prefix = format!("{}_{}", port_name, hs.name.name);
            self.emit_handshake_channel_asserts(
                &hs,
                &clk,
                rst_active.as_deref(),
                &sig_prefix,
                &label_stem,
                &mod_name,
            );
        }
        self.line("// synopsys translate_on");
    }

    /// Construct-agnostic helper: emit Tier-2 protocol-SVA for a single
    /// `handshake_channel`. Shared between the bus-port path
    /// (`emit_handshake_asserts`) and the arbiter port-list path
    /// (`emit_arbiter_handshake_asserts`).
    ///
    /// Parameters:
    /// - `hs`        — channel metadata (variant, name, array shape).
    /// - `clk`       — already-resolved clock signal name (no edge keyword).
    /// - `rst_active`— already-resolved active-level reset expression
    ///   (e.g. `rst` or `!rst_n`); `None` skips `disable iff`.
    /// - `sig_prefix`— prefix for the per-variant control signals; the
    ///   helper appends `_valid`/`_ready`/etc. For the bus
    ///   path this is `<port>_<chname>`; for arbiters it is
    ///   just `<chname>` (signals are top-level).
    /// - `label_stem`— middle of `_auto_hs_<stem>_<rule>` for SV labels.
    ///   Bus-path stem is `<port>_<chname>`; arbiter is
    ///   `<chname>` (plus a per-lane suffix when arrayed).
    /// - `mod_name`  — enclosing construct name for the `$fatal` message.
    ///
    /// When `hs.array_count` is `Some(expr)`, the property is wrapped in
    /// an SV `generate for (genvar i = 0; i < <expr>; i++) ... end
    /// endgenerate` block and signal references are indexed with `[i]`,
    /// so the assertion fires once per request lane. The bus path never
    /// sets `array_count`, so the bare-signal form is preserved
    /// byte-for-byte with prior emission.
    fn emit_handshake_channel_asserts(
        &mut self,
        hs: &crate::ast::HandshakeMeta,
        clk: &str,
        rst_active: Option<&str>,
        sig_prefix: &str,
        label_stem: &str,
        mod_name: &str,
    ) {
        let variant = hs.variant.name.as_str();
        let disable = rst_active
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();

        if !matches!(
            variant,
            "valid_ready" | "valid_stall" | "req_ack_4phase" | "req_ack_2phase"
        ) {
            return;
        }

        // Vector channels (arbiter `handshake_channel name[N]: ...`) get a
        // genvar-indexed wrapper. The bus-body path always passes
        // `array_count = None`, so this branch is dead for buses and the
        // emitted SV is byte-identical to pre-refactor behaviour.
        let (open_gen, close_gen, idx, lane_label_suffix) = match &hs.array_count {
            Some(count_expr) => {
                let count_str = self.emit_expr_str(count_expr);
                self.line(&format!(
                    "generate for (genvar i = 0; i < {count_str}; i++) begin: g_auto_hs_{label_stem}"
                ));
                self.indent += 1;
                ("", "", "[i]", "__lane")
            }
            None => ("", "", "", ""),
        };
        let _ = (open_gen, close_gen); // generate-for header already emitted

        let sig = |s: &str| format!("{}_{}{}", sig_prefix, s, idx);
        let mk_label =
            |rule: &str| format!("_auto_hs_{}{}_{}", label_stem, lane_label_suffix, rule);
        let emit_property = |cg: &mut Codegen, rule: &str, predicate: String, message: &str| {
            let label = mk_label(rule);
            cg.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {predicate})"
            ));
            cg.line(&format!(
                "  else $fatal(1, \"HANDSHAKE VIOLATION ({message}): {mod_name}.{label}\");"
            ));
        };
        match variant {
            "valid_ready" => {
                let v = sig("valid");
                let r = sig("ready");
                emit_property(
                    self,
                    "valid_stable",
                    format!("({v} && !{r}) |=> {v}"),
                    "valid must stay asserted until ready",
                );
            }
            "valid_stall" => {
                let v = sig("valid");
                let s = sig("stall");
                emit_property(
                    self,
                    "valid_stable_while_stall",
                    format!("({v} && {s}) |=> {v}"),
                    "valid must not change while stalled",
                );
            }
            "req_ack_4phase" => {
                let rq = sig("req");
                let ak = sig("ack");
                emit_property(
                    self,
                    "req_holds_until_ack",
                    format!("({rq} && !{ak}) |=> {rq}"),
                    "req must stay asserted until ack",
                );
            }
            "req_ack_2phase" => {
                let rq = sig("req");
                let ak = sig("ack");
                emit_property(
                    self,
                    "req_toggles_only_when_idle",
                    format!("({rq} != $past({rq})) |-> ($past({rq}) == $past({ak}))"),
                    "req may toggle only when no transfer is pending",
                );
                emit_property(
                    self,
                    "ack_toggles_only_when_pending",
                    format!("({ak} != $past({ak})) |-> ($past({rq}) != $past({ak}))"),
                    "ack may toggle only after a req toggle",
                );
            }
            _ => unreachable!("unsupported handshake variant pre-filtered"),
        }

        if hs.array_count.is_some() {
            self.indent -= 1;
            self.line("end endgenerate");
        }
    }

    /// Tier 2 of the handshake primitive for `arbiter` constructs: for
    /// every `handshake_channel` declared in the arbiter's port list,
    /// emit the same per-variant SVA the bus path emits, wrapped in
    /// `generate for` blocks when the channel has an explicit `[N]`
    /// array shape (the typical `request[NUM_REQ]` case).
    ///
    /// PR #343 desugars `handshake_channel` to underlying valid/ready/
    /// payload ports — the same Tier-2 property templates apply, only
    /// the signal-name convention differs: top-level `<chname>_<sig>`
    /// (or array-element `<chname>_<sig>[i]`) instead of bus-port
    /// `<port>_<chname>_<sig>`. Labels follow `_auto_hs_<chname>_<rule>`
    /// (with `__lane` appended inside generate blocks).
    pub(crate) fn emit_arbiter_handshake_asserts(&mut self, a: &crate::ast::ArbiterDecl) {
        if a.handshakes.is_empty() {
            return;
        }

        // Pick clock/reset the same way emit_arbiter does.
        let clk = a
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_active = a
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });

        // Filter to variants the shared helper actually emits property
        // text for, so we don't emit a translate-off block whose body is
        // empty (matches the bus side, which skips the wrapper when no
        // emission would land).
        let any_emits = a.handshakes.iter().any(|hs| {
            matches!(
                hs.variant.name.as_str(),
                "valid_ready" | "valid_stall" | "req_ack_4phase" | "req_ack_2phase"
            )
        });
        if !any_emits {
            return;
        }

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Auto-generated handshake protocol assertions (Tier 2)");
        let mod_name = a.name.name.clone();
        let handshakes = a.handshakes.clone();
        for hs in &handshakes {
            let label_stem = hs.name.name.clone();
            let sig_prefix = hs.name.name.clone();
            self.emit_handshake_channel_asserts(
                hs,
                &clk,
                rst_active.as_deref(),
                &sig_prefix,
                &label_stem,
                &mod_name,
            );
        }
        self.line("// synopsys translate_on");
    }

    /// Emit the synthesized credit-counter state for each `send`-role
    /// `credit_channel` sub-construct on a bus port of this module.
    ///
    /// Per port+channel pair, emits three things:
    ///
    /// 1. `logic [W-1:0] __<port>_<ch>_credit;` — the credit register,
    ///    width = clog2(DEPTH+1).
    /// 2. An `always_ff` block that resets the counter to DEPTH on reset
    ///    and updates it each cycle: `-1` when `send_valid && !credit_return`,
    ///    `+1` when `credit_return && !send_valid`, no change when both fire
    ///    in the same cycle (plan §Lowering).
    /// 3. `wire __<port>_<ch>_can_send = __<port>_<ch>_credit != 0;` —
    ///    combinational current-cycle availability. Users whose design
    ///    needs a timing-relief flop will opt in via the upcoming
    ///    `CAN_SEND_REGISTERED` channel param (next-state flop semantics,
    ///    option (b) — see doc/archive/plan_credit_channel.md).
    ///
    /// PR #3b-ii emits only the sender-side state — target-side FIFO +
    /// credit_return-pulse wiring lands in PR #3b-iii; `ch.send()` /
    /// `ch.can_send` method dispatch desugars to `__<port>_<ch>_*` in a
    /// follow-up. Users today can read `__<port>_<ch>_can_send` directly
    /// and drive `<port>_<ch>_send_valid` from their own comb to build
    /// a compliant sender without the sugar.
    fn emit_credit_channel_state(&mut self, m: &ModuleDecl) {
        let mut emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for cc in &info.credit_channels {
                // Initiator perspective drives send; on the target perspective
                // the same bus flip inverts the data direction, but the sender
                // state belongs on whichever side actually issues sends.
                let is_sender = match (cc.role_dir, bi.perspective) {
                    (Direction::Out, crate::ast::BusPerspective::Initiator) => true,
                    (Direction::In, crate::ast::BusPerspective::Target) => true,
                    _ => false,
                };
                if is_sender {
                    emissions.push((p.name.name.clone(), cc.clone()));
                }
            }
        }
        if emissions.is_empty() {
            return;
        }

        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_port = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)));
        let (rst_edge, rst_active) = match rst_port {
            Some(p) => {
                let active = match &p.ty {
                    TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                    _ => p.name.name.clone(),
                };
                let edge = match &p.ty {
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::Low) => {
                        format!(" or negedge {}", p.name.name)
                    }
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::High) => {
                        format!(" or posedge {}", p.name.name)
                    }
                    _ => String::new(),
                };
                (edge, Some(active))
            }
            None => (String::new(), None),
        };

        self.line("");
        self.line("// Auto-generated credit_channel state (PR #3b-ii, sender side)");
        for (port_name, cc) in &emissions {
            let ch = &cc.name.name;
            let depth_expr = cc
                .params
                .iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref());
            let Some(depth_expr) = depth_expr else {
                continue;
            };
            let depth_str = self.emit_expr_str(depth_expr);
            let credit_reg = format!("__{port_name}_{ch}_credit");
            let cs_name = format!("__{port_name}_{ch}_can_send");
            let send_valid = format!("{port_name}_{ch}_send_valid");
            let credit_ret = format!("{port_name}_{ch}_credit_return");
            // Look up CAN_SEND_REGISTERED (option b — next-state flop, agreed
            // semantics). Non-zero = register the can_send signal so its
            // fan-out comes off a flop; the combinational critical path then
            // ends at the flop input. Full throughput is preserved because the
            // flopped signal reflects counter_next (current counter ± same-
            // cycle send/return), so send_valid |-> counter > 0 still holds.
            let registered = cc
                .params
                .iter()
                .find(|p| p.name.name == "CAN_SEND_REGISTERED")
                .and_then(|p| p.default.as_ref())
                .map(|e| self.emit_expr_str(e))
                .map(|s| s.trim() != "0")
                .unwrap_or(false);
            // Width = $clog2(DEPTH + 1). Emit as-is; SV reduces at elab.
            self.line(&format!(
                "logic [$clog2(({depth_str}) + 1) - 1:0] {credit_reg};"
            ));
            if registered {
                self.line(&format!("logic {cs_name};"));
            } else {
                self.line(&format!("wire  {cs_name} = {credit_reg} != 0;"));
            }
            // Emit the counter update (always_ff). If registered, also flop
            // can_send: `__..._can_send <= counter_next > 0`. The counter_next
            // is not an SV-visible signal; we inline the next-state expression
            // to preserve semantics without introducing an extra wire.
            //
            // counter_next =  credit + 1   when (credit_return && !send_valid)
            //               | credit - 1   when (send_valid && !credit_return)
            //               | credit       otherwise
            //
            // So counter_next > 0 reduces to:
            //   (credit_return && !send_valid) ? 1
            //   : (send_valid && !credit_return) ? (credit > 1)
            //   : (credit > 0)
            let cs_next = format!(
                "({credit_ret} && !{send_valid}) ? 1'b1 : \
                 ({send_valid} && !{credit_ret}) ? ({credit_reg} > 1) : \
                 ({credit_reg} != 0)"
            );
            match &rst_active {
                Some(r) => {
                    self.line(&format!("always_ff @(posedge {clk}{rst_edge}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({r}) begin"));
                    self.indent += 1;
                    self.line(&format!("{credit_reg} <= {depth_str};"));
                    if registered {
                        self.line(&format!("{cs_name} <= ({depth_str}) != 0;"));
                    }
                    self.indent -= 1;
                    self.line("end else begin");
                    self.indent += 1;
                    self.line(&format!(
                        "if ({send_valid} && !{credit_ret}) {credit_reg} <= {credit_reg} - 1;"
                    ));
                    self.line(&format!(
                        "else if ({credit_ret} && !{send_valid}) {credit_reg} <= {credit_reg} + 1;"
                    ));
                    if registered {
                        self.line(&format!("{cs_name} <= {cs_next};"));
                    }
                    self.indent -= 1;
                    self.line("end");
                    self.indent -= 1;
                    self.line("end");
                }
                None => {
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!(
                        "if ({send_valid} && !{credit_ret}) {credit_reg} <= {credit_reg} - 1;"
                    ));
                    self.line(&format!(
                        "else if ({credit_ret} && !{send_valid}) {credit_reg} <= {credit_reg} + 1;"
                    ));
                    if registered {
                        self.line(&format!("{cs_name} <= {cs_next};"));
                    }
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// Emit the receiver-side FIFO + pop wiring for each credit_channel
    /// where this module is the consumer (target on a `send`-role channel,
    /// or initiator on a `receive`-role channel). Pops when the user-driven
    /// `<port>_<ch>_credit_return` is asserted and the FIFO is non-empty.
    ///
    /// Emits the following per (port, credit_channel):
    ///   logic [W-1:0]      __<port>_<ch>_buf [DEPTH];
    ///   logic [AW-1:0]     __<port>_<ch>_head;
    ///   logic [AW-1:0]     __<port>_<ch>_tail;
    ///   logic [OW-1:0]     __<port>_<ch>_occ;     // 0..DEPTH
    ///   wire              __<port>_<ch>_valid = __<port>_<ch>_occ != 0;
    ///   wire [W-1:0]      __<port>_<ch>_data  = __<port>_<ch>_buf[head];
    ///   always_ff          // push on send_valid, pop on credit_return && valid
    ///
    /// Where W = type width of the payload T, AW = $clog2(DEPTH),
    /// OW = $clog2(DEPTH+1).
    ///
    /// Scope note (PR #3b-iii): these wires are SV-level only. ARCH-level
    /// method dispatch (`port.ch.valid`, `port.ch.data`) is not yet wired
    /// up — that lands once the AST-level synthesized-wire story is
    /// locked down. In the interim, the FIFO is observable by reading
    /// the SV names directly (e.g. from a cocotb TB) or by writing raw
    /// send/credit_return drives and trusting the invariants hold.
    fn emit_credit_channel_receiver_state(&mut self, m: &ModuleDecl) {
        let mut emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for cc in &info.credit_channels {
                // Receiver side mirrors the sender-state selector:
                //   send role + target perspective → this module is the receiver
                //   receive role + initiator perspective → this module is the receiver
                let is_receiver = match (cc.role_dir, bi.perspective) {
                    (Direction::Out, crate::ast::BusPerspective::Target) => true,
                    (Direction::In, crate::ast::BusPerspective::Initiator) => true,
                    _ => false,
                };
                if is_receiver {
                    emissions.push((p.name.name.clone(), cc.clone()));
                }
            }
        }
        if emissions.is_empty() {
            return;
        }

        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_port = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)));
        let (rst_edge, rst_active) = match rst_port {
            Some(p) => {
                let active = match &p.ty {
                    TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                    _ => p.name.name.clone(),
                };
                let edge = match &p.ty {
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::Low) => {
                        format!(" or negedge {}", p.name.name)
                    }
                    TypeExpr::Reset(ResetKind::Async, ResetLevel::High) => {
                        format!(" or posedge {}", p.name.name)
                    }
                    _ => String::new(),
                };
                (edge, Some(active))
            }
            None => (String::new(), None),
        };

        self.line("");
        self.line("// Auto-generated credit_channel target-side FIFO (PR #3b-iii)");
        for (port_name, cc) in &emissions {
            let ch = &cc.name.name;
            let depth_expr = cc
                .params
                .iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref());
            let Some(depth_expr) = depth_expr else {
                continue;
            };
            let depth_str = self.emit_expr_str(depth_expr);
            // Payload type width — resolve via the ParamKind::Type default.
            let payload_ty_opt =
                cc.params
                    .iter()
                    .find(|p| p.name.name == "T")
                    .and_then(|p| match &p.kind {
                        crate::ast::ParamKind::Type(te) => Some(te.clone()),
                        _ => None,
                    });
            let Some(payload_ty) = payload_ty_opt else {
                continue;
            };
            let Some(width_str) = self.type_expr_data_width(&payload_ty) else {
                continue;
            };
            let buf = format!("__{port_name}_{ch}_buf");
            let head = format!("__{port_name}_{ch}_head");
            let tail = format!("__{port_name}_{ch}_tail");
            let occ = format!("__{port_name}_{ch}_occ");
            let valid_w = format!("__{port_name}_{ch}_valid");
            let data_w = format!("__{port_name}_{ch}_data");
            let push = format!("{port_name}_{ch}_send_valid");
            let pushd = format!("{port_name}_{ch}_send_data");
            let pop_drv = format!("{port_name}_{ch}_credit_return");

            self.line(&format!(
                "logic [({width_str}) - 1:0] {buf} [({depth_str})];"
            ));
            self.line(&format!(
                "logic [$clog2({depth_str}) == 0 ? 0 : $clog2({depth_str}) - 1:0] {head};"
            ));
            self.line(&format!(
                "logic [$clog2({depth_str}) == 0 ? 0 : $clog2({depth_str}) - 1:0] {tail};"
            ));
            self.line(&format!("logic [$clog2(({depth_str}) + 1) - 1:0] {occ};"));
            self.line(&format!("wire  {valid_w} = {occ} != 0;"));
            self.line(&format!(
                "wire [({width_str}) - 1:0] {data_w} = {buf}[{head}];"
            ));

            // Update block: push on send_valid, pop on user-driven credit_return.
            let pop_fire = format!("({pop_drv} && {valid_w})");
            match &rst_active {
                Some(r) => {
                    self.line(&format!("always_ff @(posedge {clk}{rst_edge}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({r}) begin"));
                    self.indent += 1;
                    self.line(&format!("{head} <= 0;"));
                    self.line(&format!("{tail} <= 0;"));
                    self.line(&format!("{occ}  <= 0;"));
                    self.indent -= 1;
                    self.line("end else begin");
                    self.indent += 1;
                    self.line(&format!("if ({push}) begin"));
                    self.indent += 1;
                    self.line(&format!("{buf}[{tail}] <= {pushd};"));
                    self.line(&format!("{tail} <= ({tail} + 1) % ({depth_str});"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!(
                        "if ({pop_fire}) {head} <= ({head} + 1) % ({depth_str});"
                    ));
                    self.line(&format!("if ({push} && !{pop_fire}) {occ} <= {occ} + 1;"));
                    self.line(&format!(
                        "else if (!{push} &&  {pop_fire}) {occ} <= {occ} - 1;"
                    ));
                    self.indent -= 1;
                    self.line("end");
                    self.indent -= 1;
                    self.line("end");
                }
                None => {
                    self.line(&format!("always_ff @(posedge {clk}) begin"));
                    self.indent += 1;
                    self.line(&format!("if ({push}) begin"));
                    self.indent += 1;
                    self.line(&format!("{buf}[{tail}] <= {pushd};"));
                    self.line(&format!("{tail} <= ({tail} + 1) % ({depth_str});"));
                    self.indent -= 1;
                    self.line("end");
                    self.line(&format!(
                        "if ({pop_fire}) {head} <= ({head} + 1) % ({depth_str});"
                    ));
                    self.line(&format!("if ({push} && !{pop_fire}) {occ} <= {occ} + 1;"));
                    self.line(&format!(
                        "else if (!{push} &&  {pop_fire}) {occ} <= {occ} - 1;"
                    ));
                    self.indent -= 1;
                    self.line("end");
                }
            }
        }
    }

    /// "Is this a compile-time reducible constant?" test. Matches the sim-
    /// codegen rule so runtime vs compile-time treatment of divisors stays
    /// consistent. Literals, `$clog2(const)`, arithmetic over reducibles, and
    /// identifier references to const params declared in the current module.
    /// Runs during `emit_bound_asserts`, which already has the module's
    /// const-param set in scope.
    /// True iff `e` is the literal `1` (decimal, hex, binary, or sized
    /// like `1'b1` / `1'd1`). Used by `emit_type_and_array_suffix` to
    /// detect `UInt<1>` so the redundant `[0:0]` inner dim can be
    /// collapsed when emitting `Vec<UInt<1>, N>` ports.
    fn is_const_one(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(n))
            | ExprKind::Literal(LitKind::Hex(n))
            | ExprKind::Literal(LitKind::Bin(n))
            | ExprKind::Literal(LitKind::Sized(_, n)) => *n == 1,
            _ => false,
        }
    }

    fn is_const_reducible_with(e: &Expr, const_params: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            ExprKind::Literal(_) => true,
            ExprKind::Ident(n) => const_params.contains(n),
            ExprKind::Clog2(a) => Self::is_const_reducible_with(a, const_params),
            ExprKind::Binary(_, a, b) => {
                Self::is_const_reducible_with(a, const_params)
                    && Self::is_const_reducible_with(b, const_params)
            }
            ExprKind::Unary(_, a) => Self::is_const_reducible_with(a, const_params),
            _ => false,
        }
    }

    /// Emit Tier-2 SVA protocol assertions for each credit_channel on this
    /// module. Labels follow `_auto_cc_<port>_<ch>_<rule>`, mirroring the
    /// handshake / bounds / divide-by-zero naming so EBMC and Verilator
    /// `--assert` consumers can target them uniformly.
    ///
    /// Invariants emitted:
    /// - **credit_bounds** (sender): `__<port>_<ch>_credit <= DEPTH`. Holds
    ///   because the counter update is strictly ±1 and the reset value is
    ///   DEPTH — but provable properties catch any future regression that
    ///   double-decrements or misses reset.
    /// - **send_requires_credit** (sender): `send_valid |-> credit > 0`.
    ///   The user is responsible for gating send_valid on can_send; if they
    ///   fail to, this trips.
    /// - **credit_return_requires_buffered** (receiver): `credit_return |->
    ///   __<port>_<ch>_valid`. The receiver must only pulse credit_return
    ///   when the FIFO actually has something to pop; otherwise the sender
    ///   sees a spurious credit and can overflow the buffer.
    ///
    /// Deferred: occupancy = DEPTH - credit (cross-module property; lands
    /// with a hierarchical-formal story).
    fn emit_credit_channel_asserts(&mut self, m: &ModuleDecl) {
        let mut sender_emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        let mut receiver_emissions: Vec<(String, crate::ast::CreditChannelMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for cc in &info.credit_channels {
                let is_sender = matches!(
                    (cc.role_dir, bi.perspective),
                    (Direction::Out, crate::ast::BusPerspective::Initiator)
                        | (Direction::In, crate::ast::BusPerspective::Target)
                );
                if is_sender {
                    sender_emissions.push((p.name.name.clone(), cc.clone()));
                } else {
                    receiver_emissions.push((p.name.name.clone(), cc.clone()));
                }
            }
        }
        if sender_emissions.is_empty() && receiver_emissions.is_empty() {
            return;
        }

        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_active = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });
        let disable = rst_active
            .as_ref()
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();
        let mod_name = m.name.name.clone();

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Auto-generated credit_channel protocol assertions (Tier 2)");

        for (port_name, cc) in &sender_emissions {
            let ch = &cc.name.name;
            let Some(depth_expr) = cc
                .params
                .iter()
                .find(|p| p.name.name == "DEPTH")
                .and_then(|p| p.default.as_ref())
            else {
                continue;
            };
            let depth_str = self.emit_expr_str(depth_expr);
            let credit_reg = format!("__{port_name}_{ch}_credit");
            let send_valid = format!("{port_name}_{ch}_send_valid");

            let label = format!("_auto_cc_{port_name}_{ch}_credit_bounds");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {credit_reg} <= ({depth_str}))"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): {mod_name}.{label}\");"
            ));

            let label = format!("_auto_cc_{port_name}_{ch}_send_requires_credit");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {send_valid} |-> {credit_reg} > 0)"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (send without credit): {mod_name}.{label}\");"
            ));
        }

        for (port_name, cc) in &receiver_emissions {
            let ch = &cc.name.name;
            let credit_ret = format!("{port_name}_{ch}_credit_return");
            let buf_valid = format!("__{port_name}_{ch}_valid");
            let label = format!("_auto_cc_{port_name}_{ch}_credit_return_requires_buffered");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} {credit_ret} |-> {buf_valid})"
            ));
            self.line(&format!(
                "  else $fatal(1, \"CREDIT-CHANNEL VIOLATION (credit_return without buffered data): {mod_name}.{label}\");"
            ));
        }

        self.line("// synopsys translate_on");
    }

    /// TLM method protocol assertions for the flattened req/rsp handshake.
    ///
    /// These properties are endpoint-local and intentionally symmetric across
    /// initiator/target perspectives: if a request or response is held under
    /// backpressure, the valid bit and payload observed on that module's
    /// flattened TLM wires must remain stable on the next cycle. That catches
    /// the common generated/interconnect bug where address/data/tag changes
    /// while `valid && !ready`.
    fn emit_tlm_method_asserts(&mut self, m: &ModuleDecl) {
        let mut emissions: Vec<(String, crate::ast::TlmMethodMeta)> = Vec::new();
        for p in &m.ports {
            let Some(ref bi) = p.bus_info else {
                continue;
            };
            let Some((crate::resolve::Symbol::Bus(info), _)) =
                self.symbols.globals.get(&bi.bus_name.name)
            else {
                continue;
            };
            for tm in &info.tlm_methods {
                emissions.push((p.name.name.clone(), tm.clone()));
            }
        }
        if emissions.is_empty() {
            return;
        }

        let clk = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.clone());
        let Some(clk) = clk else {
            return;
        };
        let rst_active = m
            .ports
            .iter()
            .find(|p| matches!(&p.ty, TypeExpr::Reset(_, _)))
            .map(|p| match &p.ty {
                TypeExpr::Reset(_, ResetLevel::Low) => format!("!{}", p.name.name),
                _ => p.name.name.clone(),
            });
        let disable = rst_active
            .as_ref()
            .map(|r| format!(" disable iff ({r})"))
            .unwrap_or_default();
        let mod_name = m.name.name.clone();

        self.line("");
        self.line("// synopsys translate_off");
        self.line("// Auto-generated TLM method protocol assertions");

        for (port_name, tm) in &emissions {
            let method = &tm.name.name;
            let sig = |suffix: &str| format!("{port_name}_{method}_{suffix}");
            let req_valid = sig("req_valid");
            let req_ready = sig("req_ready");
            let rsp_valid = sig("rsp_valid");
            let rsp_ready = sig("rsp_ready");

            let mut req_hold_terms = vec![req_valid.clone()];
            if tm.out_of_order_tags.is_some() {
                req_hold_terms.push(format!("$stable({})", sig("req_tag")));
            }
            for (arg_name, _) in &tm.args {
                req_hold_terms.push(format!("$stable({})", sig(&arg_name.name)));
            }
            let req_hold = req_hold_terms.join(" && ");
            let label = format!("_auto_tlm_{port_name}_{method}_req_stable");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} ({req_valid} && !{req_ready}) |=> ({req_hold}))"
            ));
            self.line(&format!(
                "  else $fatal(1, \"TLM VIOLATION (request changed while stalled): {mod_name}.{label}\");"
            ));

            let mut rsp_hold_terms = vec![rsp_valid.clone()];
            if tm.out_of_order_tags.is_some() {
                rsp_hold_terms.push(format!("$stable({})", sig("rsp_tag")));
            }
            if tm.ret.is_some() {
                rsp_hold_terms.push(format!("$stable({})", sig("rsp_data")));
            }
            let rsp_hold = rsp_hold_terms.join(" && ");
            let label = format!("_auto_tlm_{port_name}_{method}_rsp_stable");
            self.line(&format!(
                "{label}: assert property (@(posedge {clk}){disable} ({rsp_valid} && !{rsp_ready}) |=> ({rsp_hold}))"
            ));
            self.line(&format!(
                "  else $fatal(1, \"TLM VIOLATION (response changed while stalled): {mod_name}.{label}\");"
            ));
        }

        self.line("// synopsys translate_on");
    }

    /// Stringify a compile-time constant expression to an SV literal/expression.
    /// For the common case (integer literal) just prints the number; for
    /// `$clog2(...)` / param refs / arithmetic, prints the SV form.
    fn expr_to_sv_const(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(v))
            | ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => v.to_string(),
            ExprKind::Ident(n) => n.clone(),
            _ => "0".to_string(),
        }
    }

    /// Recursively collect bound-assertion sites from a seq-context Stmt.
    fn collect_bound_stmt(
        &self,
        s: &Stmt,
        vec_sizes: &std::collections::HashMap<String, String>,
        scalar_widths: &std::collections::HashMap<String, String>,
        const_params: &std::collections::HashSet<String>,
        loop_iters: &std::collections::HashSet<String>,
        guard: Option<&str>,
        sites: &mut Vec<(String, String)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match s {
            Stmt::Assign(a) => {
                self.collect_bound_expr(
                    &a.target,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    &a.value,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            Stmt::IfElse(ie) => {
                self.collect_bound_expr(
                    &ie.cond,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                let cond = self.emit_expr_str(&ie.cond);
                let then_guard = Self::and_bound_guard(guard, &cond);
                let else_guard = Self::and_bound_guard(guard, &format!("!({cond})"));
                for s in &ie.then_stmts {
                    self.collect_bound_stmt(
                        s,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        Some(&then_guard),
                        sites,
                        seen,
                    );
                }
                for s in &ie.else_stmts {
                    self.collect_bound_stmt(
                        s,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        Some(&else_guard),
                        sites,
                        seen,
                    );
                }
            }
            Stmt::Match(m) => {
                self.collect_bound_expr(
                    &m.scrutinee,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                for arm in &m.arms {
                    for s in &arm.body {
                        self.collect_bound_stmt(
                            s,
                            vec_sizes,
                            scalar_widths,
                            const_params,
                            loop_iters,
                            guard,
                            sites,
                            seen,
                        );
                    }
                }
            }
            Stmt::For(f) => {
                if let ForRange::Range(lo, hi) = &f.range {
                    self.collect_bound_expr(
                        lo,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                    self.collect_bound_expr(
                        hi,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
                // Add the loop iterator name to the in-scope set so any
                // `Vec[iter]` index inside the body elides the bound assertion:
                // the iterator is statically constrained by the loop range, and
                // the auto-emitted assertion would reference `iter` outside the
                // for-loop scope at SV level — Verilator can't resolve that
                // (`Can't find definition of variable: 'fb'`).
                let mut nested = loop_iters.clone();
                nested.insert(f.var.name.clone());
                for s in &f.body {
                    self.collect_bound_stmt(
                        s,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        &nested,
                        guard,
                        sites,
                        seen,
                    );
                }
            }
            Stmt::Init(ib) => {
                for s in &ib.body {
                    self.collect_bound_stmt(
                        s,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
            }
            Stmt::WaitUntil(e, _) => self.collect_bound_expr(
                e,
                vec_sizes,
                scalar_widths,
                const_params,
                loop_iters,
                guard,
                sites,
                seen,
            ),
            Stmt::DoUntil { body, cond, .. } => {
                for s in body {
                    self.collect_bound_stmt(
                        s,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
                self.collect_bound_expr(
                    cond,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            Stmt::Log(_) => {}
        }
    }

    fn and_bound_guard(parent: Option<&str>, cond: &str) -> String {
        match parent {
            Some(p) if !p.is_empty() => format!("({p}) && ({cond})"),
            _ => cond.to_string(),
        }
    }

    fn guard_bound_predicate(predicate: String, guard: Option<&str>) -> String {
        match guard {
            Some(g) if !g.is_empty() => format!("(({g}) |-> ({predicate}))"),
            _ => predicate,
        }
    }

    /// True when `predicate` mentions a for-loop iterator as a standalone SV
    /// identifier, which makes it unhoistable to module scope.
    ///
    /// `collect_bound_expr` already elides a site whose *index* is a bare
    /// iterator, but the iterator also reaches the predicate two other ways:
    /// through an enclosing `if` inside the loop body, whose condition is
    /// folded in as the `|->` antecedent, and through a compound index such
    /// as `mem[i + 1]`. In both cases the emitted concurrent assertion sits
    /// outside the generated `for` block, where the iterator does not exist,
    /// and Verilator rejects the whole file with "Can't find definition of
    /// variable: 'i'" — the SV `arch build` produced would not elaborate.
    ///
    /// Skipping the site is the conservative choice: an unguarded variant
    /// would assert on cycles the access is not taken, which is a false
    /// failure. The `arch sim` runtime check still covers these accesses,
    /// exactly as it does for the comb/let sites that are not mirrored either.
    fn predicate_uses_loop_iter(
        predicate: &str,
        loop_iters: &std::collections::HashSet<String>,
    ) -> bool {
        if loop_iters.is_empty() {
            return false;
        }
        let bytes = predicate.as_bytes();
        let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
        loop_iters.iter().any(|iter| {
            if iter.is_empty() {
                return false;
            }
            let mut from = 0usize;
            while let Some(offset) = predicate[from..].find(iter.as_str()) {
                let start = from + offset;
                let end = start + iter.len();
                let boundary_before = start == 0 || !is_ident_byte(bytes[start - 1]);
                let boundary_after = end == bytes.len() || !is_ident_byte(bytes[end]);
                if boundary_before && boundary_after {
                    return true;
                }
                from = start + 1;
            }
            false
        })
    }

    /// Recursively collect bound-assertion sites from an expression. At each
    /// Index / PartSelect with a non-literal index whose base is an ident of
    /// known size, push a predicate. Also catches `/` and `%` with non-const
    /// divisor. Dedups by predicate string.
    fn collect_bound_expr(
        &self,
        e: &Expr,
        vec_sizes: &std::collections::HashMap<String, String>,
        scalar_widths: &std::collections::HashMap<String, String>,
        const_params: &std::collections::HashSet<String>,
        loop_iters: &std::collections::HashSet<String>,
        guard: Option<&str>,
        sites: &mut Vec<(String, String)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        let idx_is_const = |ex: &Expr| matches!(&ex.kind, ExprKind::Literal(_));
        // True when the index is a bare identifier that names a for-loop
        // iterator currently in scope. The iterator is statically bounded
        // by the loop range, so emitting an `int'(iter) < (LIMIT)`
        // assertion at module scope would (a) reference an SV identifier
        // that doesn't exist outside the for-loop body and (b) duplicate a
        // check the loop range already enforces.
        let idx_is_loop_iter = |ex: &Expr| -> bool {
            if let ExprKind::Ident(n) = &ex.kind {
                loop_iters.contains(n)
            } else {
                false
            }
        };
        let base_ident = |ex: &Expr| -> Option<String> {
            if let ExprKind::Ident(n) = &ex.kind {
                Some(n.clone())
            } else {
                None
            }
        };
        let push = |predicate: String,
                    tag: &str,
                    sites: &mut Vec<(String, String)>,
                    seen: &mut std::collections::HashSet<String>| {
            let predicate = Self::guard_bound_predicate(predicate, guard);
            if Self::predicate_uses_loop_iter(&predicate, loop_iters) {
                return;
            }
            if seen.insert(predicate.clone()) {
                sites.push((predicate, tag.to_string()));
            }
        };
        match &e.kind {
            ExprKind::Index(base, idx) => {
                if !idx_is_const(idx) && !idx_is_loop_iter(idx) {
                    if let Some(name) = base_ident(base) {
                        let idx_s = self.emit_expr_str(idx);
                        if let Some(limit) = vec_sizes.get(&name) {
                            push(format!("int'({idx_s}) < ({limit})"), "vec", sites, seen);
                        } else if let Some(w) = scalar_widths.get(&name) {
                            push(format!("int'({idx_s}) < ({w})"), "bitsel", sites, seen);
                        }
                    }
                }
                self.collect_bound_expr(
                    base,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    idx,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            ExprKind::PartSelect(base, start, width, up) => {
                if !idx_is_const(start) && !idx_is_loop_iter(start) {
                    if let Some(name) = base_ident(base) {
                        if let Some(bw) = scalar_widths.get(&name) {
                            let s_s = self.emit_expr_str(start);
                            let w_s = Self::expr_to_sv_const(width);
                            let (pred, tag) = if *up {
                                // [+:W]: need start + W <= bw
                                (format!("(({s_s}) + ({w_s})) <= ({bw})"), "partsel_up")
                            } else {
                                // [-:W]: need start < bw AND start >= W-1
                                (
                                    format!("(({s_s}) < ({bw})) && (({s_s}) >= (({w_s}) - 1))"),
                                    "partsel_down",
                                )
                            };
                            push(pred, tag, sites, seen);
                        }
                    }
                }
                self.collect_bound_expr(
                    base,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    start,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            ExprKind::Binary(op, a, b) => {
                // Divide-by-zero assertion: divisor must be non-zero at every
                // clock edge this access is live. Skip if divisor is a
                // compile-time reducible constant (typecheck already rejected
                // literal zero; non-zero folded constants need no check).
                if matches!(op, BinOp::Div | BinOp::Mod)
                    && !Self::is_const_reducible_with(b, const_params)
                {
                    let rhs_s = self.emit_expr_str(b);
                    let tag = if *op == BinOp::Div { "div0" } else { "mod0" };
                    let pred = Self::guard_bound_predicate(format!("({rhs_s}) != 0"), guard);
                    if !Self::predicate_uses_loop_iter(&pred, loop_iters)
                        && seen.insert(pred.clone())
                    {
                        sites.push((pred, tag.to_string()));
                    }
                }
                self.collect_bound_expr(
                    a,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    b,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            ExprKind::Unary(_, a) => self.collect_bound_expr(
                a,
                vec_sizes,
                scalar_widths,
                const_params,
                loop_iters,
                guard,
                sites,
                seen,
            ),
            ExprKind::Ternary(c, t, f) => {
                self.collect_bound_expr(
                    c,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    t,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                self.collect_bound_expr(
                    f,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
            }
            ExprKind::MethodCall(base, _, args) => {
                self.collect_bound_expr(
                    base,
                    vec_sizes,
                    scalar_widths,
                    const_params,
                    loop_iters,
                    guard,
                    sites,
                    seen,
                );
                for a in args {
                    self.collect_bound_expr(
                        a,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
            }
            ExprKind::FunctionCall(_, args) => {
                for a in args {
                    self.collect_bound_expr(
                        a,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.collect_bound_expr(
                        p,
                        vec_sizes,
                        scalar_widths,
                        const_params,
                        loop_iters,
                        guard,
                        sites,
                        seen,
                    );
                }
            }
            ExprKind::FieldAccess(base, _) => self.collect_bound_expr(
                base,
                vec_sizes,
                scalar_widths,
                const_params,
                loop_iters,
                guard,
                sites,
                seen,
            ),
            ExprKind::BitSlice(base, _, _) => self.collect_bound_expr(
                base,
                vec_sizes,
                scalar_widths,
                const_params,
                loop_iters,
                guard,
                sites,
                seen,
            ),
            _ => {}
        }
    }

    /// Walk all seq blocks in the module and return a list of SV-string path
    /// conditions under which `reg_name` is written. For `if cond data <= ...`,
    /// returns `["cond"]`. For `if A data <= 1; else if B data <= 2;`, returns
    /// `["(A)", "(!(A) && (B))"]`. Conditions are AND-ed down the nesting; the
    /// caller OR-reduces them to get the full write condition.
    ///
    /// Used by the guard-contract SVA emitter to tightly track when a guarded
    /// reg has been written at least once.
    fn collect_write_conds(&self, m: &ModuleDecl, reg_name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for item in &m.body {
            if let ModuleBodyItem::RegBlock(rb) = item {
                for s in &rb.stmts {
                    self.walk_stmt_for_writes(s, reg_name, &[], &mut out);
                }
            }
        }
        out
    }

    /// Recursively walk a Stmt, appending the path-condition (stringified) to
    /// `out` whenever an assignment to `reg_name` is found.
    /// `path` is the stack of conditions (each already stringified) leading here.
    fn walk_stmt_for_writes(
        &self,
        s: &Stmt,
        reg_name: &str,
        path: &[String],
        out: &mut Vec<String>,
    ) {
        match s {
            Stmt::Assign(a) => {
                // Check if target root is reg_name
                let targets_reg = match &a.target.kind {
                    ExprKind::Ident(n) => n == reg_name,
                    ExprKind::Index(base, _)
                    | ExprKind::FieldAccess(base, _)
                    | ExprKind::BitSlice(base, _, _)
                    | ExprKind::PartSelect(base, _, _, _) => {
                        matches!(&base.kind, ExprKind::Ident(n) if n == reg_name)
                    }
                    _ => false,
                };
                if targets_reg {
                    // Path is the AND of all conditions leading here
                    let cond = if path.is_empty() {
                        "1'b1".to_string()
                    } else {
                        path.join(" && ")
                    };
                    out.push(cond);
                }
            }
            Stmt::IfElse(ie) => {
                let c_str = format!("({})", self.emit_expr_str(&ie.cond));
                let mut then_path: Vec<String> = path.to_vec();
                then_path.push(c_str.clone());
                for child in &ie.then_stmts {
                    self.walk_stmt_for_writes(child, reg_name, &then_path, out);
                }
                let mut else_path: Vec<String> = path.to_vec();
                else_path.push(format!("!{}", c_str));
                for child in &ie.else_stmts {
                    self.walk_stmt_for_writes(child, reg_name, &else_path, out);
                }
            }
            Stmt::Init(ib) => {
                for child in &ib.body {
                    self.walk_stmt_for_writes(child, reg_name, path, out);
                }
            }
            Stmt::For(fl) => {
                for child in &fl.body {
                    self.walk_stmt_for_writes(child, reg_name, path, out);
                }
            }
            // Match and Log: skip for v1 (match with pattern conditions is more complex)
            _ => {}
        }
    }

    fn emit_pattern(&self, pat: &Pattern) -> String {
        match pat {
            Pattern::Ident(id) => id.name.clone(),
            Pattern::EnumVariant(_, variant) => variant.name.to_uppercase(),
            Pattern::Literal(expr) => self.emit_expr_str(expr),
            Pattern::Wildcard => "default".to_string(),
        }
    }

    /// Return operator precedence for SV emission (higher = tighter binding).
    ///
    /// ARCH and SV disagree on the relative precedence of comparison operators
    /// (`==`, `!=`, `<`, `>`, `<=`, `>=`) vs bitwise operators (`&`, `^`, `|`):
    ///   - SV (IEEE 1800-2017):  `==`/relational bind TIGHTER than `&`/`^`/`|`
    ///   - ARCH:                 `&`/`^`/`|` bind TIGHTER than `==`/relational
    ///
    /// To guarantee correct SV regardless of which precedence the reader assumes,
    /// we collapse comparison and bitwise ops into a single precedence tier.
    /// This forces parentheses whenever they are mixed (e.g. `(a == b) & (c == d)`),
    /// which is always safe and improves readability.
    fn sv_binop_prec(op: &BinOp) -> u8 {
        match op {
            BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::MulWrap => 12,
            BinOp::Add | BinOp::Sub | BinOp::AddWrap | BinOp::SubWrap => 11,
            BinOp::Shl | BinOp::Shr => 10,
            // Collapsed tier: comparison and bitwise ops share the same level
            // so any mixing produces parentheses.
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => 7,
            BinOp::Eq | BinOp::Neq => 7,
            BinOp::BitAnd => 7,
            BinOp::BitXor => 7,
            BinOp::BitOr => 7,
            BinOp::And => 4,
            BinOp::Or => 3,
            BinOp::Implies | BinOp::ImpliesNext => 2,
        }
    }

    /// Precedence of the outermost operator in `expr`, or u8::MAX for atoms.
    fn expr_prec(expr: &Expr) -> u8 {
        match &expr.kind {
            ExprKind::Binary(op, _, _) => Self::sv_binop_prec(op),
            ExprKind::Unary(..) => 14,
            ExprKind::Ternary(..) => 2,
            _ => u8::MAX, // atoms — never need wrapping
        }
    }

    fn emit_expr_str(&self, expr: &Expr) -> String {
        self.emit_expr_prec(expr, 0)
    }

    /// Bit-width of a `TypeExpr` resolved through the symbol table for
    /// named struct/enum types. Returns `None` if any width sub-expression
    /// is non-literal (parametric) or the named type is unresolved.
    ///
    /// Mirrors `typecheck::TypeChecker::type_expr_width`. Kept private to
    /// codegen for now — promote to a shared util if a third caller appears.
    fn type_expr_width(&self, ty: &TypeExpr) -> Option<u32> {
        let eval = |e: &Expr| match &e.kind {
            ExprKind::Literal(LitKind::Dec(n)) | ExprKind::Literal(LitKind::Hex(n)) => {
                Some(*n as u32)
            }
            _ => None,
        };
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => eval(w),
            TypeExpr::Bool | TypeExpr::Bit | TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => Some(1),
            TypeExpr::FP32 => Some(32),
            TypeExpr::BF16 => Some(16),
            TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => Some(8),
            TypeExpr::FP4E2M1 => Some(4),
            TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => Some(6),
            TypeExpr::E8M0 | TypeExpr::UE4M3 => Some(8),
            TypeExpr::Vec(inner, size) => {
                let iw = self.type_expr_width(inner)?;
                let n = eval(size)?;
                Some(iw * n)
            }
            TypeExpr::ScaledVec(elem, size, scale) => {
                crate::fp_format::scaled_vec_width(elem, eval(size)?, scale)
            }
            TypeExpr::Named(ident) => match self.symbols.globals.get(&ident.name) {
                Some((Symbol::Struct(info), _)) => {
                    let mut total = 0u32;
                    for (_, field_ty) in &info.fields {
                        total = total.checked_add(self.type_expr_width(field_ty)?)?;
                    }
                    Some(total)
                }
                Some((Symbol::Enum(info), _)) => Some(enum_width(info.variants.len())),
                _ => None,
            },
        }
    }

    /// Emit a struct-field value sized to the field's declared width.
    ///
    /// Inside an SV positional concatenation `{a, b, c}`, IEEE 1800
    /// §11.4.12 requires every operand to be sized — bare unsized
    /// numeric literals (`0`, `42`) are illegal and Verilator rejects
    /// them with `WIDTHCONCAT`. Named identifiers, slices, casts, etc.
    /// already carry a declared width and pass through unchanged.
    fn emit_field_value_sized(&self, value: &Expr, field_ty: &TypeExpr) -> String {
        let w = match self.type_expr_width(field_ty) {
            Some(w) => w,
            None => return self.emit_expr_str(value),
        };
        match &value.kind {
            ExprKind::Literal(LitKind::Dec(n)) => format!("{w}'d{n}"),
            ExprKind::Literal(LitKind::Hex(n)) => format!("{w}'h{n:x}"),
            ExprKind::Literal(LitKind::Bin(n)) => format!("{w}'b{n:b}"),
            ExprKind::Bool(b) => format!("1'b{}", *b as u8),
            _ => self.emit_expr_str(value),
        }
    }

    /// Best-effort struct name for an expression. Walks a small set of
    /// expression shapes that typically produce a struct value in ARCH
    /// today (method calls returning structs, function calls, struct
    /// literals, struct-typed ports/regs/wires/lets). Returns None if
    /// the type isn't determinable at codegen time — caller falls back
    /// to emitting a `logic` wire.
    fn infer_expr_struct_name(&self, e: &Expr) -> Option<String> {
        // Struct literal: `'{field: value, ...}` carries the struct name.
        if let ExprKind::StructLiteral(name, _) = &e.kind {
            return Some(name.name.clone());
        }
        // Plain identifier: look up in the current module's symbol scope.
        if let ExprKind::Ident(n) = &e.kind {
            let scope = self.symbols.module_scopes.get(&self.current_construct)?;
            let (sym, _) = scope.get(n.as_str())?;
            let te_opt: Option<&TypeExpr> = match sym {
                Symbol::Port(p) => Some(&p.ty),
                Symbol::Reg(r) => Some(&r.ty),
                _ => None,
            };
            if let Some(TypeExpr::Named(struct_name)) = te_opt {
                return Some(struct_name.name.clone());
            }
            // Let bindings: scan the module body for the declared type.
            for item in &self.source.items {
                if let Item::Module(m) = item {
                    if m.name.name == self.current_construct {
                        for bi in &m.body {
                            if let ModuleBodyItem::LetBinding(lb) = bi {
                                if lb.name.name == *n {
                                    if let Some(TypeExpr::Named(sn)) = &lb.ty {
                                        return Some(sn.name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<TypeExpr> {
        for item in &self.source.items {
            if let Item::Struct(s) = item {
                if s.name.name == struct_name {
                    for f in &s.fields {
                        if f.name.name == field_name {
                            return Some(f.ty.clone());
                        }
                    }
                }
            }
            if let Item::Package(pkg) = item {
                for s in &pkg.structs {
                    if s.name.name == struct_name {
                        for f in &s.fields {
                            if f.name.name == field_name {
                                return Some(f.ty.clone());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Lower a Vec method call (any/all/count/contains/reduce_*) to a
    /// parallel-compare + reduction expression. Fully unrolled at codegen
    /// time because N is known.
    ///
    /// Predicate identifier substitution for `item` / `index` is applied
    /// via `self.ident_subst`, which is a reentrant context we push before
    /// emitting each iteration's expression and pop after.
    ///
    /// Safety: this is `&self`, but we need to temporarily mutate
    /// `ident_subst`. We cast away immutability in a narrowly-scoped
    /// block that restores the previous state before returning. The
    /// alternative (threading a mutable binding map through every
    /// `emit_expr_str` caller) would touch ~30 sites; this is localized.
    #[allow(clippy::ptr_arg)]
    fn emit_vec_method(&self, recv_b: &str, recv: &Expr, method: &Ident, args: &[Expr]) -> String {
        // Resolve N. The receiver is an Ident in v1; more complex
        // expressions are not lowered (falls through to placeholder).
        let n = match &recv.kind {
            ExprKind::Ident(n) => self.vec_sizes.get(n).copied(),
            _ => None,
        };
        let Some(n) = n else {
            // Size unknown → bail to the fallback shape; SV tools will
            // reject it, telling the user we couldn't unroll.
            return format!("{recv_b}.{}()", method.name);
        };
        let n_usize = n as usize;
        let idx_w = crate::width::index_width(n as u64);

        // Helper: emit an expression with `item` bound to recv[i] and
        // `index` bound to a sized literal. `ident_subst` is a field of
        // Codegen; we use interior-mutability-via-unsafe here because
        // emit_expr_str is `&self`. The Codegen type is `!Sync` and
        // emission is single-threaded, so this is safe.
        let emit_at = |i: u32| -> String {
            let this = self as *const Codegen as *mut Codegen;
            // SAFETY: single-threaded emission; no aliasing.
            unsafe {
                (*this)
                    .ident_subst
                    .insert("item".to_string(), format!("{recv_b}[{i}]"));
                (*this)
                    .ident_subst
                    .insert("index".to_string(), format!("{idx_w}'d{i}"));
            }
            let result = if let Some(pred) = args.first() {
                self.emit_expr_str(pred)
            } else {
                // contains / reduce_*: see caller below; we won't be called
                // without args from those paths.
                String::new()
            };
            unsafe {
                (*this).ident_subst.remove("item");
                (*this).ident_subst.remove("index");
            }
            result
        };

        match method.name.as_str() {
            "any" => {
                if n_usize == 0 {
                    return "1'b0".to_string();
                }
                (0..n).map(emit_at).collect::<Vec<_>>().join(" || ")
            }
            "all" => {
                if n_usize == 0 {
                    return "1'b1".to_string();
                }
                (0..n).map(emit_at).collect::<Vec<_>>().join(" && ")
            }
            "count" => {
                if n_usize == 0 {
                    return "0".to_string();
                }
                let w = crate::width::index_width((n + 1) as u64);
                // Sum of bool conversions. SV auto-widens `+` per 1800-2012 §11.6.
                let terms: Vec<String> = (0..n)
                    .map(|i| format!("{w}'({} ? 1 : 0)", emit_at(i)))
                    .collect();
                format!("({})", terms.join(" + "))
            }
            "contains" => {
                // `contains(x)` is `any(item == x)` — but the user supplies x,
                // not a predicate. Emit n equality comparisons against the
                // argument, OR'd.
                let Some(x_expr) = args.first() else {
                    return "1'b0".to_string();
                };
                let x = self.emit_expr_str(x_expr);
                if n_usize == 0 {
                    return "1'b0".to_string();
                }
                (0..n)
                    .map(|i| format!("({recv_b}[{i}] == {x})"))
                    .collect::<Vec<_>>()
                    .join(" || ")
            }
            "reduce_or" => {
                if n_usize == 0 {
                    return "0".to_string();
                }
                (0..n)
                    .map(|i| format!("{recv_b}[{i}]"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
            "reduce_and" => {
                if n_usize == 0 {
                    return "0".to_string();
                }
                (0..n)
                    .map(|i| format!("{recv_b}[{i}]"))
                    .collect::<Vec<_>>()
                    .join(" & ")
            }
            "reduce_xor" => {
                if n_usize == 0 {
                    return "0".to_string();
                }
                (0..n)
                    .map(|i| format!("{recv_b}[{i}]"))
                    .collect::<Vec<_>>()
                    .join(" ^ ")
            }
            "find_first" => {
                // Record the index width so a matching typedef is emitted
                // at the top of the generated SV file.
                self.find_first_widths.borrow_mut().insert(idx_w);
                if n_usize == 0 {
                    return format!("'{{found: 1'b0, index: {idx_w}'d0}}");
                }
                // Per-iteration hit expression: <pred with item=recv[i], index=i'd>.
                let hits: Vec<String> = (0..n).map(emit_at).collect();
                // found: OR of all hits.
                let found = hits.join(" || ");
                // index: priority-encoded first hit via nested ternary,
                // lowest-index-wins. Falls through to 0 when no hit.
                let mut index = format!("{idx_w}'d0");
                for i in (0..n).rev() {
                    let hit = &hits[i as usize];
                    index = format!("({hit}) ? {idx_w}'d{i} : {index}");
                }
                format!("'{{found: ({found}), index: ({index})}}")
            }
            _ => format!("{recv_b}.{}()", method.name),
        }
    }

    /// Evaluate a compile-time constant expression (Vec size, etc.) to a u32.
    /// Handles literals, const-param references, and simple binary ops.
    /// Returns None if the expression can't be reduced — caller then treats
    /// the receiver as size-unknown and skips Vec method lowering.
    fn eval_const_u32(&self, e: &Expr, params: &[ParamDecl]) -> Option<u32> {
        self.eval_const_u32_depth(e, params, 0)
    }

    /// Depth-guarded core of `eval_const_u32`. A self-referential param default
    /// (e.g. a child param `NUM => NUM` produced by an inst override that shares
    /// a name with a parent param) would otherwise recurse until the stack
    /// overflows. Bail to None past a generous depth — no legitimate
    /// compile-time size expression nests anywhere near this far.
    fn eval_const_u32_depth(&self, e: &Expr, params: &[ParamDecl], depth: u32) -> Option<u32> {
        const MAX_DEPTH: u32 = 64;
        if depth > MAX_DEPTH {
            return None;
        }
        match &e.kind {
            ExprKind::Literal(LitKind::Dec(v)) => Some(*v as u32),
            ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as u32),
            ExprKind::Ident(n) => {
                let p = params.iter().find(|p| p.name.name == *n)?;
                match &p.kind {
                    ParamKind::Const | ParamKind::WidthConst(..) => {}
                    _ => return None,
                }
                let d = p.default.as_ref()?;
                self.eval_const_u32_depth(d, params, depth + 1)
            }
            ExprKind::Binary(op, l, r) => {
                let lv = self.eval_const_u32_depth(l, params, depth + 1)?;
                let rv = self.eval_const_u32_depth(r, params, depth + 1)?;
                Some(match op {
                    BinOp::Add => lv + rv,
                    BinOp::Sub => lv.saturating_sub(rv),
                    BinOp::Mul => lv * rv,
                    BinOp::Div if rv != 0 => lv / rv,
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    /// Infer the SV bit-width of an expression as a string constant expression.
    /// Used to emit the width cast for wrapping arithmetic operators (+%, -%, *%).
    ///
    /// Ordinary (module/fsm) emission scope: expressions render through
    /// `emit_expr_str` and bare identifiers resolve through `module_scopes`.
    /// See `infer_sv_width_str_in` for the pipeline-scoped variant.
    fn infer_sv_width_str(&self, expr: &Expr) -> String {
        self.infer_sv_width_str_in(expr, &|e| self.emit_expr_str(e), &|_| None)
    }

    /// `infer_sv_width_str` parameterized on the *emission scope* the width
    /// will be used in (arch#845).
    ///
    /// The pipeline emitters rewrite identifiers as they go — a stage reg
    /// `cap` renders as `s0_cap`, a cross-stage `S0.cap` read as `s0_cap` —
    /// and pipelines have no `module_scopes` entry at all (resolve.rs builds
    /// those for modules and fsms only). So both halves of this function are
    /// wrong for a pipeline under the default scope: the `Ident` arm can't
    /// resolve anything, and every `$bits(...)` fallback would name an
    /// identifier that does not exist in the emitted SV.
    ///
    /// - `emit` renders a sub-expression as SV text in the caller's scope.
    ///   It is used *only* inside `$bits(...)` fallbacks; width
    ///   sub-expressions of a type (`UInt<W>`'s `W`, `.trunc<N>()`'s `N`)
    ///   stay on `emit_expr_str` — those are constant/param expressions,
    ///   never stage-rewritten signal references.
    /// - `signal_width` optionally resolves a *plain signal reference* to
    ///   its declared width, consulted just before each `$bits(...)`
    ///   fallback. `|_| None` keeps the fallback (the module/fsm scope
    ///   already resolves identifiers inline in the `Ident` arm).
    fn infer_sv_width_str_in(
        &self,
        expr: &Expr,
        emit: &dyn Fn(&Expr) -> String,
        signal_width: &dyn Fn(&Expr) -> Option<String>,
    ) -> String {
        // Declared width if the caller's scope can resolve this reference,
        // else a `$bits(...)` of the expression as *that scope* renders it.
        let bits = |e: &Expr| signal_width(e).unwrap_or_else(|| format!("$bits({})", emit(e)));
        match &expr.kind {
            ExprKind::Ident(name) => {
                // Inside a `function` body (`fn_local_types` is non-empty
                // only there), arguments and `let` locals are not in
                // `module_scopes` — resolve them here first. Falling
                // through to `$bits(<name>)` is not merely verbose in that
                // scope, it is *wrong*: Icarus 12.0 computes a bogus width
                // for `$bits(<function argument>)` used in a declaration,
                // so a hoist temp sized that way reads back all-X
                // (arch#846).
                if let Some(te) = self.fn_local_types.get(name.as_str()) {
                    match te {
                        TypeExpr::UInt(w) | TypeExpr::SInt(w) => return self.emit_expr_str(w),
                        TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                        _ => {}
                    }
                }
                if let Some(scope) = self.symbols.module_scopes.get(&self.current_construct) {
                    if let Some((sym, _)) = scope.get(name.as_str()) {
                        let te_opt: Option<&TypeExpr> = match sym {
                            Symbol::Port(p) => Some(&p.ty),
                            Symbol::Reg(r) => Some(&r.ty),
                            _ => None,
                        };
                        if let Some(te) = te_opt {
                            match te {
                                TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
                                    return self.emit_expr_str(w)
                                }
                                TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                                _ => {}
                            }
                        }
                        // For Let bindings, look up in AST
                        if matches!(sym, Symbol::Let(_)) {
                            for item in &self.source.items {
                                if let Item::Module(m) = item {
                                    if m.name.name == self.current_construct {
                                        for bi in &m.body {
                                            if let ModuleBodyItem::LetBinding(lb) = bi {
                                                if lb.name.name == *name {
                                                    if let Some(ty) = &lb.ty {
                                                        match ty {
                                                            TypeExpr::UInt(w)
                                                            | TypeExpr::SInt(w) => {
                                                                return self.emit_expr_str(w)
                                                            }
                                                            TypeExpr::Bool | TypeExpr::Bit => {
                                                                return "1".to_string()
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                bits(expr)
            }
            // Unsized literals: compute minimum bit width from value (never 0 bits)
            ExprKind::Literal(LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v)) => {
                let bits = if *v == 0 {
                    1
                } else {
                    (64 - v.leading_zeros()) as u32
                };
                bits.to_string()
            }
            ExprKind::Literal(LitKind::Sized(w, _)) => w.to_string(),
            ExprKind::Literal(LitKind::ParamSized(name, _)) => name.clone(),
            ExprKind::MethodCall(_, method, args)
                if matches!(method.name.as_str(), "trunc" | "zext" | "sext" | "resize") =>
            {
                args.first()
                    .map(|w| self.emit_expr_str(w))
                    .unwrap_or_else(|| bits(expr))
            }
            // `.reverse()` bit-reverses within fixed-size chunks — same
            // total width as the receiver. Recurse into the receiver
            // rather than falling to `$bits({<<N{e}})`: Icarus does not
            // reliably accept a streaming-concat operator as the argument
            // to `$bits(...)` (arch#650, discovered indexing a `.reverse()`
            // result — same root cause class as the `$bits($signed(...))`
            // case above, different SV shape).
            ExprKind::MethodCall(recv, method, _) if method.name == "reverse" => {
                self.infer_sv_width_str_in(recv, emit, signal_width)
            }
            ExprKind::Cast(_, ty) => self.type_expr_data_width(ty).unwrap_or_else(|| bits(expr)),
            // `signed(e)`/`unsigned(e)` are pure bit reinterpretations
            // (never change width) — recurse into `e` rather than falling
            // to `$bits($signed(e))`/`$bits($unsigned(e))`. Icarus does not
            // reliably accept `$bits(...)` wrapping a nested system-function
            // call (arch#650); this also gives a tighter (often literal)
            // width than the runtime-computed fallback whenever `e`'s width
            // is statically known.
            ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
                self.infer_sv_width_str_in(inner, emit, signal_width)
            }
            // Vec element access: width comes from the inner element type
            ExprKind::Index(base, _) => {
                if let ExprKind::Ident(name) = &base.kind {
                    // Search current scope, then fallback to thread submodule scope
                    // (thread-driven regs are moved to _ModuleName_threads after lowering)
                    let fallback = format!("_{}_threads", self.current_construct);
                    let scopes = [self.current_construct.as_str(), fallback.as_str()];
                    'outer: for scope_key in &scopes {
                        if let Some(scope) = self.symbols.module_scopes.get(*scope_key) {
                            if let Some((sym, _)) = scope.get(name.as_str()) {
                                let te_opt: Option<&TypeExpr> = match sym {
                                    Symbol::Port(p) => Some(&p.ty),
                                    Symbol::Reg(r) => Some(&r.ty),
                                    _ => None,
                                };
                                if let Some(TypeExpr::Vec(inner, _)) = te_opt {
                                    match inner.as_ref() {
                                        TypeExpr::UInt(w) | TypeExpr::SInt(w) => {
                                            return self.emit_expr_str(w)
                                        }
                                        TypeExpr::Bool | TypeExpr::Bit => return "1".to_string(),
                                        _ => {}
                                    }
                                }
                                break 'outer;
                            }
                        }
                    }
                }
                bits(expr)
            }
            // Chained wrapping ops: result width = max(lhs width, rhs width)
            ExprKind::Binary(BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap, lhs, rhs) => {
                let lw = self.infer_sv_width_str_in(lhs, emit, signal_width);
                let rw = self.infer_sv_width_str_in(rhs, emit, signal_width);
                if lw == rw {
                    lw
                } else {
                    format!("({lw} > {rw} ? {lw} : {rw})")
                }
            }
            // Widening arithmetic. The ARCH type system widens `a + b` / `a - b`
            // to `max(w_a, w_b) + 1` and `a * b` to `w_a + w_b` (typecheck.rs,
            // `binop_result_ty`), matching IEEE 1800 §11.6 plus the carry/product
            // bit. SV's *self-determined* width — what `$bits(a + b)` reports —
            // does NOT include those bits, so the `_ => $bits(...)` fallback
            // under-sizes a hoist temp. A slice reaching the widened high bit
            // then drops it: iverilog reads X, Verilator flags the select as
            // out-of-range — a silent build-vs-sim divergence for code that
            // `arch check` and `arch sim` both accept and compute correctly.
            // Only reachable since #813 P1 (#875) let arithmetic bases be
            // sliced at all. Non-widening ops (Div/Mod → lhs, bitwise/shift →
            // max/lhs) already agree with `$bits`, so they keep the fallback.
            ExprKind::Binary(BinOp::Add | BinOp::Sub, lhs, rhs) => {
                let lw = self.infer_sv_width_str_in(lhs, emit, signal_width);
                let rw = self.infer_sv_width_str_in(rhs, emit, signal_width);
                match (lw.parse::<u64>().ok(), rw.parse::<u64>().ok()) {
                    (Some(l), Some(r)) => (l.max(r) + 1).to_string(),
                    _ => {
                        let m = if lw == rw {
                            lw
                        } else {
                            format!("({lw} > {rw} ? {lw} : {rw})")
                        };
                        format!("{} + 1", Self::paren_width(&m))
                    }
                }
            }
            ExprKind::Binary(BinOp::Mul, lhs, rhs) => {
                let lw = self.infer_sv_width_str_in(lhs, emit, signal_width);
                let rw = self.infer_sv_width_str_in(rhs, emit, signal_width);
                match (lw.parse::<u64>().ok(), rw.parse::<u64>().ok()) {
                    (Some(l), Some(r)) => (l + r).to_string(),
                    _ => format!("{} + {}", Self::paren_width(&lw), Self::paren_width(&rw)),
                }
            }
            // Concat/Repeat width for the arch#807 slice-base hoist temp:
            // sum of part widths / count*value-width. Recurse into parts
            // rather than falling to `$bits({...})` on the whole
            // expression — Icarus does not reliably accept `$bits(...)`
            // wrapping a concatenation/replication argument (same class of
            // gap as the `$bits($signed(...))` nesting issue arch#650
            // documents elsewhere in this function).
            ExprKind::Concat(parts) => {
                let widths: Vec<String> = parts
                    .iter()
                    .map(|p| self.infer_sv_width_str_in(p, emit, signal_width))
                    .collect();
                match widths
                    .iter()
                    .map(|w| w.parse::<u64>().ok())
                    .sum::<Option<u64>>()
                {
                    Some(total) => total.to_string(),
                    None => widths.join(" + "),
                }
            }
            ExprKind::Repeat(count, value) => {
                let vw = self.infer_sv_width_str_in(value, emit, signal_width);
                match (literal_expr_u64(count), vw.parse::<u64>().ok()) {
                    (Some(n), Some(w)) => (n * w).to_string(),
                    (Some(n), None) => format!("{n} * {}", Self::paren_width(&vw)),
                    (None, _) => {
                        let c = self.emit_expr_str(count);
                        format!("({c}) * {}", Self::paren_width(&vw))
                    }
                }
            }
            _ => bits(expr),
        }
    }

    /// Wrap a width expression in parens if it contains operators,
    /// so that `W'(expr)` SV cast syntax parses correctly even when W is e.g. `DATA_WIDTH + 1`.
    fn paren_width(w: &str) -> String {
        if w.contains('+') || w.contains('-') || w.contains('*') || w.contains('/') {
            format!("({w})")
        } else {
            w.to_string()
        }
    }

    /// Peel `signed(...)` / `unsigned(...)` / `(... as T)` wrappers, returning
    /// the innermost non-cast expression. All three are pure bit
    /// reinterpretations in ARCH: `Signed`/`Unsigned` never change width, and
    /// `Cast` is typecheck-rejected when source and target widths are both
    /// known and differ (see `TypeChecker::resolve_expr_type`'s `Cast` arm).
    /// So the innermost expression carries the identical bit pattern as the
    /// wrapped form — callers that only need a single bit (`Index`'s base)
    /// can safely use it in place of the cast. Used by arch#650's
    /// indexed-cast portability fix; see `ExprKind::Index`.
    fn unwrap_reinterpret_cast(mut e: &Expr) -> &Expr {
        loop {
            match &e.kind {
                ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => e = inner,
                ExprKind::Cast(inner, _) => e = inner,
                _ => return e,
            }
        }
    }

    /// True when `base` is one of the SV forms that compose directly with a
    /// trailing single-bit `[i]` index on Icarus. Starts from the same set
    /// typecheck's `is_portable_bit_slice_base` allows as a `BitSlice`/
    /// `PartSelect` base (spec §3.2.1: identifier, literal, indexed access,
    /// field access, concat, replication, function/method-call result) —
    /// reused here because `Index` (`expr[i]`) also serves ordinary `Vec`
    /// element access (portable for any base), so typecheck does not gate
    /// it the way it gates `BitSlice`/`PartSelect`. But the two forms are
    /// *not* portable-equivalent on Icarus: empirically (Icarus 12.0),
    /// `{a, b}[i]`, `{2{a}}[i]`, and `{<<1{a}}[i]` are all rejected the same
    /// way an indexed cast is, even though the corresponding `[hi:lo]`
    /// range form is accepted bare (arch#653/#656/#659) — so `Concat`/
    /// `Repeat` are *not* atomic here despite being portable `BitSlice`
    /// bases. (This file's `BitSlice` arm is unchanged — that shipped,
    /// tested behavior is out of this fix's scope; see arch#650's PR body
    /// for the follow-up filed to track it.)
    /// The only bases SystemVerilog lets a `[hi:lo]` bit-slice or
    /// `[start +: w]` part-select apply to directly. Everything else is
    /// bound to a named temp by `hoist_slice_base` (arch#813 P1).
    ///
    /// This replaced typecheck's `is_portable_bit_slice_base` allowlist,
    /// which named the base kinds `arch check` would *accept* and had been
    /// wrong in both directions: too permissive for `Concat`/`Repeat`
    /// (arch#807), `FunctionCall`/`MethodCall` (arch#810) and `Literal`
    /// (`8'hff[i +: 2]`, rejected by both frontends), and too restrictive
    /// for `BitSlice`/`PartSelect`/`Bool`/`EnumVariant`, which arch#653
    /// turned into a permanent user-visible language restriction to work
    /// around a codegen limitation. Stating the rule the other way round —
    /// what SV can select from, everything else hoisted — makes a new
    /// `ExprKind` safe by default instead of silently non-portable until
    /// someone runs the right simulator.
    ///
    /// Deliberately *not* shared with `is_atomic_index_base`: the
    /// single-bit `Index` form has a different accepted set (a
    /// `FunctionCall` result can be indexed but not sliced) and its own
    /// history. Keeping them separate keeps each honest about what was
    /// actually measured.
    fn is_bare_selectable_slice_base(base: &Expr) -> bool {
        matches!(
            base.kind,
            ExprKind::Ident(_)
                | ExprKind::SynthIdent(_, _)
                | ExprKind::Index(_, _)
                | ExprKind::FieldAccess(_, _)
        )
    }

    fn is_atomic_index_base(base: &Expr) -> bool {
        match &base.kind {
            ExprKind::Ident(_)
            | ExprKind::SynthIdent(_, _)
            | ExprKind::Literal(_)
            | ExprKind::Index(_, _)
            | ExprKind::FieldAccess(_, _)
            | ExprKind::FunctionCall(_, _) => true,
            // `trunc`/`zext`/`resize` lower to an SV size-cast (`N'(...)`);
            // `sext`/`reverse` lower to a replication/concat/streaming-
            // concat (`{...}`). Both shapes are rejected by Icarus when
            // directly indexed (`N'(x)[i]`, `{...}[i]`) — the same
            // "indexed cast/conversion" and brace-index issues above,
            // just reached via a method call instead of a bare literal
            // concat/repeat. Everything else (any/all/count/contains/
            // reduce_*/find_first, to_fp32/to_bf16/to_fp8*/to_uint/
            // to_sint, or a generic pass-through `.method()`) keeps the
            // prior atomic classification — those don't produce a `{...}`
            // or `N'(...)` wrapper around the receiver.
            ExprKind::MethodCall(_, method, _) => !matches!(
                method.name.as_str(),
                "trunc" | "zext" | "resize" | "sext" | "reverse"
            ),
            _ => false,
        }
    }

    /// True when `.sext<N>()`'s replicand (`base`, already run through
    /// `unwrap_reinterpret_cast`) is safe to reference bare — twice, once
    /// indexed at its own top bit (`base[sw-1]`) and once whole — in the
    /// hand-emitted `{{(w-sw){base[sw-1]}}, base}` expansion (arch#827
    /// B1/B2). `sext`'s emitter builds that string directly rather than
    /// going through `hoist_slice_base`/the `ExprKind::Index` codegen arm,
    /// so it needs its own atomicity check rather than inheriting theirs
    /// automatically — see below for why the two existing checks
    /// (`is_bare_selectable_slice_base`, `is_atomic_index_base`) are each
    /// close but not quite right here.
    ///
    /// `Concat`/`Repeat`/`FunctionCall`/`MethodCall` bases are never atomic
    /// — same reasoning `hoist_slice_base` documents for `BitSlice`/
    /// `PartSelect`: none of them compose with a further `[...]` select on
    /// either frontend.
    ///
    /// A non-constant-indexed `Index` (`din[sel]`) is *also* not atomic
    /// here, even though both `is_bare_selectable_slice_base` and
    /// `is_atomic_index_base` classify bare `Index` as safe. Verified
    /// against raw hand-written SV, independent of any ARCH codegen path:
    /// `din[sel]` (a dynamic/indexed select into a packed multi-dim array,
    /// which is how a `Vec` element read with a runtime index lowers)
    /// does not compose with a further select layered on top — Icarus
    /// 12.0 rejects `din[sel][7]` and `din[sel][7:4]` identically
    /// ("reference... not allowed in a constant expression"), and binding
    /// `din[sel]` to a plain temp first (`logic [7:0] t = din[sel]; ...
    /// t[7] ...`) fixes both. A *constant*-indexed `Index` (`din[2]`) is
    /// fine bare — it folds to a plain element reference.
    ///
    /// This predicate is deliberately scoped to `sext`'s own hand-rolled
    /// emission rather than folded into `is_bare_selectable_slice_base`/
    /// `is_atomic_index_base`: those two are also reached from the bare
    /// `ExprKind::Index` codegen arm and `hoist_slice_base` respectively,
    /// used far beyond synthesized sext/zext, and widening them to also
    /// catch a non-const-indexed `Index` base is a separate, wider-blast-
    /// radius fix — `din[sel][7]`/`din[sel][7:4]` written directly by a
    /// user hits the identical bug today, tracked as a follow-up rather
    /// than folded into this one.
    ///
    /// `ExprKind::LatencyAt(inner, n)` (`pipe_reg` `@N` read, e.g.
    /// `x_pipe@1`) is atomic when `inner` is itself a plain identifier:
    /// codegen's own `LatencyAt` arm (below) renders that shape to a bare
    /// name — the pipe_reg's source, its final flop, or a synthesized
    /// `{name}_stg{n}` — never anything requiring a select. Found via a
    /// real crash: `tests/cvdp/iir_filter.arch`'s `x_pipe@1.sext<48>()`
    /// has `LatencyAt` fall through this predicate's `_ => false` arm,
    /// hoisting it into `logic [$bits(x_pipe_stg1)-1:0] arch_idx_base_1;`
    /// — a `$bits(<plain identifier>)` *declaration* width, which
    /// segfaults Icarus 12.0 outright (reproduced in isolation with just
    /// `logic [$bits(x)-1:0] t; assign t = x;`, independent of sext or
    /// pipe_reg — a `$bits()`-in-declaration-width crash, not a `sext`
    /// bug). The pre-fix code never hit this because it never hoisted
    /// `LatencyAt` at all; avoiding the hoist here avoids the crash the
    /// same way. `n` doesn't need checking — it's already a parsed `u32`,
    /// not a runtime expression, so there is no non-constant case.
    fn is_atomic_sext_receiver(base: &Expr) -> bool {
        match &base.kind {
            ExprKind::Ident(_) | ExprKind::SynthIdent(_, _) | ExprKind::FieldAccess(_, _) => true,
            ExprKind::Index(_, idx) => literal_expr_u64(idx).is_some(),
            ExprKind::LatencyAt(inner, _) => {
                matches!(inner.kind, ExprKind::Ident(_) | ExprKind::SynthIdent(_, _))
            }
            _ => false,
        }
    }

    /// True if `e` contains a bare `Ident` whose name is in `names`.
    /// Conservative and shallow-recursive (walks every expression kind that
    /// can appear inside an arithmetic/logical sub-expression) — used only to
    /// decide whether an `Index`-hoist temp (module scope) would reference an
    /// out-of-scope runtime `for`-loop variable; false positives just mean a
    /// missed portability fix (falls back to prior behavior), never a
    /// miscompile.
    fn expr_references_any(e: &Expr, names: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            ExprKind::Ident(n) => names.contains(n),
            ExprKind::Binary(_, a, b) => {
                Self::expr_references_any(a, names) || Self::expr_references_any(b, names)
            }
            ExprKind::Unary(_, a) => Self::expr_references_any(a, names),
            ExprKind::Ternary(c, t, f) => {
                Self::expr_references_any(c, names)
                    || Self::expr_references_any(t, names)
                    || Self::expr_references_any(f, names)
            }
            ExprKind::Index(base, idx) => {
                Self::expr_references_any(base, names) || Self::expr_references_any(idx, names)
            }
            ExprKind::BitSlice(base, hi, lo) => {
                Self::expr_references_any(base, names)
                    || Self::expr_references_any(hi, names)
                    || Self::expr_references_any(lo, names)
            }
            ExprKind::PartSelect(base, start, width, _) => {
                Self::expr_references_any(base, names)
                    || Self::expr_references_any(start, names)
                    || Self::expr_references_any(width, names)
            }
            ExprKind::FieldAccess(base, _) => Self::expr_references_any(base, names),
            ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
                Self::expr_references_any(inner, names)
            }
            ExprKind::Cast(inner, _) => Self::expr_references_any(inner, names),
            ExprKind::MethodCall(base, _, args) => {
                Self::expr_references_any(base, names)
                    || args.iter().any(|a| Self::expr_references_any(a, names))
            }
            ExprKind::FunctionCall(_, args) => {
                args.iter().any(|a| Self::expr_references_any(a, names))
            }
            ExprKind::Concat(parts) => parts.iter().any(|p| Self::expr_references_any(p, names)),
            ExprKind::Repeat(count, value) => {
                Self::expr_references_any(count, names) || Self::expr_references_any(value, names)
            }
            _ => false,
        }
    }

    /// Fresh id for an `Index`-hoist temp name (`arch_idx_base_<n>`, see
    /// `index_hoist_temps`). Monotonic for the whole file — uniqueness
    /// within one module is all that's required, and a file-wide counter is
    /// simplest and rules out any cross-module collision.
    fn next_index_hoist_id(&self) -> u32 {
        let id = self.index_hoist_counter.get();
        self.index_hoist_counter.set(id + 1);
        id
    }

    /// Queue one hoist temp for the next `line()` to place (see
    /// `index_hoist_temps` / `HoistScope`). Pushed in generation order,
    /// which is also dependency order: a nested hoist is created while the
    /// outer hoist's RHS is being emitted, so it lands ahead of it.
    ///
    /// `in_loop` is `true` when `rhs` references a live runtime `for`-loop
    /// iterator — see `HoistTemp::in_loop` (arch#861). There used to be a
    /// `push_hoist_temp` convenience wrapper that hardcoded `in_loop:
    /// false`; arch#861 retrofitted every existing hoist site (the
    /// `Index` arm, `hoist_slice_base_in`) to compute a real `in_loop`
    /// instead, and every hoist site added since (this one included) needs
    /// the same check, so nothing has called the always-false wrapper in
    /// a long time — removed as dead code rather than kept around as an
    /// easy-to-reach-for-but-wrong shortcut for a future hoist site.
    fn push_hoist_temp_in_loop(&self, width: String, name: String, rhs: String, in_loop: bool) {
        self.index_hoist_temps.borrow_mut().push(HoistTemp {
            width,
            name,
            rhs,
            in_loop,
        });
    }

    /// Does `expr` reference a `for`-loop iterator whose SV loop body is
    /// currently being emitted? Such a base cannot be bound by a
    /// module-scope continuous `assign` (the iterator does not exist
    /// there), so its temp is flagged `in_loop` and the assignment stays
    /// inside the loop (arch#861).
    fn base_references_live_loop_var(&self, base: &Expr) -> bool {
        !self.runtime_for_loop_vars.is_empty()
            && Self::expr_references_any(base, &self.runtime_for_loop_vars)
    }

    /// Portability hoist for a non-atomic `BitSlice`/`PartSelect` base
    /// (`{a,c}[hi:lo]`, `{N{a}}[start +: w]`, `f(x)[hi:lo]`,
    /// `x.trunc<N>()[hi:lo]`, ...). All of these base kinds are classified
    /// portable by typecheck's `is_portable_bit_slice_base` (spec §3.2.1)
    /// and were emitted bare — but no SV frontend actually accepts the
    /// bare form for any of them:
    ///
    /// - **`Concat`/`Repeat`** (arch#807): Icarus 12.0 rejects
    ///   `{a,c}[hi:lo]`/`{N{a}}[hi:lo]` outright, bare *or* parenthesized;
    ///   Verilator accepts. PR #656's own repro (`{2{a}}[3:0]`) fails
    ///   `iverilog -g2012` with a syntax error. The spec text claiming
    ///   iverilog acceptance was never verified against a real iverilog
    ///   binary when PR #656 shipped it — only Verilator 5.048 was tested.
    /// - **`FunctionCall`** (arch#810): Icarus 12.0 rejects
    ///   `f(x)[hi:lo]`; Verilator accepts.
    /// - **`MethodCall`** (arch#810): rejected by *both* frontends. The
    ///   width-cast methods lower to an SV size cast — `x.trunc<8>()`
    ///   emits `8'(x)`, `x.zext<16>()` emits `16'($unsigned(x))` — and
    ///   neither Icarus 12.0 nor Verilator 5.048 accepts a select applied
    ///   to a size cast (`%Error: syntax error, unexpected '['`). So this
    ///   arm is not a portability nicety: without the hoist, `arch build`
    ///   emits SystemVerilog that the project's primary simulator refuses
    ///   to compile. `.reverse<C>()` is covered by the same arm — since
    ///   PR #834 lowers it to a chunked ordinary concatenation, its result
    ///   used as a slice base reintroduced exactly the bare-`{...}[hi:lo]`
    ///   shape arch#807 fixed, because this guard keys on the *ARCH*
    ///   `ExprKind`, not on the emitted SV shape.
    ///
    /// Fix: hoist the base to a named module-scope temp, the same "bind to
    /// a named `let`" strategy the spec already recommends at the source
    /// level for non-portable bases, and the same mechanism arch#650 uses
    /// for a non-atomic single-bit `Index` base. The hoisted form was
    /// verified to compile clean on both Icarus 12.0 and Verilator 5.048
    /// for all three shapes.
    ///
    /// Returns `None` (caller falls back to its prior bare-emission
    /// behavior) for a base that *is* directly selectable
    /// (`Ident`/`SynthIdent`/`Index`/`FieldAccess`, plus `Literal`, which
    /// the `BitSlice` emitter const-folds before reaching here), or when
    /// the base references a live runtime `for`-loop variable — a
    /// module-scope temp can't see a loop-local index, and Icarus doesn't
    /// support `logic` declarations inside `always_*` blocks either (same
    /// reasoning as the `Index` hoist's loop-var skip).
    fn hoist_slice_base(&self, base: &Expr) -> Option<String> {
        self.hoist_slice_base_in(base, &|e| self.emit_expr_str(e), &|_| None)
    }

    /// `hoist_slice_base` parameterized on the emission scope (arch#845).
    ///
    /// The `pipeline` construct has its own expression emitters, which
    /// rewrite identifiers as they render (a stage reg `cap` becomes
    /// `s0_cap`, a cross-stage `S0.cap` read becomes `s0_cap`). Reusing
    /// `hoist_slice_base` there would emit a temp whose RHS — and whose
    /// `$bits(...)` width fallback — named the *un-prefixed* AST
    /// identifiers, i.e. signals that don't exist in the emitted SV. So the
    /// pipeline emitters pass their own `emit`/`signal_width` pair instead
    /// of getting a second copy of the hoist. See `infer_sv_width_str_in`
    /// for what each callback is responsible for.
    ///
    /// Everything else — which base kinds are claimed, the runtime
    /// `for`-loop-variable bail, the shared `arch_idx_base_<n>` counter and
    /// the `index_hoist_temps` queue — is identical for every scope, so it
    /// lives here and only here. Note that *where* the queued temp is
    /// finally written is decided later, by `line()`'s `HoistScope`
    /// (arch#846), keyed on the SV block being emitted rather than on which
    /// emitter queued it — so a pipeline's `always_ff`/`always_comb` gets
    /// the module-scope splice with no pipeline-specific machinery.
    fn hoist_slice_base_in(
        &self,
        base: &Expr,
        emit: &dyn Fn(&Expr) -> String,
        signal_width: &dyn Fn(&Expr) -> Option<String>,
    ) -> Option<String> {
        if Self::is_bare_selectable_slice_base(base) {
            return None;
        }
        let in_loop = self.base_references_live_loop_var(base);
        let tmp = format!("arch_idx_base_{}", self.next_index_hoist_id());
        let w = Self::paren_width(&self.infer_sv_width_str_in(base, emit, signal_width));
        let rhs = emit(base);
        self.push_hoist_temp_in_loop(w, tmp.clone(), rhs, in_loop);
        Some(tmp)
    }

    /// Params of the construct currently being emitted (module / fsm /
    /// pipeline), for `eval_const_u32` width resolution. `None` when the
    /// current construct can't be found by name — callers fall back
    /// conservatively.
    fn current_construct_params(&self) -> Option<&[ParamDecl]> {
        self.source.items.iter().find_map(|item| match item {
            Item::Module(m) if m.name.name == self.current_construct => Some(m.params.as_slice()),
            Item::Fsm(f) if f.name.name == self.current_construct => Some(f.params.as_slice()),
            Item::Pipeline(p) if p.name.name == self.current_construct => Some(p.params.as_slice()),
            _ => None,
        })
    }

    /// Numeric bit-width of a `TypeExpr`, const-evaluating the width
    /// sub-expression through `params` (unlike `type_expr_width`, which
    /// only folds literals). Covers just the shapes a `.reverse()`
    /// receiver can legally have (typecheck restricts it to
    /// UInt/SInt/Bool) — everything else is `None`.
    fn type_expr_width_const(&self, ty: &TypeExpr, params: &[ParamDecl]) -> Option<u32> {
        match ty {
            TypeExpr::UInt(w) | TypeExpr::SInt(w) => self.eval_const_u32(w, params),
            TypeExpr::Bool | TypeExpr::Bit => Some(1),
            _ => None,
        }
    }

    /// Declared `TypeExpr` of a plain identifier in the current
    /// module/fsm scope (ports, regs, typed lets, wires). Pipelines have
    /// no `module_scopes` entry (resolve.rs builds those for modules and
    /// fsms only) — the pipeline emitters resolve through the pipeline's
    /// own AST via `pipeline_ident_type` instead.
    fn construct_ident_type(&self, name: &str) -> Option<TypeExpr> {
        let scope = self.symbols.module_scopes.get(&self.current_construct)?;
        match scope.get(name)? {
            (Symbol::Port(p), _) => Some(p.ty.clone()),
            (Symbol::Reg(r), _) => Some(r.ty.clone()),
            (Symbol::Let(_), _) => self.source.items.iter().find_map(|item| match item {
                Item::Module(m) if m.name.name == self.current_construct => {
                    m.body.iter().find_map(|bi| match bi {
                        ModuleBodyItem::LetBinding(l) if l.name.name == name => l.ty.clone(),
                        ModuleBodyItem::WireDecl(wd) if wd.name.name == name => Some(wd.ty.clone()),
                        _ => None,
                    })
                }
                Item::Fsm(f) if f.name.name == self.current_construct => f
                    .lets
                    .iter()
                    .find(|l| l.name.name == name)
                    .and_then(|l| l.ty.clone())
                    .or_else(|| {
                        f.wires
                            .iter()
                            .find(|w| w.name.name == name)
                            .map(|w| w.ty.clone())
                    }),
                _ => None,
            }),
            _ => None,
        }
    }

    /// Numeric bit-width of a `.reverse()` receiver — `None` when it
    /// can't be proven here (the caller then falls back to the
    /// streaming-concat emission, i.e. the pre-arch#808 Verilator-only
    /// behavior). `lookup_ident` abstracts over module/fsm scope lookup
    /// vs the pipeline emitters' stage-aware lookup.
    fn reverse_recv_width_u32(
        &self,
        recv: &Expr,
        params: &[ParamDecl],
        lookup_ident: &dyn Fn(&str) -> Option<TypeExpr>,
    ) -> Option<u32> {
        match &recv.kind {
            ExprKind::Ident(n) => self.type_expr_width_const(&lookup_ident(n)?, params),
            ExprKind::SynthIdent(_, ty) => self.type_expr_width_const(ty, params),
            ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => {
                self.reverse_recv_width_u32(inner, params, lookup_ident)
            }
            ExprKind::Cast(_, ty) => self.type_expr_width_const(ty, params),
            ExprKind::Literal(LitKind::Sized(w, _)) => Some(*w as u32),
            // Unsized literals: minimum bit width from the value, never 0
            // bits — same rule as `infer_sv_width_str`.
            ExprKind::Literal(LitKind::Dec(v) | LitKind::Hex(v) | LitKind::Bin(v)) => {
                Some(if *v == 0 { 1 } else { 64 - v.leading_zeros() })
            }
            ExprKind::MethodCall(inner, method, margs) => match method.name.as_str() {
                "trunc" | "zext" | "sext" | "resize" => self.eval_const_u32(margs.first()?, params),
                "reverse" => self.reverse_recv_width_u32(inner, params, lookup_ident),
                _ => None,
            },
            ExprKind::BitSlice(_, hi, lo) => {
                let h = self.eval_const_u32(hi, params)?;
                let l = self.eval_const_u32(lo, params)?;
                (h >= l).then(|| h - l + 1)
            }
            ExprKind::PartSelect(_, _, w, _) => self.eval_const_u32(w, params),
            // `v[i].reverse<C>()` — the element width of an
            // identifier-typed `Vec` receiver.
            ExprKind::Index(base, _) => match &base.kind {
                ExprKind::Ident(n) => match lookup_ident(n)? {
                    TypeExpr::Vec(inner, _) => self.type_expr_width_const(&inner, params),
                    _ => None,
                },
                _ => None,
            },
            // Operator result widths — these MUST mirror typecheck's
            // `resolve_expr_type` rules (IEEE 1800 §11.6) exactly: the
            // width computed here sizes both the hoist temp and the
            // chunk part-selects, so a divergence from the checked type
            // would be a silent miscompile, not merely a portability
            // miss. Anything uncertain returns `None` (streaming-concat
            // fallback).
            ExprKind::Binary(op, l, r) => {
                let lw = self.reverse_recv_width_u32(l, params, lookup_ident);
                let rw = self.reverse_recv_width_u32(r, params, lookup_ident);
                match op {
                    BinOp::Eq
                    | BinOp::Neq
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Lte
                    | BinOp::Gte
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::Implies
                    | BinOp::ImpliesNext => Some(1),
                    BinOp::Add | BinOp::Sub => Some(lw?.max(rw?) + 1),
                    BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap => Some(lw?.max(rw?)),
                    BinOp::Mul => Some(lw? + rw?),
                    BinOp::Div | BinOp::Mod => lw,
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => Some(lw?.max(rw?)),
                    BinOp::Shl | BinOp::Shr => lw,
                }
            }
            ExprKind::Unary(op, operand) => match op {
                UnaryOp::Not | UnaryOp::RedAnd | UnaryOp::RedOr | UnaryOp::RedXor => Some(1),
                UnaryOp::BitNot => self.reverse_recv_width_u32(operand, params, lookup_ident),
                // typecheck: `-UInt<w>` → `SInt<w+1>`, `-SInt<w>` → `SInt<w>`
                // — receiver signedness isn't tracked here, so bail.
                UnaryOp::Neg => None,
            },
            // typecheck takes the then-branch type.
            ExprKind::Ternary(_, then_e, _) => {
                self.reverse_recv_width_u32(then_e, params, lookup_ident)
            }
            ExprKind::Concat(parts) => parts.iter().try_fold(0u32, |acc, p| {
                Some(acc + self.reverse_recv_width_u32(p, params, lookup_ident)?)
            }),
            ExprKind::Repeat(count, value) => Some(
                self.eval_const_u32(count, params)?
                    * self.reverse_recv_width_u32(value, params, lookup_ident)?,
            ),
            _ => None,
        }
    }

    /// arch#808: portable lowering for `.reverse<C>()`. Icarus Verilog
    /// (12.0) rejects the SV streaming-concat operator `{<<C{x}}` in
    /// every context, so when the receiver width `w` is known, emit an
    /// ordinary concatenation of the receiver's C-bit chunks in reversed
    /// order — bit-identical to `{<<C{x}}` (the receiver's lowest chunk
    /// lands most-significant; verified exhaustively against Verilator
    /// for chunks 1/2/4 over all 8-bit inputs) and accepted by both
    /// simulators. `w == c` (single chunk) is the identity — still
    /// emitted as a singleton concat `{x}` so the result stays an
    /// unsigned self-determined vector, exactly like the streaming form.
    fn emit_reverse_chunks(sel: &str, w: u32, c: u32) -> String {
        if w == c {
            return format!("{{{sel}}}");
        }
        let parts: Vec<String> = (0..w / c)
            .map(|i| {
                if c == 1 {
                    format!("{sel}[{i}]")
                } else {
                    format!("{sel}[{} +: {c}]", i * c)
                }
            })
            .collect();
        format!("{{{}}}", parts.join(", "))
    }

    /// Attempt the arch#808 portable `.reverse<chunk>()` lowering in the
    /// main (module/fsm) expression emitter. `None` — the caller keeps
    /// the streaming-concat emission — when the chunk or receiver width
    /// can't be const-resolved here, the divisibility doesn't hold
    /// (typecheck rejects both upstream), or the receiver needs a hoist
    /// temp but references a live runtime `for`-loop variable
    /// (module-scope temps can't see loop-locals; same guard as the
    /// `Index`/`BitSlice` hoists).
    fn try_emit_reverse_chunked(&self, base: &Expr, chunk: &Expr) -> Option<String> {
        let params = self.current_construct_params()?;
        let c = self.eval_const_u32(chunk, params)?;
        let unwrapped = Self::unwrap_reinterpret_cast(base);
        let w =
            self.reverse_recv_width_u32(unwrapped, params, &|n| self.construct_ident_type(n))?;
        if c == 0 || w == 0 || w % c != 0 {
            return None;
        }
        let sel = match &unwrapped.kind {
            // Directly part-selectable SV shapes — the same family the
            // `Index` emitter treats as atomic (minus literals, which
            // can't take a select at all).
            ExprKind::Ident(_)
            | ExprKind::SynthIdent(_, _)
            | ExprKind::Index(_, _)
            | ExprKind::FieldAccess(_, _) => self.emit_expr_str(unwrapped),
            // Anything else (a computation, literal, cast, or `{...}`
            // shape) can't take a part-select — hoist it to a named
            // module-scope temp, the same mechanism arch#650/#807 use.
            _ => {
                let in_loop = self.base_references_live_loop_var(unwrapped);
                let tmp = format!("arch_idx_base_{}", self.next_index_hoist_id());
                let rhs = self.emit_expr_str(unwrapped);
                self.push_hoist_temp_in_loop(w.to_string(), tmp.clone(), rhs, in_loop);
                tmp
            }
        };
        Some(Self::emit_reverse_chunks(&sel, w, c))
    }

    /// Emit an expression, wrapping in parens only when its precedence is
    /// below `parent_prec` (i.e. the context requires tighter binding).
    fn emit_expr_prec(&self, expr: &Expr, parent_prec: u8) -> String {
        let result = self.emit_expr_inner(expr);
        let my_prec = Self::expr_prec(expr);
        if my_prec < parent_prec {
            format!("({result})")
        } else {
            result
        }
    }

    /// Core expression emitter — never adds outer parens itself.
    fn emit_expr_inner(&self, expr: &Expr) -> String {
        match &expr.kind {
            // `pipelined_ops::lower_pipelined_calls` (proposal phase 3)
            // rewrites every codegen-backed `PipelinedCall` into a plain
            // `FunctionCall` before codegen starts, and
            // `main.rs::lower_pipelined_calls_before_codegen` refuses to
            // proceed (with a clear error) for any row lacking a codegen
            // binding. So this arm should be unreachable in practice; kept
            // as a loud backstop rather than silently falling back to a
            // comb cone, which would misrepresent an un-retimed operator
            // as pipelined.
            // `scaled_quantize<Fmt, policy, rounding>(v)` — the block shape
            // comes from the EXPRESSION's own format argument, not from the
            // assignment target, so nothing here has to be inferred.
            ExprKind::ScaledQuantize(value, fmt, policy, round) => {
                let shape = crate::fp_block::shape_of_type(fmt).unwrap_or_else(|| {
                    panic!(
                        "scaled_quantize format has no resolvable block shape — \
                         typecheck accepts only `ScaledVec` formats, so this means the \
                         block size did not fold to a literal (arch#884)"
                    )
                });
                let h = crate::fp_block::BlockHelper::Quantize {
                    shape,
                    policy: *policy,
                    round: *round,
                };
                self.fp_helpers_used.set(true);
                self.block_helpers.borrow_mut().insert(h);
                format!("{}({})", h.sv_name(), self.emit_expr_str(value))
            }
            ExprKind::PipelinedCall(name, _, stages) => unreachable!(
                "codegen reached `{name}<pipelined, {stages}>(...)` — this should have been \
                 lowered by pipelined_ops::lower_pipelined_calls before codegen started"
            ),
            // `q@K` on RHS lowers to the K-th tap of the pipe_reg
            // chain (`q` being the final flop, source being the input
            // before any flop). Numbering counts cycles of delay from
            // the input: `@0` = source comb, `@K` = after K flops,
            // `@N` = bare `q`. Falls through transparently when the
            // base isn't a known pipe_reg name (typecheck rejects
            // out-of-range / non-pipe-reg uses earlier).
            ExprKind::LatencyAt(inner, n) => {
                if let ExprKind::Ident(name) = &inner.kind {
                    if let Some((source, stages)) = self.pipe_regs.get(name) {
                        let stages = *stages;
                        if *n == 0 {
                            return source.clone();
                        }
                        if *n == stages {
                            return name.clone();
                        }
                        if *n < stages {
                            return format!("{name}_stg{n}");
                        }
                    }
                }
                self.emit_expr_inner(inner)
            }
            // SVA forward-shift: `##N expr` only legal inside an assert
            // /cover property (typecheck enforces). Emit verbatim — SV
            // accepts it natively in property context.
            ExprKind::SvaNext(n, inner) => format!("##{n} {}", self.emit_expr_inner(inner)),
            // SynthIdent: compiler-synthesized name pointing at codegen-
            // emitted SV wires (credit_channel dispatch targets). Emits as
            // a plain identifier — the declaration + driver live elsewhere
            // in the emitted SV.
            ExprKind::SynthIdent(name, _) => name.clone(),
            ExprKind::Literal(lit) => match lit {
                LitKind::Dec(v) => format!("{v}"),
                LitKind::Hex(v) => format!("'h{v:X}"),
                LitKind::Bin(v) => format!("'b{v:b}"),
                LitKind::Sized(w, v) => format!("{w}'d{v}"),
                // A parameter-width sized literal (`W'd5`) is NOT legal
                // SystemVerilog — the size of a sized literal must be a decimal
                // *number*, not a parameter reference (both Verilator and
                // iverilog reject `W'd5` with a syntax error). Emit the legal
                // parameterized form instead: a size cast, but keep the operand
                // explicitly unsigned. Plain `W'(15)` parses, yet it behaves like
                // a signed 4-bit value (`-1`) while `4'd15` is unsigned `15`.
                // `W'($unsigned(15))` preserves the original literal semantics
                // while staying valid under both Verilator and iverilog. (The
                // type checker already resolves the ARCH type to `UInt<W>` via
                // `resolve_param_sized_literal_width`; this is the SV-surface
                // counterpart.)
                LitKind::ParamSized(name, v) => format!("{name}'($unsigned({v}))"),
                // Float literal (FP32 by default) → 32-bit binary32 bit pattern.
                LitKind::Float(bits) => {
                    format!("32'h{:08X}", (f64::from_bits(*bits) as f32).to_bits())
                }
                // A literal already rounded to its context float type at
                // compile time (arch#622/#624) — emit the exact width-correct
                // constant directly, no runtime helper call (avoids the #620/
                // #624 WIDTHTRUNC truncation bug for narrow formats).
                LitKind::TypedFloat(fmt, bits) => match fmt {
                    FloatLitFmt::Fp32 => format!("32'h{bits:08X}"),
                    FloatLitFmt::Bf16 => format!("16'h{bits:04X}"),
                    FloatLitFmt::E4m3 | FloatLitFmt::E5m2 => format!("8'h{bits:02X}"),
                    FloatLitFmt::E2m1 => format!("4'h{bits:01X}"),
                    FloatLitFmt::E2m3 | FloatLitFmt::E3m2 => format!("6'h{bits:02X}"),
                },
            },
            ExprKind::Bool(true) => "1'b1".to_string(),
            ExprKind::Bool(false) => "1'b0".to_string(),
            ExprKind::Ident(name) => {
                // Context-sensitive substitution: used by Vec method predicate
                // lowering to rebind `item` → `recv[i]`, `index` → `W'd<i>`.
                if let Some(sub) = self.ident_subst.get(name) {
                    return sub.clone();
                }
                // Static for-loop unroll bound the loop variable to a literal
                // integer (e.g. `chans[i].v` inside `for i in 0..N-1`). Emit
                // the integer instead of the bare identifier so the RHS of
                // the unrolled body uses the iteration value.
                if let Some(v) = self.loop_var_subst.get(name) {
                    return v.to_string();
                }
                name.clone()
            }
            ExprKind::Binary(op, lhs, rhs) => {
                // Floating-point operands dispatch to the emitted `arch_f32_*` /
                // `arch_bf16_*` SystemVerilog helper functions.
                if let Some(fmt) = self
                    .expr_float_fmt(lhs)
                    .or_else(|| self.expr_float_fmt(rhs))
                {
                    let fop = match op {
                        BinOp::Add => Some("add"),
                        BinOp::Sub => Some("sub"),
                        BinOp::Mul => Some("mul"),
                        BinOp::Eq => Some("eq"),
                        BinOp::Neq => Some("ne"),
                        BinOp::Lt => Some("lt"),
                        BinOp::Gt => Some("gt"),
                        BinOp::Lte => Some("le"),
                        BinOp::Gte => Some("ge"),
                        _ => None,
                    };
                    if let Some(fop) = fop {
                        self.fp_helpers_used.set(true);
                        let l = self.emit_expr_prec(lhs, 0);
                        let r = self.emit_expr_prec(rhs, 0);
                        return format!("arch_{fmt}_{fop}({l}, {r})");
                    }
                }
                // `implies` lowers to (!lhs || rhs)
                if *op == BinOp::Implies {
                    let l = self.emit_expr_prec(lhs, 14); // unary prec for !
                    let r = self.emit_expr_prec(rhs, 4); // || prec
                    return format!("{l} |-> {r}");
                }
                if *op == BinOp::ImpliesNext {
                    // SVA next-cycle implication. Only valid inside
                    // assert/cover property contexts (typechecker enforces).
                    let l = self.emit_expr_prec(lhs, 4);
                    let r = self.emit_expr_prec(rhs, 4);
                    return format!("{l} |=> {r}");
                }
                let prec = Self::sv_binop_prec(op);
                // LHS: same-prec left-assoc chain of the SAME associative op → no wrap;
                // otherwise wrap if same-or-lower precedence.
                let lhs_prec = if matches!(&lhs.kind, ExprKind::Binary(lop, _, _) if lop == op
                    && matches!(op, BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::And | BinOp::Or))
                {
                    prec // same assoc op — don't wrap
                } else {
                    prec + 1 // different op at same level — wrap
                };
                let l = self.emit_expr_prec(lhs, lhs_prec);
                // RHS: wrap if same-or-lower precedence to respect left-associativity,
                // EXCEPT for the same commutative/associative op (chain without parens).
                let rhs_prec = if matches!(&rhs.kind, ExprKind::Binary(rop, _, _) if rop == op
                    && matches!(op, BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::And | BinOp::Or))
                {
                    prec // same assoc op — don't wrap
                } else {
                    prec + 1 // different op at same level — wrap
                };
                let r = self.emit_expr_prec(rhs, rhs_prec);
                // Use arithmetic shift (>>>) when LHS is cast to SInt
                let shr_str = if matches!(op, BinOp::Shr) && self.expr_is_signed(lhs) {
                    ">>>"
                } else {
                    ">>"
                };
                let op_str = match op {
                    BinOp::Add | BinOp::AddWrap => "+",
                    BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Eq => "==",
                    BinOp::Neq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Lte => "<=",
                    BinOp::Gte => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<",
                    BinOp::Shr => shr_str,
                    BinOp::Implies | BinOp::ImpliesNext => unreachable!("implies handled above"),
                };
                if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                    let lw = self.infer_sv_width_str(lhs);
                    let rw = self.infer_sv_width_str(rhs);
                    let w = if lw == rw {
                        lw
                    } else {
                        format!("({lw} > {rw} ? {lw} : {rw})")
                    };
                    let wp = Self::paren_width(&w);
                    format!("{wp}'({l} {op_str} {r})")
                } else {
                    format!("{l} {op_str} {r}")
                }
            }
            ExprKind::Unary(op, operand) => {
                // Unary has prec 14 — wrap child only if it's a binary/ternary.
                //
                // A nested unary is the exception (arch#892): two adjacent
                // prefix operators have equal precedence, so the generic
                // rule leaves them juxtaposed, and every same-operator pair
                // is then either a syntax error or a different token:
                //
                //   ~~a    Icarus 12.0: syntax error   (Verilator accepts)
                //   !!a    Icarus 12.0: syntax error   (Verilator accepts)
                //   ^^a    Icarus 12.0: syntax error   (Verilator accepts)
                //   - -a   Icarus 12.0: syntax error even with the space
                //   --a    BOTH reject — lexes as the decrement token
                //   &&a    BOTH reject — lexes as logical-AND
                //   ||a    BOTH reject — lexes as logical-OR
                //
                // Parenthesizing the operand fixes all of them and is
                // always legal, so the rule is uniform rather than a list
                // of unsafe pairs — a new `UnaryOp` can't reintroduce the
                // bug. (`~&a`/`~^a` do happen to be valid NAND/XNOR
                // reduction tokens meaning the same thing, but they are
                // parenthesized too rather than special-cased.)
                //
                // Folding `~~x` to `x` would be wrong: the operators are
                // width- and sign-significant in a self-determined context.
                let o = if matches!(operand.kind, ExprKind::Unary(..)) {
                    format!("({})", self.emit_expr_prec(operand, 0))
                } else {
                    self.emit_expr_prec(operand, 14)
                };
                match op {
                    UnaryOp::Not => format!("!{o}"),
                    UnaryOp::BitNot => format!("~{o}"),
                    UnaryOp::Neg => format!("-{o}"),
                    UnaryOp::RedAnd => format!("&{o}"),
                    UnaryOp::RedOr => format!("|{o}"),
                    UnaryOp::RedXor => format!("^{o}"),
                }
            }
            ExprKind::FieldAccess(base, field) => {
                // rst.asserted — polarity-abstracted reset active check
                if field.name == "asserted" {
                    if let ExprKind::Ident(base_name) = &base.kind {
                        if let Some((_, level)) = self.reset_ports.get(base_name) {
                            return if *level == ResetLevel::Low {
                                format!("(!{base_name})")
                            } else {
                                base_name.clone()
                            };
                        }
                    }
                }
                // Bus port / bus wire: axi.aw_valid → axi_aw_valid (flat).
                // Bus wires flatten to individual SV signals, same naming.
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.bus_ports.contains_key(base_name)
                        || self.bus_wires.contains_key(base_name)
                    {
                        return format!("{}_{}", base_name, field.name);
                    }
                }
                // Indexed bus port or bus wire: `m_axi[i].valid`.
                //
                // D2 emission for Vec-of-bus *ports*: SV-side is one unpacked
                // array per (port, signal), so `m_axi[i].valid` becomes
                // `m_axi_valid[i]`. The index is emitted as a SV expression —
                // a literal substitutes directly; a loop variable is left as
                // its SV genvar identifier; a static-unroll-bound loop var
                // still substitutes to its literal value.
                //
                // Bus wires (and scalar bus ports) keep the flat name —
                // `axi_aw_valid` for scalar, `m_axi_<i>_valid` for indexed
                // wire — because they're internal to the module body and
                // their consumers haven't been migrated yet.
                if let ExprKind::Index(arr, idx) = &base.kind {
                    // 2D bus wire element: `edges[m][n].sig`. The arr is
                    // itself `Index(Ident(name), m_idx)`, and `idx` is n_idx.
                    // SV-side storage is now packed: `logic [M-1:0][N-1:0]
                    // <wire>_<sig>`, so `edges[m][n].sig` lowers to
                    // `<wire>_<sig>[m][n]`. m and n can be literals OR
                    // loop variables (genvar references in a preserved
                    // generate_for, though insts force unrolling — so today
                    // they're always literals).
                    if let ExprKind::Index(inner_arr, inner_idx) = &arr.kind {
                        if let ExprKind::Ident(arr_name) = &inner_arr.kind {
                            let resolve_str = |e: &Expr| -> String {
                                match &e.kind {
                                    ExprKind::Ident(loopvar) => {
                                        if let Some(&v) = self.loop_var_subst.get(loopvar) {
                                            v.to_string()
                                        } else {
                                            loopvar.clone()
                                        }
                                    }
                                    _ => self.emit_expr_str(e),
                                }
                            };
                            // Check that the wire is registered (any cell).
                            let cell0 = format!("{}_0_0", arr_name);
                            if self.bus_wires.contains_key(&cell0) {
                                let m_str = resolve_str(inner_idx);
                                let n_str = resolve_str(idx);
                                return format!(
                                    "{}_{}[{}][{}]",
                                    arr_name, field.name, m_str, n_str
                                );
                            }
                        }
                    }
                    if let ExprKind::Ident(arr_name) = &arr.kind {
                        // Vec-of-bus *port* → packed indexed ref `<port>_<sig>[i]`.
                        if self.vec_of_bus_port_count.contains_key(arr_name) {
                            let idx_str = match &idx.kind {
                                ExprKind::Ident(loopvar) => {
                                    if let Some(&v) = self.loop_var_subst.get(loopvar) {
                                        v.to_string()
                                    } else {
                                        loopvar.clone()
                                    }
                                }
                                _ => self.emit_expr_str(idx),
                            };
                            return format!("{}_{}[{}]", arr_name, field.name, idx_str);
                        }
                        // 1D Vec-of-bus *wire* — also packed now. Same form.
                        let cell0 = format!("{}_0", arr_name);
                        if self.bus_wires.contains_key(&cell0) {
                            let idx_str = match &idx.kind {
                                ExprKind::Ident(loopvar) => {
                                    if let Some(&v) = self.loop_var_subst.get(loopvar) {
                                        v.to_string()
                                    } else {
                                        loopvar.clone()
                                    }
                                }
                                _ => self.emit_expr_str(idx),
                            };
                            return format!("{}_{}[{}]", arr_name, field.name, idx_str);
                        }
                    }
                }
                let b = self.emit_expr_str(base);
                format!("{b}.{}", field.name)
            }
            ExprKind::MethodCall(base, method, args) => self.emit_method_call_str(
                base,
                method,
                args,
                MethodCallHost::Main,
                &|e: &Expr| self.emit_expr_str(e),
                // The main emitter's chunked lowering re-emits the receiver
                // itself; the pre-emitted receiver string is unused.
                &|recv: &Expr, chunk: &Expr, _emitted: &str| {
                    self.try_emit_reverse_chunked(recv, chunk)
                },
            ),
            ExprKind::Cast(expr, ty) => {
                let e = self.emit_expr_str(expr);
                match &**ty {
                    TypeExpr::SInt(_) => {
                        format!("$signed({e})")
                    }
                    TypeExpr::UInt(w) => {
                        let ws = self.emit_expr_str(w);
                        format!("{ws}'($unsigned({e}))")
                    }
                    // `as Vec<T, N>` is a typecheck-only view (UInt<N>'s
                    // bits read as N elements). Width is identical so SV
                    // can pass the inner expression through unchanged.
                    TypeExpr::Vec(_, _) => e,
                    _ => {
                        let t = self.emit_type_str(ty);
                        format!("{t}'({e})")
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                if let (Some(v), Some(idx_v)) = (literal_expr_u64(base), literal_expr_u64(idx)) {
                    if idx_v < 64 {
                        return format!("1'd{}", (v >> idx_v) & 1);
                    }
                }
                // Vec-of-const param `B[i]`: rewrite to packed part-select
                // `B[i*W +: W]` since iverilog rejects unpacked-array params.
                if let ExprKind::Ident(name) = &base.kind {
                    if let Some(elem_ty) = self.vec_params.get(name) {
                        let w = match elem_ty {
                            TypeExpr::UInt(w) | TypeExpr::SInt(w) => self.emit_expr_str(w),
                            _ => "1".to_string(),
                        };
                        let i = self.emit_expr_str(idx);
                        // The packed param is declared `signed` for SInt
                        // elements, so the part-select inherits signedness
                        // without an explicit `$signed()` wrap.
                        return format!("{name}[({i}) * ({w}) +: ({w})]");
                    }
                }
                // Icarus portability (arch#650): unlike `BitSlice`/`PartSelect`,
                // typecheck's `is_portable_bit_slice_base` does not gate the
                // single-bit `Index` form — it also serves plain `Vec`-element
                // access (`v[i]`), which is portable for *any* `v`. So
                // `unsigned(x)[i]`, `signed(x)[i]`, `(x as T)[i]`, and
                // `(a - b)[i]` all pass `arch check` today, and previously hit
                // this arm's unconditional bare `{b}[{i}]` emission.
                //
                // `unsigned(...)`/`signed(...)`/`as T` casts are pure bit
                // reinterpretations — typecheck requires same-width casts, so
                // indexing the cast reads the identical bit as indexing the
                // cast's inner expression. Unwrap them rather than emitting
                // e.g. `$unsigned(x)[i]`: Verilator accepts an indexed
                // system-function-call result, but Icarus rejects it.
                let unwrapped = Self::unwrap_reinterpret_cast(base);
                if Self::is_atomic_index_base(unwrapped) {
                    let b = self.emit_expr_str(unwrapped);
                    let i = self.emit_expr_str(idx);
                    return format!("{b}[{i}]");
                }
                // Remaining case: a real computation (arithmetic, logical,
                // ternary, ...) as the base — e.g. `(a - b)[i]`. Parenthesizing
                // doesn't help (`(a - b)[i]` is illegal SV on both Verilator
                // and Icarus — bit-select doesn't compose with a parenthesized
                // non-selectable expression, same rule spec §3.2.1 documents
                // for `BitSlice`/`PartSelect`); emitting it bare is worse than
                // nonportable — with no base handling at all, this arm used to
                // literally concatenate `base` and `[idx]` as text, so
                // `(a - b)[i]` silently became `a - b[i]`, reparsed as
                // `a - (b[i])` — a precedence miscompile, not merely
                // non-portable SV. Hoist to a module-scope named temp instead,
                // the same "bind to a let" fix the spec recommends at the
                // source level for BitSlice/PartSelect.
                //
                // This hoist is unconditional. It used to be skipped when
                // the base referenced a live runtime `for`-loop variable,
                // on the grounds that a module-scope temp can't see a
                // loop-local `int` — but "skipped" meant falling back to
                // exactly the bare text concatenation described above, so
                // the loop-var case kept emitting the precedence
                // miscompile this arm exists to prevent: `(a + v[i])[3]`
                // became `a + v[i][3]`, which Verilator compiles silently
                // and evaluates as `a + (v[i][3])` (arch#861).
                //
                // The premise was also only half true. The *declaration*
                // must indeed leave the loop, but the *assignment* must
                // stay in it — so the temp is flagged `in_loop` and
                // `place_hoist_temps` splits it: declaration spliced to
                // module scope, value computed inside the loop body as a
                // blocking assignment, where the iterator is in scope.
                // Same split `HoistScope::Function` already uses for a
                // base referencing function arguments (arch#846).
                let in_loop = self.base_references_live_loop_var(unwrapped);
                // Deliberately NOT underscore-prefixed (unlike most other
                // compiler-synthesized names in this file, e.g.
                // `__shared_*_out`, `_auto_bound_*`): Icarus Verilog 12.0
                // segfaults elaborating `logic [$bits(...)-1:0] <name>;`
                // when `<name>` starts with `_` — reproduced in isolation
                // (crashes for `_tmp`/`_x`/`_0`/..., but not for the
                // identical declaration renamed to e.g. `tmp`). The width
                // computed by `infer_sv_width_str` below routinely falls
                // back to `$bits(<expr>)` for a non-const-foldable
                // arithmetic base, so this hoist temp hits that combination
                // whenever it's needed — avoid the crash outright rather
                // than depend on which fallback width shape happens to be
                // "safe" today.
                let tmp = format!("arch_idx_base_{}", self.next_index_hoist_id());
                let w = Self::paren_width(&self.infer_sv_width_str(unwrapped));
                let rhs = self.emit_expr_str(unwrapped);
                self.push_hoist_temp_in_loop(w, tmp.clone(), rhs, in_loop);
                let i = self.emit_expr_str(idx);
                format!("{tmp}[{i}]")
            }
            ExprKind::BitSlice(base, hi, lo) => {
                if let (Some(v), Some(hi_v), Some(lo_v)) = (
                    literal_expr_u64(base),
                    literal_expr_u64(hi),
                    literal_expr_u64(lo),
                ) {
                    if hi_v >= lo_v && hi_v < 64 {
                        let width = (hi_v - lo_v + 1) as u32;
                        let mask = if width >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << width) - 1
                        };
                        return format!("{width}'d{}", (v >> lo_v) & mask);
                    }
                }
                // Portability (arch#807, arch#810): a `Concat`/`Repeat`/
                // `FunctionCall`/`MethodCall` base must be hoisted to a
                // named module-scope temp rather than emitted bare — see
                // `hoist_slice_base` for why (Icarus 12.0 rejects all four
                // bare, contra the spec's old §3.2.1 claim that iverilog
                // accepts them; Verilator additionally rejects the
                // `MethodCall` size-cast shape `8'(x)[hi:lo]`).
                let b = if let Some(tmp) = self.hoist_slice_base(base) {
                    tmp
                } else {
                    let b = self.emit_expr_str(base);
                    // Parenthesize complex base expressions to avoid precedence issues.
                    // SynthIdent is a compiler-renamed bare identifier with the same
                    // semantics as Ident — no parens needed (Verilator rejects
                    // `(__name)[hi:lo]` as a syntax error).
                    // `FunctionCall`/`MethodCall` normally never reach here
                    // — `hoist_slice_base` above claims them — but they do
                    // when the base references a live runtime `for`-loop
                    // variable and the hoist bails. Bare is still the best
                    // available fallback there: `(func())[hi:lo]` is
                    // rejected by *both* frontends (a select doesn't
                    // compose with a parenthesized expression), whereas
                    // `func()[hi:lo]` at least compiles on Verilator.
                    if matches!(
                        base.kind,
                        ExprKind::Ident(_)
                            | ExprKind::SynthIdent(_, _)
                            | ExprKind::Literal(_)
                            | ExprKind::Index(_, _)
                            | ExprKind::FieldAccess(_, _)
                            | ExprKind::FunctionCall(_, _)
                            | ExprKind::MethodCall(_, _, _)
                    ) {
                        b
                    } else {
                        format!("({})", b)
                    }
                };
                // Try to emit indexed part-select: base[lo +: width]
                if let Some(width) = Self::try_indexed_part_select(hi, lo) {
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{l} +: {width}]")
                } else {
                    let h = self.emit_expr_str(hi);
                    let l = self.emit_expr_str(lo);
                    format!("{b}[{h}:{l}]")
                }
            }
            ExprKind::PartSelect(base, start, width, up) => {
                // Portability (arch#807, arch#810): same hoist as BitSlice
                // above — `{a,c}[start +: w]`, `{N{a}}[start +: w]`,
                // `f(x)[start +: w]` are rejected by Icarus 12.0 (bare or
                // parenthesized) and `8'(x)[start +: w]` by Verilator too,
                // despite all being portable `is_portable_bit_slice_base`
                // bases.
                let b = if let Some(tmp) = self.hoist_slice_base(base) {
                    tmp
                } else {
                    self.emit_expr_str(base)
                };
                let s = self.emit_expr_str(start);
                let w = self.emit_expr_str(width);
                let op = if *up { "+:" } else { "-:" };
                format!("{b}[{s} {op} {w}]")
            }
            ExprKind::StructLiteral(name, fields) => {
                // Emit packed struct literals as a positional concatenation
                // rather than an SV assignment pattern. Verilator accepts both,
                // but iverilog rejects assignment patterns in some continuous
                // assignment contexts. ARCH packed structs declare fields in
                // MSB-first order, so concatenating values in declaration order
                // preserves the bit layout.
                if let Some((crate::resolve::Symbol::Struct(info), _)) =
                    self.symbols.globals.get(&name.name)
                {
                    let mut vals = Vec::new();
                    for (field_name, field_ty) in &info.fields {
                        if let Some(f) = fields.iter().find(|f| f.name.name == *field_name) {
                            vals.push(self.emit_field_value_sized(&f.value, field_ty));
                        }
                    }
                    if vals.len() == info.fields.len() {
                        format!("{{{}}}", vals.join(", "))
                    } else {
                        let field_strs: Vec<String> = fields
                            .iter()
                            .map(|f| format!("{}: {}", f.name.name, self.emit_expr_str(&f.value)))
                            .collect();
                        format!("'{{{}}}", field_strs.join(", "))
                    }
                } else {
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name.name, self.emit_expr_str(&f.value)))
                        .collect();
                    format!("'{{{}}}", field_strs.join(", "))
                }
            }
            ExprKind::EnumVariant(enum_name, variant) => {
                // Extern types from `extern package` — emit bare variant name
                // (preserving case), relying on `import Pkg::*;` for resolution.
                if matches!(
                    self.symbols.globals.get(&enum_name.name),
                    Some((Symbol::ExternEnum(_), _))
                ) {
                    variant.name.clone()
                // Known ARCH-side enum → emit just the variant name in uppercase.
                } else if matches!(
                    self.symbols.globals.get(&enum_name.name),
                    Some((Symbol::Enum(_), _))
                ) {
                    variant.name.to_uppercase()
                // Cross-package qualified refs (e.g. `ibex_pkg::RV32MFast`) —
                // preserve the package prefix and original case.
                } else {
                    format!("{}::{}", enum_name.name, variant.name)
                }
            }
            ExprKind::Todo => "'0 /* TODO: todo! placeholder */".to_string(),
            ExprKind::Concat(parts) => {
                let strs: Vec<String> = parts.iter().map(|p| self.emit_expr_str(p)).collect();
                format!("{{{}}}", strs.join(", "))
            }
            ExprKind::Repeat(count, value) => {
                let c = self.emit_expr_str(count);
                let v = self.emit_expr_str(value);
                format!("{{{c}{{{v}}}}}")
            }
            ExprKind::Clog2(arg) => {
                let a = self.emit_expr_str(arg);
                format!("$clog2({a})")
            }
            ExprKind::Onehot(index) => {
                let idx = self.emit_expr_str(index);
                format!("(1 << {idx})")
            }
            ExprKind::Signed(inner) => {
                let e = self.emit_expr_str(inner);
                format!("$signed({e})")
            }
            ExprKind::Unsigned(inner) => {
                let e = self.emit_expr_str(inner);
                format!("$unsigned({e})")
            }
            ExprKind::Match(scrutinee, _arms) => {
                let s = self.emit_expr_str(scrutinee);
                format!("/* match({s}) */ '0")
            }
            ExprKind::ExprMatch(scrutinee, arms) => {
                // Emit as nested ternary: (cond) ? val : (cond) ? val : default
                let s = self.emit_expr_str(scrutinee);
                let mut result = "'0".to_string();
                for arm in arms.iter().rev() {
                    let val = self.emit_expr_str(&arm.value);
                    let cond = match &arm.pattern {
                        Pattern::Wildcard => {
                            result = val;
                            continue;
                        }
                        Pattern::Literal(e) => {
                            let lit = self.emit_expr_str(e);
                            format!("({s} == {lit})")
                        }
                        Pattern::Ident(id) if id.name == "_" => {
                            result = val;
                            continue;
                        }
                        Pattern::Ident(id) => format!("({s} == {id})", id = id.name),
                        Pattern::EnumVariant(en, vr) => {
                            format!(
                                "({s} == {en}__{vr})",
                                en = en.name.to_uppercase(),
                                vr = vr.name.to_uppercase()
                            )
                        }
                    };
                    result = format!("({cond} ? {val} : {result})");
                }
                result
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                // Inside ?: operands, any precedence is fine (delimited by ? and :)
                let c = self.emit_expr_prec(cond, 3); // wrap only if lower than ternary
                let t = self.emit_expr_str(then_expr);
                let e = self.emit_expr_str(else_expr);
                format!("{c} ? {t} : {e}")
            }
            ExprKind::Inside(scrutinee, members) => {
                let s = self.emit_expr_str(scrutinee);
                let member_strs: Vec<String> = members
                    .iter()
                    .map(|m| match m {
                        InsideMember::Single(e) => self.emit_expr_str(e),
                        InsideMember::Range(lo, hi) => {
                            let l = self.emit_expr_str(lo);
                            let h = self.emit_expr_str(hi);
                            format!("[{l}:{h}]")
                        }
                    })
                    .collect();
                format!("{s} inside {{{}}}", member_strs.join(", "))
            }
            ExprKind::FunctionCall(name, args) => {
                self.emit_function_call_str_in(expr, name, args, &|a| self.emit_expr_str(a))
            }
        }
    }

    /// Emit a `FunctionCall` — the `shared function` output-wire rewrite,
    /// the SVA (`past`/`rose`/`fell`) and float (`fma`/`is_nan`) intrinsics,
    /// and overload name mangling — with each argument rendered by
    /// `emit_arg`.
    ///
    /// Parameterized on the argument emitter so a context with its own
    /// identifier rewriting reuses all of the above instead of forking it:
    /// the `pipeline` emitters prefix a stage signal `r` as `<stage>_r`, and
    /// before arch#852 their fall-through to `emit_expr_str` emitted the
    /// bare source name, so `Ident8(r)` referenced a signal that does not
    /// exist in the emitted SV. Mirrors `hoist_slice_base_in`'s `emit`
    /// parameter (arch#845).
    fn emit_function_call_str_in(
        &self,
        expr: &Expr,
        name: &str,
        args: &[Expr],
        emit_arg: &dyn Fn(&Expr) -> String,
    ) -> String {
        // `shared function` rewrite: if this call site was
        // collected by the pre-pass, return the shared output
        // wire instead of the inline `FN(args)` form. The
        // harness `assign __shared_FN_out = FN(...)` emitted at
        // module scope is the single textual call site that
        // carries the operand mux.
        if let Some(out_wire) = self.shared_call_sites.get(&expr.span.start) {
            return out_wire.clone();
        }
        let arg_strs: Vec<String> = args.iter().map(emit_arg).collect();
        // `scaled_dequantize(b)` → the generated block helper. The shape comes
        // from the OPERAND's declared type; typecheck has already refused a
        // non-block operand, so a `None` here means the block's `N` did not
        // fold to a literal, which is a real limitation and must not be
        // guessed at.
        if name == "scaled_dequantize" && args.len() == 1 {
            let shape = self
                .expr_decl_type(&args[0])
                .as_ref()
                .and_then(|t| crate::fp_block::shape_of_type(t))
                .unwrap_or_else(|| {
                    panic!(
                        "scaled_dequantize operand has no resolvable block shape — \
                         typecheck accepts only `ScaledVec` operands, so this means the \
                         block size did not fold to a literal (arch#884)"
                    )
                });
            let h = crate::fp_block::BlockHelper::Dequantize { shape };
            self.fp_helpers_used.set(true);
            self.block_helpers.borrow_mut().insert(h);
            return format!("{}({})", h.sv_name(), arg_strs[0]);
        }
        // `scaled_dot(a, b)` → the generated block helper. Unlike quantize /
        // dequantize this returns a scalar FP32, so it stays an ordinary
        // expression on both backends rather than needing a statement form.
        if name == "scaled_dot" && args.len() == 2 {
            let shape = self
                .expr_decl_type(&args[0])
                .as_ref()
                .and_then(|t| crate::fp_block::shape_of_type(t))
                .unwrap_or_else(|| {
                    panic!(
                        "scaled_dot operand has no resolvable block shape — typecheck accepts \
                         only matching `ScaledVec` operands, so this means the block size did \
                         not fold to a literal (arch#884 phase 3)"
                    )
                });
            let h = crate::fp_block::BlockHelper::Dot { shape };
            self.fp_helpers_used.set(true);
            self.block_helpers.borrow_mut().insert(h);
            return format!("{}({}, {})", h.sv_name(), arg_strs[0], arg_strs[1]);
        }
        // Built-in SVA: past/rose/fell → SV $past/$rose/$fell
        if name == "past" || name == "rose" || name == "fell" {
            return format!("${name}({})", arg_strs.join(", "));
        }
        // Float intrinsics → emitted helper functions.
        if name == "fma" && args.len() == 3 {
            self.fp_helpers_used.set(true);
            let fmt = self
                .expr_float_fmt(&args[0])
                .or_else(|| self.expr_float_fmt(&args[1]))
                .or_else(|| self.expr_float_fmt(&args[2]))
                .unwrap_or("f32");
            return format!("arch_fma_{fmt}({})", arg_strs.join(", "));
        }
        if name == "is_nan" && args.len() == 1 {
            // exponent all-ones and mantissa nonzero.
            let a = &arg_strs[0];
            // E8M0 is a SCALE type, not a float, so it carries no
            // float tag and would otherwise fall through to the f32
            // test — reading bits [30:23] of an 8-bit signal, which
            // SV zero-fills into a silent constant false. Its NaN is
            // the single code 0xFF.
            if matches!(self.scale_type_of(&args[0]), Some(TypeExpr::E8M0)) {
                return format!("({a} == 8'hFF)");
            }
            // UE4M3 is the OTHER scale type, and its sole NaN is a DIFFERENT
            // code: 0x7F, not 0xFF. Sharing E8M0's arm would make `is_nan`
            // silently constant-false on every NVFP4 scale.
            if matches!(self.scale_type_of(&args[0]), Some(TypeExpr::UE4M3)) {
                return format!("({a} == 8'h7F)");
            }
            // The NaN test is DERIVED from the format table rather
            // than tabulated per tag. The old hand-written match
            // ended in `_ =>` returning the f32 test — at f32's bit
            // offsets — so any format it did not name was silently
            // probed at the wrong bits. Deriving it means a new
            // format is a table row, not a fifth arm here.
            let tag = self.expr_float_fmt(&args[0]).unwrap_or("f32");
            let d = crate::fp_format::by_tag(tag)
                .unwrap_or_else(|| crate::fp_format::by_id(crate::fp_format::FpFormatId::Fp32));
            return match d.nan_rule {
                crate::fp_format::NanRule::IeeeExpAllOnes => {
                    let (eh, el) = d.exp_field();
                    let (mh, ml) = d
                        .mant_field()
                        .expect("IEEE-shaped format must have a mantissa");
                    let exp_ones = (1u64 << d.exp_bits) - 1;
                    format!(
                        "(({a}[{eh}:{el}] == {}'h{exp_ones:X}) && ({a}[{mh}:{ml}] != {}'b0))",
                        d.exp_bits, d.mant_bits
                    )
                }
                crate::fp_format::NanRule::OcpAllMagnitudeOnes => {
                    let (gh, gl) = d.magnitude_field();
                    let mag_bits = d.magnitude_bits();
                    let mag_ones = (1u64 << mag_bits) - 1;
                    format!("({a}[{gh}:{gl}] == {mag_bits}'h{mag_ones:X})")
                }
                // Unreachable: typecheck rejects `is_nan` on a format
                // with no NaN encoding (`Ty::is_float_arith`).
                crate::fp_format::NanRule::NoNan => unreachable!(
                    "is_nan on `{}`, which has no NaN encoding — typecheck \
                         should have rejected this",
                    d.type_name
                ),
            };
        }
        // Resolve mangled name if this is an overloaded function.
        let sv_name = if let Some((Symbol::Function(overloads), _)) = self.symbols.globals.get(name)
        {
            if overloads.len() > 1 {
                let idx = self
                    .overload_map
                    .get(&expr.span.start)
                    .copied()
                    .unwrap_or(0);
                let ov = &overloads[idx];
                let suffix: String = ov
                    .arg_types
                    .iter()
                    .map(|t| Self::type_mangle_tag(t))
                    .collect::<Vec<_>>()
                    .join("_");
                format!("{name}_{suffix}")
            } else {
                name.to_string()
            }
        } else {
            name.to_string()
        };
        format!("{sv_name}({})", arg_strs.join(", "))
    }

    /// Convert a width expression to a Verilog range string `[N:0]`.
    /// For literal widths, folds the arithmetic: `Dec(8)` → `"7:0"`.
    /// For expressions (params, binaries), keeps the expression: `"N-1:0"`.
    fn emit_width_range(&self, w: &Expr) -> String {
        match &w.kind {
            ExprKind::Literal(LitKind::Dec(n)) => {
                format!("{}:0", n.saturating_sub(1))
            }
            _ => {
                let ws = self.emit_expr_str(w);
                format!("{ws}-1:0")
            }
        }
    }

    /// Width of the element plane of a `ScaledVec` (`N * elem_w`) as an SV
    /// expression, folded to a literal when `N` is constant. `"0"` if `ty` is
    /// not a ScaledVec.
    pub(crate) fn scaled_vec_elems_width(&self, ty: &TypeExpr) -> String {
        let TypeExpr::ScaledVec(elem, n, _) = ty else {
            return "0".to_string();
        };
        let ew = self
            .type_expr_data_width(elem)
            .unwrap_or_else(|| "0".to_string());
        let nstr = self.emit_expr_str(n);
        match (ew.parse::<u64>(), nstr.parse::<u64>()) {
            (Ok(e), Ok(k)) => (e * k).to_string(),
            _ => format!("({ew}) * ({nstr})"),
        }
    }

    /// Width of the scale field of a `ScaledVec`. `"0"` if not a ScaledVec.
    pub(crate) fn scaled_vec_scale_width(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::ScaledVec(_, _, scale) => self
                .type_expr_data_width(scale)
                .unwrap_or_else(|| "0".to_string()),
            _ => "0".to_string(),
        }
    }

    /// Fold a width string (output of emit_expr_str) to a range.
    /// If `s` parses as a decimal integer, emits `"N-1:0"` pre-computed.
    /// Otherwise keeps `"s-1:0"`.
    fn fold_width_str(s: &str) -> String {
        if let Ok(n) = s.parse::<u64>() {
            format!("{}:0", n.saturating_sub(1))
        } else {
            format!("{s}-1:0")
        }
    }

    fn emit_type_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::UInt(w) => {
                let range = self.emit_width_range(w);
                format!("logic [{range}]")
            }
            TypeExpr::SInt(w) => {
                let range = self.emit_width_range(w);
                format!("logic signed [{range}]")
            }
            TypeExpr::Bool => "logic".to_string(),
            TypeExpr::Bit => "logic".to_string(),
            // Floats are carried as packed bit vectors (the FP unit modules
            // operate on the raw [W-1:0] bit pattern).
            TypeExpr::FP32 => "logic [31:0]".to_string(),
            TypeExpr::BF16 => "logic [15:0]".to_string(),
            TypeExpr::FP8E4M3 | TypeExpr::FP8E5M2 => "logic [7:0]".to_string(),
            TypeExpr::FP4E2M1 => "logic [3:0]".to_string(),
            TypeExpr::FP6E2M3 | TypeExpr::FP6E3M2 => "logic [5:0]".to_string(),
            TypeExpr::E8M0 | TypeExpr::UE4M3 => "logic [7:0]".to_string(),
            // One packed word `{scale, P[N-1], …, P[0]}` — NOT an array, so
            // there is no dimension suffix to carry (contrast Vec below).
            TypeExpr::ScaledVec(..) => {
                let w = self
                    .type_expr_data_width(ty)
                    .unwrap_or_else(|| "0".to_string());
                format!("logic [{}]", Self::fold_width_str(&w))
            }
            TypeExpr::Clock(_) => "logic".to_string(),
            TypeExpr::Reset(_, _) => "logic".to_string(),
            TypeExpr::Vec(_, _) => {
                // Packed multi-dimensional: all dims are in the type string, no suffix.
                let (type_str, _suffix) = self.emit_type_and_array_suffix(ty);
                type_str
            }
            TypeExpr::Named(ident) => ident.name.clone(),
        }
    }

    fn emit_port_type_str(&self, ty: &TypeExpr) -> String {
        // Port types use the same emission as internal types.
        self.emit_type_str(ty)
    }

    /// Substitute bus parameter names in a TypeExpr with actual value expressions.
    fn subst_type_expr(
        ty: &TypeExpr,
        params: &std::collections::HashMap<String, &Expr>,
    ) -> TypeExpr {
        match ty {
            TypeExpr::UInt(w) => TypeExpr::UInt(Box::new(Self::subst_expr(w, params))),
            TypeExpr::SInt(w) => TypeExpr::SInt(Box::new(Self::subst_expr(w, params))),
            TypeExpr::Vec(inner, len) => TypeExpr::Vec(
                Box::new(Self::subst_type_expr(inner, params)),
                Box::new(Self::subst_expr(len, params)),
            ),
            other => other.clone(),
        }
    }

    fn subst_expr(expr: &Expr, params: &std::collections::HashMap<String, &Expr>) -> Expr {
        let kind = match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some(replacement) = params.get(name) {
                    return (*replacement).clone();
                }
                ExprKind::Ident(name.clone())
            }
            // Recurse into expression trees so arithmetic width expressions
            // (e.g. `UInt<DATA_W / 8>`, `UInt<DATA_W * 2>`) get the param
            // substituted in every operand. Without this, the ident shows
            // up verbatim in the emitted SV and downstream tools fail.
            ExprKind::Binary(op, l, r) => ExprKind::Binary(
                *op,
                Box::new(Self::subst_expr(l, params)),
                Box::new(Self::subst_expr(r, params)),
            ),
            ExprKind::Unary(op, e) => ExprKind::Unary(*op, Box::new(Self::subst_expr(e, params))),
            ExprKind::Ternary(c, t, e) => ExprKind::Ternary(
                Box::new(Self::subst_expr(c, params)),
                Box::new(Self::subst_expr(t, params)),
                Box::new(Self::subst_expr(e, params)),
            ),
            ExprKind::Clog2(e) => ExprKind::Clog2(Box::new(Self::subst_expr(e, params))),
            ExprKind::Index(b, i) => ExprKind::Index(
                Box::new(Self::subst_expr(b, params)),
                Box::new(Self::subst_expr(i, params)),
            ),
            _ => return expr.clone(),
        };
        Expr {
            kind,
            span: expr.span,
            parenthesized: expr.parenthesized,
        }
    }

    fn emit_logic_type_str(&self, ty: &TypeExpr) -> String {
        self.emit_type_str(ty)
    }

    /// For Vec types (including nested), returns (packed_type_str, "").
    /// The array dimensions are folded into the type as SV packed multi-dimensional
    /// ranges, e.g. `Vec<UInt<16>, 4>` → `("logic [3:0][15:0]", "")`.
    /// Packed arrays are portable across Verilator, Yosys, and iverilog; unpacked
    /// array dimensions after the signal name are rejected by Yosys during synthesis.
    /// For non-Vec types, returns (type_str, "").
    fn emit_type_and_array_suffix(&self, ty: &TypeExpr) -> (String, String) {
        let mut dims = Vec::new();
        let mut cur = ty;
        while let TypeExpr::Vec(inner, size) = cur {
            let range = self.emit_width_range(size);
            dims.push(format!("[{range}]"));
            cur = inner;
        }
        if dims.is_empty() {
            return (self.emit_type_str(ty), String::new());
        }
        // For 1-bit elements (`UInt<1>`, `Bool`, `Bit`), collapse the
        // redundant `[0:0]` inner dim so a `Vec<UInt<1>, N>` emits as
        // `logic [N-1:0]` (single packed) instead of `logic [N-1:0][0:0]`
        // (multi-dim). Necessary for clean interop with upstream SV
        // ports declared `logic [N-1:0] x` — yosys-slang's elaboration
        // can mis-resolve the multi-dim form's port-by-position
        // mapping and silently DCE the connection. arch-ibex IbexTop's
        // `ic_tag_req_o`/`ic_data_req_o` (Vec<UInt<1>, 2>) hit this and
        // got their entire RAM-bank connections eliminated post-flatten.
        let cur = match cur {
            TypeExpr::UInt(w) if Self::is_const_one(w) => &TypeExpr::Bool,
            _ => cur,
        };
        // Build packed multi-dim type: "logic [outerDim][innerDim][baseRange]"
        // emit_type_str(cur) returns e.g. "logic [15:0]" for UInt<16>.
        // We insert the packed dims immediately after the "logic" keyword.
        let inner_type = self.emit_type_str(cur);
        let packed_dims: String = dims.join("");
        let type_str = if let Some(rest) = inner_type.strip_prefix("logic") {
            // rest is e.g. " [15:0]", " signed [15:0]", or "" for Bool.
            // For signed inner types hoist "signed" before the packed dims so the
            // result is valid SV: "logic signed [M-1:0][N-1:0]" not the illegal
            // "logic [M-1:0] signed [N-1:0]".
            if let Some(after_signed) = rest.strip_prefix(" signed") {
                format!("logic signed {packed_dims}{after_signed}")
            } else {
                format!("logic {packed_dims}{rest}")
            }
        } else {
            format!("{inner_type} {packed_dims}")
        };
        (type_str, String::new())
    }

    /// Emit `Vec<T,N>` as an SV **unpacked** array at port boundaries:
    /// base type is the element type (e.g. `logic [W-1:0]`); array
    /// dimensions go in the suffix after the port name (e.g. `[N-1:0]`).
    ///
    /// Used only for ports declared with the `unpacked` modifier. Caller
    /// is responsible for restricting this to port emission — unpacked
    /// arrays are fine in Verilator but Yosys-unfriendly in synthesis,
    /// so all internal nets/regs/signals continue to use the packed shape
    /// from `emit_type_and_array_suffix`.
    /// Emit the SV unpacked-array form for a Vec<T,N>: returns
    /// (base type string, suffix). When `ascending` is true, the unpacked
    /// dim is emitted as `[0:N-1]` instead of the default `[N-1:0]`.
    /// Required for interop with upstream SV that declares the connecting
    /// array as `logic [W-1:0] x [N]` shorthand (= `[0:N-1]`); without
    /// this, IEEE 1800-2017 §10.10 element-by-position port mapping
    /// silently reverses the indices. See arch-com#307.
    fn emit_type_and_unpacked_suffix_dir(
        &self,
        ty: &TypeExpr,
        ascending: bool,
    ) -> (String, String) {
        let mut dims = Vec::new();
        let mut cur = ty;
        while let TypeExpr::Vec(inner, size) = cur {
            let range = if ascending {
                self.emit_width_range_ascending(size)
            } else {
                self.emit_width_range(size)
            };
            dims.push(format!("[{range}]"));
            cur = inner;
        }
        if dims.is_empty() {
            return (self.emit_type_str(ty), String::new());
        }
        let base_ty = self.emit_type_str(cur);
        let suffix: String = dims.iter().map(|d| format!(" {d}")).collect();
        (base_ty, suffix)
    }

    /// Emit an unpacked array dim as `0:N-1` (ascending). Mirrors
    /// `emit_width_range` but flips the direction. The packed Vec dim
    /// (controlled by `emit_width_range`) is always descending — only
    /// the unpacked dim ever needs ascending.
    fn emit_width_range_ascending(&self, w: &Expr) -> String {
        match &w.kind {
            ExprKind::Literal(LitKind::Dec(n)) => {
                format!("0:{}", n.saturating_sub(1))
            }
            _ => {
                let ws = self.emit_expr_str(w);
                format!("0:{ws}-1")
            }
        }
    }

    // ── Synchronizer ─────────────────────────────────────────────────────────
    // ── RAM ───────────────────────────────────────────────────────────────────
}

fn literal_expr_u64(expr: &Expr) -> Option<u64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
        _ => None,
    }
}
