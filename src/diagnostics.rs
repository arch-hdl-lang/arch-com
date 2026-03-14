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
}
