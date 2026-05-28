use ciane::{
    ast::{AstNode, Root},
    formatter,
    parser::Parse,
};
use tower_lsp_server::ls_types::{Position, Range, TextEdit};

use super::util::offset_to_position;

/// Returns a whole-document `TextEdit` with the formatted source, or an empty
/// vec if the document is already canonical, or `None` if formatting is not
/// possible (e.g. the file contains a `defaults` block).
#[must_use]
pub(super) fn edits(parse: &Parse, source: &str) -> Option<Vec<TextEdit>> {
    let root = Root::cast(parse.syntax())?;
    let formatted = formatter::format(&root).ok()?;
    if formatted == source {
        return Some(vec![]);
    }
    let end = offset_to_position(source, source.len());
    Some(vec![TextEdit {
        range: Range {
            start: Position::new(0, 0),
            end,
        },
        new_text: formatted,
    }])
}
