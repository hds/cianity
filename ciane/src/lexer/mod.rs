mod token;

use crate::syntax::SyntaxKind;
pub(crate) use token::keyword_kind;

/// The lexer context determines how raw input is tokenised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexMode {
    /// Normal tokenisation — keywords, identifiers, punctuation, trivia.
    Normal,
    /// After `=` inside a `(…)` attribute list (and the next non-ws char is not `[`):
    /// consume everything up to the next `,`, `)`, or newline as a `BareValue` token.
    AttrValue,
    /// Inside the `{ }` body of a step or single-step job: capture raw text.
    Shell,
    /// A single item inside an `artifacts = [...]` list.
    /// Captures everything up to the next `,`, `]`, or newline as a `PathValue` token.
    /// Falls back to Normal mode when a `]` or newline is the first non-whitespace
    /// character (handles trailing commas and empty lists gracefully).
    PathItem,
}

struct Lexer<'src> {
    src: &'src str,
    pos: usize,
    mode: LexMode,
    /// Brace nesting depth when in `Shell` mode.
    shell_depth: u32,
    /// Nesting depth of `(…)` blocks.
    paren_depth: u32,
}

impl<'src> Lexer<'src> {
    fn new(src: &'src str) -> Self {
        Self {
            src,
            pos: 0,
            mode: LexMode::Normal,
            shell_depth: 0,
            paren_depth: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos + 1).copied()
    }

