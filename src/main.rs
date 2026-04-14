use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, NamedSource, Report};
use std::fs;
use std::path::PathBuf;

use arch::ast::Item;
use arch::codegen::Codegen;
use arch::diagnostics::CompileError;
use arch::elaborate;
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
        /// Generate pybind11 Python module for cocotb-compatible testing
        #[arg(long)]
        pybind: bool,
        /// Python test file to run with arch_cocotb adapter (requires --pybind)
        #[arg(long)]
        test: Option<PathBuf>,
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

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { files } => {
            let all_files = resolve_use_imports(&files)?;
            let ms = MultiSource::from_files(&all_files)?;
            let _ = run_check_multi(&ms)?;
            eprintln!("OK: no errors");
            Ok(())
        }
        Command::Sim { arch_files, tb_files, outdir, check_uninit, cdc_random, wave, debug, debug_depth, debug_fsm, pybind, test } => {
            let dbg_ports = debug || debug_fsm;  // any debug option implies port logging
            run_sim(&arch_files, &tb_files, outdir.as_deref(), check_uninit, cdc_random, wave.as_deref(), dbg_ports, debug_depth, debug_fsm, pybind, test.as_deref())
        }
        Command::Build { files, o } => {
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
        }
    }
}

fn run_sim(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
    check_uninit: bool,
    cdc_random: bool,
    wave: Option<&std::path::Path>,
    debug: bool,
    debug_depth: u32,
    debug_fsm: bool,
    pybind: bool,
    test_file: Option<&std::path::Path>,
) -> miette::Result<()> {
    // 1. Parse + type-check
    let all_files = resolve_use_imports(arch_files)?;
    let ms = MultiSource::from_files(&all_files)?;
    let (ast, symbols, overload_map) = run_check_multi(&ms)?;

    // 2. Set up output directory
    let build_dir = outdir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("arch_sim_build"));
    fs::create_dir_all(&build_dir).into_diagnostic()?;

    // 3. Generate C++ models
    let sim = SimCodegen::new(&symbols, &ast, overload_map).check_uninit(check_uninit).cdc_random(cdc_random).debug(debug, debug_depth).with_debug_fsm(debug_fsm);
    let models = sim.generate();

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

    // ── Pybind11 mode ────────────────────────────────────────────────────
    if pybind {
        let pybind_wrappers = sim.generate_pybind();
        if pybind_wrappers.is_empty() {
            eprintln!("warning: no pybind11 wrappers generated");
            return Ok(());
        }

        let mut pybind_cpps: Vec<PathBuf> = Vec::new();
        let mut pybind_module_name = String::new();
        for wrapper in &pybind_wrappers {
            let cpp_path = build_dir.join(format!("{}.cpp", wrapper.class_name));
            fs::write(&cpp_path, &wrapper.impl_).into_diagnostic()?;
            eprintln!("Generated pybind11 wrapper: {}", cpp_path.display());
            pybind_cpps.push(cpp_path);
            if pybind_module_name.is_empty() {
                pybind_module_name = wrapper.class_name.clone();
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

        // Compile shared library
        let so_path = build_dir.join(format!("{pybind_module_name}{ext_suffix}"));
        let mut cmd = std::process::Command::new("g++");
        cmd.arg("-std=c++17")
           .arg("-O2")
           .arg("-shared")
           .arg("-fPIC")
           .arg("-I").arg(&build_dir);

        for flag in py_includes.split_whitespace() {
            cmd.arg(flag);
        }
        for cpp in &generated_cpps {
            cmd.arg(cpp);
        }
        for cpp in &pybind_cpps {
            cmd.arg(cpp);
        }
        cmd.arg("-o").arg(&so_path);

        // macOS: suppress undefined symbol errors (Python symbols resolved at import time)
        #[cfg(target_os = "macos")]
        cmd.arg("-undefined").arg("dynamic_lookup");

        eprintln!("Compiling pybind11 module...");
        let status = cmd.status().into_diagnostic()?;
        if !status.success() {
            eprintln!("Pybind11 compilation failed");
            std::process::exit(1);
        }
        eprintln!("Built: {}", so_path.display());

        // If --test is given, run the test file
        if let Some(test_path) = test_file {
            eprintln!("Running test: {}", test_path.display());
            let project_root = std::env::current_dir().into_diagnostic()?;
            let python_dir = project_root.join("python");
            let shim_dir = python_dir.join("cocotb_shim");
            let cocotb_dir = python_dir.to_str().unwrap_or(".");
            let shim_str = shim_dir.to_str().unwrap_or(".");
            let build_str = build_dir.to_str().unwrap_or(".");

            let pythonpath = format!("{shim_str}:{cocotb_dir}:{build_str}");

            let status = std::process::Command::new("python3")
                .arg(test_path)
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
    cmd.arg("-std=c++17")
       .arg("-O1")
       .arg("-I").arg(&build_dir);

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
    let run_status = run_cmd
        .status()
        .into_diagnostic()?;

    std::process::exit(run_status.code().unwrap_or(1));
}

/// Resolve `use PkgName;` imports: find PkgName.arch files relative to the
/// first input file's directory. Returns an extended MultiSource with
/// dependency files prepended.
fn resolve_use_imports(files: &[PathBuf]) -> miette::Result<Vec<PathBuf>> {
    use std::collections::HashSet;

    let base_dir = files.first()
        .and_then(|f| f.parent())
        .unwrap_or(std::path::Path::new("."));

    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
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
                let dep_path = base_dir.join(format!("{}.arch", u.name.name));
                if dep_path.exists() {
                    deps.push(dep_path);
                }
            }
        }

        // Find inst references and look for .archi interface files
        let defined_modules: HashSet<String> = parsed.items.iter()
            .filter_map(|item| match item {
                Item::Module(m) => Some(m.name.name.clone()),
                Item::Fsm(f) => Some(f.name.name.clone()),
                Item::Counter(c) => Some(c.name.name.clone()),
                Item::Pipeline(p) => Some(p.name.name.clone()),
                _ => None,
            })
            .collect();

        for item in &parsed.items {
            let insts = match item {
                Item::Module(m) => m.body.iter()
                    .filter_map(|b| if let arch::ast::ModuleBodyItem::Inst(i) = b { Some(&i.module_name.name) } else { None })
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for inst_name in insts {
                if defined_modules.contains(inst_name.as_str()) { continue; }
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

    // Lower thread blocks to FSM + inst
    let ast = elaborate::lower_threads(ast).map_err(|errs| {
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

