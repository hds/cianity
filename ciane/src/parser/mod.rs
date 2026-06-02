mod grammar;

use rowan::{GreenNodeBuilder, Language};

use crate::{
    error::ParseError,
    lexer::LexerHandle,
    syntax::{CianeLanguage, SyntaxKind, SyntaxNode},
};

/// Return type of `Parser::advance_impl`: the next non-trivia token plus
/// any trivia tokens that preceded it.
type AdvanceResult<'src> = (
    Option<(SyntaxKind, &'src str)>,
    Vec<(SyntaxKind, &'src str)>,
);

/// The result of parsing a `ciane` source file.
///
/// Always contains a complete `GreenNode` (even when errors occurred) because
/// the parser recovers from errors rather than aborting.
#[must_use]
pub struct Parse {
    green: rowan::GreenNode,
    errors: Vec<ParseError>,
}

impl Parse {
    /// Return the root `SyntaxNode` of the concrete syntax tree.
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The parse errors collected during parsing.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// `true` if no errors were produced.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Parse a `ciane` source string into a [`Parse`] result.
pub fn parse(source: &str) -> Parse {
    let mut p = Parser::new(source);
    grammar::root(&mut p);
    p.finish()
}

// ─── Parser ──────────────────────────────────────────────────────────────────

pub(super) struct Parser<'src> {
    source: &'src str,
    lexer: LexerHandle<'src>,
    /// The current (peeked) token.  `None` means EOF.
    current: Option<(SyntaxKind, &'src str)>,
    /// Trivia tokens that precede the current non-trivia token.
    pending_trivia: Vec<(SyntaxKind, &'src str)>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    /// When `true`, the next `LBrace` emitted by `bump` will immediately
    /// switch the lexer to shell mode so the following token is `ShellBody`.
    pending_shell: bool,
    /// When `true`, the next `LBracket` or `Comma` emitted by `bump` will
    /// switch the lexer to path-item mode so the following token is `PathValue`.
    /// Cleared after any `bump` call, whether or not it triggered the mode change.
    pending_path_item: bool,
}

impl<'src> Parser<'src> {
    fn new(source: &'src str) -> Self {
        let mut lexer = LexerHandle::new(source);
        // Pre-fill current token.
        let (current, trivia) = Self::advance_impl(&mut lexer);
        Self {
            source,
            lexer,
            current,
            pending_trivia: trivia,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            pending_shell: false,
            pending_path_item: false,
        }
    }

    /// Advance the lexer past trivia, collecting trivia into `pending_trivia`.
    /// Returns the next non-trivia token (or `None` for EOF) and the trivia
    /// tokens that preceded it.
    fn advance_impl(lexer: &mut LexerHandle<'src>) -> AdvanceResult<'src> {
        let mut trivia = Vec::new();
        loop {
            match lexer.next_token() {
                None => return (None, trivia),
                Some((kind, text)) if kind.is_trivia() => trivia.push((kind, text)),
                Some(tok) => return (Some(tok), trivia),
            }
        }
    }

    /// The kind of the current non-trivia token, or `Eof`.
    pub(super) fn current(&self) -> SyntaxKind {
        self.current.map_or(SyntaxKind::Eof, |(k, _)| k)
    }

    /// `true` if the current token matches `kind`.
    #[must_use]
    pub(super) fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// `true` if the current token is any keyword.
    #[must_use]
    pub(super) fn at_keyword(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::KwUse
                | SyntaxKind::KwStage
                | SyntaxKind::KwJob
                | SyntaxKind::KwStep
                | SyntaxKind::KwSteps
                | SyntaxKind::KwTemplate
                | SyntaxKind::KwWorkflow
                | SyntaxKind::KwDefaults
        )
    }

    /// Flush pending trivia into the builder, then consume and emit the current token.
    pub(super) fn bump(&mut self) {
        self.flush_trivia();
        if let Some((kind, text)) = self.current.take() {
            self.builder.token(CianeLanguage::kind_to_raw(kind), text);
            if self.pending_shell && kind == SyntaxKind::LBrace {
                self.lexer.enter_shell_now();
                self.pending_shell = false;
            }
            if self.pending_path_item {
                if matches!(kind, SyntaxKind::LBracket | SyntaxKind::Comma) {
                    self.lexer.enter_path_item_now();
                }
                self.pending_path_item = false;
            }
        }
        let (next, trivia) = Self::advance_impl(&mut self.lexer);
        self.current = next;
        self.pending_trivia = trivia;
    }

    /// Consume the current token only if it matches `kind`.
    ///
    /// Returns `true` if the token was consumed.  Use [`Parser::eat_optional`]
    /// when the return value is not needed (e.g. for optional trailing commas).
    #[must_use]
    pub(super) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume the current token if it matches `kind`, discarding the result.
    pub(super) fn eat_optional(&mut self, kind: SyntaxKind) {
        if self.at(kind) {
            self.bump();
        }
    }

    /// Consume the current token if it matches `kind`; otherwise record an error.
    pub(super) fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            let span = self.current_span();
            self.errors.push(ParseError {
                message: format!("expected {kind:?}, found {:?}", self.current()),
                span,
            });
        }
    }

    /// Open a new CST node.  Must be balanced with [`Parser::finish_node`].
    ///
    /// Trivia is flushed into the *parent* node before the child opens.
    /// For the root node (no parent), use [`Parser::start_root_node`] instead.
    pub(super) fn start_node(&mut self, kind: SyntaxKind) {
        self.flush_trivia();
        self.builder.start_node(CianeLanguage::kind_to_raw(kind));
    }

    /// Open the top-level `Root` node.
    ///
    /// Unlike [`Parser::start_node`], trivia is flushed *after* opening so
    /// that any leading whitespace or comments belong inside `Root` rather
    /// than being emitted before it, which would leave rowan's builder with
    /// two top-level children and panic on `finish()`.
    pub(super) fn start_root_node(&mut self) {
        self.builder
            .start_node(CianeLanguage::kind_to_raw(SyntaxKind::Root));
        self.flush_trivia();
    }

    /// Close the most recently opened CST node.
    ///
    /// Any pending trivia accumulated since the last token is flushed into this
    /// node before it closes, ensuring the CST is lossless.
    pub(super) fn finish_node(&mut self) {
        self.flush_trivia();
        self.builder.finish_node();
    }

    /// Signal that the next `{` token will open a shell-command body.
    ///
    /// Must be called immediately before the `{` is consumed via [`bump`].
    /// Because `{` is pre-fetched into `current`, we set a flag here and
    /// then call [`LexerHandle::enter_shell_now`] inside `bump` once the
    /// `LBrace` token has been emitted.
    pub(super) fn signal_shell(&mut self) {
        self.pending_shell = true;
    }

    /// Signal that the next `[` or `,` token will open a path-item context.
    ///
    /// The lexer will enter `PathItem` mode immediately after emitting that
    /// token, so the very next [`next_token`](crate::lexer::LexerHandle::next_token)
    /// call returns a `PathValue`.  The flag is cleared after any `bump` call.
    pub(super) fn signal_path_item(&mut self) {
        self.pending_path_item = true;
    }

    /// The raw text of the current (pre-fetched) token, or `""` at EOF.
    pub(super) fn current_text(&self) -> &str {
        self.current.map_or("", |(_, text)| text)
    }

    /// Record a parse error at the current position without consuming any token.
    pub(super) fn error(&mut self, message: impl Into<String>) {
        let span = self.current_span();
        let message = message.into();
        self.errors.push(ParseError { message, span });
    }

    /// Record an error and wrap the current token in an `ErrorNode`, then advance.
    pub(super) fn error_bump(&mut self, message: impl Into<String>) {
        self.error(message);
        self.start_node(SyntaxKind::ErrorNode);
        if !self.at(SyntaxKind::Eof) {
            self.bump();
        }
        self.finish_node();
    }

    /// Byte span of the current token within the source.
    fn current_span(&self) -> std::ops::Range<usize> {
        match self.current {
            None => self.source.len()..self.source.len(),
            Some((_, text)) => {
                let start = text.as_ptr() as usize - self.source.as_ptr() as usize;
                start..start + text.len()
            }
        }
    }

    /// Emit all pending trivia tokens into the builder.
    fn flush_trivia(&mut self) {
        for (kind, text) in self.pending_trivia.drain(..) {
            self.builder.token(CianeLanguage::kind_to_raw(kind), text);
        }
    }

    fn finish(self) -> Parse {
        let green = self.builder.finish();
        Parse {
            green,
            errors: self.errors,
        }
    }
}
