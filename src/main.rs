use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, NamedSource, Report};
use std::fs;
use std::path::PathBuf;

use arch::ast::Item;
use arch::codegen::Codegen;
use arch::diagnostics::CompileError;
use arch::elaborate;
use arch::formal;
use arch::lexer;
use arch::parser;
use arch::resolve;
use arch::sim_codegen::SimCodegen;
use arch::typecheck::TypeChecker;

#[derive(Parser)]
#[command(name = "arch", about = "ARCH HDL compiler")]
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
    },
    /// Show stats about the local learning store
    LearnStats,
    /// Compile ARCH to SystemVerilog
    Build {
        /// Input .arch file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Output .sv file
        #[arg(short, long)]
        o: Option<PathBuf>,
    },
    /// Compile ARCH + C++ testbench and run simulation
    ///
    /// Example: arch sim Foo.arch Foo_tb.cpp
    ///
    /// Generates Verilator-compatible C++ models, compiles with g++, and runs.
    Sim {
        /// Input .arch file(s)
        #[arg(required = true)]
        arch_files: Vec<PathBuf>,
        /// C++ testbench file(s) to compile alongside the generated models
        #[arg(long = "tb", num_args = 1..)]
        tb_files: Vec<PathBuf>,
        /// Output directory for generated C++ files (default: arch_sim_build/)
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
        /// Per-property solver timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u32,
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
        Report::new(relocated_err)
            .with_source_code(NamedSource::new(filename.to_string(), file_source.to_string()))
    }

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
        Command::Check { files } => {
            learn_wrap(&files, || {
                let all_files = resolve_use_imports(&files)?;
                let ms = MultiSource::from_files(&all_files)?;
                run_check_multi(&ms)?;
                eprintln!("OK: no errors");
                Ok(())
            })
        }
        Command::LearnIndex => {
            let n = arch::learn::build_index().into_diagnostic()?;
            eprintln!("Indexed {} events.", n);
            Ok(())
        }
        Command::Advise { query, top, from_stderr } => {
            let mut q = query.join(" ");
            if from_stderr {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf).into_diagnostic()?;
                if !buf.trim().is_empty() {
                    if !q.is_empty() { q.push(' '); }
                    q.push_str(buf.trim());
                }
            }
            if q.trim().is_empty() {
                eprintln!("error: empty query (pass a query string or pipe via --from-stderr)");
                std::process::exit(2);
            }
            let matches = arch::learn::advise(&q, top).into_diagnostic()?;
            if matches.is_empty() {
                eprintln!("No matches.");
                return Ok(());
            }
            for (i, m) in matches.iter().enumerate() {
                println!("── match #{} (score {:.3}, retrieved {}×) ──────────────────────",
                         i + 1, m.score, m.retrieved_count);
                println!("  code:    {}", m.event.error_code);
                println!("  message: {}", m.event.error_message);
                println!("  file:    {}", m.event.file_path);
                println!("  diff:    {}", m.event.diff_summary);
                println!();
            }
            Ok(())
        }
        Command::LearnStats => {
            arch::learn::print_stats().into_diagnostic()?;
            Ok(())
        }
        Command::LearnClear => {
            arch::learn::clear_store().into_diagnostic()?;
            eprintln!("Cleared ~/.arch/learn/");
            Ok(())
        }
        Command::LearnPrune { code, contains, older_than_days, dry_run } => {
            if code.is_none() && contains.is_none() && older_than_days.is_none() {
                eprintln!("error: specify at least one of --code / --contains / --older-than-days");
                std::process::exit(2);
            }
            let (kept, removed) = arch::learn::prune(
                code.as_deref(),
                contains.as_deref(),
                older_than_days,
                dry_run,
            ).into_diagnostic()?;
            if dry_run {
                eprintln!("Would remove {} events; {} would remain.", removed, kept);
            } else {
                eprintln!("Removed {} events; {} remain. Run `arch learn-index` to refresh the index.", removed, kept);
            }
            Ok(())
        }
        Command::Sim { arch_files, tb_files, outdir, check_uninit, inputs_start_uninit, check_uninit_ram, cdc_random, wave, debug, debug_depth, debug_fsm, coverage, coverage_dat, thread_sim, threads, pybind, test, pybind_module_name } => {
            let dbg_ports = debug || debug_fsm;  // any debug option implies port logging
            // --inputs-start-uninit and --check-uninit-ram both imply --check-uninit
            let check_uninit = check_uninit || inputs_start_uninit || check_uninit_ram;
            // --coverage-dat resolves to a path: explicit --coverage-dat=foo
            // → Some(Some("foo")) → "foo"; bare --coverage-dat
            // → Some(None) → default "coverage.dat"; absent → None.
            let cov_dat_path: Option<String> = coverage_dat.map(|opt| opt.unwrap_or_else(|| "coverage.dat".to_string()));
            let coverage = coverage || cov_dat_path.is_some();
            if threads > 1 && thread_sim != "parallel" {
                return Err(miette::miette!("--threads N (N>1) requires --thread-sim parallel"));
            }
            match thread_sim.as_str() {
                "fsm" => learn_wrap(&arch_files, || {
                    run_sim(&arch_files, &tb_files, outdir.as_deref(), check_uninit, inputs_start_uninit, check_uninit_ram, cdc_random, wave.as_deref(), dbg_ports, debug_depth, debug_fsm, coverage, cov_dat_path.clone(), false, threads, pybind, test.as_deref(), pybind_module_name.as_deref())
                }),
                "parallel" => learn_wrap(&arch_files, || {
                    run_sim(&arch_files, &tb_files, outdir.as_deref(), check_uninit, inputs_start_uninit, check_uninit_ram, cdc_random, wave.as_deref(), dbg_ports, debug_depth, debug_fsm, coverage, cov_dat_path.clone(), true, threads, pybind, test.as_deref(), pybind_module_name.as_deref())
                }),
                "both" => {
                    // Cross-check: build + run both fsm and parallel sims
                    // independently with --debug, then diff the port-change
                    // traces. Mismatch ⇒ abort with first divergence.
                    run_thread_sim_cross_check(&arch_files, &tb_files, outdir.as_deref())
                }
                other => return Err(miette::miette!("--thread-sim: expected `fsm`, `parallel`, or `both`, got `{}`", other)),
            }
        }
        Command::Build { files, o } => {
            let files_for_learn = files.clone();
            learn_wrap(&files_for_learn, move || {
            let all_files = resolve_use_imports(&files)?;
            let ms = MultiSource::from_files(&all_files)?;
            let (ast, symbols, overload_map) = run_check_multi(&ms)?;

            let comments = lexer::extract_comments(&ms.combined);

            if files.len() == 1 || o.is_some() {
                // Single file or explicit -o: emit one combined SV file
                let codegen = Codegen::new(&symbols, &ast, overload_map).with_comments(comments);
                let sv = codegen.generate();
                let out_path = o.unwrap_or_else(|| files[0].with_extension("sv"));
                fs::write(&out_path, &sv).into_diagnostic()?;
                eprintln!("Wrote {}", out_path.display());
            } else {
                // Multi-file: emit one .sv per .arch input file
                for (seg_start, seg_end, filename, _) in &ms.segments {
                    // Collect items whose span falls within this file's segment
                    let file_items: Vec<_> = ast.items.iter()
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
                    let file_comments: Vec<_> = comments.iter()
                        .filter(|(span, _)| span.start >= *seg_start && span.start < *seg_end)
                        .cloned()
                        .collect();

                    let mut codegen = Codegen::new(&symbols, &ast, overload_map.clone()).with_comments(file_comments);
                    let sv = codegen.generate_items(&file_items);

                    let out_path = std::path::Path::new(filename).with_extension("sv");
                    fs::write(&out_path, &sv).into_diagnostic()?;
                    eprintln!("Wrote {}", out_path.display());
                }
            }

            // Emit .archi interface files alongside .sv (for separate compilation)
            for item in &ast.items {
                if let Some(content) = arch::interface::emit_interface(item) {
                    let name = match item {
                        Item::Module(m) => &m.name.name,
                        Item::Fsm(f) => &f.name.name,
                        Item::Counter(c) => &c.name.name,
                        Item::Pipeline(p) => &p.name.name,
                        Item::Bus(b) => &b.name.name,
                        Item::Struct(s) => &s.name.name,
                        Item::Enum(e) => &e.name.name,
                        Item::Package(p) => &p.name.name,
                        Item::Synchronizer(s) => &s.name.name,
                        Item::Fifo(f) => &f.name.name,
                        Item::Ram(r) => &r.name.name,
                        Item::Arbiter(a) => &a.name.name,
                        Item::Regfile(r) => &r.name.name,
                        Item::Clkgate(c) => &c.name.name,
                        Item::Linklist(l) => &l.name.name,
                        _ => continue,
                    };
                    // Write .archi next to the .sv output
                    let archi_dir = files[0].parent()
                        .unwrap_or(std::path::Path::new(".")).to_path_buf();
                    let archi_path = archi_dir.join(format!("{name}.archi"));
                    fs::write(&archi_path, &content).into_diagnostic()?;
                    eprintln!("Wrote {}", archi_path.display());
                }
            }

            Ok(())
            })
        }
        Command::Formal { files, top, bound, solver, emit_smt, timeout } => {
            let files_for_learn = files.clone();
            learn_wrap(&files_for_learn, move || {
                let all_files = resolve_use_imports(&files)?;
                let ms = MultiSource::from_files(&all_files)?;
                let (ast, symbols, _overload_map) = run_check_multi(&ms)?;

                let args = formal::FormalArgs {
                    top: top.clone(),
                    bound,
                    solver: solver.clone(),
                    emit_smt: emit_smt.clone(),
                    timeout,
                };
                let report = formal::run(&ast, &symbols, &args).map_err(|err| {
                    ms.report_error(err)
                })?;
                std::process::exit(report.exit_code());
            })
        }
    }
}

