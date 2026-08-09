use crate::lexer::Span;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum CompileError {
    #[error("unexpected token: expected {expected}, found {found}")]
    UnexpectedToken {
        expected: String,
        found: String,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("unexpected end of file")]
    UnexpectedEof,

    #[error("mismatched closing name: expected `{expected}`, found `{found}`")]
    MismatchedClosingName {
        expected: String,
        found: String,
        #[label("closing name here")]
        span: SourceSpan,
    },

    #[error("undefined name: `{name}`")]
    UndefinedName {
        name: String,
        #[label("not found")]
        span: SourceSpan,
    },

    #[error("undefined module: `{name}`")]
    #[diagnostic(help("{hint}"))]
    UndefinedModule {
        name: String,
        hint: String,
        #[label("not found")]
        span: SourceSpan,
    },

    #[error("duplicate definition: `{name}`")]
    DuplicateDefinition {
        name: String,
        #[label("redefined here")]
        span: SourceSpan,
    },

    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("width mismatch: target is {target_width} bits, value is {value_width} bits")]
    WidthMismatch {
        target_width: u32,
        value_width: u32,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("signal `{name}` has multiple drivers")]
    MultipleDrivers {
        name: String,
        #[label("second driver here")]
        span: SourceSpan,
    },

    #[error("output port `{name}` is not driven")]
    UndriveOutput {
        name: String,
        #[label("declared here")]
        span: SourceSpan,
    },

    #[error("naming convention violation: {message}")]
    NamingViolation {
        message: String,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("lexer error")]
    LexerError {
        #[label("invalid token")]
        span: SourceSpan,
    },

    #[error("{message}")]
    General {
        message: String,
        #[label("here")]
        span: SourceSpan,
    },
}

#[derive(Debug)]
pub struct CompileWarning {
    pub message: String,
    pub span: Span,
}

pub fn span_to_source_span(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), (span.end - span.start).into())
}

