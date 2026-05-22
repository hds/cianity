/// Severity level of a diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A structured diagnostic produced by the parser or the validator.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: std::ops::Range<usize>,
}

/// An error produced during parsing, before semantic validation.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: std::ops::Range<usize>,
}

impl From<ParseError> for Diagnostic {
    fn from(e: ParseError) -> Self {
        Self {
            severity: Severity::Error,
            message: e.message,
            span: e.span,
        }
    }
}