/// `--thread-sim both` driver: build + run fsm and parallel sims
/// independently with --debug, then diff their port-change traces.
/// Mismatch ⇒ abort with the first divergence highlighted.
fn run_thread_sim_cross_check(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
) -> miette::Result<()> {
    let base = outdir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("arch_sim_build"));
    let fsm_dir = base.with_file_name(format!("{}_fsm", base.file_name().and_then(|s| s.to_str()).unwrap_or("arch_sim_build")));
    let par_dir = base.with_file_name(format!("{}_par", base.file_name().and_then(|s| s.to_str()).unwrap_or("arch_sim_build")));

    eprintln!("=== arch sim --thread-sim both: building fsm path ===");
    let fsm_trace = build_and_capture(arch_files, tb_files, &fsm_dir, /*parallel=*/false)?;
    eprintln!("=== arch sim --thread-sim both: building parallel path ===");
    let par_trace = build_and_capture(arch_files, tb_files, &par_dir, /*parallel=*/true)?;

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
        eprintln!("=== Cross-check PASS: {} port-change events match ===", fsm_lines.len());
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
    Err(miette::miette!("--thread-sim both: trace lengths differ ({} fsm vs {} parallel)",
        fsm_lines.len(), par_lines.len()))
}

/// Helper for run_thread_sim_cross_check: build a sim binary in `dir`
/// (fsm or parallel mode), run it, capture its stdout.
fn build_and_capture(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    dir: &std::path::Path,
    parallel: bool,
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
        arch_files, tb_files, Some(dir),
        /*check_uninit*/ false, /*inputs_start_uninit*/ false, /*check_uninit_ram*/ false,
        /*cdc_random*/ false, /*wave*/ None,
        /*debug*/ true, /*debug_depth*/ 1, /*debug_fsm*/ false,
        /*coverage*/ false, /*coverage_dat*/ None,
        parallel,
        /*threads*/ 1,
        /*pybind*/ false, /*test_file*/ None, /*pybind_module_name_override*/ None,
        /*no_exit*/ true,
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
) -> miette::Result<()> {
    run_sim_opts(arch_files, tb_files, outdir, check_uninit, inputs_start_uninit, check_uninit_ram,
        cdc_random, wave, debug, debug_depth, debug_fsm, coverage, coverage_dat, thread_sim_parallel,
        threads, pybind, test_file, pybind_module_name_override, /*no_exit=*/false)
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
) -> miette::Result<()> {
    // 1. Parse + type-check
    let all_files = resolve_use_imports(arch_files)?;
    let ms = MultiSource::from_files(&all_files)?;
    let (ast, symbols, overload_map) = run_check_multi_opts(&ms, thread_sim_parallel)?;

    // 2. Set up output directory
    let build_dir = outdir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("arch_sim_build"));
    fs::create_dir_all(&build_dir).into_diagnostic()?;

    // 3. Generate C++ models
    let models: Vec<arch::sim_codegen::SimModel> = if thread_sim_parallel {
        // Pre-lowering thread sim path: route every module containing
        // a `thread` block through the new emitter; reject mixed
        // modules (Phase 1 limitation).
        let mut out = Vec::new();
        for item in &ast.items {
            if let arch::ast::Item::Module(m) = item {
                let has_thread = m.body.iter().any(|i| matches!(i, arch::ast::ModuleBodyItem::Thread(_)));
                if has_thread {
                    let model = arch::sim_codegen::thread_sim::gen_module_thread(m, debug, wave.is_some(), threads)
                        .map_err(|e| miette::miette!("thread sim: {}", e))?;
                    out.push(model);
                }
            }
        }
        out
    } else {
        let mut sim = SimCodegen::new(&symbols, &ast, overload_map.clone()).check_uninit(check_uninit).inputs_start_uninit(inputs_start_uninit).check_uninit_ram(check_uninit_ram).cdc_random(cdc_random).debug(debug, debug_depth).with_debug_fsm(debug_fsm).coverage(coverage).coverage_dat(coverage_dat.clone());
        if coverage {
            // Build a SourceMap so the coverage dumper can render
            // file:line instead of opaque branch[N] ordinals.
            let segs: Vec<(usize, String, String)> = ms.segments.iter()
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
        let h_path   = build_dir.join(format!("{}.h",   model.class_name));
        let cpp_path = build_dir.join(format!("{}.cpp", model.class_name));
        fs::write(&h_path,   &model.header).into_diagnostic()?;
        fs::write(&cpp_path, &model.impl_).into_diagnostic()?;
        eprintln!("Generated {}", cpp_path.display());
        generated_cpps.push(cpp_path);
    }

    // 4. Write verilated.h / verilated.cpp stubs
    let verilated_h   = build_dir.join("verilated.h");
    let verilated_cpp = build_dir.join("verilated.cpp");
    fs::write(&verilated_h,   SimCodegen::verilated_h()).into_diagnostic()?;
    fs::write(&verilated_cpp, SimCodegen::verilated_cpp()).into_diagnostic()?;
    generated_cpps.push(verilated_cpp);

    // 4b. Thread sim runtime header (only used under --thread-sim parallel,
    // but emit unconditionally — cheap and keeps the build dir self-contained).
    let arch_thread_rt_h = build_dir.join("arch_thread_rt.h");
    fs::write(&arch_thread_rt_h, arch::sim_codegen::thread_sim::arch_thread_rt_h()).into_diagnostic()?;

    // ── Pybind11 mode ────────────────────────────────────────────────────
    if pybind {
        if thread_sim_parallel {
            return Err(miette::miette!("--pybind not yet supported with --thread-sim parallel"));
        }
        let mut sim = SimCodegen::new(&symbols, &ast, overload_map.clone()).check_uninit(check_uninit).inputs_start_uninit(inputs_start_uninit).check_uninit_ram(check_uninit_ram).cdc_random(cdc_random).debug(debug, debug_depth).with_debug_fsm(debug_fsm).coverage(coverage).coverage_dat(coverage_dat.clone());
        if coverage {
            let segs: Vec<(usize, String, String)> = ms.segments.iter()
                .map(|(start, _end, name, src)| (*start, name.clone(), src.clone()))
                .collect();
            sim = sim.with_source_map(arch::sim_codegen::SourceMap::new(segs));
        }
        let pybind_wrappers = sim.generate_pybind();
        if pybind_wrappers.is_empty() {
            eprintln!("warning: no pybind11 wrappers generated");
            return Ok(());
        }

        // Apply --pybind-module-name if provided. Retarget only the first
        // wrapper (the user's top module); subsequent wrappers (nested
        // modules) keep their auto-derived names. The override is a
        // string-replace on the generated .cpp so the PYBIND11_MODULE macro
        // matches the new class_name.
        let default_first_name = pybind_wrappers[0].class_name.clone();
        let effective_first_name = pybind_module_name_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_first_name.clone());

        let mut pybind_cpps: Vec<PathBuf> = Vec::new();
        let mut pybind_module_name = String::new();
        for (i, wrapper) in pybind_wrappers.iter().enumerate() {
            let (class_name, impl_src) = if i == 0 && pybind_module_name_override.is_some() {
                let new_name = &effective_first_name;
                let retargeted = wrapper.impl_
                    .replace(&format!("PYBIND11_MODULE({}, m)", default_first_name),
                             &format!("PYBIND11_MODULE({}, m)", new_name));
                (new_name.clone(), retargeted)
            } else {
                (wrapper.class_name.clone(), wrapper.impl_.clone())
            };
            let cpp_path = build_dir.join(format!("{}.cpp", class_name));
            fs::write(&cpp_path, &impl_src).into_diagnostic()?;
            eprintln!("Generated pybind11 wrapper: {}", cpp_path.display());
            pybind_cpps.push(cpp_path);
            if pybind_module_name.is_empty() {
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
            let obj = build_dir.join(
                cpp.file_stem().unwrap().to_string_lossy().into_owned() + ".o",
            );
            let mut cmd = std::process::Command::new("g++");
            cmd.arg("-std=c++17")
               .arg("-O2")
               .arg("-fPIC")
               .arg("-c")
               .arg("-I").arg(&build_dir);
            for flag in py_includes.split_whitespace() { cmd.arg(flag); }
            cmd.arg(cpp).arg("-o").arg(&obj);
            let status = cmd.status().into_diagnostic()?;
            if !status.success() {
                eprintln!("Pybind11 compilation failed (shared .o for {})", cpp.display());
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
            let mut cmd = std::process::Command::new("g++");
            cmd.arg("-std=c++17")
               .arg("-O2")
               .arg("-shared")
               .arg("-fPIC")
               .arg("-I").arg(&build_dir);
            for flag in py_includes.split_whitespace() { cmd.arg(flag); }
            cmd.arg(cpp_path);
            for obj in &shared_objs { cmd.arg(obj); }
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
            let python_dir = std::env::current_exe().ok()
                .and_then(|exe| exe.parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .map(|p| p.join("python")))
                .filter(|p| p.is_dir())
                .or_else(|| std::env::var("ARCH_PYTHON_DIR").ok().map(PathBuf::from))
                .or_else(|| std::env::current_dir().ok().map(|cwd| cwd.join("python")))
                .unwrap_or_else(|| PathBuf::from("python"));

            let shim_dir = python_dir.join("cocotb_shim");
            let cocotb_dir = python_dir.to_str().unwrap_or(".");
            let shim_str = shim_dir.to_str().unwrap_or(".");
            let build_str = build_dir.to_str().unwrap_or(".");

            let pythonpath = format!("{shim_str}:{cocotb_dir}:{build_str}");

            let test_path_abs = test_path.canonicalize().unwrap_or_else(|_| test_path.to_path_buf());
            let test_dir = test_path_abs.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let test_module_name = test_path_abs.file_stem()
                .unwrap_or_default().to_string_lossy().into_owned();

            // Derive the model class name. The class is the pybind module
            // name minus the `_pybind` suffix (matches emit_pybind_module).
            let model_class = pybind_module_name.strip_suffix("_pybind")
                .unwrap_or(&pybind_module_name).to_string();

            // Generated runner: runs user __main__, then dispatches any
            // registered @cocotb.test() functions.
            let runner_py = build_dir.join("_arch_cocotb_runner.py");
            let runner_src = format!(r#"import sys
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
                test_path   = test_path_abs.display(),
                test_dir    = test_dir.display(),
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
        eprintln!("No testbench files supplied — generated models are in {}/", build_dir.display());
        eprintln!("Compile with: g++ {}/verilated.cpp {}/V*.cpp <your_tb.cpp> -I{} -o sim_out",
            build_dir.display(), build_dir.display(), build_dir.display());
        return Ok(());
    }

    // 5. Compile with g++
    let sim_bin = build_dir.join("sim_out");
    let mut cmd = std::process::Command::new("g++");
    let cpp_std = if thread_sim_parallel { "-std=c++20" } else { "-std=c++17" };
    cmd.arg(cpp_std);
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
    let run_status = run_cmd
        .status()
        .into_diagnostic()?;

    std::process::exit(run_status.code().unwrap_or(1));
}

/// Resolve `use PkgName;` imports: find PkgName.arch files relative to the
/// first input file's directory. Returns an extended MultiSource with
/// dependency files prepended.
/// Locate the shipped standard library directory containing curated bus
/// definitions (BusAxiStream, BusAxiLite, BusApb, etc.). Resolution:
///   1. `ARCH_STDLIB_PATH` env override (absolute path to stdlib/)
///   2. Disabled entirely if `ARCH_NO_STDLIB=1`
///   3. `<exe>/../stdlib/` — matches `cargo run` layout (target/debug/arch → ../../stdlib)
///   4. `<exe>/../../stdlib/` — matches cargo workspace runs
///   5. `<exe>/../share/arch/stdlib/` — matches Unix `<prefix>/bin/arch` installs
/// Returns None if none of these resolve to an existing directory.
fn resolve_stdlib_dir() -> Option<PathBuf> {
    if std::env::var("ARCH_NO_STDLIB").is_ok() { return None; }
    if let Ok(p) = std::env::var("ARCH_STDLIB_PATH") {
        let p = PathBuf::from(p);
        if p.is_dir() { return Some(p); }
    }
    let exe = std::env::current_exe().ok()?;
    for up in 1..=4 {
        let mut candidate = exe.clone();
        for _ in 0..up {
            candidate = candidate.parent()?.to_path_buf();
        }
        let stdlib = candidate.join("stdlib");
        if stdlib.is_dir() { return Some(stdlib); }
    }
    // Unix prefix install: /usr/local/bin/arch → /usr/local/share/arch/stdlib
    let exe_parent = exe.parent()?;
    let prefix = exe_parent.parent()?;
    let share = prefix.join("share").join("arch").join("stdlib");
    if share.is_dir() { return Some(share); }
    None
}

fn resolve_use_imports(files: &[PathBuf]) -> miette::Result<Vec<PathBuf>> {
    use std::collections::HashSet;

    let base_dir = files.first()
        .and_then(|f| f.parent())
        .unwrap_or(std::path::Path::new("."));

    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut all_defined_modules: HashSet<String> = HashSet::new();
    let mut queue: Vec<PathBuf> = files.to_vec();

    // Process files, discovering new dependencies via `use`
    while let Some(file) = queue.pop() {
        let canon = file.canonicalize().unwrap_or_else(|_| file.clone());
        if seen.contains(&canon) {
            continue;
        }
        seen.insert(canon);

        let source = fs::read_to_string(&file).into_diagnostic()?;
        let tokens = lexer::tokenize(&source).map_err(|_| {
            miette::miette!("Lexer error in {}", file.display())
        })?;
        let mut p = parser::Parser::new(tokens, &source);
        let parsed = p.parse_source_file().map_err(|err| {
            Report::new(err).with_source_code(NamedSource::new(file.display().to_string(), source.clone()))
        })?;

        // Find `use` items and queue their files
        let mut deps = Vec::new();
        for item in &parsed.items {
            if let arch::ast::Item::Use(u) = item {
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
                        if p.exists() { deps.push(p); found = true; break; }
                    }
                }
                if found { continue; }
                if let Some(stdlib) = resolve_stdlib_dir() {
                    let p = stdlib.join(&file_name);
                    if p.exists() {
                        deps.push(p);
                    }
                }
            }
        }

        // Track all module names defined across all input files
        for item in &parsed.items {
            match item {
                Item::Module(m) => { all_defined_modules.insert(m.name.name.clone()); }
                Item::Fsm(f) => { all_defined_modules.insert(f.name.name.clone()); }
                Item::Counter(c) => { all_defined_modules.insert(c.name.name.clone()); }
                Item::Pipeline(p) => { all_defined_modules.insert(p.name.name.clone()); }
                Item::Synchronizer(s) => { all_defined_modules.insert(s.name.name.clone()); }
                Item::Fifo(f) => { all_defined_modules.insert(f.name.name.clone()); }
                Item::Ram(r) => { all_defined_modules.insert(r.name.name.clone()); }
                Item::Arbiter(a) => { all_defined_modules.insert(a.name.name.clone()); }
                _ => {}
            }
        }

        // Find inst references and look for .archi interface files
        for item in &parsed.items {
            let insts = match item {
                Item::Module(m) => m.body.iter()
                    .filter_map(|b| if let arch::ast::ModuleBodyItem::Inst(i) = b { Some(&i.module_name.name) } else { None })
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for inst_name in insts {
                if all_defined_modules.contains(inst_name.as_str()) { continue; }
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
                        if p.exists() { deps.push(p); break; }
                        let p = std::path::Path::new(dir).join(format!("{inst_name}.arch"));
                        if p.exists() { deps.push(p); break; }
                    }
                }
                // Fall back to the shipped standard library.
                if let Some(stdlib) = resolve_stdlib_dir() {
                    let p = stdlib.join(format!("{inst_name}.arch"));
                    if p.exists() { deps.push(p); continue; }
                    let p = stdlib.join(format!("{inst_name}.archi"));
                    if p.exists() { deps.push(p); }
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
    let orig_set: HashSet<PathBuf> = files.iter()
        .map(|f| f.canonicalize().unwrap_or_else(|_| f.clone()))
        .collect();
    let mut dep_files: Vec<PathBuf> = Vec::new();
    let mut main_files: Vec<PathBuf> = Vec::new();
    let mut seen2: HashSet<PathBuf> = HashSet::new();
    for f in &all_files {
        let canon = f.canonicalize().unwrap_or_else(|_| f.clone());
        if seen2.contains(&canon) { continue; }
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

fn run_check_multi(
    ms: &MultiSource,
) -> miette::Result<(arch::ast::SourceFile, resolve::SymbolTable, std::collections::HashMap<usize, usize>)> {
    run_check_multi_opts(ms, /*skip_lower_threads=*/ false)
}

fn run_check_multi_opts(
    ms: &MultiSource,
    skip_lower_threads: bool,
) -> miette::Result<(arch::ast::SourceFile, resolve::SymbolTable, std::collections::HashMap<usize, usize>)> {
    let source = &ms.combined;

    // Lex
    let tokens = lexer::tokenize(source).map_err(|spans| {
        let offset = spans[0].start;
        let (filename, file_source, local_offset) = ms.locate(offset);
        let err = CompileError::LexerError {
            span: miette::SourceSpan::new(local_offset.into(), (spans[0].end - spans[0].start).into()),
        };
        Report::new(err).with_source_code(NamedSource::new(filename.to_string(), file_source.to_string()))
    })?;

    // Parse
    let mut p = parser::Parser::new(tokens, source);
    let parsed_ast = p.parse_source_file().map_err(|err| {
        ms.report_error(err)
    })?;

    // Precedence ambiguity check on user source (pre-elaboration, so generated
    // reductions from thread lowering etc. don't trigger spurious warnings)
    let prec_errors = arch::typecheck::check_precedence(&parsed_ast);
    if !prec_errors.is_empty() {
        let err = prec_errors.into_iter().next().unwrap();
        return Err(ms.report_error(err));
    }

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

    // Lower thread blocks to FSM + inst (skipped under --thread-sim parallel,
    // where the new pre-lowering thread sim emitter consumes thread blocks
    // directly via coroutines).
    let ast = if skip_lower_threads {
        ast
    } else {
        elaborate::lower_threads(ast).map_err(|errs| {
            let err = errs.into_iter().next().unwrap();
            ms.report_error(err)
        })?
    };

    // Lower `pipe_reg<T, N>` ports with N > 1 into an N-stage cascade.
    let ast = elaborate::lower_pipe_reg_ports(ast).map_err(|errs| {
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
    let (warnings, overload_map) = checker.check().map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        ms.report_error(err)
    })?;

    for w in &warnings {
        let (filename, _, local_offset) = ms.locate(w.span.start);
        eprintln!("warning: {} ({}:{})", w.message, filename, local_offset);
    }

    Ok((ast, symbols, overload_map))
}

