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

