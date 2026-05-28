mod printer;

use crate::ast::Root;

/// Formatting failed — the source cannot be formatted without data loss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// `defaults` blocks have no typed AST node yet; formatting one would
    /// silently discard its content.
    DefaultsBlockUnsupported,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DefaultsBlockUnsupported => {
                f.write_str("`defaults` blocks are not yet supported by the formatter")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Format a parsed `ciane` source file into canonical text.
///
/// # Errors
///
/// Returns [`FormatError::DefaultsBlockUnsupported`] if the file contains a
/// `defaults` block, which cannot be safely formatted without losing content.
#[must_use = "the formatted output is the return value"]
pub fn format(root: &Root) -> Result<String, FormatError> {
    printer::Printer::new().print_root(root)
}