    fn advance(&mut self, n: usize) -> &'src str {
        let slice = &self.src[self.pos..self.pos + n];
        self.pos += n;
        slice
    }

    fn take_while(&mut self, predicate: impl Fn(u8) -> bool) -> &'src str {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if predicate(b) {
                self.pos += 1;
            } else {
                break;
            }
        }
        &self.src[start..self.pos]
    }

    /// Peek past whitespace to check whether the next non-whitespace byte is `[`.
    fn next_non_ws_is_bracket(&self) -> bool {
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        bytes.get(i) == Some(&b'[')
    }

    fn next_normal(&mut self) -> Option<(SyntaxKind, &'src str)> {
        let b = self.peek()?;

        // Trivia: horizontal whitespace
        if b == b' ' || b == b'\t' {
            let text = self.take_while(|c| c == b' ' || c == b'\t');
            return Some((SyntaxKind::Whitespace, text));
        }

        // Trivia: newlines (LF, CRLF, or bare CR)
        if b == b'\n' {
            return Some((SyntaxKind::Newline, self.advance(1)));
        }
        if b == b'\r' {
            if self.peek2() == Some(b'\n') {
                return Some((SyntaxKind::Newline, self.advance(2)));
            }
            return Some((SyntaxKind::Newline, self.advance(1)));
        }

        // Trivia: line comments
        if b == b'#' {
            let text = self.take_while(|c| c != b'\n' && c != b'\r');
            return Some((SyntaxKind::LineComment, text));
        }

        // Single-character punctuation
        match b {
            b'{' => {
                return Some((SyntaxKind::LBrace, self.advance(1)));
            }
            b'}' => return Some((SyntaxKind::RBrace, self.advance(1))),
            b'[' => return Some((SyntaxKind::LBracket, self.advance(1))),
            b']' => return Some((SyntaxKind::RBracket, self.advance(1))),
            b'(' => {
                self.paren_depth += 1;
                return Some((SyntaxKind::LParen, self.advance(1)));
            }
            b')' => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
                return Some((SyntaxKind::RParen, self.advance(1)));
            }
            b'=' => {
                let text = self.advance(1);
                if self.paren_depth > 0 && !self.next_non_ws_is_bracket() {
                    self.mode = LexMode::AttrValue;
                }
                return Some((SyntaxKind::Eq, text));
            }
            b',' => return Some((SyntaxKind::Comma, self.advance(1))),
            b'/' => return Some((SyntaxKind::Slash, self.advance(1))),
            b'.' => return Some((SyntaxKind::Dot, self.advance(1))),
            b'-' if self.peek2() == Some(b'>') => {
                return Some((SyntaxKind::Arrow, self.advance(2)));
            }
            _ => {}
        }

        // Identifiers and keywords
        if is_ident_start(b) {
            let text = self.take_while(is_ident_continue);
            let kind = keyword_kind(text).unwrap_or(SyntaxKind::Ident);
            return Some((kind, text));
        }

        // Unknown character
        Some((SyntaxKind::Error, self.advance(1)))
    }

    fn next_attr_value(&mut self) -> (SyntaxKind, &'src str) {
        let start = self.pos;

        // Skip leading horizontal whitespace
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
        let value_start = self.pos;

        // Consume until stop character
        while let Some(b) = self.peek() {
            match b {
                b',' | b')' | b'\n' | b'\r' => break,
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Trim trailing horizontal whitespace
        while self.pos > value_start
            && matches!(self.src.as_bytes().get(self.pos - 1), Some(b' ' | b'\t'))
        {
            self.pos -= 1;
        }

        self.mode = LexMode::Normal;

        let leading_ws = &self.src[start..value_start];
        let value = &self.src[value_start..self.pos];

        if !leading_ws.is_empty() {
            // Re-enter AttrValue mode (without leading ws) on the next call.
            self.pos = value_start;
            self.mode = LexMode::AttrValue;
            return (SyntaxKind::Whitespace, leading_ws);
        }

        if value.is_empty() {
            (SyntaxKind::Error, value)
        } else {
            (SyntaxKind::BareValue, value)
        }
    }

    fn next_shell(&mut self) -> Option<(SyntaxKind, &'src str)> {
        let start = self.pos;
        loop {
            match self.peek() {
                None => break,
                Some(b'{') => {
                    self.shell_depth += 1;
                    self.pos += 1;
                }
                Some(b'}') => {
                    if self.shell_depth == 0 {
                        self.mode = LexMode::Normal;
                        let slice = &self.src[start..self.pos];
                        // `}` is NOT consumed — the parser emits it as RBrace.
                        return Some((SyntaxKind::ShellBody, slice));
                    }
                    self.shell_depth -= 1;
                    self.pos += 1;
                }
                Some(_) => {
                    self.pos += 1;
                }
            }
        }
        // EOF inside shell body
        self.mode = LexMode::Normal;
        let slice = &self.src[start..self.pos];
        if slice.is_empty() {
            None
        } else {
            Some((SyntaxKind::Error, slice))
        }
    }

    /// Lex one path-item token inside an `artifacts = [...]` list.
    ///
    /// Leading horizontal whitespace is emitted as a `Whitespace` token and the
    /// mode is kept as `PathItem` so that `advance_impl` (which skips trivia) will
    /// call back and capture the actual value on the next iteration.
    ///
    /// If the first non-whitespace character is a list terminator (`]` or newline),
    /// the function switches back to `Normal` mode and delegates to `next_normal`,
    /// returning whatever comes next.  This lets trailing commas and empty lists
    /// work without producing spurious error tokens.
    fn next_path_item(&mut self) -> Option<(SyntaxKind, &'src str)> {
        let start = self.pos;

        // Emit leading horizontal whitespace as trivia; stay in PathItem mode so
        // `advance_impl` loops back and captures the value on the next call.
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
        if self.pos > start {
            return Some((SyntaxKind::Whitespace, &self.src[start..self.pos]));
        }

        // At a terminator with no value: fall back to Normal to lex the `]` or newline.
        if matches!(self.peek(), Some(b']' | b'\n' | b'\r') | None) {
            self.mode = LexMode::Normal;
            return self.next_normal();
        }

        let value_start = self.pos;
        while let Some(b) = self.peek() {
            match b {
                b',' | b']' | b'\n' | b'\r' => break,
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Trim trailing horizontal whitespace from the captured value.
        while self.pos > value_start
            && matches!(self.src.as_bytes().get(self.pos - 1), Some(b' ' | b'\t'))
        {
            self.pos -= 1;
        }

        self.mode = LexMode::Normal;
        let value = &self.src[value_start..self.pos];
        if value.is_empty() {
            // Shouldn't happen after the terminator check above, but be safe.
            self.mode = LexMode::Normal;
            self.next_normal()
        } else {
            Some((SyntaxKind::PathValue, value))
        }
    }

    fn next_token(&mut self) -> Option<(SyntaxKind, &'src str)> {
        match self.mode {
            LexMode::Normal => self.next_normal(),
            LexMode::AttrValue => Some(self.next_attr_value()),
            LexMode::Shell => self.next_shell(),
            LexMode::PathItem => self.next_path_item(),
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Token-by-token lexer handle used by the parser.
///
/// The parser drives lexing lazily.  When it determines that the next `{` opens
/// a shell-command body, it calls [`LexerHandle::signal_shell`] *before*
/// consuming the `{` token.  The lexer will then transition to `Shell` mode
/// immediately after emitting the `LBrace` token, so the very next
/// [`LexerHandle::next_token`] call returns a `ShellBody` token.
pub(crate) struct LexerHandle<'src> {
    lexer: Lexer<'src>,
}

impl<'src> LexerHandle<'src> {
    #[must_use]
    pub(crate) fn new(src: &'src str) -> Self {
        Self {
            lexer: Lexer::new(src),
        }
    }

    /// Immediately enter shell mode.
    ///
    /// Use this when the `{` opening a shell body has *already been emitted*
    /// as an `LBrace` token and the parser now needs the subsequent
    /// [`next_token`](Self::next_token) call to return `ShellBody`.
    pub(crate) fn enter_shell_now(&mut self) {
        self.lexer.mode = LexMode::Shell;
        self.lexer.shell_depth = 0;
    }

    /// Immediately enter path-item mode.
    ///
    /// Use this when a `[` or `,` inside an `artifacts` list has *already been
    /// emitted* as an `LBracket` / `Comma` token and the parser now needs the
    /// subsequent [`next_token`](Self::next_token) call to return a `PathValue`.
    pub(crate) fn enter_path_item_now(&mut self) {
        self.lexer.mode = LexMode::PathItem;
    }

    #[must_use]
    pub(crate) fn next_token(&mut self) -> Option<(SyntaxKind, &'src str)> {
        self.lexer.next_token()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: lex `src` and return all `(kind, text)` pairs including Eof.
    fn lex_all(src: &str) -> Vec<(SyntaxKind, &str)> {
        let mut handle = LexerHandle::new(src);
        let mut out = Vec::new();
        loop {
            if let Some(tok) = handle.next_token() {
                out.push(tok);
            } else {
                out.push((SyntaxKind::Eof, ""));
                break;
            }
        }
        out
    }

    fn kinds(src: &str) -> Vec<SyntaxKind> {
        lex_all(src).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn empty_input() {
        assert_eq!(kinds(""), vec![SyntaxKind::Eof]);
    }

    #[test]
    fn keywords() {
        let got = kinds("use stage job step steps template workflow defaults");
        let expected = [
            SyntaxKind::KwUse,
            SyntaxKind::Whitespace,
            SyntaxKind::KwStage,
            SyntaxKind::Whitespace,
            SyntaxKind::KwJob,
            SyntaxKind::Whitespace,
            SyntaxKind::KwStep,
            SyntaxKind::Whitespace,
            SyntaxKind::KwSteps,
            SyntaxKind::Whitespace,
            SyntaxKind::KwTemplate,
            SyntaxKind::Whitespace,
            SyntaxKind::KwWorkflow,
            SyntaxKind::Whitespace,
            SyntaxKind::KwDefaults,
            SyntaxKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn ident_vs_keyword() {
        let got = kinds("stage_name build");
        assert_eq!(
            got,
            vec![
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn attr_value_image_tag() {
        // "image = rust:1.94.0," — the `=` triggers AttrValue mode;
        // next token should be BareValue("rust:1.94.0").
        let src = "(image = rust:1.94.0,)";
        let toks = lex_all(src);
        let bare = toks
            .iter()
            .find(|(k, _)| *k == SyntaxKind::BareValue)
            .map(|(_, t)| *t);
        assert_eq!(bare, Some("rust:1.94.0"));
    }

    #[test]
    fn attr_value_list_not_bare() {
        // "dependencies = [build.build_debug]" — the `[` prevents AttrValue mode;
        // BareValue should NOT appear; LBracket should follow Eq.
        let src = "(dependencies = [build.build_debug])";
        let toks = lex_all(src);
        let has_bare = toks.iter().any(|(k, _)| *k == SyntaxKind::BareValue);
        assert!(!has_bare, "unexpected BareValue token in ref-list attr");
        let kinds_only: Vec<_> = toks
            .iter()
            .filter(|(k, _)| !matches!(k, SyntaxKind::Whitespace | SyntaxKind::Eof))
            .map(|(k, _)| *k)
            .collect();
        // Should contain LBracket, Ident, Dot, Ident, RBracket
        assert!(kinds_only.contains(&SyntaxKind::LBracket));
        assert!(kinds_only.contains(&SyntaxKind::Dot));
    }

    #[test]
    fn shell_body_capture() {
        let src = "{ cargo build --release\necho done }";
        let mut handle = LexerHandle::new(src);
        let lbrace = handle.next_token().unwrap();
        assert_eq!(lbrace.0, SyntaxKind::LBrace);
        // Enter shell mode after consuming the `{` token.
        handle.enter_shell_now();
        let body = handle.next_token().unwrap();
        assert_eq!(body.0, SyntaxKind::ShellBody);
        assert_eq!(body.1.trim(), "cargo build --release\necho done");
        let rbrace = handle.next_token().unwrap();
        assert_eq!(rbrace.0, SyntaxKind::RBrace);
    }

    #[test]
    fn shell_body_nested_braces() {
        let src = "{ echo ${VAR} }";
        let mut handle = LexerHandle::new(src);
        let _ = handle.next_token(); // LBrace
        handle.enter_shell_now();
        let body = handle.next_token().unwrap();
        assert_eq!(body.0, SyntaxKind::ShellBody);
        // Contains `${VAR}` — the inner braces are consumed into the body
        assert!(body.1.contains("${VAR}"));
        let rbrace = handle.next_token().unwrap();
        assert_eq!(rbrace.0, SyntaxKind::RBrace);
    }

    #[test]
    fn line_comment() {
        let got = kinds("# this is a comment\nstage");
        assert_eq!(
            got,
            vec![
                SyntaxKind::LineComment,
                SyntaxKind::Newline,
                SyntaxKind::KwStage,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn punctuation() {
        let got: Vec<_> = lex_all("{ } [ ] ( ) = , / .")
            .into_iter()
            .filter(|(k, _)| !matches!(k, SyntaxKind::Whitespace | SyntaxKind::Eof))
            .map(|(k, _)| k)
            .collect();
        assert_eq!(
            got,
            vec![
                SyntaxKind::LBrace,
                SyntaxKind::RBrace,
                SyntaxKind::LBracket,
                SyntaxKind::RBracket,
                SyntaxKind::LParen,
                SyntaxKind::RParen,
                SyntaxKind::Eq,
                SyntaxKind::Comma,
                SyntaxKind::Slash,
                SyntaxKind::Dot,
            ]
        );
    }
}