impl CompileError {
    pub fn unexpected_token(expected: &str, found: &str, span: Span) -> Self {
        CompileError::UnexpectedToken {
            expected: expected.to_string(),
            found: found.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn mismatched_closing(expected: &str, found: &str, span: Span) -> Self {
        CompileError::MismatchedClosingName {
            expected: expected.to_string(),
            found: found.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn undefined(name: &str, span: Span) -> Self {
        CompileError::UndefinedName {
            name: name.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn undefined_module(name: &str, hint: &str, span: Span) -> Self {
        CompileError::UndefinedModule {
            name: name.to_string(),
            hint: hint.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn duplicate(name: &str, span: Span) -> Self {
        CompileError::DuplicateDefinition {
            name: name.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn type_mismatch(expected: &str, found: &str, span: Span) -> Self {
        CompileError::TypeMismatch {
            expected: expected.to_string(),
            found: found.to_string(),
            span: span_to_source_span(span),
        }
    }

    pub fn general(message: &str, span: Span) -> Self {
        CompileError::General {
            message: message.to_string(),
            span: span_to_source_span(span),
        }
    }

    /// Get the byte offset of this error's span in the combined source.
    pub fn span_offset(&self) -> usize {
        match self {
            CompileError::UnexpectedToken { span, .. }
            | CompileError::MismatchedClosingName { span, .. }
            | CompileError::UndefinedName { span, .. }
            | CompileError::UndefinedModule { span, .. }
            | CompileError::DuplicateDefinition { span, .. }
            | CompileError::TypeMismatch { span, .. }
            | CompileError::WidthMismatch { span, .. }
            | CompileError::MultipleDrivers { span, .. }
            | CompileError::UndriveOutput { span, .. }
            | CompileError::NamingViolation { span, .. }
            | CompileError::LexerError { span, .. }
            | CompileError::General { span, .. } => span.offset(),
            CompileError::UnexpectedEof => 0,
        }
    }

    /// Create a copy of this error with the span offset adjusted for multi-file reporting.
    pub fn relocate(self, new_offset: usize) -> Self {
        fn respan(span: SourceSpan, new_offset: usize) -> SourceSpan {
            SourceSpan::new(new_offset.into(), span.len().into())
        }
        match self {
            CompileError::UnexpectedToken {
                expected,
                found,
                span,
            } => CompileError::UnexpectedToken {
                expected,
                found,
                span: respan(span, new_offset),
            },
            CompileError::MismatchedClosingName {
                expected,
                found,
                span,
            } => CompileError::MismatchedClosingName {
                expected,
                found,
                span: respan(span, new_offset),
            },
            CompileError::UndefinedName { name, span } => CompileError::UndefinedName {
                name,
                span: respan(span, new_offset),
            },
            CompileError::UndefinedModule { name, hint, span } => CompileError::UndefinedModule {
                name,
                hint,
                span: respan(span, new_offset),
            },
            CompileError::DuplicateDefinition { name, span } => CompileError::DuplicateDefinition {
                name,
                span: respan(span, new_offset),
            },
            CompileError::TypeMismatch {
                expected,
                found,
                span,
            } => CompileError::TypeMismatch {
                expected,
                found,
                span: respan(span, new_offset),
            },
            CompileError::WidthMismatch {
                target_width,
                value_width,
                span,
            } => CompileError::WidthMismatch {
                target_width,
                value_width,
                span: respan(span, new_offset),
            },
            CompileError::MultipleDrivers { name, span } => CompileError::MultipleDrivers {
                name,
                span: respan(span, new_offset),
            },
            CompileError::UndriveOutput { name, span } => CompileError::UndriveOutput {
                name,
                span: respan(span, new_offset),
            },
            CompileError::NamingViolation { message, span } => CompileError::NamingViolation {
                message,
                span: respan(span, new_offset),
            },
            CompileError::LexerError { span } => CompileError::LexerError {
                span: respan(span, new_offset),
            },
            CompileError::General { message, span } => CompileError::General {
                message,
                span: respan(span, new_offset),
            },
            CompileError::UnexpectedEof => CompileError::UnexpectedEof,
        }
    }
}

/// One `CompileError` bound to the source it came from.
///
/// A batch can span several input files (`arch check a.arch b.arch`), and a
/// `miette::Report` carries a single `source_code`, so the snippet has to
/// travel with the individual error rather than with the batch. Every other
/// `Diagnostic` method forwards to the inner error, so rendering of a single
/// error is byte-identical to what it was before batching.
#[derive(Debug)]
pub struct SourcedError {
    inner: CompileError,
    source: miette::NamedSource<String>,
}

impl SourcedError {
    pub fn new(inner: CompileError, filename: &str, source: &str) -> Self {
        Self {
            inner,
            source: miette::NamedSource::new(filename, source.to_string()),
        }
    }
}

impl std::fmt::Display for SourcedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl std::error::Error for SourcedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.inner)
    }
}

impl Diagnostic for SourcedError {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source)
    }
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.inner.code()
    }
    fn severity(&self) -> Option<miette::Severity> {
        self.inner.severity()
    }
    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.inner.help()
    }
    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.inner.url()
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.inner.labels()
    }
    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.inner.related()
    }
    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.inner.diagnostic_source()
    }
}

/// Every error from one compiler pass, rendered as a single report.
///
/// The passes already accumulate into a `Vec<CompileError>`; before this they
/// were truncated to the first element at the reporting boundary, so a file
/// with N independent errors cost N compile-and-fix round trips. `miette`
/// renders `#[related]` entries with their own spans and snippets, so the
/// per-error output is unchanged — there are just N of them now.
#[derive(Debug, Error, Diagnostic)]
#[error("{}", summary(.related.len(), *.truncated))]
pub struct CompileErrors {
    #[related]
    pub related: Vec<SourcedError>,
    /// Errors dropped past `MAX_REPORTED`, mentioned in the summary line so a
    /// truncated batch never looks complete.
    pub truncated: usize,
}

/// Upper bound on errors rendered at once. A pathological file can accumulate
/// thousands; past a screenful the list stops being actionable and just buries
/// the first few, which are the ones worth fixing.
pub const MAX_REPORTED: usize = 50;

fn summary(shown: usize, truncated: usize) -> String {
    let plural = if shown == 1 { "" } else { "s" };
    if truncated == 0 {
        format!("{shown} error{plural}")
    } else {
        format!("{shown} error{plural} shown, {truncated} more not listed")
    }
}

impl CompileErrors {
    /// Sort by source position and cap at [`MAX_REPORTED`].
    ///
    /// Source order matters as much as completeness: passes push errors in
    /// visit order, which is neither source order nor stable, so without this
    /// the list would jump around the file and could differ between runs.
    pub fn new(mut errors: Vec<(usize, SourcedError)>) -> Self {
        errors.sort_by_key(|(offset, _)| *offset);
        let total = errors.len();
        let truncated = total.saturating_sub(MAX_REPORTED);
        Self {
            related: errors
                .into_iter()
                .take(MAX_REPORTED)
                .map(|(_, e)| e)
                .collect(),
            truncated,
        }
    }
}
