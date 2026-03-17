use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, NamedSource, Report};
use std::fs;
use std::path::PathBuf;

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
            let ms = MultiSource::from_files(&files)?;
            let _ = run_check_multi(&ms)?;
            eprintln!("OK: no errors");
            Ok(())
        }
        Command::Sim { arch_files, tb_files, outdir } => {
            run_sim(&arch_files, &tb_files, outdir.as_deref())
        }
        Command::Build { files, o } => {
            let ms = MultiSource::from_files(&files)?;
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
            Ok(())
        }
    }
}

fn run_sim(
    arch_files: &[PathBuf],
    tb_files: &[PathBuf],
    outdir: Option<&std::path::Path>,
) -> miette::Result<()> {
    // 1. Parse + type-check
    let ms = MultiSource::from_files(arch_files)?;
    let (ast, symbols, overload_map) = run_check_multi(&ms)?;

    // 2. Set up output directory
    let build_dir = outdir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("arch_sim_build"));
    fs::create_dir_all(&build_dir).into_diagnostic()?;

    // 3. Generate C++ models
    let sim = SimCodegen::new(&symbols, &ast, overload_map);
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
    let run_status = std::process::Command::new(&sim_bin)
        .status()
        .into_diagnostic()?;

    std::process::exit(run_status.code().unwrap_or(1));
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
    let mut p = parser::Parser::new(tokens);
    let parsed_ast = p.parse_source_file().map_err(|err| {
        ms.report_error(err)
    })?;

    // Elaborate (expand generate blocks)
    let ast = elaborate::elaborate(parsed_ast).map_err(|errs| {
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

