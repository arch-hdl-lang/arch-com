use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, NamedSource, Report, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};

use arch::ast::{BinOp, Expr, ExprKind, Item, LitKind, ParamDecl, ParamKind, SourceFile, UnaryOp};
use arch::codegen::Codegen;
use arch::diagnostics::CompileError;
use arch::elaborate;
use arch::formal;
use arch::lexer;
use arch::parser;
use arch::resolve;
use arch::sim_codegen::SimCodegen;
use arch::typecheck::TypeChecker;

fn cxx_std_flag() -> String {
    std::env::var("ARCH_CXX_STD").unwrap_or_else(|_| "-std=c++20".to_string())
}

/// C++ compiler used to build the generated sim/testbench. Override with the
/// `ARCH_CXX` env var (mirrors harc's `HARC_CXX`); defaults to `g++`.
///
/// On Linux, real GCC miscompiles harc's C++20 coroutine testbench scheduler,
/// so harc-driven testbenches need `ARCH_CXX=clang++`. On macOS `g++` is a
/// clang shim, so the default works there.
fn cxx_compiler() -> String {
    std::env::var("ARCH_CXX").unwrap_or_else(|_| "g++".to_string())
}

#[derive(Parser)]
#[command(name = "arch", version, about = "ARCH HDL compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Type-check ARCH source file(s)
    Check {
        /// Input .arch file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Rebuild the learning retrieval index over ~/.arch/learn/events.jsonl
    LearnIndex,
    /// Delete the entire local learning store at ~/.arch/learn/
    LearnClear,
    /// Remove individual events from the learning store by filter.
    /// Combine filters freely; an event is removed if ANY filter matches.
    LearnPrune {
        /// Remove events with this error_code (e.g. "parse_error", "other")
        #[arg(long)]
        code: Option<String>,
        /// Remove events whose diff/message/file_path contains this substring
        #[arg(long)]
        contains: Option<String>,
        /// Remove events older than this many days
        #[arg(long)]
        older_than_days: Option<u64>,
        /// Report what would be removed without modifying the store
        #[arg(long)]
        dry_run: bool,
    },
    /// Retrieve past error→fix pairs matching the query
    Advise {
        /// Query string (free text; matched against error codes, messages, diffs).
        /// May be omitted when --from-stderr is set.
        query: Vec<String>,
        /// Number of top results to print
        #[arg(short = 'k', long, default_value_t = 3)]
        top: usize,
        /// Read the query from stdin (e.g. `arch check foo.arch 2>&1 | arch advise --from-stderr`)
        #[arg(long)]
        from_stderr: bool,
        /// Restrict the search to feature events (spec→RTL provenance from
        /// `///` / `//!` / `//! ---` doc comments) instead of error→fix
        /// pairs. Returns ranked `<file>::<construct>` matches with a doc
        /// snippet. See `doc/plan_arch_doc_comments.md` §6.
        #[arg(long)]
        feature: bool,
    },
    /// Show stats about the local learning store
    LearnStats,
    /// Seed the local learning store with feature events harvested from a
    /// directory of `.arch` files (default: `examples/`). Walks the path
    /// recursively, parses each file, and emits one feature event per
    /// top-level construct that carries `///` / `//!` / `//! ---` doc
    /// content. Silently skips files that fail to parse. Re-running
    /// replaces the existing feature events for each harvested file —
    /// safe to run repeatedly. Build the BM25 index afterwards with
    /// `arch learn-index`.
    LearnBootstrap {
        /// Directory to walk (default: `examples/` under the current
        /// working directory).
        #[arg(default_value = "examples")]
        path: PathBuf,
    },
    /// Build and query the compiler-native ARCH code graph
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Work with ARCH/Verilator-compatible code coverage data
    Coverage {
        #[command(subcommand)]
        command: CoverageCommand,
    },
    /// List the pipelined-operator implementation registry
    /// (doc/proposal_pipelined_operators.md). Passive listing only — for
    /// "which depth should I use" guidance, use `arch advise`.
    Ops {
        /// Emit the markdown table used for doc/generated/pipelined_ops.md
        /// instead of the plain-text listing.
        #[arg(long)]
        markdown: bool,
    },
    /// Compile ARCH to SystemVerilog
    Build {
        /// Input .arch file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Output .sv file
        #[arg(short, long)]
        o: Option<PathBuf>,
        /// Emit a static HTML thread lowering map. Bare flag writes
        /// `<sv-output-stem>.thread.html`; `--emit-thread-map=PATH` writes
        /// the explicit path. The optional value requires `=`.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_thread_map: Option<Option<PathBuf>>,
        /// Emit a machine-readable thread lowering proof certificate JSON.
        /// Bare flag writes `<sv-output-stem>.thread-proof.json`;
        /// `--emit-thread-proof=PATH` writes the explicit path. The optional
        /// value requires `=`.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_thread_proof: Option<Option<PathBuf>>,
        /// Emit a Lean replay file for the thread lowering proof certificate.
        /// Bare flag writes `<sv-output-stem>.thread-proof.lean`;
        /// `--emit-thread-proof-lean=PATH` writes the explicit path. The
        /// optional value requires `=`.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_thread_proof_lean: Option<Option<PathBuf>>,
        /// Emit the Lean thread proof file and immediately replay it with
        /// `lake env lean`. Use `--thread-proof-lean-project=DIR` or
        /// `ARCH_THREAD_PROOF_LEAN_PROJECT` to locate the Lean project.
        #[arg(long)]
        check_thread_proof_lean: bool,
        /// Lean project directory used by `--check-thread-proof-lean`.
        #[arg(long)]
        thread_proof_lean_project: Option<PathBuf>,
        /// Emit a Lean replay file for first-class fifo/arbiter construct
        /// proof certificates. Bare flag writes
        /// `<sv-output-stem>.construct-proof.lean`; `=PATH` writes the
        /// explicit path.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_construct_proof_lean: Option<Option<PathBuf>>,
        /// Emit the Lean construct proof file and immediately replay it with
        /// `lake env lean`. Use `--construct-proof-lean-project=DIR` or
        /// `ARCH_CONSTRUCT_PROOF_LEAN_PROJECT` to locate the Lean project.
        #[arg(long)]
        check_construct_proof_lean: bool,
        /// Lean project directory used by `--check-construct-proof-lean`.
        #[arg(long)]
        construct_proof_lean_project: Option<PathBuf>,
        /// Emit SMT-LIB2 sanity queries for first-class fifo/arbiter
        /// construct proof certificates. Bare flag writes
        /// `<sv-output-stem>.construct-proof.smt2`; `=PATH` writes the
        /// explicit path.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_construct_proof_smt: Option<Option<PathBuf>>,
        /// Emit the SMT-LIB2 construct proof file and immediately check
        /// each query with the selected solver. Every query must return
        /// `unsat`.
        #[arg(long)]
        check_construct_proof_smt: bool,
        /// Solver used by `--check-construct-proof-smt`.
        #[arg(long, default_value = "z3")]
        construct_proof_smt_solver: String,
        /// Auto-emit SVA properties from `thread` lowering (wait_until / wait
        /// N cycle progress, fork-join branch transitions). Wrapped in
        /// `synopsys translate_off/on` so they don't reach synthesis. Off by
        /// default; turn on for `arch formal` runs or under Verilator
        /// `--assert` to get free spec-derived coverage.
        #[arg(long)]
        auto_thread_asserts: bool,
        /// Only emit constructs from the original input .arch files, not
        /// from dependency files auto-discovered via `inst` / `use`.
        /// Avoids MODDUP when sub-modules are also built as standalone
        /// .sv files and linked together downstream.
        #[arg(long)]
        no_inline_deps: bool,
        /// Floating-point special-value compatibility profile (doc/archive/plan_fp_types.md
        /// §6.2): `riscv` (default) or `cuda`. Shares one IEEE-754 RNE arithmetic
        /// core; selects only the canonical NaN pattern (0x7FC00000/0x7FC0 vs
        /// 0x7FFFFFFF/0x7FFF) and the NaN→int result (type max vs 0).
        #[arg(long = "fp-compat", default_value = "riscv")]
        fp_compat: String,
    },
    /// Compile ARCH + C++ testbench and run simulation
    ///
    /// Example: arch sim Foo.arch Foo_tb.cpp
    ///
    /// Generates Verilator-compatible C++ models, compiles with a C++ compiler, and runs.
    ///
    /// The compiler defaults to `g++`; override it with the `ARCH_CXX` env var
    /// (e.g. `ARCH_CXX=clang++ arch sim ...`). On Linux, GCC miscompiles harc's
    /// C++20 coroutine testbench scheduler, so harc-driven testbenches require
    /// `ARCH_CXX=clang++`. Related: `ARCH_CXX_STD` (default `-std=c++20`) and
    /// `ARCH_OPT` (default `-O2 -flto`).
    Sim {
        /// Input .arch file(s)
        #[arg(required = true)]
        arch_files: Vec<PathBuf>,
        /// C++ testbench file(s) to compile alongside the generated models
        #[arg(long = "tb", num_args = 1..)]
        tb_files: Vec<PathBuf>,
        /// Output directory for generated C++ files
        /// (default: $ARCH_SIM_BUILD_DIR, else arch_sim_build/)
        #[arg(short, long)]
        outdir: Option<PathBuf>,
        /// Enable uninitialized register read detection (reset-none regs + pipe_reg propagation)
        #[arg(long)]
        check_uninit: bool,
        /// Also warn when primary inputs are read before the TB explicitly initializes them.
        /// Implies --check-uninit. The TB must call `dut.set_<port>(v)` (generated setters) to mark an input as initialized;
        /// a plain `dut.<port> = v;` does not mark init.
        #[arg(long)]
        inputs_start_uninit: bool,
        /// Also warn when a RAM cell is read before the design or TB has written it
        /// (per-cell valid bitmap; `init:` cells are marked valid at construction; ROMs are exempt).
        /// Implies --check-uninit.
        #[arg(long)]
        check_uninit_ram: bool,
        /// Randomize synchronizer latency to model CDC metastability
        #[arg(long)]
        cdc_random: bool,
        /// Emit VCD waveform to file (e.g. --wave out.vcd)
        #[arg(long)]
        wave: Option<PathBuf>,
        /// Auto-instrument I/O port value changes for debugging
        #[arg(long)]
        debug: bool,
        /// Additional debug options: fsm (print FSM state transitions). Implies --debug.
        /// Example: --debug+fsm or standalone --debug-opts fsm
        #[arg(long = "debug+fsm")]
        debug_fsm: bool,
        /// How many module levels to instrument with --debug (default 1 = top module only)
        #[arg(long = "depth", default_value_t = 1)]
        debug_depth: u32,
        /// Enable code coverage instrumentation. Counts each if/elsif/else arm
        /// in seq and comb blocks and dumps `coverage.txt` keyed to .arch source
        /// lines at sim exit. See doc/plan_arch_coverage.md for the phased rollout
        /// (branch → line → FSM → toggle → Verilator-compatible coverage.dat).
        #[arg(long)]
        coverage: bool,
        /// Also emit a Verilator-compatible coverage.dat alongside the stderr
        /// report. Path defaults to `coverage.dat` in the cwd; pass a value to
        /// override (e.g. --coverage-dat=build/cov.dat). Implies --coverage.
        /// Output is consumed by `verilator_coverage --annotate-min 1
        /// --annotate annot/ <file>`.
        #[arg(long)]
        coverage_dat: Option<Option<String>>,
        /// Thread-sim mode: `fsm` (default; threads lowered to FSM, single-core),
        /// `parallel` (pre-lowering coroutine sim — Verilator-style use
        /// `--threads N` to spread N OS threads), or `both` (cross-check
        /// fsm vs parallel). See doc/plan_thread_parallel_sim.md and
        /// doc/plan_thread_parallel_sim_phase3.md.
        #[arg(long = "thread-sim", default_value = "fsm")]
        thread_sim: String,
        /// Number of OS threads to use under `--thread-sim parallel`.
        /// Default 1 = cooperative single-OS-thread coroutine scheduler
        /// (current behavior). N>1 spawns one OS thread per user
        /// `thread` block (Verilator-style). See Phase 3 plan.
        #[arg(long = "threads", default_value_t = 1)]
        threads: u32,
        /// Generate pybind11 Python module for cocotb-compatible testing
        #[arg(long)]
        pybind: bool,
        /// Python test file to run with arch_cocotb adapter (requires --pybind)
        #[arg(long)]
        test: Option<PathBuf>,
        /// Override the pybind11 module name (default: V<Module>_pybind).
        /// Useful when multiple variants of one design need to coexist in a
        /// single Python process — each can have a distinct PyInit_* symbol.
        #[arg(long)]
        pybind_module_name: Option<String>,
        /// Auto-emit SVA properties from `thread` lowering — see `arch build`
        /// help for the property set. Picked up by Verilator `--assert`.
        #[arg(long)]
        auto_thread_asserts: bool,
        /// Override a module param default: --param NAME=VALUE (repeatable).
        /// Value must be an integer literal. The override is applied before
        /// sim codegen and also passed to the C++ compiler as -DNAME=VALUE
        /// for generated header guards.
        #[arg(long = "param", value_name = "NAME=VALUE")]
        param_overrides: Vec<String>,
        /// Floating-point special-value compatibility profile (doc/archive/plan_fp_types.md
        /// §6.2): `riscv` (default) or `cuda`. Honored identically by the SV and
        /// sim backends so they never disagree.
        #[arg(long = "fp-compat", default_value = "riscv")]
        fp_compat: String,
    },
    /// Formal verification: emit SMT-LIB2 and invoke a bit-vector SMT solver.
    ///
    /// Bounded model-checks asserts and covers in the selected module by
    /// translating ARCH AST directly to SMT-LIB2 (no Yosys in the loop).
    Formal {
        /// Input .arch file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Top module name (required if the file declares multiple modules)
        #[arg(long)]
        top: Option<String>,
        /// BMC unroll depth (cycles)
        #[arg(short, long, default_value_t = 20)]
        bound: u32,
        /// SMT solver binary: z3, boolector, or bitwuzla
        #[arg(short, long, default_value = "z3")]
        solver: String,
        /// Dump the generated SMT-LIB2 to this file (for inspection / debugging)
        #[arg(long)]
        emit_smt: Option<PathBuf>,
        /// Emit a Lean replay file for the thread lowering proof certificate.
        /// Bare flag writes `<first-input-stem>.thread-proof.lean`;
        /// `--emit-thread-proof-lean=PATH` writes the explicit path. The
        /// optional value requires `=`.
        #[arg(long, num_args = 0..=1, require_equals = true)]
        emit_thread_proof_lean: Option<Option<PathBuf>>,
        /// Emit the Lean thread proof file and immediately replay it with
        /// `lake env lean`. Use `--thread-proof-lean-project=DIR` or
        /// `ARCH_THREAD_PROOF_LEAN_PROJECT` to locate the Lean project.
        #[arg(long)]
        check_thread_proof_lean: bool,
        /// Lean project directory used by `--check-thread-proof-lean`.
        #[arg(long)]
        thread_proof_lean_project: Option<PathBuf>,
        /// After emitting/replaying the Lean thread-lowering proof, skip the
        /// SMT-LIB2 backend. Use this when the goal is compiler lowering
        /// proof replay rather than bounded design-property checking.
        #[arg(long)]
        thread_proof_only: bool,
        /// Per-property solver timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u32,
        /// Auto-emit SVA properties from `thread` lowering — provable by the
        /// formal backend when the lowering is correct (the properties hold
        /// by construction). See `arch build` help for the property set.
        #[arg(long)]
        auto_thread_asserts: bool,
        /// Floating-point special-value compatibility profile (doc/archive/plan_fp_types.md
        /// §6.2): `riscv` (default) or `cuda`. Accepted for parity with `build`/`sim`.
        #[arg(long = "fp-compat", default_value = "riscv")]
        fp_compat: String,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    /// Index ARCH source files into JSONL graph records
    Index {
        /// Input .arch files or directories to index
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Root used to make graph paths and IDs stable
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output graph directory
        #[arg(long, default_value = ".archgraph")]
        out: PathBuf,
        /// Replace an existing output graph directory
        #[arg(long)]
        clean: bool,
    },
    /// Query graph nodes by symbol, path, or doc text
    Query {
        /// Symbol or free-text query
        query: String,
        /// Input graph directory
        #[arg(long, default_value = ".archgraph")]
        index: PathBuf,
        /// Emit JSON instead of compact human text
        #[arg(long)]
        json: bool,
        /// Maximum result count
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show indexed callers for a function or method name
    Callers {
        /// Function or method name
        target: String,
        /// Input graph directory
        #[arg(long, default_value = ".archgraph")]
        index: PathBuf,
        /// Emit JSON instead of compact human text
        #[arg(long)]
        json: bool,
        /// Maximum result count
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show a bounded graph neighborhood for a symbol
    Impact {
        /// Symbol or text used to choose start nodes
        symbol: String,
        /// Traversal depth
        #[arg(long, default_value_t = 2)]
        depth: usize,
        /// Input graph directory
        #[arg(long, default_value = ".archgraph")]
        index: PathBuf,
        /// Emit JSON instead of compact human text
        #[arg(long)]
        json: bool,
        /// Maximum result count
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// Return a bounded context slice for a task description
    Context {
        /// Task description
        task: String,
        /// Input graph directory
        #[arg(long, default_value = ".archgraph")]
        index: PathBuf,
        /// Emit JSON instead of compact human text
        #[arg(long)]
        json: bool,
        /// Maximum result count
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Render an indexed graph as a standalone clickable HTML file
    Html {
        /// Input graph directory
        #[arg(long, default_value = ".archgraph")]
        index: PathBuf,
        /// Output HTML file
        #[arg(long, default_value = "arch-graph.html")]
        out: PathBuf,
        /// Page title
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Subcommand)]
enum CoverageCommand {
    /// Merge Verilator-compatible coverage.dat files by summing matching counters
    Merge {
        /// Input coverage.dat files to merge
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output merged coverage.dat path
        #[arg(short, long, default_value = "coverage.dat")]
        out: PathBuf,
    },
}

/// Tracks which portions of a concatenated source belong to which file.
struct MultiSource {
    /// (start_offset, end_offset, filename, original_source)
    segments: Vec<(usize, usize, String, String)>,
    combined: String,
}

impl MultiSource {
    fn from_files(files: &[PathBuf]) -> miette::Result<Self> {
        let mut combined = String::new();
        let mut segments = Vec::new();

        for file in files {
            let source = fs::read_to_string(file).into_diagnostic()?;
            let start = combined.len();
            combined.push_str(&source);
            let end = combined.len();
            segments.push((start, end, file.display().to_string(), source));
            // Add newline separator between files
            combined.push('\n');
        }

        Ok(MultiSource { segments, combined })
    }

    /// Find which file a byte offset belongs to and return (filename, file_source, local_offset).
    fn locate(&self, offset: usize) -> (&str, &str, usize) {
        for (start, end, name, src) in &self.segments {
            if offset >= *start && offset < *end {
                return (name, src, offset - start);
            }
        }
        // Fallback to last file
        if let Some((start, _, name, src)) = self.segments.last() {
            (name, src, offset.saturating_sub(*start))
        } else {
            ("unknown", "", offset)
        }
    }

    /// Build a miette Report for an error, using the correct file source.
    fn report_error(&self, err: CompileError) -> Report {
        let offset = err.span_offset();
        let (filename, file_source, local_offset) = self.locate(offset);
        let relocated_err = err.relocate(local_offset);
        Report::new(relocated_err).with_source_code(NamedSource::new(
            filename.to_string(),
            file_source.to_string(),
        ))
    }
}

/// Codegen-side binding for `arch build` / `arch sim` (not `arch check`,
/// which fully supports `<pipelined, N>` through typecheck alone): rewrites
/// every `fma<pipelined, N>(...)`-style call reaching codegen into the
/// plain comb call the registry's `codegen_impl` binds it to (proposal
/// phase 3, `doc/proposal_pipelined_operators.md` — see `pipelined_ops`
/// module docs for the comb+retime shape and why no bespoke staged-datapath
/// codegen is needed). Registry rows that typecheck but have no
/// `codegen_impl` wired yet (`codegen_impl: None` — a future row landed
/// ahead of its codegen support) still refuse loudly here, exactly like
/// the phase-2 backstop did unconditionally, rather than silently falling
/// back to comb + delay-line (which would compute correct values but
/// misrepresent an un-retimed cone as the requested pipelined operator).
fn lower_pipelined_calls_before_codegen(
    ast: &mut arch::ast::SourceFile,
    ms: &MultiSource,
) -> miette::Result<()> {
    arch::pipelined_ops::lower_pipelined_calls(ast).map_err(|f| {
        let err = CompileError::general(
            &format!(
                "`{}<pipelined, {}>(...)` typechecks (`arch check` accepts it) but codegen for \
                 this depth is not yet implemented — no `codegen_impl` is wired for this registry \
                 row (doc/proposal_pipelined_operators.md phase 3/4). Not supported by \
                 `arch build` / `arch sim` yet.",
                f.operator, f.stages
            ),
            f.span,
        );
        ms.report_error(err)
    })
}

fn thread_map_sources_from_multi(ms: &MultiSource) -> Vec<arch::thread_map::ThreadMapSource> {
    ms.segments
        .iter()
        .map(
            |(start, end, filename, source)| arch::thread_map::ThreadMapSource {
                start: *start,
                end: *end,
                filename: filename.clone(),
                source: source.clone(),
            },
        )
        .collect()
}

fn check_thread_proof_lean_file(
    proof_path: &Path,
    explicit_project: Option<&Path>,
) -> miette::Result<()> {
    let project_dir = thread_proof_lean_project_dir(explicit_project)?;
    let proof_path = proof_path
        .canonicalize()
        .unwrap_or_else(|_| proof_path.to_path_buf());
    let lake = resolve_lake_bin();
    let output = std::process::Command::new(&lake)
        .arg("env")
        .arg("lean")
        .arg(&proof_path)
        .current_dir(&project_dir)
        .output()
        .into_diagnostic()
        .map_err(|err| {
            miette::miette!(
                "failed to run Lean proof replay via `{}`; install elan or set PATH/ELAN_HOME so `lake` is available: {err}",
                lake.display()
            )
        })?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!(
            "Lean thread proof replay failed for {} in {}\nstdout:\n{}\nstderr:\n{}",
            proof_path.display(),
            project_dir.display(),
            stdout,
            stderr
        ));
    }
    eprintln!("Lean proof replay OK {}", proof_path.display());
    Ok(())
}

fn check_construct_proof_lean_file(
    proof_path: &Path,
    explicit_project: Option<&Path>,
) -> miette::Result<()> {
    let project_dir = construct_proof_lean_project_dir(explicit_project)?;
    let proof_path = proof_path
        .canonicalize()
        .unwrap_or_else(|_| proof_path.to_path_buf());
    let lake = resolve_lake_bin();
    let output = std::process::Command::new(&lake)
        .arg("env")
        .arg("lean")
        .arg(&proof_path)
        .current_dir(&project_dir)
        .output()
        .into_diagnostic()
        .map_err(|err| {
            miette::miette!(
                "failed to run Lean construct proof replay via `{}`; install elan or set PATH/ELAN_HOME so `lake` is available: {err}",
                lake.display()
            )
        })?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!(
            "Lean construct proof replay failed for {} in {}\nstdout:\n{}\nstderr:\n{}",
            proof_path.display(),
            project_dir.display(),
            stdout,
            stderr
        ));
    }
    eprintln!("Lean construct proof replay OK {}", proof_path.display());
    Ok(())
}

fn resolve_lake_bin() -> PathBuf {
    find_executable_on_path("lake")
        .or_else(|| {
            std::env::var_os("ELAN_HOME")
                .map(PathBuf::from)
                .map(|p| p.join("bin").join("lake"))
                .filter(|p| p.is_file())
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".elan").join("bin").join("lake"))
                .filter(|p| p.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("lake"))
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn check_construct_proof_smt_file(proof_path: &Path, solver: &str) -> miette::Result<()> {
    let output = std::process::Command::new(solver)
        .arg(proof_path)
        .output()
        .into_diagnostic()
        .map_err(|err| {
            miette::miette!(
                "failed to run construct SMT proof solver `{solver}`; set PATH so it is available: {err}"
            )
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(miette::miette!(
            "construct SMT proof solver `{solver}` failed for {}\nstdout:\n{}\nstderr:\n{}",
            proof_path.display(),
            stdout,
            stderr
        ));
    }
    let results: Vec<_> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| matches!(*line, "sat" | "unsat" | "unknown"))
        .collect();
    if results.is_empty() {
        return Err(miette::miette!(
            "construct SMT proof solver `{solver}` produced no check-sat result for {}\nstdout:\n{}\nstderr:\n{}",
            proof_path.display(),
            stdout,
            stderr
        ));
    }
    if let Some((idx, status)) = results
        .iter()
        .enumerate()
        .find(|(_, status)| **status != "unsat")
    {
        return Err(miette::miette!(
            "construct SMT proof query {} returned `{}` for {}; expected `unsat`\nstdout:\n{}\nstderr:\n{}",
            idx,
            status,
            proof_path.display(),
            stdout,
            stderr
        ));
    }
    eprintln!(
        "Construct SMT proof OK {} ({} queries via {})",
        proof_path.display(),
        results.len(),
        solver
    );
    Ok(())
}

fn thread_proof_lean_project_dir(explicit_project: Option<&Path>) -> miette::Result<PathBuf> {
    let candidate = if let Some(path) = explicit_project {
        path.to_path_buf()
    } else if let Ok(path) = std::env::var("ARCH_THREAD_PROOF_LEAN_PROJECT") {
        PathBuf::from(path)
    } else {
        PathBuf::from("proofs/lean_thread_lowering")
    };
    if !candidate.join("lakefile.toml").exists() {
        return Err(miette::miette!(
            "Lean thread proof project not found at {}; pass --thread-proof-lean-project=DIR or set ARCH_THREAD_PROOF_LEAN_PROJECT",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn construct_proof_lean_project_dir(explicit_project: Option<&Path>) -> miette::Result<PathBuf> {
    let candidate = if let Some(path) = explicit_project {
        path.to_path_buf()
    } else if let Ok(path) = std::env::var("ARCH_CONSTRUCT_PROOF_LEAN_PROJECT") {
        PathBuf::from(path)
    } else if let Ok(path) = std::env::var("ARCH_THREAD_PROOF_LEAN_PROJECT") {
        PathBuf::from(path)
    } else {
        PathBuf::from("proofs/lean_thread_lowering")
    };
    if !candidate.join("lakefile.toml").exists() {
        return Err(miette::miette!(
            "Lean construct proof project not found at {}; pass --construct-proof-lean-project=DIR or set ARCH_CONSTRUCT_PROOF_LEAN_PROJECT",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn filter_thread_map_by_ranges(
    map: &arch::thread_map::ThreadMap,
    ranges: &[(usize, usize)],
) -> arch::thread_map::ThreadMap {
    let modules = map
        .modules
        .iter()
        .filter(|module| {
            ranges
                .iter()
                .any(|(start, end)| module.span.start >= *start && module.span.start < *end)
        })
        .cloned()
        .collect();
    arch::thread_map::ThreadMap { modules }
}

/// Run a compiler-command body and record its success/failure into the
/// local learning store. Respects `ARCH_NO_LEARN=1` opt-out.
fn learn_wrap<F>(files: &[PathBuf], f: F) -> miette::Result<()>
where
    F: FnOnce() -> miette::Result<()>,
{
    let enabled = arch::learn::is_enabled();
    if enabled {
        let _ = arch::learn::maybe_print_first_run_notice();
    }
    let result = f();
    if !enabled {
        return result;
    }
    match &result {
        Ok(()) => {
            for file in files {
                let path_str = file.display().to_string();
                if let Ok(src) = fs::read_to_string(file) {
                    if let Ok(Some(ev)) = arch::learn::record_success_if_pending(&path_str, &src) {
                        eprintln!("📚 Learned: [{}] {}", ev.error_code, ev.diff_summary);
                    }
                }
            }
        }
        Err(report) => {
            let msg = format!("{:?}", report);
            let code = arch::learn::classify_error(&msg);
            for file in files {
                let path_str = file.display().to_string();
                if let Ok(src) = fs::read_to_string(file) {
                    let _ = arch::learn::record_failure(&path_str, &code, &msg, &src);
                }
            }
            // Inline suggestion: if the local store has similar past fixes,
            // tell the user. `peek` does not bump retrieval counters.
            let query = format!("{} {}", code, msg);
            if let Ok(hits) = arch::learn::peek(&query, 3) {
                if !hits.is_empty() {
                    let suggest = hits[0].event.error_code.clone();
                    eprintln!(
                        "💡 arch advise found {} similar past fix{} — run `arch advise \"{}\"` to see them.",
                        hits.len(),
                        if hits.len() == 1 { "" } else { "es" },
                        suggest,
                    );
                }
            }
        }
    }
    result
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { files } => learn_wrap(&files, || {
            let all_files = resolve_use_imports(&files)?;
            let ms = MultiSource::from_files(&all_files)?;
            run_check_multi(&ms)?;
            eprintln!("OK: no errors");
            Ok(())
        }),
        Command::LearnIndex => {
            let n = arch::learn::build_index().into_diagnostic()?;
            eprintln!("Indexed {} events.", n);
            Ok(())
        }
        Command::Advise {
            query,
            top,
            from_stderr,
            feature,
        } => {
            let mut q = query.join(" ");
            if from_stderr {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .into_diagnostic()?;
                if !buf.trim().is_empty() {
                    if !q.is_empty() {
                        q.push(' ');
                    }
                    q.push_str(buf.trim());
                }
            }
            if q.trim().is_empty() {
                eprintln!("error: empty query (pass a query string or pipe via --from-stderr)");
                std::process::exit(2);
            }
            // Pull more candidates than `top` when --feature is set so we
            // can filter to feature events without starving the result set.
            let pool_size = if feature { top.max(1) * 8 } else { top };
            let matches = arch::learn::advise(&q, pool_size).into_diagnostic()?;
            let matches: Vec<_> = if feature {
                matches
                    .into_iter()
                    .filter(|m| m.event.kind == "feature")
                    .take(top)
                    .collect()
            } else {
                matches
                    .into_iter()
                    .filter(|m| m.event.kind != "feature")
                    .take(top)
                    .collect()
            };
            if matches.is_empty() {
                eprintln!("No matches.");
                return Ok(());
            }
            for (i, m) in matches.iter().enumerate() {
                println!(
                    "── match #{} (score {:.3}, retrieved {}×) ──────────────────────",
                    i + 1,
                    m.score,
                    m.retrieved_count
                );
                if m.event.kind == "feature" {
                    // Feature event: file::construct + truncated doc snippet.
                    println!("  kind:      {}", m.event.error_code);
                    println!("  construct: {}", m.event.diff_summary);
                    println!("  file:      {}", m.event.file_path);
                    let snippet: String = m.event.error_message.chars().take(240).collect();
                    let truncated = m.event.error_message.chars().count() > 240;
                    println!(
                        "  doc:       {}{}",
                        snippet.replace('\n', " "),
                        if truncated { " …" } else { "" }
                    );
                } else {
                    println!("  code:    {}", m.event.error_code);
                    println!("  message: {}", m.event.error_message);
                    println!("  file:    {}", m.event.file_path);
                    println!("  diff:    {}", m.event.diff_summary);
                }
                println!();
            }
            Ok(())
        }
        Command::LearnStats => {
            arch::learn::print_stats().into_diagnostic()?;
            Ok(())
        }
        Command::LearnBootstrap { path } => run_learn_bootstrap(&path),
        Command::Graph { command } => match command {
            GraphCommand::Index {
                inputs,
                root,
                out,
                clean,
            } => run_graph_index(&inputs, root.as_deref(), &out, clean),
            GraphCommand::Query {
                query,
                index,
                json,
                limit,
            } => run_graph_query(&query, &index, json, limit),
            GraphCommand::Callers {
                target,
                index,
                json,
                limit,
            } => run_graph_callers(&target, &index, json, limit),
            GraphCommand::Impact {
                symbol,
                depth,
                index,
                json,
                limit,
            } => run_graph_impact(&symbol, depth, &index, json, limit),
            GraphCommand::Context {
                task,
                index,
                json,
                limit,
            } => run_graph_context(&task, &index, json, limit),
            GraphCommand::Html { index, out, title } => {
                run_graph_html(&index, &out, title.as_deref())
            }
        },
        Command::Coverage { command } => match command {
            CoverageCommand::Merge { inputs, out } => run_coverage_merge(&inputs, &out),
        },
        Command::Ops { markdown } => {
            if markdown {
                print!("{}", arch::pipelined_ops::format_markdown_table());
            } else {
                print!("{}", arch::pipelined_ops::format_text_table());
            }
            Ok(())
        }
        Command::LearnClear => {
            arch::learn::clear_store().into_diagnostic()?;
            eprintln!("Cleared ~/.arch/learn/");
            Ok(())
        }
        Command::LearnPrune {
            code,
            contains,
            older_than_days,
            dry_run,
        } => {
            if code.is_none() && contains.is_none() && older_than_days.is_none() {
                eprintln!("error: specify at least one of --code / --contains / --older-than-days");
                std::process::exit(2);
            }
            let (kept, removed) = arch::learn::prune(
                code.as_deref(),
                contains.as_deref(),
                older_than_days,
                dry_run,
            )
            .into_diagnostic()?;
            if dry_run {
                eprintln!("Would remove {} events; {} would remain.", removed, kept);
            } else {
                eprintln!(
                    "Removed {} events; {} remain. Run `arch learn-index` to refresh the index.",
                    removed, kept
                );
            }
            Ok(())
        }
        Command::Sim {
            arch_files,
            tb_files,
            outdir,
            check_uninit,
            inputs_start_uninit,
            check_uninit_ram,
            cdc_random,
            wave,
            debug,
            debug_depth,
            debug_fsm,
            coverage,
            coverage_dat,
            thread_sim,
            threads,
            pybind,
            test,
            pybind_module_name,
            auto_thread_asserts,
            param_overrides,
            fp_compat,
        } => {
            let _ = auto_thread_asserts;
            let fp_compat = arch::FpCompat::parse(&fp_compat).map_err(|e| miette::miette!(e))?;

            // Parse --param NAME=VALUE overrides
            let mut param_overrides_map: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for ov in &param_overrides {
                let (name, val_str) = ov
                    .split_once('=')
                    .ok_or_else(|| miette::miette!("--param: expected NAME=VALUE, got '{ov}'"))?;
                let val: u64 = val_str.parse().map_err(|_| {
                    miette::miette!("--param: value must be an integer, got '{val_str}' in '{ov}'")
                })?;
                param_overrides_map.insert(name.to_string(), val);
            }

            let dbg_ports = debug || debug_fsm; // any debug option implies port logging
                                                // --inputs-start-uninit and --check-uninit-ram both imply --check-uninit
            let check_uninit = check_uninit || inputs_start_uninit || check_uninit_ram;
            // --coverage-dat resolves to a path: explicit --coverage-dat=foo
            // → Some(Some("foo")) → "foo"; bare --coverage-dat
            // → Some(None) → default "coverage.dat"; absent → None.
            let cov_dat_path: Option<String> =
                coverage_dat.map(|opt| opt.unwrap_or_else(|| "coverage.dat".to_string()));
            let coverage = coverage || cov_dat_path.is_some();
            if threads > 1 && thread_sim != "parallel" {
                return Err(miette::miette!(
                    "--threads N (N>1) requires --thread-sim parallel"
                ));
            }
            match thread_sim.as_str() {
                "fsm" => learn_wrap(&arch_files, || {
                    run_sim(
                        &arch_files,
                        &tb_files,
                        outdir.as_deref(),
                        check_uninit,
                        inputs_start_uninit,
                        check_uninit_ram,
                        cdc_random,
                        wave.as_deref(),
                        dbg_ports,
                        debug_depth,
                        debug_fsm,
                        coverage,
                        cov_dat_path.clone(),
                        false,
                        threads,
                        pybind,
                        test.as_deref(),
                        pybind_module_name.as_deref(),
                        &param_overrides_map,
                        fp_compat,
                    )
                }),
                "parallel" => learn_wrap(&arch_files, || {
                    run_sim(
                        &arch_files,
                        &tb_files,
                        outdir.as_deref(),
                        check_uninit,
                        inputs_start_uninit,
                        check_uninit_ram,
                        cdc_random,
                        wave.as_deref(),
                        dbg_ports,
                        debug_depth,
                        debug_fsm,
                        coverage,
                        cov_dat_path.clone(),
                        true,
                        threads,
                        pybind,
                        test.as_deref(),
                        pybind_module_name.as_deref(),
                        &param_overrides_map,
                        fp_compat,
                    )
                }),
                "both" => {
                    // Cross-check: build + run both fsm and parallel sims
                    // independently with --debug, then diff the port-change
                    // traces. Mismatch ⇒ abort with first divergence.
                    run_thread_sim_cross_check(&arch_files, &tb_files, outdir.as_deref(), fp_compat)
                }
                other => {
                    return Err(miette::miette!(
                        "--thread-sim: expected `fsm`, `parallel`, or `both`, got `{}`",
                        other
                    ))
                }
            }
        }
        Command::Build {
            files,
            o,
            emit_thread_map,
            emit_thread_proof,
            emit_thread_proof_lean,
            check_thread_proof_lean,
            thread_proof_lean_project,
            emit_construct_proof_lean,
            check_construct_proof_lean,
            construct_proof_lean_project,
            emit_construct_proof_smt,
            check_construct_proof_smt,
            construct_proof_smt_solver,
            auto_thread_asserts,
            no_inline_deps,
            fp_compat,
        } => {
            let fp_compat = arch::FpCompat::parse(&fp_compat).map_err(|e| miette::miette!(e))?;
            let files_for_learn = files.clone();
            learn_wrap(&files_for_learn, move || {
                if matches!(emit_thread_map, Some(Some(_))) && files.len() > 1 && o.is_none() {
                    return Err(miette::miette!(
                    "--emit-thread-map=PATH requires a single combined build output; pass -o or use bare --emit-thread-map for per-file maps"
                ));
                }
                if matches!(emit_thread_proof, Some(Some(_))) && files.len() > 1 && o.is_none() {
                    return Err(miette::miette!(
                    "--emit-thread-proof=PATH requires a single combined build output; pass -o or use bare --emit-thread-proof for per-file certificates"
                ));
                }
                if matches!(emit_thread_proof_lean, Some(Some(_))) && files.len() > 1 && o.is_none()
                {
                    return Err(miette::miette!(
                    "--emit-thread-proof-lean=PATH requires a single combined build output; pass -o or use bare --emit-thread-proof-lean for per-file Lean certificates"
                ));
                }
                if matches!(emit_construct_proof_lean, Some(Some(_)))
                    && files.len() > 1
                    && o.is_none()
                {
                    return Err(miette::miette!(
                    "--emit-construct-proof-lean=PATH requires a single combined build output; pass -o or use bare --emit-construct-proof-lean for per-file Lean certificates"
                ));
                }
                if matches!(emit_construct_proof_smt, Some(Some(_)))
                    && files.len() > 1
                    && o.is_none()
                {
                    return Err(miette::miette!(
                    "--emit-construct-proof-smt=PATH requires a single combined build output; pass -o or use bare --emit-construct-proof-smt for per-file SMT certificates"
                ));
                }
                let need_thread_proof_lean =
                    emit_thread_proof_lean.is_some() || check_thread_proof_lean;
                let need_construct_proof_lean =
                    emit_construct_proof_lean.is_some() || check_construct_proof_lean;
                let need_construct_proof_smt =
                    emit_construct_proof_smt.is_some() || check_construct_proof_smt;
                let all_files = resolve_use_imports(&files)?;
                let ms = MultiSource::from_files(&all_files)?;
                let collect_thread_metadata = emit_thread_map.is_some()
                    || emit_thread_proof.is_some()
                    || need_thread_proof_lean;
                let thread_map_store = collect_thread_metadata.then(|| {
                    std::rc::Rc::new(std::cell::RefCell::new(
                        arch::thread_map::ThreadMap::default(),
                    ))
                });
                let (mut ast, symbols, overload_map) = run_check_multi_opts_with_thread_map(
                    &ms,
                    false,
                    auto_thread_asserts,
                    thread_map_store.clone(),
                )?;
                lower_pipelined_calls_before_codegen(&mut ast, &ms)?;
                let ast = ast;
                let thread_map_sources = thread_map_sources_from_multi(&ms);

                let comments = lexer::extract_comments(&ms.combined);

                // --no-inline-deps: only emit constructs from the original
                // input files, not from auto-discovered dependency files.
                let original_names: std::collections::HashSet<String> = if no_inline_deps {
                    files
                        .iter()
                        .map(|f| f.to_string_lossy().to_string())
                        .collect()
                } else {
                    std::collections::HashSet::new()
                };

                if files.len() == 1 || o.is_some() {
                    // Single file or explicit -o: emit one combined SV file
                    let (sv, sdc) = if no_inline_deps {
                        // Only items from the original input files
                        let file_items: Vec<_> = ast
                            .items
                            .iter()
                            .filter(|item| {
                                let s = item.span().start;
                                ms.segments.iter().any(|(seg_start, seg_end, fname, _)| {
                                    s >= *seg_start
                                        && s < *seg_end
                                        && original_names.contains(fname)
                                })
                            })
                            .cloned()
                            .collect();
                        let mut codegen = Codegen::new(&symbols, &ast, overload_map)
                            .with_comments(comments)
                            .with_fp_compat(fp_compat);
                        let sv = codegen.generate_items(&file_items);
                        let out_path_hint =
                            o.clone().unwrap_or_else(|| files[0].with_extension("sv"));
                        let sdc = codegen.emit_sdc(&out_path_hint.to_string_lossy());
                        (sv, sdc)
                    } else {
                        let mut codegen = Codegen::new(&symbols, &ast, overload_map)
                            .with_comments(comments)
                            .with_fp_compat(fp_compat);
                        let sv = codegen.generate();
                        let out_path_hint =
                            o.clone().unwrap_or_else(|| files[0].with_extension("sv"));
                        let sdc = codegen.emit_sdc(&out_path_hint.to_string_lossy());
                        (sv, sdc)
                    };
                    let out_path = o.unwrap_or_else(|| files[0].with_extension("sv"));
                    fs::write(&out_path, &sv).into_diagnostic()?;
                    eprintln!("Wrote {}", out_path.display());
                    if let (Some(req), Some(map_store)) = (&emit_thread_map, &thread_map_store) {
                        let html_path = req
                            .clone()
                            .unwrap_or_else(|| out_path.with_extension("thread.html"));
                        let map = if no_inline_deps {
                            let keep: Vec<(usize, usize)> = ms
                                .segments
                                .iter()
                                .filter_map(|(start, end, filename, _)| {
                                    if original_names.contains(filename) {
                                        Some((*start, *end))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            filter_thread_map_by_ranges(&map_store.borrow(), &keep)
                        } else {
                            map_store.borrow().clone()
                        };
                        let html = arch::thread_map::render_html(
                            &map,
                            &thread_map_sources,
                            &format!("Thread map for {}", out_path.display()),
                        );
                        fs::write(&html_path, html).into_diagnostic()?;
                        eprintln!("Wrote {}", html_path.display());
                    }
                    if let (Some(req), Some(map_store)) = (&emit_thread_proof, &thread_map_store) {
                        let proof_path = req
                            .clone()
                            .unwrap_or_else(|| out_path.with_extension("thread-proof.json"));
                        let map = if no_inline_deps {
                            let keep: Vec<(usize, usize)> = ms
                                .segments
                                .iter()
                                .filter_map(|(start, end, filename, _)| {
                                    if original_names.contains(filename) {
                                        Some((*start, *end))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            filter_thread_map_by_ranges(&map_store.borrow(), &keep)
                        } else {
                            map_store.borrow().clone()
                        };
                        let json = arch::thread_proof_cert::render_json(&map);
                        fs::write(&proof_path, json).into_diagnostic()?;
                        eprintln!("Wrote {}", proof_path.display());
                    }
                    if need_thread_proof_lean {
                        let map_store = thread_map_store
                            .as_ref()
                            .expect("thread map store must exist for Lean proof emission");
                        let proof_path = emit_thread_proof_lean
                            .as_ref()
                            .and_then(|req| req.clone())
                            .unwrap_or_else(|| out_path.with_extension("thread-proof.lean"));
                        let map = if no_inline_deps {
                            let keep: Vec<(usize, usize)> = ms
                                .segments
                                .iter()
                                .filter_map(|(start, end, filename, _)| {
                                    if original_names.contains(filename) {
                                        Some((*start, *end))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            filter_thread_map_by_ranges(&map_store.borrow(), &keep)
                        } else {
                            map_store.borrow().clone()
                        };
                        let lean =
                            arch::thread_proof_cert::render_lean_checked(&map).map_err(|err| {
                                miette::miette!("thread proof Lean emission failed: {err}")
                            })?;
                        fs::write(&proof_path, lean).into_diagnostic()?;
                        eprintln!("Wrote {}", proof_path.display());
                        if check_thread_proof_lean {
                            check_thread_proof_lean_file(
                                &proof_path,
                                thread_proof_lean_project.as_deref(),
                            )?;
                        }
                    }
                    if need_construct_proof_lean {
                        let proof_path = emit_construct_proof_lean
                            .as_ref()
                            .and_then(|req| req.clone())
                            .unwrap_or_else(|| out_path.with_extension("construct-proof.lean"));
                        let construct_items: Vec<_> = if no_inline_deps {
                            let keep: Vec<(usize, usize)> = ms
                                .segments
                                .iter()
                                .filter_map(|(start, end, filename, _)| {
                                    if original_names.contains(filename) {
                                        Some((*start, *end))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            ast.items
                                .iter()
                                .filter(|item| {
                                    let s = item.span().start;
                                    keep.iter()
                                        .any(|(seg_start, seg_end)| s >= *seg_start && s < *seg_end)
                                })
                                .cloned()
                                .collect()
                        } else {
                            ast.items.clone()
                        };
                        let lean = arch::construct_proof_cert::render_lean_checked_items(
                            construct_items.iter(),
                        )
                        .map_err(|err| {
                            miette::miette!("construct proof Lean emission failed: {err}")
                        })?;
                        fs::write(&proof_path, lean).into_diagnostic()?;
                        eprintln!("Wrote {}", proof_path.display());
                        if check_construct_proof_lean {
                            check_construct_proof_lean_file(
                                &proof_path,
                                construct_proof_lean_project.as_deref(),
                            )?;
                        }
                    }
                    if need_construct_proof_smt {
                        let proof_path = emit_construct_proof_smt
                            .as_ref()
                            .and_then(|req| req.clone())
                            .unwrap_or_else(|| out_path.with_extension("construct-proof.smt2"));
                        let construct_items: Vec<_> = if no_inline_deps {
                            let keep: Vec<(usize, usize)> = ms
                                .segments
                                .iter()
                                .filter_map(|(start, end, filename, _)| {
                                    if original_names.contains(filename) {
                                        Some((*start, *end))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            ast.items
                                .iter()
                                .filter(|item| {
                                    let s = item.span().start;
                                    keep.iter()
                                        .any(|(seg_start, seg_end)| s >= *seg_start && s < *seg_end)
                                })
                                .cloned()
                                .collect()
                        } else {
                            ast.items.clone()
                        };
                        let smt = arch::construct_proof_cert::render_smt2_checked_items(
                            construct_items.iter(),
                        )
                        .map_err(|err| {
                            miette::miette!("construct proof SMT emission failed: {err}")
                        })?;
                        fs::write(&proof_path, smt).into_diagnostic()?;
                        eprintln!("Wrote {}", proof_path.display());
                        if check_construct_proof_smt {
                            check_construct_proof_smt_file(
                                &proof_path,
                                &construct_proof_smt_solver,
                            )?;
                        }
                    }
                    // Companion .sdc file: only written if any module contained
                    // a `multicycle <N>` reg. No-op for legacy `.arch` sources.
                    if let Some(sdc_text) = sdc {
                        let sdc_path = out_path.with_extension("sdc");
                        fs::write(&sdc_path, &sdc_text).into_diagnostic()?;
                        eprintln!("Wrote {}", sdc_path.display());
                    }
                } else {
                    // Multi-file: emit one .sv per .arch input file
                    for (seg_start, seg_end, filename, _) in &ms.segments {
                        // --no-inline-deps: skip dependency files not in the original input set
                        if no_inline_deps && !original_names.contains(filename.as_str()) {
                            continue;
                        }
                        // Collect items whose span falls within this file's segment
                        let file_items: Vec<_> = ast
                            .items
                            .iter()
                            .filter(|item| {
                                let s = item.span().start;
                                s >= *seg_start && s < *seg_end
                            })
                            .cloned()
                            .collect();

                        if file_items.is_empty() {
                            continue; // skip domain-only files etc. that produce no SV
                        }

                        // Filter comments belonging to this file's segment
                        let file_comments: Vec<_> = comments
                            .iter()
                            .filter(|(span, _)| span.start >= *seg_start && span.start < *seg_end)
                            .cloned()
                            .collect();

                        let mut codegen = Codegen::new(&symbols, &ast, overload_map.clone())
                            .with_comments(file_comments)
                            .with_fp_compat(fp_compat);
                        let sv = codegen.generate_items(&file_items);

                        let out_path = std::path::Path::new(filename).with_extension("sv");
                        fs::write(&out_path, &sv).into_diagnostic()?;
                        eprintln!("Wrote {}", out_path.display());
                        if let (Some(None), Some(map_store)) = (&emit_thread_map, &thread_map_store)
                        {
                            let map = filter_thread_map_by_ranges(
                                &map_store.borrow(),
                                &[(*seg_start, *seg_end)],
                            );
                            let html_path = out_path.with_extension("thread.html");
                            let html = arch::thread_map::render_html(
                                &map,
                                &thread_map_sources,
                                &format!("Thread map for {}", out_path.display()),
                            );
                            fs::write(&html_path, html).into_diagnostic()?;
                            eprintln!("Wrote {}", html_path.display());
                        }
                        if let (Some(None), Some(map_store)) =
                            (&emit_thread_proof, &thread_map_store)
                        {
                            let map = filter_thread_map_by_ranges(
                                &map_store.borrow(),
                                &[(*seg_start, *seg_end)],
                            );
                            let proof_path = out_path.with_extension("thread-proof.json");
                            let json = arch::thread_proof_cert::render_json(&map);
                            fs::write(&proof_path, json).into_diagnostic()?;
                            eprintln!("Wrote {}", proof_path.display());
                        }
                        if need_thread_proof_lean {
                            let map_store = thread_map_store
                                .as_ref()
                                .expect("thread map store must exist for Lean proof emission");
                            let map = filter_thread_map_by_ranges(
                                &map_store.borrow(),
                                &[(*seg_start, *seg_end)],
                            );
                            let proof_path = out_path.with_extension("thread-proof.lean");
                            let lean = arch::thread_proof_cert::render_lean_checked(&map).map_err(
                                |err| miette::miette!("thread proof Lean emission failed: {err}"),
                            )?;
                            fs::write(&proof_path, lean).into_diagnostic()?;
                            eprintln!("Wrote {}", proof_path.display());
                            if check_thread_proof_lean {
                                check_thread_proof_lean_file(
                                    &proof_path,
                                    thread_proof_lean_project.as_deref(),
                                )?;
                            }
                        }
                        if need_construct_proof_lean {
                            let proof_path = out_path.with_extension("construct-proof.lean");
                            let lean = arch::construct_proof_cert::render_lean_checked_items(
                                file_items.iter(),
                            )
                            .map_err(|err| {
                                miette::miette!(
                                    "construct proof Lean emission failed for {}: {err}",
                                    filename
                                )
                            })?;
                            fs::write(&proof_path, lean).into_diagnostic()?;
                            eprintln!("Wrote {}", proof_path.display());
                            if check_construct_proof_lean {
                                check_construct_proof_lean_file(
                                    &proof_path,
                                    construct_proof_lean_project.as_deref(),
                                )?;
                            }
                        }
                        if need_construct_proof_smt {
                            let proof_path = out_path.with_extension("construct-proof.smt2");
                            let smt = arch::construct_proof_cert::render_smt2_checked_items(
                                file_items.iter(),
                            )
                            .map_err(|err| {
                                miette::miette!(
                                    "construct proof SMT emission failed for {}: {err}",
                                    filename
                                )
                            })?;
                            fs::write(&proof_path, smt).into_diagnostic()?;
                            eprintln!("Wrote {}", proof_path.display());
                            if check_construct_proof_smt {
                                check_construct_proof_smt_file(
                                    &proof_path,
                                    &construct_proof_smt_solver,
                                )?;
                            }
                        }
                        // Companion .sdc per-file: only if this file's items
                        // declared `multicycle <N>` regs.
                        if let Some(sdc_text) = codegen.emit_sdc(&out_path.to_string_lossy()) {
                            let sdc_path = out_path.with_extension("sdc");
                            fs::write(&sdc_path, &sdc_text).into_diagnostic()?;
                            eprintln!("Wrote {}", sdc_path.display());
                        }
                    }
                }

                // Emit .archi interface files alongside .sv (for separate compilation)
                for item in &ast.items {
                    // Don't re-emit .archi for an interface stub we just
                    // loaded — we'd be overwriting the source file we
                    // read. Covers module + every ConstructCommon-bearing
                    // variant via `Item::is_interface`.
                    if item.is_interface() {
                        continue;
                    }
                    if let Some(content) = arch::interface::emit_interface(item) {
                        let name = &item.as_construct().name().name;
                        // Write .archi next to the .sv output
                        let archi_dir = files[0]
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf();
                        let archi_path = archi_dir.join(format!("{name}.archi"));
                        fs::write(&archi_path, &content).into_diagnostic()?;
                        eprintln!("Wrote {}", archi_path.display());
                    }
                }

                Ok(())
            })
        }
        Command::Formal {
            files,
            top,
            bound,
            solver,
            emit_smt,
            emit_thread_proof_lean,
            check_thread_proof_lean,
            thread_proof_lean_project,
            thread_proof_only,
            timeout,
            auto_thread_asserts,
            fp_compat,
        } => {
            // Validated for parity with build/sim; FP types are rejected by the
            // formal backend in v1, so the profile has no effect here yet.
            let _ = arch::FpCompat::parse(&fp_compat).map_err(|e| miette::miette!(e))?;
            let files_for_learn = files.clone();
            learn_wrap(&files_for_learn, move || {
                let all_files = resolve_use_imports(&files)?;
                let ms = MultiSource::from_files(&all_files)?;
                let need_thread_proof_lean =
                    emit_thread_proof_lean.is_some() || check_thread_proof_lean;
                if thread_proof_only && !need_thread_proof_lean {
                    return Err(miette::miette!(
                        "--thread-proof-only requires --emit-thread-proof-lean or --check-thread-proof-lean"
                    ));
                }
                if thread_proof_only && emit_smt.is_some() {
                    return Err(miette::miette!(
                        "--thread-proof-only skips SMT-LIB2 generation; remove --emit-smt"
                    ));
                }
                let thread_map_store = need_thread_proof_lean.then(|| {
                    std::rc::Rc::new(std::cell::RefCell::new(
                        arch::thread_map::ThreadMap::default(),
                    ))
                });
                let (ast, symbols, _overload_map) = run_check_multi_opts_with_thread_map(
                    &ms,
                    false,
                    auto_thread_asserts,
                    thread_map_store.clone(),
                )?;
                if need_thread_proof_lean {
                    let map_store = thread_map_store
                        .as_ref()
                        .expect("thread map store must exist for Lean proof emission");
                    let proof_path = emit_thread_proof_lean
                        .as_ref()
                        .and_then(|req| req.clone())
                        .unwrap_or_else(|| files[0].with_extension("thread-proof.lean"));
                    let lean = arch::thread_proof_cert::render_lean_checked(&map_store.borrow())
                        .map_err(|err| {
                            miette::miette!("thread proof Lean emission failed: {err}")
                        })?;
                    fs::write(&proof_path, lean).into_diagnostic()?;
                    eprintln!("Wrote {}", proof_path.display());
                    if check_thread_proof_lean {
                        check_thread_proof_lean_file(
                            &proof_path,
                            thread_proof_lean_project.as_deref(),
                        )?;
                    }
                }
                if thread_proof_only {
                    return Ok(());
                }

                let args = formal::FormalArgs {
                    top: top.clone(),
                    bound,
                    solver: solver.clone(),
                    emit_smt: emit_smt.clone(),
                    timeout,
                };
                let report =
                    formal::run(&ast, &symbols, &args).map_err(|err| ms.report_error(err))?;
                std::process::exit(report.exit_code());
            })
        }
    }
}

/// Default sim build/output directory used when neither `--outdir` nor an
/// explicit path is supplied. Honors the `ARCH_SIM_BUILD_DIR` env var so
/// callers can route build artifacts to a scratch location instead of
/// dropping `arch_sim_build/` in the current directory; falls back to
/// `arch_sim_build` (relative to cwd) when the var is unset or empty.
fn default_sim_build_dir() -> PathBuf {
    std::env::var_os("ARCH_SIM_BUILD_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("arch_sim_build"))
}

/// `--thread-sim both` driver: build + run fsm and parallel sims
/// independently with --debug, then diff their port-change traces.
/// Mismatch ⇒ abort with the first divergence highlighted.
fn run_thread_sim_cross_check(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
    fp_compat: arch::FpCompat,
) -> miette::Result<()> {
    let base = outdir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_sim_build_dir);
    let fsm_dir = base.with_file_name(format!(
        "{}_fsm",
        base.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("arch_sim_build")
    ));
    let par_dir = base.with_file_name(format!(
        "{}_par",
        base.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("arch_sim_build")
    ));

    eprintln!("=== arch sim --thread-sim both: building fsm path ===");
    let fsm_trace = build_and_capture(
        arch_files, tb_files, &fsm_dir, /*parallel=*/ false, fp_compat,
    )?;
    eprintln!("=== arch sim --thread-sim both: building parallel path ===");
    let par_trace = build_and_capture(
        arch_files, tb_files, &par_dir, /*parallel=*/ true, fp_compat,
    )?;

    // Filter to just the [cycle][Mod.port](in/out) debug lines, ignore
    // TB stdout. The fsm path uses --debug --depth N to optionally
    // include sub-instance traces; we only invoke with depth=1 above,
    // so only top-module ports appear in either trace. (For cross-check
    // we want top-module observable behavior — sub-module internals
    // are implementation detail and may legitimately differ.)
    let extract_trace = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| l.starts_with('[') && (l.contains("](in)") || l.contains("](out)")))
            .map(|l| l.to_string())
            .collect()
    };
    let fsm_lines = extract_trace(&fsm_trace);
    let par_lines = extract_trace(&par_trace);

    if fsm_lines == par_lines {
        eprintln!(
            "=== Cross-check PASS: {} port-change events match ===",
            fsm_lines.len()
        );
        return Ok(());
    }

    eprintln!("=== Cross-check FAIL: traces diverge ===");
    let n = fsm_lines.len().max(par_lines.len());
    let empty = String::new();
    for i in 0..n {
        let f = fsm_lines.get(i).unwrap_or(&empty);
        let p = par_lines.get(i).unwrap_or(&empty);
        if f != p {
            eprintln!("  first divergence at event #{}:", i);
            eprintln!("    fsm:      {}", f);
            eprintln!("    parallel: {}", p);
            return Err(miette::miette!("--thread-sim both: cross-check failed"));
        }
    }
    Err(miette::miette!(
        "--thread-sim both: trace lengths differ ({} fsm vs {} parallel)",
        fsm_lines.len(),
        par_lines.len()
    ))
}

/// Helper for run_thread_sim_cross_check: build a sim binary in `dir`
/// (fsm or parallel mode), run it, capture its stdout.
fn build_and_capture(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    dir: &std::path::Path,
    parallel: bool,
    fp_compat: arch::FpCompat,
) -> miette::Result<String> {
    // Capture stdout via a temp file: redirect the child's stdout to it,
    // then read it back. Easier than threading capture through run_sim.
    let stdout_path = dir.with_extension("trace.txt");
    // Ensure clean dir + previous trace.
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_file(&stdout_path);
    fs::create_dir_all(dir).into_diagnostic()?;

    // Generate models + verilated stubs into dir.
    run_sim_opts(
        arch_files,
        tb_files,
        Some(dir),
        /*check_uninit*/ false,
        /*inputs_start_uninit*/ false,
        /*check_uninit_ram*/ false,
        /*cdc_random*/ false,
        /*wave*/ None,
        /*debug*/ true,
        /*debug_depth*/ 1,
        /*debug_fsm*/ false,
        /*coverage*/ false,
        /*coverage_dat*/ None,
        parallel,
        /*threads*/ 1,
        /*pybind*/ false,
        /*test_file*/ None,
        /*pybind_module_name_override*/ None,
        /*no_exit*/ true,
        &std::collections::HashMap::new(),
        fp_compat,
    )?;

    // The sim_out binary was already executed by run_sim; capture its
    // stdout would require modifying run_sim. For now, run the binary
    // a second time and capture this run.
    let sim_bin = dir.join("sim_out");
    let output = std::process::Command::new(&sim_bin)
        .output()
        .into_diagnostic()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[allow(clippy::too_many_arguments)]
fn run_sim(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
    check_uninit: bool,
    inputs_start_uninit: bool,
    check_uninit_ram: bool,
    cdc_random: bool,
    wave: Option<&std::path::Path>,
    debug: bool,
    debug_depth: u32,
    debug_fsm: bool,
    coverage: bool,
    coverage_dat: Option<String>,
    thread_sim_parallel: bool,
    threads: u32,
    pybind: bool,
    test_file: Option<&std::path::Path>,
    pybind_module_name_override: Option<&str>,
    param_overrides: &std::collections::HashMap<String, u64>,
    fp_compat: arch::FpCompat,
) -> miette::Result<()> {
    run_sim_opts(
        arch_files,
        tb_files,
        outdir,
        check_uninit,
        inputs_start_uninit,
        check_uninit_ram,
        cdc_random,
        wave,
        debug,
        debug_depth,
        debug_fsm,
        coverage,
        coverage_dat,
        thread_sim_parallel,
        threads,
        pybind,
        test_file,
        pybind_module_name_override,
        /*no_exit=*/ false,
        param_overrides,
        fp_compat,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_sim_opts(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
    check_uninit: bool,
    inputs_start_uninit: bool,
    check_uninit_ram: bool,
    cdc_random: bool,
    wave: Option<&std::path::Path>,
    debug: bool,
    debug_depth: u32,
    debug_fsm: bool,
    coverage: bool,
    coverage_dat: Option<String>,
    thread_sim_parallel: bool,
    threads: u32,
    pybind: bool,
    test_file: Option<&std::path::Path>,
    pybind_module_name_override: Option<&str>,
    no_exit: bool,
    param_overrides: &std::collections::HashMap<String, u64>,
    fp_compat: arch::FpCompat,
) -> miette::Result<()> {
    // 1. Parse + type-check
    let all_files = resolve_use_imports(arch_files)?;
    let ms = MultiSource::from_files(&all_files)?;
    let (mut ast, symbols, overload_map) = if param_overrides.is_empty() {
        run_check_multi_opts(
            &ms,
            thread_sim_parallel,
            /*auto_thread_asserts=*/ false,
        )?
    } else {
        run_check_multi_opts_with_param_overrides(
            &ms,
            thread_sim_parallel,
            /*auto_thread_asserts=*/ false,
            param_overrides,
        )?
    };
    lower_pipelined_calls_before_codegen(&mut ast, &ms)?;
    let ast = ast;

    // 2. Set up output directory
    let build_dir = outdir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_sim_build_dir);
    fs::create_dir_all(&build_dir).into_diagnostic()?;

    // 3. Generate C++ models
    let models: Vec<arch::sim_codegen::SimModel> = if thread_sim_parallel {
        // Pre-lowering thread sim path: route every module containing
        // a `thread` block through the new emitter. TLM lowering can
        // consume an initiator cohort completely before this point,
        // leaving an ordinary no-thread module; emit those modules with
        // the regular sim codegen so `--thread-sim parallel` remains a
        // usable integration mode for TLM designs.
        let mut out = Vec::new();
        let mut regular_items = Vec::new();
        let mut thread_sim_warnings: Vec<arch::diagnostics::CompileWarning> = Vec::new();
        for item in &ast.items {
            match item {
                arch::ast::Item::Module(m) => {
                    let has_thread = m
                        .body
                        .iter()
                        .any(|i| matches!(i, arch::ast::ModuleBodyItem::Thread(_)));
                    if has_thread {
                        let model = arch::sim_codegen::thread_sim::gen_module_thread_with_warnings(
                            m,
                            debug,
                            wave.is_some(),
                            threads,
                            &mut thread_sim_warnings,
                        )
                        .map_err(|e| miette::miette!("thread sim: {}", e))?;
                        out.push(model);
                    } else {
                        regular_items.push(item.clone());
                    }
                }
                _ => regular_items.push(item.clone()),
            }
        }
        // Surface --thread-sim-specific warnings on the same stderr path
        // as typecheck warnings so users running `arch sim --thread-sim`
        // see them without having to consult docs.
        for w in &thread_sim_warnings {
            let (filename, _, local_offset) = ms.locate(w.span.start);
            eprintln!("warning: {} ({}:{})", w.message, filename, local_offset);
        }
        if regular_items.iter().any(|item| {
            matches!(
                item,
                arch::ast::Item::Module(_)
                    | arch::ast::Item::Fsm(_)
                    | arch::ast::Item::Fifo(_)
                    | arch::ast::Item::Ram(_)
                    | arch::ast::Item::Cam(_)
                    | arch::ast::Item::Counter(_)
                    | arch::ast::Item::Arbiter(_)
                    | arch::ast::Item::Regfile(_)
                    | arch::ast::Item::Pipeline(_)
                    | arch::ast::Item::Synchronizer(_)
                    | arch::ast::Item::Clkgate(_)
                    | arch::ast::Item::Function(_)
                    | arch::ast::Item::Package(_)
                    | arch::ast::Item::Struct(_)
                    | arch::ast::Item::Enum(_)
            )
        }) {
            let regular_ast = arch::ast::SourceFile {
                items: regular_items,
                inner_doc: ast.inner_doc.clone(),
                frontmatter: ast.frontmatter.clone(),
            };
            let mut sim = SimCodegen::new(&symbols, &regular_ast, overload_map.clone())
                .check_uninit(check_uninit)
                .inputs_start_uninit(inputs_start_uninit)
                .check_uninit_ram(check_uninit_ram)
                .cdc_random(cdc_random)
                .debug(debug, debug_depth)
                .with_debug_fsm(debug_fsm)
                .coverage(coverage)
                .coverage_dat(coverage_dat.clone());
            if coverage {
                let segs: Vec<(usize, String, String)> = ms
                    .segments
                    .iter()
                    .map(|(start, _end, name, src)| (*start, name.clone(), src.clone()))
                    .collect();
                sim = sim.with_source_map(arch::sim_codegen::SourceMap::new(segs));
            }
            for model in sim.generate() {
                if model.class_name == "VStructs" && out.iter().any(|m| m.class_name == "VStructs")
                {
                    continue;
                }
                if !out.iter().any(|m| m.class_name == model.class_name) {
                    out.push(model);
                }
            }
        }
        out
    } else {
        let mut sim = SimCodegen::new(&symbols, &ast, overload_map.clone())
            .check_uninit(check_uninit)
            .inputs_start_uninit(inputs_start_uninit)
            .check_uninit_ram(check_uninit_ram)
            .cdc_random(cdc_random)
            .debug(debug, debug_depth)
            .with_debug_fsm(debug_fsm)
            .coverage(coverage)
            .coverage_dat(coverage_dat.clone());
        if coverage {
            // Build a SourceMap so the coverage dumper can render
            // file:line instead of opaque branch[N] ordinals.
            let segs: Vec<(usize, String, String)> = ms
                .segments
                .iter()
                .map(|(start, _end, name, src)| (*start, name.clone(), src.clone()))
                .collect();
            sim = sim.with_source_map(arch::sim_codegen::SourceMap::new(segs));
        }
        sim.generate()
    };

    if models.is_empty() {
        eprintln!("warning: no synthesizable constructs found (module/counter/fsm)");
    }

    let mut generated_cpps: Vec<PathBuf> = Vec::new();

    for model in &models {
        let h_path = build_dir.join(format!("{}.h", model.class_name));
        let cpp_path = build_dir.join(format!("{}.cpp", model.class_name));
        fs::write(&h_path, &model.header).into_diagnostic()?;
        fs::write(&cpp_path, &model.impl_).into_diagnostic()?;
        eprintln!("Generated {}", cpp_path.display());
        generated_cpps.push(cpp_path);
    }

    // 4. Write verilated.h / verilated.cpp stubs
    let verilated_h = build_dir.join("verilated.h");
    let verilated_cpp = build_dir.join("verilated.cpp");
    fs::write(&verilated_h, SimCodegen::verilated_h(fp_compat)).into_diagnostic()?;
    fs::write(&verilated_cpp, SimCodegen::verilated_cpp()).into_diagnostic()?;
    generated_cpps.push(verilated_cpp);

    // 4b. Thread sim runtime header (only used under --thread-sim parallel,
    // but emit unconditionally — cheap and keeps the build dir self-contained).
    let arch_thread_rt_h = build_dir.join("arch_thread_rt.h");
    fs::write(
        &arch_thread_rt_h,
        arch::sim_codegen::thread_sim::arch_thread_rt_h(),
    )
    .into_diagnostic()?;

    // ── Pybind11 mode ────────────────────────────────────────────────────
    if pybind {
        let mut sim = SimCodegen::new(&symbols, &ast, overload_map.clone())
            .check_uninit(check_uninit)
            .inputs_start_uninit(inputs_start_uninit)
            .check_uninit_ram(check_uninit_ram)
            .cdc_random(cdc_random)
            .debug(debug, debug_depth)
            .with_debug_fsm(debug_fsm)
            .coverage(coverage)
            .coverage_dat(coverage_dat.clone());
        if coverage {
            let segs: Vec<(usize, String, String)> = ms
                .segments
                .iter()
                .map(|(start, _end, name, src)| (*start, name.clone(), src.clone()))
                .collect();
            sim = sim.with_source_map(arch::sim_codegen::SourceMap::new(segs));
        }
        let pybind_wrappers = sim.generate_pybind();
        if pybind_wrappers.is_empty() {
            eprintln!("warning: no pybind11 wrappers generated");
            return Ok(());
        }

        // Pick the "user-facing" wrapper as the testbench-default top.
        // `lower_threads` prepends generated `_threads` submodules to the
        // AST item list so their SV definitions precede the parent's
        // instantiation. That puts them at index 0 of `pybind_wrappers`,
        // which previously made `--test` import the wrong pybind module
        // (the thread submodule has no parent ports — every test fails
        // with `AttributeError: No signal '<port>' on DUT`).
        // Heuristic: thread-lowered submodule class names start with
        // `V_`; user-module names start with `V<word>` (e.g. `Vibex_alu`).
        // Prefer the LAST wrapper whose class name doesn't start with
        // `V_`; fall back to wrapper[0] for designs without thread-
        // submodule lowering (the existing default shape).
        let user_top_idx = pybind_wrappers
            .iter()
            .rposition(|w| !w.class_name.starts_with("V_"))
            .unwrap_or(0);

        // Apply --pybind-module-name if provided. Retarget only the
        // user-top wrapper (the user's intended top module); other
        // wrappers (thread submodules / nested modules) keep their
        // auto-derived names. The override is a string-replace on the
        // generated .cpp so the PYBIND11_MODULE macro matches.
        let default_first_name = pybind_wrappers[user_top_idx].class_name.clone();
        let effective_first_name = pybind_module_name_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_first_name.clone());

        let mut pybind_cpps: Vec<PathBuf> = Vec::new();
        let mut pybind_module_name = String::new();
        for (i, wrapper) in pybind_wrappers.iter().enumerate() {
            let (class_name, impl_src) =
                if i == user_top_idx && pybind_module_name_override.is_some() {
                    let new_name = &effective_first_name;
                    let retargeted = wrapper.impl_.replace(
                        &format!("PYBIND11_MODULE({}, m)", default_first_name),
                        &format!("PYBIND11_MODULE({}, m)", new_name),
                    );
                    (new_name.clone(), retargeted)
                } else {
                    (wrapper.class_name.clone(), wrapper.impl_.clone())
                };
            let cpp_path = build_dir.join(format!("{}.cpp", class_name));
            fs::write(&cpp_path, &impl_src).into_diagnostic()?;
            eprintln!("Generated pybind11 wrapper: {}", cpp_path.display());
            pybind_cpps.push(cpp_path);
            if i == user_top_idx {
                pybind_module_name = class_name;
            }
        }

        // Get pybind11 includes
        let py_includes = std::process::Command::new("python3")
            .args(["-m", "pybind11", "--includes"])
            .output();
        let py_includes = match py_includes {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => {
                eprintln!("error: pybind11 not found. Install with: pip install pybind11");
                std::process::exit(1);
            }
        };

        // Get Python extension suffix
        let ext_suffix = std::process::Command::new("python3-config")
            .arg("--extension-suffix")
            .output();
        let ext_suffix = match ext_suffix {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => ".so".to_string(),
        };

        // Each pybind wrapper becomes its OWN `.so`. A previous iteration
        // emitted one combined .so and symlinked each module name to it
        // (arch-com PR #40), but that collided on pybind11's per-DSO type
        // registry when two modules registered the same struct — even with
        // `py::module_local()`, which only grants cross-module independence,
        // not cross-registration independence inside the same physical DSO.
        //
        // To keep build time bounded we precompile the shared pieces —
        // verilated runtime plus every generated module model — into .o
        // files once, then link each pybind wrapper's tiny .cpp against
        // those same .o files to produce a module-specific .so.
        eprintln!("Compiling pybind11 module...");

        // Precompile shared C++ into .o.
        let mut shared_objs: Vec<PathBuf> = Vec::new();
        for cpp in &generated_cpps {
            let obj =
                build_dir.join(cpp.file_stem().unwrap().to_string_lossy().into_owned() + ".o");
            let mut cmd = std::process::Command::new(cxx_compiler());
            cmd.arg(cxx_std_flag())
                .arg("-O2")
                .arg("-fPIC")
                .arg("-c")
                .arg("-I")
                .arg(&build_dir);
            for flag in py_includes.split_whitespace() {
                cmd.arg(flag);
            }
            for (name, val) in param_overrides {
                cmd.arg(format!("-D{name}={val}"));
            }
            cmd.arg(cpp).arg("-o").arg(&obj);
            let status = cmd.status().into_diagnostic()?;
            if !status.success() {
                eprintln!(
                    "Pybind11 compilation failed (shared .o for {})",
                    cpp.display()
                );
                std::process::exit(1);
            }
            shared_objs.push(obj);
        }

        // Link each wrapper into its own .so, reusing the precompiled shared objs.
        // Track whether the first (or `--pybind-module-name`) output has been
        // built so legacy consumers that only check for one .so keep working.
        let _ = pybind_module_name; // retained for callers referencing the variable
        for (i, (wrapper, cpp_path)) in pybind_wrappers.iter().zip(pybind_cpps.iter()).enumerate() {
            // The first wrapper honors --pybind-module-name; later wrappers
            // keep their auto-derived names.
            let class_name = if i == 0 {
                effective_first_name.clone()
            } else {
                wrapper.class_name.clone()
            };
            let so_path = build_dir.join(format!("{class_name}{ext_suffix}"));
            let mut cmd = std::process::Command::new(cxx_compiler());
            cmd.arg(cxx_std_flag())
                .arg("-O2")
                .arg("-shared")
                .arg("-fPIC")
                .arg("-I")
                .arg(&build_dir);
            for flag in py_includes.split_whitespace() {
                cmd.arg(flag);
            }
            for (name, val) in param_overrides {
                cmd.arg(format!("-D{name}={val}"));
            }
            cmd.arg(cpp_path);
            for obj in &shared_objs {
                cmd.arg(obj);
            }
            cmd.arg("-o").arg(&so_path);
            #[cfg(target_os = "macos")]
            cmd.arg("-undefined").arg("dynamic_lookup");
            let status = cmd.status().into_diagnostic()?;
            if !status.success() {
                eprintln!("Pybind11 link failed for {class_name}");
                std::process::exit(1);
            }
            eprintln!("Built: {}", so_path.display());
        }

        // If --test is given, run the test file. The launcher:
        //   1. Executes the test file as `__main__` via `runpy.run_path` so
        //      existing scripts with `if __name__ == "__main__": main()`
        //      blocks fire (backward-compat).
        //   2. After __main__ returns, if any `@cocotb.test()` functions are
        //      in `arch_cocotb.decorators._test_registry`, runs them through
        //      `arch_cocotb.runner.run_tests`. Previously the decorator only
        //      registered the test and the launcher never iterated the
        //      registry, so `@cocotb.test` functions were silent no-ops.
        if let Some(test_path) = test_file {
            eprintln!("Running test: {}", test_path.display());

            // Resolve arch-com's python/ directory relative to the arch
            // binary, not cwd. The binary lives at
            // `<arch-com>/target/{debug,release}/arch`, so go up twice and
            // look for a sibling `python/` directory. Fall back to
            // `$ARCH_PYTHON_DIR` or the current cwd for development layouts.
            let python_dir = std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    exe.parent()
                        .and_then(|p| p.parent())
                        .and_then(|p| p.parent())
                        .map(|p| p.join("python"))
                })
                .filter(|p| p.is_dir())
                .or_else(|| std::env::var("ARCH_PYTHON_DIR").ok().map(PathBuf::from))
                .or_else(|| std::env::current_dir().ok().map(|cwd| cwd.join("python")))
                .unwrap_or_else(|| PathBuf::from("python"));

            let shim_dir = python_dir.join("cocotb_shim");
            let cocotb_dir = python_dir.to_str().unwrap_or(".");
            let shim_str = shim_dir.to_str().unwrap_or(".");
            let build_str = build_dir.to_str().unwrap_or(".");

            let pythonpath = format!("{shim_str}:{cocotb_dir}:{build_str}");

            let test_path_abs = test_path
                .canonicalize()
                .unwrap_or_else(|_| test_path.to_path_buf());
            let test_dir = test_path_abs
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let test_module_name = test_path_abs
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            // Derive the model class name. The class is the pybind module
            // name minus the `_pybind` suffix (matches emit_pybind_module).
            let model_class = pybind_module_name
                .strip_suffix("_pybind")
                .unwrap_or(&pybind_module_name)
                .to_string();

            // Generated runner: runs user __main__, then dispatches any
            // registered @cocotb.test() functions.
            let runner_py = build_dir.join("_arch_cocotb_runner.py");
            let runner_src = format!(
                r#"import sys
import runpy
import importlib
from pathlib import Path

TEST_PATH     = r"{test_path}"
TEST_DIR      = r"{test_dir}"
TEST_MODULE   = "{test_module}"
PYBIND_MODULE = "{pybind_module}"
MODEL_CLASS   = "{model_class}"

if TEST_DIR and TEST_DIR not in sys.path:
    sys.path.insert(0, TEST_DIR)

# 1. Backward-compat: execute the test file as __main__ so any existing
#    `if __name__ == "__main__": main()` block fires.
runpy.run_path(TEST_PATH, run_name="__main__")

# 2. Auto-invoke any `@cocotb.test()` functions the user left in the
#    registry. Silent no-op if arch_cocotb isn't importable or the
#    registry is empty.
try:
    from arch_cocotb.decorators import _test_registry
except Exception:
    sys.exit(0)

if not _test_registry:
    sys.exit(0)

pybind_mod = importlib.import_module(PYBIND_MODULE)
model_class = getattr(pybind_mod, MODEL_CLASS, None)
if model_class is None:
    print(f"arch sim: pybind module {{PYBIND_MODULE!r}} has no class {{MODEL_CLASS!r}}; "
          f"cannot auto-run @cocotb.test functions", file=sys.stderr)
    sys.exit(1)

from arch_cocotb.runner import run_tests
ok = run_tests(model_class, TEST_MODULE)
sys.exit(0 if ok else 1)
"#,
                test_path = test_path_abs.display(),
                test_dir = test_dir.display(),
                test_module = test_module_name,
                pybind_module = pybind_module_name,
                model_class = model_class,
            );
            fs::write(&runner_py, runner_src).into_diagnostic()?;

            let status = std::process::Command::new("python3")
                .arg(&runner_py)
                .env("PYTHONPATH", &pythonpath)
                .status()
                .into_diagnostic()?;

            std::process::exit(status.code().unwrap_or(1));
        }

        return Ok(());
    }

    // ── Normal sim mode (C++ testbench) ──────────────────────────────────
    if tb_files.is_empty() {
        eprintln!(
            "No testbench files supplied — generated models are in {}/",
            build_dir.display()
        );
        eprintln!(
            "Compile with: {} {} {}/verilated.cpp {}/V*.cpp <your_tb.cpp> -I{} -o sim_out",
            cxx_compiler(),
            cxx_std_flag(),
            build_dir.display(),
            build_dir.display(),
            build_dir.display()
        );
        return Ok(());
    }

    // 5. Compile with the configured C++ compiler (ARCH_CXX, default g++)
    let sim_bin = build_dir.join("sim_out");
    let mut cmd = std::process::Command::new(cxx_compiler());
    cmd.arg(cxx_std_flag());
    // Phase 3.3: opt-in ThreadSanitizer for parallel multi-OS-thread
    // builds. Catches data races at runtime — useful in CI to verify
    // the owned-output invariant remains intact as more features (e.g.
    // shared(or) under MT) get added. Triggered by ARCH_TSAN=1 env.
    if thread_sim_parallel && std::env::var("ARCH_TSAN").is_ok() {
        cmd.arg("-fsanitize=thread").arg("-g");
        eprintln!("(ARCH_TSAN=1: building with -fsanitize=thread)");
    }
    // -O2 + -flto: meaningful uplift for hot inner loops in generated
    // sim code. LTO is the big win for designs with sub-instance
    // (`inst`) calls — without it, the cross-TU function calls between
    // the top class's eval() and the sub-instance's eval_comb() can't
    // be inlined. Compile time goes up modestly; sim throughput up
    // substantially. Override via ARCH_OPT env.
    let opt = std::env::var("ARCH_OPT").unwrap_or_else(|_| "-O2 -flto".to_string());
    for tok in opt.split_whitespace() {
        cmd.arg(tok);
    }
    cmd.arg("-I").arg(&build_dir);

    // --param overrides: inject as -DNAME=VAL preprocessor definitions.
    // The generated headers use `#ifndef NAME` / `#define NAME val` / `#endif`,
    // so a -D flag on the command line takes precedence.
    for (name, val) in param_overrides {
        cmd.arg(format!("-D{name}={val}"));
    }

    for cpp in &generated_cpps {
        cmd.arg(cpp);
    }
    for tb in tb_files {
        cmd.arg(tb);
    }
    cmd.arg("-o").arg(&sim_bin);

    eprintln!("Compiling simulation binary...");
    let status = cmd.status().into_diagnostic()?;
    if !status.success() {
        eprintln!("Compilation failed");
        std::process::exit(1);
    }

    // 6. Run the simulation binary, forwarding remaining args
    eprintln!("Running simulation...");
    let mut run_cmd = std::process::Command::new(&sim_bin);
    if debug {
        run_cmd.arg("+arch_verbosity=5");
    }
    if let Some(wave_path) = wave {
        run_cmd.arg(format!("+trace+{}", wave_path.display()));
        eprintln!("VCD waveform will be written to {}", wave_path.display());
    }
    if no_exit {
        // Cross-check mode: don't take over stdout/exit. Inherit stdio
        // so the binary's output appears interleaved with the parent's
        // logs as usual; the cross-check driver re-runs the binary
        // itself to capture stdout.
        let _ = run_cmd.status();
        return Ok(());
    }
    let run_status = run_cmd.status().into_diagnostic()?;

    std::process::exit(run_status.code().unwrap_or(1));
}

/// Resolve `use PkgName;` imports: find PkgName.arch files relative to the
/// first input file's directory. Returns an extended MultiSource with
/// dependency files prepended.
/// Locate the shipped standard library directory containing curated bus
/// definitions (BusAxiStream, BusAxiLite, BusApb, etc.). Resolution:
/// 1. `ARCH_STDLIB_PATH` env override (absolute path to stdlib/)
/// 2. Disabled entirely if `ARCH_NO_STDLIB=1`
/// 3. `<exe>/../stdlib/` — matches `cargo run` layout (target/debug/arch → ../../stdlib)
/// 4. `<exe>/../../stdlib/` — matches cargo workspace runs
/// 5. `<exe>/../share/arch/stdlib/` — matches Unix `<prefix>/bin/arch` installs
///
/// Returns None if none of these resolve to an existing directory.
fn resolve_stdlib_dir() -> Option<PathBuf> {
    if std::env::var("ARCH_NO_STDLIB").is_ok() {
        return None;
    }
    if let Ok(p) = std::env::var("ARCH_STDLIB_PATH") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    for up in 1..=4 {
        let mut candidate = exe.clone();
        for _ in 0..up {
            candidate = candidate.parent()?.to_path_buf();
        }
        let stdlib = candidate.join("stdlib");
        if stdlib.is_dir() {
            return Some(stdlib);
        }
    }
    // Unix prefix install: /usr/local/bin/arch → /usr/local/share/arch/stdlib
    let exe_parent = exe.parent()?;
    let prefix = exe_parent.parent()?;
    let share = prefix.join("share").join("arch").join("stdlib");
    if share.is_dir() {
        return Some(share);
    }
    None
}

/// Port list for any port-bearing construct. Returns an empty slice for
/// items that carry no ports (struct, enum, function, bus, package, …).
/// Used by the dep-discovery scan to find `initiator`/`target` bus-port
/// type references the `inst` scan cannot see.
fn item_ports(item: &Item) -> &[arch::ast::PortDecl] {
    match item {
        // Own `ports` field.
        Item::Module(m) => &m.ports,
        Item::Synchronizer(s) => &s.ports,
        Item::Clkgate(c) => &c.ports,
        Item::Template(t) => &t.ports,
        // `ports` via `Deref<Target = ConstructCommon>`.
        Item::Fsm(f) => &f.ports,
        Item::Fifo(f) => &f.ports,
        Item::Ram(r) => &r.ports,
        Item::Cam(c) => &c.ports,
        Item::Counter(c) => &c.ports,
        Item::Arbiter(a) => &a.ports,
        Item::Regfile(r) => &r.ports,
        Item::Pipeline(p) => &p.ports,
        Item::Linklist(l) => &l.ports,
        Item::Domain(_)
        | Item::Struct(_)
        | Item::Enum(_)
        | Item::Function(_)
        | Item::Bus(_)
        | Item::Package(_)
        | Item::Use(_)
        | Item::ExternPackage(_) => &[],
    }
}

fn resolve_use_imports(files: &[PathBuf]) -> miette::Result<Vec<PathBuf>> {
    use std::collections::HashSet;

    let base_dir = files
        .first()
        .and_then(|f| f.parent())
        .unwrap_or(std::path::Path::new("."));

    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut all_defined_modules: HashSet<String> = HashSet::new();
    let mut all_defined_buses: HashSet<String> = HashSet::new();
    let mut queue: Vec<PathBuf> = files.to_vec();

    // Process files, discovering new dependencies via `use`
    while let Some(file) = queue.pop() {
        let canon = file.canonicalize().unwrap_or_else(|_| file.clone());
        if seen.contains(&canon) {
            continue;
        }
        seen.insert(canon);

        let source = fs::read_to_string(&file).into_diagnostic()?;
        let tokens = lexer::tokenize(&source)
            .map_err(|_| miette::miette!("Lexer error in {}", file.display()))?;
        let mut p = parser::Parser::new(tokens, &source);
        let parsed = p.parse_source_file().map_err(|err| {
            Report::new(err)
                .with_source_code(NamedSource::new(file.display().to_string(), source.clone()))
        })?;

        // Find `use` items and queue their files
        let mut deps = Vec::new();
        let mut use_names: HashSet<String> = HashSet::new();
        for item in &parsed.items {
            if let arch::ast::Item::Use(u) = item {
                use_names.insert(u.name.name.clone());
                // Resolution order:
                //   1. Same-directory relative path
                //   2. ARCH_LIB_PATH entries (colon-separated)
                //   3. <install>/stdlib/ (unless ARCH_NO_STDLIB=1)
                let file_name = format!("{}.arch", u.name.name);
                let same_dir = base_dir.join(&file_name);
                if same_dir.exists() {
                    deps.push(same_dir);
                    continue;
                }
                let mut found = false;
                if let Ok(lib_path) = std::env::var("ARCH_LIB_PATH") {
                    for dir in lib_path.split(':') {
                        let p = std::path::Path::new(dir).join(&file_name);
                        if p.exists() {
                            deps.push(p);
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    continue;
                }
                if let Some(stdlib) = resolve_stdlib_dir() {
                    let p = stdlib.join(&file_name);
                    if p.exists() {
                        deps.push(p);
                    }
                }
            }
        }

        // Track every construct name defined across all input files, so the
        // `.archi` auto-discovery below does NOT pull in a (possibly stale)
        // interface stub for a construct that is ALREADY defined in-source —
        // doing so adds a duplicate item and the construct emits twice (the
        // stub copy missing its port-array ports → broken SV/sim).
        for item in &parsed.items {
            match item {
                Item::Bus(b) => {
                    all_defined_buses.insert(b.name.name.clone());
                }
                // Types / values / packages / imports — never `inst` targets.
                Item::Domain(_)
                | Item::Struct(_)
                | Item::Enum(_)
                | Item::Function(_)
                | Item::Package(_)
                | Item::Use(_)
                | Item::ExternPackage(_) => {}
                // Everything else is an instantiable construct (module, fsm,
                // fifo, ram, arbiter, regfile, cam, counter, synchronizer,
                // pipeline, clkgate, linklist, template). Use the generic
                // construct-name accessor rather than a per-variant arm so a
                // future construct can't be silently omitted — the missing
                // `regfile` arm here is exactly what duplicated GPV's regfile.
                instantiable => {
                    all_defined_modules.insert(instantiable.as_construct().name().name.clone());
                }
            }
        }

        // Find inst references and look for .archi interface files
        for item in &parsed.items {
            let insts = match item {
                Item::Module(m) => m
                    .body
                    .iter()
                    .filter_map(|b| {
                        if let arch::ast::ModuleBodyItem::Inst(i) = b {
                            Some(&i.module_name.name)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                Item::Pipeline(p) => p
                    .stages
                    .iter()
                    .flat_map(|s| s.body.iter())
                    .filter_map(|b| {
                        if let arch::ast::ModuleBodyItem::Inst(i) = b {
                            Some(&i.module_name.name)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for inst_name in insts {
                if all_defined_modules.contains(inst_name.as_str()) {
                    continue;
                }
                // Look for .arch first, then .archi
                let arch_path = base_dir.join(format!("{inst_name}.arch"));
                let archi_path = base_dir.join(format!("{inst_name}.archi"));
                if arch_path.exists() {
                    deps.push(arch_path);
                } else if archi_path.exists() {
                    deps.push(archi_path);
                }
                // Also check ARCH_LIB_PATH
                if let Ok(lib_path) = std::env::var("ARCH_LIB_PATH") {
                    for dir in lib_path.split(':') {
                        let p = std::path::Path::new(dir).join(format!("{inst_name}.archi"));
                        if p.exists() {
                            deps.push(p);
                            break;
                        }
                        let p = std::path::Path::new(dir).join(format!("{inst_name}.arch"));
                        if p.exists() {
                            deps.push(p);
                            break;
                        }
                    }
                }
                // Fall back to the shipped standard library.
                if let Some(stdlib) = resolve_stdlib_dir() {
                    let p = stdlib.join(format!("{inst_name}.arch"));
                    if p.exists() {
                        deps.push(p);
                        continue;
                    }
                    let p = stdlib.join(format!("{inst_name}.archi"));
                    if p.exists() {
                        deps.push(p);
                    }
                }
            }
        }

        // Find bus port-type references (`port p: initiator|target BusName<…>`)
        // and look for their `.arch` / `.archi` definitions with the same
        // search chain as `inst` references. Bus types referenced across files
        // appear in port declarations, not `inst` nodes, so the scan above
        // never queues them — without this a single-file `arch check` of a
        // module with a bus port fails with "unknown bus type" even when the
        // bus's `.arch`/`.archi` sits right next to it.
        let mut bus_refs: Vec<String> = Vec::new();
        for item in &parsed.items {
            for port in item_ports(item) {
                if let Some(bus) = &port.bus_info {
                    bus_refs.push(bus.bus_name.name.clone());
                }
            }
        }
        for bus_name in bus_refs {
            if all_defined_buses.contains(bus_name.as_str()) {
                continue;
            }
            // An explicit `use <Bus>;` is authoritative — it already queued
            // the canonical definition above. Skip the fallback scan for it
            // so we don't also pull in a stale build-artifact `<Bus>.archi`
            // sitting in the source directory (which would double-define the
            // bus).
            if use_names.contains(bus_name.as_str()) {
                continue;
            }
            // Same-directory `.arch` first, then `.archi`.
            let arch_path = base_dir.join(format!("{bus_name}.arch"));
            let archi_path = base_dir.join(format!("{bus_name}.archi"));
            if arch_path.exists() {
                deps.push(arch_path);
            } else if archi_path.exists() {
                deps.push(archi_path);
            }
            // Then ARCH_LIB_PATH entries.
            if let Ok(lib_path) = std::env::var("ARCH_LIB_PATH") {
                for dir in lib_path.split(':') {
                    let p = std::path::Path::new(dir).join(format!("{bus_name}.arch"));
                    if p.exists() {
                        deps.push(p);
                        break;
                    }
                    let p = std::path::Path::new(dir).join(format!("{bus_name}.archi"));
                    if p.exists() {
                        deps.push(p);
                        break;
                    }
                }
            }
            // Finally the shipped standard library.
            if let Some(stdlib) = resolve_stdlib_dir() {
                let p = stdlib.join(format!("{bus_name}.arch"));
                if p.exists() {
                    deps.push(p);
                    continue;
                }
                let p = stdlib.join(format!("{bus_name}.archi"));
                if p.exists() {
                    deps.push(p);
                }
            }
        }

        // Dependencies go first (before the file that uses them)
        for dep in deps.into_iter().rev() {
            queue.push(dep);
        }
        all_files.push(file);
    }

    // Reverse so dependencies come before dependents
    // Actually, deps were pushed to queue and will be processed before
    // the current file is added. But since we push the current file at the
    // end, all_files has: first input files first, then deps. We need deps first.
    // Let's just deduplicate and reorder: deps before users.
    // Simple approach: move any file that is NOT in the original `files` list to front.
    let orig_set: HashSet<PathBuf> = files
        .iter()
        .map(|f| f.canonicalize().unwrap_or_else(|_| f.clone()))
        .collect();
    let mut dep_files: Vec<PathBuf> = Vec::new();
    let mut main_files: Vec<PathBuf> = Vec::new();
    let mut seen2: HashSet<PathBuf> = HashSet::new();
    for f in &all_files {
        let canon = f.canonicalize().unwrap_or_else(|_| f.clone());
        if seen2.contains(&canon) {
            continue;
        }
        seen2.insert(canon.clone());
        if orig_set.contains(&canon) {
            main_files.push(f.clone());
        } else {
            dep_files.push(f.clone());
        }
    }
    dep_files.extend(main_files);
    Ok(dep_files)
}

/// Recursively walk a directory and collect every `.arch` file. Returns the
/// list sorted for deterministic output. Returns an empty vec if the path
/// doesn't exist.
fn collect_arch_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(p: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(p) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    walk(&path, out);
                } else if ft.is_file() && path.extension().and_then(|s| s.to_str()) == Some("arch")
                {
                    out.push(path);
                }
            }
        }
    }
    walk(root, &mut out);
    out.sort();
    out
}

/// Bootstrap the local learning store with feature events harvested from a
/// directory of `.arch` files. Parses each file standalone (no elaborate /
/// resolve / typecheck — just enough to populate the AST so `harvest_features`
/// can read `///` / `//!` / `//! ---` docs). Files that fail to parse are
/// silently skipped — bootstrap should never fail because one example
/// happens to use a feature the local toolchain doesn't yet support.
fn run_learn_bootstrap(path: &std::path::Path) -> miette::Result<()> {
    if !arch::learn::is_enabled() {
        eprintln!("ARCH_NO_LEARN is set — bootstrap skipped.");
        return Ok(());
    }
    let _ = arch::learn::maybe_print_first_run_notice();

    if !path.exists() {
        eprintln!("error: path does not exist: {}", path.display());
        std::process::exit(2);
    }

    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        collect_arch_files(path)
    };
    if files.is_empty() {
        eprintln!("No .arch files found under {}", path.display());
        return Ok(());
    }

    let mut total_events = 0usize;
    let mut parsed_files = 0usize;
    let mut skipped_files = 0usize;
    for file in &files {
        let src = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        let tokens = match arch::lexer::tokenize(&src) {
            Ok(t) => t,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        let mut p = parser::Parser::new(tokens, &src);
        let ast = match p.parse_source_file() {
            Ok(a) => a,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        let path_str = file.display().to_string();
        let path_str_for_closure = path_str.clone();
        let n =
            arch::learn::harvest_features(&ast, |_item| path_str_for_closure.clone()).unwrap_or(0);
        total_events += n;
        parsed_files += 1;
    }

    eprintln!(
        "Bootstrap: parsed {} file{}, skipped {}, emitted {} feature event{}.",
        parsed_files,
        if parsed_files == 1 { "" } else { "s" },
        skipped_files,
        total_events,
        if total_events == 1 { "" } else { "s" },
    );
    let n_indexed = arch::learn::build_index().into_diagnostic()?;
    eprintln!("Indexed {} total events.", n_indexed);
    eprintln!("Try: arch advise --feature \"<query>\"");
    Ok(())
}

fn run_coverage_merge(inputs: &[PathBuf], out: &Path) -> miette::Result<()> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for input in inputs {
        let text = fs::read_to_string(input)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read coverage data {}", input.display()))?;
        for (line_idx, raw_line) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = raw_line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !line.starts_with("C ") {
                return Err(miette::miette!(
                    "{}:{}: unsupported coverage.dat record; expected `C ... <count>`",
                    input.display(),
                    line_no
                ));
            }

            let (record, count) = parse_coverage_dat_record(line).ok_or_else(|| {
                miette::miette!(
                    "{}:{}: malformed coverage.dat record; expected `C ... <count>`",
                    input.display(),
                    line_no
                )
            })?;
            if !counts.contains_key(record) {
                order.push(record.to_string());
            }
            let total = counts.entry(record.to_string()).or_insert(0);
            *total = total.checked_add(count).ok_or_else(|| {
                miette::miette!(
                    "{}:{}: coverage counter overflow while merging `{}`",
                    input.display(),
                    line_no,
                    record
                )
            })?;
        }
    }

    let mut merged = String::from("# SystemC::Coverage-3\n");
    for record in &order {
        let count = counts
            .get(record)
            .expect("coverage record order and count map diverged");
        merged.push_str(record);
        merged.push(' ');
        merged.push_str(&count.to_string());
        merged.push('\n');
    }
    fs::write(out, merged)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write merged coverage data {}", out.display()))?;
    eprintln!(
        "Merged {} coverage file(s), {} point(s) -> {}",
        inputs.len(),
        order.len(),
        out.display()
    );
    Ok(())
}

fn parse_coverage_dat_record(line: &str) -> Option<(&str, u64)> {
    let mut split_idx = None;
    for (idx, ch) in line.char_indices().rev() {
        if ch.is_whitespace() {
            split_idx = Some(idx);
            break;
        }
    }
    let idx = split_idx?;
    let record = line[..idx].trim_end();
    let count = line[idx..].trim().parse::<u64>().ok()?;
    if record.is_empty() {
        return None;
    }
    Some((record, count))
}

fn run_graph_index(
    inputs: &[PathBuf],
    root: Option<&std::path::Path>,
    out: &std::path::Path,
    clean: bool,
) -> miette::Result<()> {
    let index_root = graph_index_root(inputs, root)?;
    let root_files = expand_graph_inputs(inputs)?;
    if root_files.is_empty() {
        return Err(miette::miette!("graph index found no .arch files"));
    }

    let has_directory_input = inputs.iter().any(|p| p.is_dir());
    let index = if has_directory_input {
        let mut indexes = Vec::new();
        for root_file in &root_files {
            match build_graph_index_for_roots(std::slice::from_ref(root_file), &index_root) {
                Ok(index) => indexes.push(index),
                Err(err) => {
                    eprintln!(
                        "warning: skipped graph root {}: {:?}",
                        root_file.display(),
                        err
                    );
                }
            }
        }
        if indexes.is_empty() {
            return Err(miette::miette!("graph index found no valid .arch roots"));
        }
        arch::graph::merge_indexes(indexes)
    } else {
        build_graph_index_for_roots(&root_files, &index_root)?
    };
    arch::graph::write_index(&index, out, clean).into_diagnostic()?;
    eprintln!(
        "Indexed {} file{}, {} node{}, {} edge{} into {}",
        index.files.len(),
        if index.files.len() == 1 { "" } else { "s" },
        index.nodes.len(),
        if index.nodes.len() == 1 { "" } else { "s" },
        index.edges.len(),
        if index.edges.len() == 1 { "" } else { "s" },
        out.display()
    );
    eprintln!("Graph root: {}", index_root.display());
    Ok(())
}

fn build_graph_index_for_roots(
    root_files: &[PathBuf],
    index_root: &std::path::Path,
) -> miette::Result<arch::graph::GraphIndex> {
    let all_files = resolve_use_imports(root_files)?;
    let ms = MultiSource::from_files(&all_files)?;
    // Validate through the normal compiler pipeline. The graph itself is
    // built from source-level AST below so docs, imports, and thread bodies
    // stay close to what users wrote.
    let _ = run_check_multi(&ms)?;
    let parsed_ast = parse_graph_source_ast(&ms)?;

    let root_inputs: std::collections::BTreeSet<String> = root_files
        .iter()
        .flat_map(|p| {
            let display = p.display().to_string();
            let canon = p
                .canonicalize()
                .unwrap_or_else(|_| p.clone())
                .display()
                .to_string();
            [display, canon]
        })
        .collect();
    let segments: Vec<arch::graph::SourceSegment> = ms
        .segments
        .iter()
        .map(
            |(start, end, filename, source)| arch::graph::SourceSegment {
                start: *start,
                end: *end,
                filename: filename.clone(),
                source: source.clone(),
            },
        )
        .collect();
    arch::graph::build_index(&parsed_ast, &segments, &root_inputs, index_root).into_diagnostic()
}

fn graph_index_root(
    inputs: &[PathBuf],
    explicit_root: Option<&std::path::Path>,
) -> miette::Result<PathBuf> {
    if let Some(root) = explicit_root {
        if !root.is_dir() {
            return Err(miette::miette!(
                "graph --root must be an existing directory: {}",
                root.display()
            ));
        }
        return root
            .canonicalize()
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to canonicalize graph root {}", root.display()));
    }

    let mut anchors = Vec::new();
    for input in inputs {
        if input.is_dir() {
            anchors.push(input.canonicalize().into_diagnostic().wrap_err_with(|| {
                format!("failed to canonicalize graph input {}", input.display())
            })?);
        } else if input.is_file() {
            let file = input.canonicalize().into_diagnostic().wrap_err_with(|| {
                format!("failed to canonicalize graph input {}", input.display())
            })?;
            anchors.push(
                file.parent()
                    .unwrap_or_else(|| std::path::Path::new("/"))
                    .to_path_buf(),
            );
        } else {
            return Err(miette::miette!(
                "graph input does not exist: {}",
                input.display()
            ));
        }
    }

    common_path_prefix(&anchors)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| miette::miette!("failed to infer graph root"))
}

fn common_path_prefix(paths: &[PathBuf]) -> Option<PathBuf> {
    let first = paths.first()?;
    let mut prefix: Vec<_> = first.components().collect();
    for path in &paths[1..] {
        let mut n = 0;
        for (a, b) in prefix.iter().zip(path.components()) {
            if *a == b {
                n += 1;
            } else {
                break;
            }
        }
        prefix.truncate(n);
    }
    if prefix.is_empty() {
        None
    } else {
        let mut out = PathBuf::new();
        for component in prefix {
            out.push(component.as_os_str());
        }
        Some(out)
    }
}

fn run_graph_query(
    query: &str,
    index: &std::path::Path,
    json: bool,
    limit: usize,
) -> miette::Result<()> {
    if query.trim().is_empty() {
        return Err(miette::miette!("graph query requires a non-empty query"));
    }
    if limit == 0 {
        return Err(miette::miette!("--limit must be greater than 0"));
    }
    let graph = arch::graph::load_index(index).into_diagnostic()?;
    let hits = arch::graph::query(&graph, query, limit);
    if json {
        println!("{}", serde_json::to_string_pretty(&hits).into_diagnostic()?);
    } else {
        println!("{}", arch::graph::format_query_hits(&hits));
    }
    Ok(())
}

fn run_graph_callers(
    target: &str,
    index: &std::path::Path,
    json: bool,
    limit: usize,
) -> miette::Result<()> {
    if target.trim().is_empty() {
        return Err(miette::miette!("graph callers requires a non-empty target"));
    }
    if limit == 0 {
        return Err(miette::miette!("--limit must be greater than 0"));
    }
    let graph = arch::graph::load_index(index).into_diagnostic()?;
    let hits = arch::graph::callers(&graph, target, limit);
    if json {
        println!("{}", serde_json::to_string_pretty(&hits).into_diagnostic()?);
    } else {
        println!("{}", arch::graph::format_callers(&hits));
    }
    Ok(())
}

fn run_graph_impact(
    symbol: &str,
    depth: usize,
    index: &std::path::Path,
    json: bool,
    limit: usize,
) -> miette::Result<()> {
    if symbol.trim().is_empty() {
        return Err(miette::miette!("graph impact requires a non-empty symbol"));
    }
    if limit == 0 {
        return Err(miette::miette!("--limit must be greater than 0"));
    }
    let graph = arch::graph::load_index(index).into_diagnostic()?;
    let hits = arch::graph::impact(&graph, symbol, depth, limit);
    if json {
        println!("{}", serde_json::to_string_pretty(&hits).into_diagnostic()?);
    } else {
        println!("{}", arch::graph::format_impact(&hits));
    }
    Ok(())
}

fn run_graph_context(
    task: &str,
    index: &std::path::Path,
    json: bool,
    limit: usize,
) -> miette::Result<()> {
    if task.trim().is_empty() {
        return Err(miette::miette!(
            "graph context requires a non-empty task description"
        ));
    }
    if limit == 0 {
        return Err(miette::miette!("--limit must be greater than 0"));
    }
    let graph = arch::graph::load_index(index).into_diagnostic()?;
    let hits = arch::graph::context(&graph, task, limit);
    if json {
        println!("{}", serde_json::to_string_pretty(&hits).into_diagnostic()?);
    } else {
        println!("{}", arch::graph::format_query_hits(&hits));
    }
    Ok(())
}

fn run_graph_html(
    index: &std::path::Path,
    out: &std::path::Path,
    title: Option<&str>,
) -> miette::Result<()> {
    let graph = arch::graph::load_index(index).into_diagnostic()?;
    let title = title.unwrap_or("ARCH graph");
    let html = arch::graph::render_html(&graph, title).into_diagnostic()?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
    }
    fs::write(out, html).into_diagnostic()?;
    eprintln!("Wrote {}", out.display());
    Ok(())
}

fn expand_graph_inputs(inputs: &[PathBuf]) -> miette::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            files.extend(
                collect_arch_files(input)
                    .into_iter()
                    .filter(|p| !is_graph_directory_skip(p)),
            );
        } else if input.is_file() {
            if input.extension().and_then(|s| s.to_str()) == Some("arch") {
                files.push(input.clone());
            }
        } else {
            return Err(miette::miette!(
                "graph input does not exist: {}",
                input.display()
            ));
        }
    }
    files.sort();
    files.dedup_by(|a, b| {
        a.canonicalize().unwrap_or_else(|_| a.clone())
            == b.canonicalize().unwrap_or_else(|_| b.clone())
    });
    Ok(files)
}

fn is_graph_directory_skip(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.ends_with("_tb.arch"))
        .unwrap_or(false)
}

fn parse_graph_source_ast(ms: &MultiSource) -> miette::Result<arch::ast::SourceFile> {
    let tokens = lexer::tokenize(&ms.combined).map_err(|spans| {
        let offset = spans.first().map(|s| s.start).unwrap_or(0);
        let (filename, file_source, local_offset) = ms.locate(offset);
        let err = CompileError::LexerError {
            span: miette::SourceSpan::new(local_offset.into(), 1_usize.into()),
        };
        Report::new(err).with_source_code(NamedSource::new(
            filename.to_string(),
            file_source.to_string(),
        ))
    })?;
    let mut p = parser::Parser::new(tokens, &ms.combined);
    let mut parsed_ast = p.parse_source_file().map_err(|err| ms.report_error(err))?;
    for item in parsed_ast.items.iter_mut() {
        let span = item.span();
        let (filename, _, _) = ms.locate(span.start);
        if filename.ends_with(".archi") {
            item.set_is_interface(true);
        }
    }
    Ok(parsed_ast)
}

fn params_mut_for_item(item: &mut Item) -> Option<(&str, &mut Vec<ParamDecl>)> {
    match item {
        Item::Module(m) => Some((&m.name.name, &mut m.params)),
        Item::Fsm(f) => Some((&f.common.name.name, &mut f.common.params)),
        Item::Fifo(f) => Some((&f.common.name.name, &mut f.common.params)),
        Item::Ram(r) => Some((&r.common.name.name, &mut r.common.params)),
        Item::Cam(c) => Some((&c.common.name.name, &mut c.common.params)),
        Item::Counter(c) => Some((&c.common.name.name, &mut c.common.params)),
        Item::Arbiter(a) => Some((&a.common.name.name, &mut a.common.params)),
        Item::Regfile(r) => Some((&r.common.name.name, &mut r.common.params)),
        Item::Pipeline(p) => Some((&p.common.name.name, &mut p.common.params)),
        Item::Linklist(l) => Some((&l.common.name.name, &mut l.common.params)),
        Item::Synchronizer(s) => Some((&s.name.name, &mut s.params)),
        Item::Clkgate(c) => Some((&c.name.name, &mut c.params)),
        Item::Bus(b) => Some((&b.name.name, &mut b.params)),
        Item::Template(t) => Some((&t.name.name, &mut t.params)),
        Item::Package(p) => Some((&p.name.name, &mut p.params)),
        _ => None,
    }
}

fn const_expr_i64(expr: &Expr, values: &std::collections::HashMap<String, i64>) -> Option<i64> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v))
        | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v as i64),
        ExprKind::Ident(name) => values.get(name).copied(),
        ExprKind::Unary(op, inner) => {
            let v = const_expr_i64(inner, values)?;
            match op {
                UnaryOp::Neg => Some(v.wrapping_neg()),
                UnaryOp::Not => Some(!v),
                _ => None,
            }
        }
        ExprKind::Binary(op, lhs, rhs) => {
            let l = const_expr_i64(lhs, values)?;
            let r = const_expr_i64(rhs, values)?;
            match op {
                BinOp::Add | BinOp::AddWrap => Some(l.wrapping_add(r)),
                BinOp::Sub | BinOp::SubWrap => Some(l.wrapping_sub(r)),
                BinOp::Mul | BinOp::MulWrap => Some(l.wrapping_mul(r)),
                BinOp::Div => (r != 0).then_some(l / r),
                BinOp::Mod => (r != 0).then_some(l % r),
                BinOp::Shl => Some(l.wrapping_shl(r as u32)),
                BinOp::Shr => Some(((l as u64).wrapping_shr(r as u32)) as i64),
                BinOp::BitAnd => Some(l & r),
                BinOp::BitOr => Some(l | r),
                BinOp::BitXor => Some(l ^ r),
                _ => None,
            }
        }
        ExprKind::Clog2(inner) => {
            let v = const_expr_i64(inner, values)?;
            if v <= 1 {
                Some(0)
            } else {
                Some(64 - ((v as u64) - 1).leading_zeros() as i64)
            }
        }
        _ => None,
    }
}

fn literal_for_param_override(
    param: &ParamDecl,
    value: u64,
    values: &std::collections::HashMap<String, i64>,
) -> LitKind {
    if let ParamKind::WidthConst(hi, lo) = &param.kind {
        if let (Some(h), Some(l)) = (const_expr_i64(hi, values), const_expr_i64(lo, values)) {
            if h >= l {
                return LitKind::Sized((h - l + 1) as u32, value);
            }
        }
    }
    LitKind::Dec(value)
}

fn apply_overrides_to_params(
    construct_name: &str,
    params: &mut [ParamDecl],
    overrides: &std::collections::HashMap<String, u64>,
    seen: &mut std::collections::HashSet<String>,
) -> miette::Result<()> {
    let mut values = std::collections::HashMap::<String, i64>::new();
    for p in params.iter() {
        if let Some(default) = &p.default {
            if let Some(v) = const_expr_i64(default, &values) {
                values.insert(p.name.name.clone(), v);
            }
        }
    }

    for p in params.iter_mut() {
        let Some(&value) = overrides.get(&p.name.name) else {
            continue;
        };
        seen.insert(p.name.name.clone());
        if p.is_local {
            return Err(miette::miette!(
                "--param {}={} targets local param `{}` in `{}`; local params are not overridable",
                p.name.name,
                value,
                p.name.name,
                construct_name,
            ));
        }
        if !matches!(
            p.kind,
            ParamKind::Const | ParamKind::WidthConst(..) | ParamKind::Logic(_)
        ) {
            return Err(miette::miette!(
                "--param {}={} targets non-integer param `{}` in `{}`; only const/logic value params are supported",
                p.name.name,
                value,
                p.name.name,
                construct_name,
            ));
        }
        let lit = literal_for_param_override(p, value, &values);
        p.default = Some(Expr::new(ExprKind::Literal(lit), p.name.span));
        values.insert(p.name.name.clone(), value as i64);
    }
    Ok(())
}

fn apply_top_param_overrides(
    ast: &mut SourceFile,
    overrides: &std::collections::HashMap<String, u64>,
) -> miette::Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }

    let mut seen = std::collections::HashSet::<String>::new();
    for item in ast.items.iter_mut() {
        let Some((construct_name, params)) = params_mut_for_item(item) else {
            continue;
        };
        apply_overrides_to_params(construct_name, params, overrides, &mut seen)?;
    }

    let mut unknown: Vec<_> = overrides
        .keys()
        .filter(|name| !seen.contains(*name))
        .cloned()
        .collect();
    unknown.sort();
    if !unknown.is_empty() {
        return Err(miette::miette!(
            "--param override(s) did not match any non-local value parameter: {}",
            unknown.join(", ")
        ));
    }
    Ok(())
}

fn run_check_multi(
    ms: &MultiSource,
) -> miette::Result<(
    arch::ast::SourceFile,
    resolve::SymbolTable,
    std::collections::HashMap<usize, usize>,
)> {
    run_check_multi_opts(
        ms, /*skip_lower_threads=*/ false, /*auto_thread_asserts=*/ false,
    )
}

fn run_check_multi_opts(
    ms: &MultiSource,
    skip_lower_threads: bool,
    auto_thread_asserts: bool,
) -> miette::Result<(
    arch::ast::SourceFile,
    resolve::SymbolTable,
    std::collections::HashMap<usize, usize>,
)> {
    run_check_multi_opts_with_thread_map_and_params(
        ms,
        skip_lower_threads,
        auto_thread_asserts,
        None,
        None,
    )
}

fn run_check_multi_opts_with_thread_map(
    ms: &MultiSource,
    skip_lower_threads: bool,
    auto_thread_asserts: bool,
    thread_map: Option<std::rc::Rc<std::cell::RefCell<arch::thread_map::ThreadMap>>>,
) -> miette::Result<(
    arch::ast::SourceFile,
    resolve::SymbolTable,
    std::collections::HashMap<usize, usize>,
)> {
    run_check_multi_opts_with_thread_map_and_params(
        ms,
        skip_lower_threads,
        auto_thread_asserts,
        thread_map,
        None,
    )
}

fn run_check_multi_opts_with_param_overrides(
    ms: &MultiSource,
    skip_lower_threads: bool,
    auto_thread_asserts: bool,
    param_overrides: &std::collections::HashMap<String, u64>,
) -> miette::Result<(
    arch::ast::SourceFile,
    resolve::SymbolTable,
    std::collections::HashMap<usize, usize>,
)> {
    run_check_multi_opts_with_thread_map_and_params(
        ms,
        skip_lower_threads,
        auto_thread_asserts,
        None,
        Some(param_overrides),
    )
}

fn run_check_multi_opts_with_thread_map_and_params(
    ms: &MultiSource,
    skip_lower_threads: bool,
    auto_thread_asserts: bool,
    thread_map: Option<std::rc::Rc<std::cell::RefCell<arch::thread_map::ThreadMap>>>,
    param_overrides: Option<&std::collections::HashMap<String, u64>>,
) -> miette::Result<(
    arch::ast::SourceFile,
    resolve::SymbolTable,
    std::collections::HashMap<usize, usize>,
)> {
    let source = &ms.combined;

    // Lex
    let tokens = lexer::tokenize(source).map_err(|spans| {
        let offset = spans[0].start;
        let (filename, file_source, local_offset) = ms.locate(offset);
        let err = CompileError::LexerError {
            span: miette::SourceSpan::new(
                local_offset.into(),
                (spans[0].end - spans[0].start).into(),
            ),
        };
        Report::new(err).with_source_code(NamedSource::new(
            filename.to_string(),
            file_source.to_string(),
        ))
    })?;

    // Parse
    let mut p = parser::Parser::new(tokens, source);
    let mut parsed_ast = p.parse_source_file().map_err(|err| ms.report_error(err))?;

    // Tag items loaded from `.archi` interface stubs (port-only, no body).
    // Body-only downstream passes — typecheck's output-driven check /
    // body validation, SV codegen, sim model emission, and `.archi`
    // re-emission — skip these to avoid spurious diagnostics and
    // duplicate output. `Item::set_is_interface` covers `module` plus
    // every `ConstructCommon`-bearing variant that can appear in an
    // `.archi` (fsm, fifo, ram, cam, counter, arbiter, regfile,
    // pipeline, linklist). Variants that can't appear in an `.archi`
    // (domain, struct, enum, function, template, package, use, ...)
    // silently no-op via `set_is_interface`'s `_ => false` arm.
    for item in parsed_ast.items.iter_mut() {
        let span = item.span();
        let (filename, _, _) = ms.locate(span.start);
        if filename.ends_with(".archi") {
            item.set_is_interface(true);
        }
    }

    // Surface any deprecated-`implies`-keyword usages as a single stderr
    // warning (one line per site). The symbolic `|->` form is the
    // recommended spelling; the keyword is still accepted in this
    // release.
    if !p.deprecated_implies_spans.is_empty() {
        for span in &p.deprecated_implies_spans {
            let (filename, _, local_offset) = ms.locate(span.start);
            eprintln!(
                "warning: `implies` keyword is deprecated; use `|->` instead — {}:{}",
                filename, local_offset,
            );
        }
    }

    // Harvest doc-comment / frontmatter content into the local learn store
    // (PR-doc-3). Runs on the *parsed* AST — before elaboration — so we
    // capture the user's source-level intent unchanged. Each top-level
    // construct becomes one `kind: "feature"` event that `arch advise
    // --feature <query>` can retrieve.
    if arch::learn::is_enabled() {
        let _ = arch::learn::harvest_features(&parsed_ast, |item| {
            let (filename, _, _) = ms.locate(item.span().start);
            filename.to_string()
        });
    }

    // Precedence ambiguity check on user source (pre-elaboration, so generated
    // reductions from thread lowering etc. don't trigger spurious warnings)
    let prec_errors = arch::typecheck::check_precedence(&parsed_ast);
    if !prec_errors.is_empty() {
        let err = prec_errors.into_iter().next().unwrap();
        return Err(ms.report_error(err));
    }

    if let Some(overrides) = param_overrides {
        apply_top_param_overrides(&mut parsed_ast, overrides)?;
    }

    // Resolve module-scope `type Name = ...;` aliases by inlining them at
    // every use site. Runs before elaboration so downstream passes see
    // aliases as if hand-inlined.
    let parsed_ast = arch::type_alias::resolve_type_aliases(parsed_ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Elaborate (expand generate blocks)
    let ast = elaborate::elaborate(parsed_ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Rewrite TLM target threads (`thread port.method(args) ...`) into
    // regular threads that drive the req/rsp handshake. Must run before
    // the generic lower_threads, which rejects TLM-bound threads.
    let ast = elaborate::lower_tlm_target_threads(ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Expand initiator-side TLM call sites (`x <= m.method(args);`) in
    // thread bodies into the issue + wait-response state pair.
    let ast = elaborate::lower_tlm_initiator_calls(ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Dead-skid combinational-feedback lint (issue #245). Runs on the
    // pre-thread-lowering AST so `ModuleBodyItem::Thread` is still present and
    // its source-level write/read sets are intact. Emits warnings only (never
    // errors); suppressed per-module by `pragma allow_dead_skid_feedback;`.
    // Hazards are also collected for the thread-map HTML overlay below.
    let mut collected_hazards: Vec<(String, arch::signal_flow::DeadSkidHazard)> = Vec::new();
    for item in &ast.items {
        if let arch::ast::Item::Module(m) = item {
            if m.allow_dead_skid_feedback {
                continue;
            }
            for hz in arch::signal_flow::find_dead_skid_hazards(m, &ast) {
                let (filename, _, local_offset) = ms.locate(hz.read_span.start);
                eprintln!(
                    "warning: dead-skid feedback: thread `{}` reads `{}`, a combinational \
                     function of `{}` that it drives — during dead-skid cycles `{}` falls to its \
                     default and `{}` may read spuriously (comb path: {}). Read the upstream input \
                     directly, or add `pragma allow_dead_skid_feedback;` to module `{}` if the \
                     read-back is intentional. ({}:{})",
                    hz.thread_name,
                    hz.read_signal,
                    hz.driven_signal,
                    hz.driven_signal,
                    hz.read_signal,
                    hz.path.join(" -> "),
                    m.name.name,
                    filename,
                    local_offset,
                );
                collected_hazards.push((m.name.name.clone(), hz));
            }
        }
    }

    // Lower thread blocks to FSM + inst (skipped under --thread-sim parallel,
    // where the new pre-lowering thread sim emitter consumes thread blocks
    // directly via coroutines).
    let map_handle = thread_map.clone();
    let ast = if skip_lower_threads {
        ast
    } else {
        let opts = elaborate::ThreadLowerOpts {
            auto_asserts: auto_thread_asserts,
            thread_map,
        };
        elaborate::lower_threads_with_opts(ast, &opts).map_err(|errs| {
            let err = errs.into_iter().next().unwrap();
            ms.report_error(err)
        })?
    };

    // Overlay the collected dead-skid hazards onto the thread map (issue #245)
    // so `--emit-thread-map` renders a ⚠ badge + comb path next to the
    // offending thread and highlights the read site in the source panel.
    if let Some(map_rc) = &map_handle {
        if !collected_hazards.is_empty() {
            let mut map = map_rc.borrow_mut();
            for (mod_name, hz) in &collected_hazards {
                for tmm in map
                    .modules
                    .iter_mut()
                    .filter(|m| m.module_name == *mod_name)
                {
                    for tmt in tmm.threads.iter_mut().filter(|t| t.name == hz.thread_name) {
                        tmt.hazards.push(arch::thread_map::CombFeedbackHazard {
                            read_signal: hz.read_signal.clone(),
                            driven_signal: hz.driven_signal.clone(),
                            path_summary: hz.path.join(" -> "),
                            read_span: hz.read_span,
                        });
                    }
                }
            }
        }
    }

    // Lower `pipe_reg<T, N>` ports with N > 1 into an N-stage cascade.
    let (ast, pipe_reg_warnings) = elaborate::lower_pipe_reg_ports(ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Rewrite `port.ch.valid` / `.data` / `.can_send` to SynthIdent so they
    // reference the codegen-emitted SV wires (credit_channel method dispatch).
    let ast = elaborate::lower_credit_channel_dispatch(ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Resolve
    let symbols = resolve::resolve(&ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    // Type check
    let checker = TypeChecker::new(&symbols, &ast);
    let (mut warnings, overload_map) = checker.check().map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;
    warnings.extend(pipe_reg_warnings);

    for w in &warnings {
        let (filename, _, local_offset) = ms.locate(w.span.start);
        eprintln!("warning: {} ({}:{})", w.message, filename, local_offset);
    }

    Ok((ast, symbols, overload_map))
}
