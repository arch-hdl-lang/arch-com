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
    /// Type-check an ARCH source file
    Check {
        /// Input .arch file
        file: PathBuf,
    },
    /// Compile ARCH to SystemVerilog
    Build {
        /// Input .arch file
        file: PathBuf,
        /// Output .sv file
        #[arg(short, long)]
        o: Option<PathBuf>,
    },
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { file } => {
            let source = fs::read_to_string(&file).into_diagnostic()?;
            run_check(&source, file.display().to_string())?;
            eprintln!("OK: no errors");
            Ok(())
        }
        Command::Build { file, o } => {
            let source = fs::read_to_string(&file).into_diagnostic()?;
            let (ast, symbols) = run_check(&source, file.display().to_string())?;

            let comments = lexer::extract_comments(&source);
            let codegen = Codegen::new(&symbols, &ast).with_comments(comments);
            let sv = codegen.generate();

            let out_path = o.unwrap_or_else(|| file.with_extension("sv"));
            fs::write(&out_path, &sv).into_diagnostic()?;
            eprintln!("Wrote {}", out_path.display());
            Ok(())
        }
    }
}

fn run_check(
    source: &str,
    filename: String,
) -> miette::Result<(arch::ast::SourceFile, resolve::SymbolTable)> {
    // Lex
    let tokens = lexer::tokenize(source).map_err(|spans| {
        let err = CompileError::LexerError {
            span: crate::lexer_span_to_source_span(spans[0]),
        };
        Report::new(err).with_source_code(NamedSource::new(filename.clone(), source.to_string()))
    })?;

    // Parse
    let mut p = parser::Parser::new(tokens);
    let parsed_ast = p.parse_source_file().map_err(|err| {
        Report::new(err).with_source_code(NamedSource::new(filename.clone(), source.to_string()))
    })?;

    // Elaborate (expand generate blocks)
    let ast = elaborate::elaborate(parsed_ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        Report::new(err).with_source_code(NamedSource::new(filename.clone(), source.to_string()))
    })?;

    // Resolve
    let symbols = resolve::resolve(&ast).map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        Report::new(err).with_source_code(NamedSource::new(filename.clone(), source.to_string()))
    })?;

    // Type check
    let checker = TypeChecker::new(&symbols, &ast);
    let warnings = checker.check().map_err(|errs| {
        let err = errs.into_iter().next().unwrap();
        Report::new(err).with_source_code(NamedSource::new(filename.clone(), source.to_string()))
    })?;

    for w in &warnings {
        eprintln!("warning: {} (at byte offset {})", w.message, w.span.start);
    }

    Ok((ast, symbols))
}

fn lexer_span_to_source_span(span: arch::lexer::Span) -> miette::SourceSpan {
    miette::SourceSpan::new(span.start.into(), (span.end - span.start).into())
}
