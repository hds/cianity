mod rules;

use crate::{ast::Root, error::Diagnostic};

/// Run all static validation rules over a parsed `Root` node.
///
/// Returns the complete list of semantic [`Diagnostic`]s.  Parse errors are
/// reported separately via [`crate::parser::Parse::errors`].
#[must_use]
pub fn validate(root: &Root) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    rules::check_root(root, &mut diagnostics);
    diagnostics
}
