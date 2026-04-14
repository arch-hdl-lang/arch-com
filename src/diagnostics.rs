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
            CompileError::UnexpectedToken { expected, found, span } =>
                CompileError::UnexpectedToken { expected, found, span: respan(span, new_offset) },
            CompileError::MismatchedClosingName { expected, found, span } =>
                CompileError::MismatchedClosingName { expected, found, span: respan(span, new_offset) },
            CompileError::UndefinedName { name, span } =>
                CompileError::UndefinedName { name, span: respan(span, new_offset) },
            CompileError::UndefinedModule { name, hint, span } =>
                CompileError::UndefinedModule { name, hint, span: respan(span, new_offset) },
            CompileError::DuplicateDefinition { name, span } =>
                CompileError::DuplicateDefinition { name, span: respan(span, new_offset) },
            CompileError::TypeMismatch { expected, found, span } =>
                CompileError::TypeMismatch { expected, found, span: respan(span, new_offset) },
            CompileError::WidthMismatch { target_width, value_width, span } =>
                CompileError::WidthMismatch { target_width, value_width, span: respan(span, new_offset) },
            CompileError::MultipleDrivers { name, span } =>
                CompileError::MultipleDrivers { name, span: respan(span, new_offset) },
            CompileError::UndriveOutput { name, span } =>
                CompileError::UndriveOutput { name, span: respan(span, new_offset) },
            CompileError::NamingViolation { message, span } =>
                CompileError::NamingViolation { message, span: respan(span, new_offset) },
            CompileError::LexerError { span } =>
                CompileError::LexerError { span: respan(span, new_offset) },
            CompileError::General { message, span } =>
                CompileError::General { message, span: respan(span, new_offset) },
            CompileError::UnexpectedEof => CompileError::UnexpectedEof,
        }
    }
}
