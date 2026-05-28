use ciane::syntax::{SyntaxNode, SyntaxToken};
use rowan::{TextRange, TextSize, TokenAtOffset};
use tower_lsp_server::ls_types::{Position, Range};

pub(super) fn text_size(offset: usize) -> TextSize {
    TextSize::from(u32::try_from(offset).unwrap_or(u32::MAX))
}

pub(super) fn offset_to_position(source: &str, offset: usize) -> Position {
    let clamped = offset.min(source.len());
    let prefix = &source[..clamped];
    let line = prefix.bytes().filter(|&b| b == b'\n').count();
    let col = prefix.rfind('\n').map_or(clamped, |nl| clamped - nl - 1);
    Position::new(
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}

pub(super) fn position_to_offset(source: &str, pos: Position) -> usize {
    let mut current_line = 0u32;
    let mut line_start = 0usize;
    for (i, ch) in source.char_indices() {
        if current_line == pos.line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }
    if current_line < pos.line {
        return source.len();
    }
    let remainder = &source[line_start..];
    let line_len = remainder.find('\n').unwrap_or(remainder.len());
    line_start + (pos.character as usize).min(line_len)
}

pub(super) fn range_to_lsp(source: &str, range: TextRange) -> Range {
    Range {
        start: offset_to_position(source, usize::from(range.start())),
        end: offset_to_position(source, usize::from(range.end())),
    }
}

/// Returns the token at `offset`, preferring rightward on boundaries over trivia.
pub(super) fn token_at(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    match root.token_at_offset(text_size(offset)) {
        TokenAtOffset::None => None,
        TokenAtOffset::Single(t) => Some(t),
        TokenAtOffset::Between(left, right) => {
            if right.kind().is_trivia() {
                Some(left)
            } else {
                Some(right)
            }
        }
    }
}
