/// All token and node kinds used in the `ciane` CST.
///
/// Tokens come first (produced by the lexer), then composite node kinds.
/// The `#[repr(u16)]` allows lossless conversion to/from rowan's raw `SyntaxKind`.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // ── Keywords ────────────────────────────────────────────────────────────
    KwUse,
    KwStage,
    KwJob,
    KwStep,
    KwSteps,
    KwTemplate,
    KwWorkflow,
    KwDefaults,

    // ── Punctuation ─────────────────────────────────────────────────────────
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Eq,
    Comma,
    Slash,
    Dot,
    Arrow,

    // ── Literals / identifiers ───────────────────────────────────────────────
    /// Identifiers and item names.
    Ident,
    /// Attribute RHS values: paths, image tags, registry URLs, etc.
    /// Lexed as everything up to the next `,`, `)`, or newline inside `(...)`.
    BareValue,

    // ── Special ─────────────────────────────────────────────────────────────
    /// Raw shell text captured inside `{ }` of a step or inline job.
    /// The outer braces are emitted as separate `LBrace` / `RBrace` tokens.
    ShellBody,
    /// A single path or glob captured inside an `artifacts = [...]` list.
    /// Lexed by the `PathItem` lexer mode; may contain any characters except
    /// `,`, `]`, and newlines (e.g. `dist/`, `**/*.so`, `target/debug/app`).
    PathValue,

    // ── Trivia ───────────────────────────────────────────────────────────────
    Whitespace,
    Newline,
    LineComment,

    // ── Errors / end-of-file ─────────────────────────────────────────────────
    Error,
    Eof,

    // ════════════════════════════════════════════════════════════════════════
    // Node kinds (composite, produced by the parser)
    // ════════════════════════════════════════════════════════════════════════
    /// Top-level file node.
    Root,
    /// `workflow name (attrs)? { … }` — top-level workflow definition.
    WorkflowDef,
    /// The body of a `WorkflowDef` (may or may not be brace-wrapped).
    WorkflowBody,
    /// `use name (attrs)?` — a single external workflow import statement.
    UseDecl,
    /// `( key = val, … )` attribute list.
    AttrList,
    /// A single `key = value` attribute.
    Attr,
    /// The value part of an attribute.
    AttrValue,
    /// `[ a.b, c.d ]` — dependency list or bare step-ref list in attributes.
    RefList,
    /// A dotted or slashed reference, e.g. `build.build_debug` or `ns/template`.
    Ref,
    /// `[ path, glob ]` — list of file paths or globs in an `artifacts` attribute.
    PathList,
    /// A single entry in a `PathList`.
    PathItem,
    /// `stage name (attrs) { body }`.
    Stage,
    /// `{ jobs and templates }` — the body of a stage.
    StageBody,
    /// A job definition — either inline (`{ shell }`) or with steps (`[ … ]`).
    Job,
    /// `template name [ … ]` — a job template that is never run directly.
    TemplateDef,
    /// `{ ShellBody }` — single-step shorthand job body.
    JobBodyInline,
    /// `[ step* ]` — explicit step-list job body.
    JobBodySteps,
    /// `step name { shell_body }` — a named step.
    Step,
    /// `step name,` — reference to an inherited step without a body override.
    StepRef,
    /// Bare `steps` keyword inside a step list (inherit all template steps).
    StepsKeyword,
    /// Wrapper node holding the `Ident` token for an item's name.
    Name,
    /// `-> [path_or_$var, …]` — optional return annotation after a job or template body.
    /// Items starting with `$` are env var names to export; others are artifact paths.
    ReturnAnnotation,
    /// Error-recovery node wrapping skipped/unexpected tokens.
    ErrorNode,
}

impl SyntaxKind {
    /// Returns `true` for trivia tokens (whitespace, newlines, comments).
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline | Self::LineComment)
    }
}

/// Zero-sized marker type that ties the `ciane` language to rowan's generic CST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CianeLanguage;

impl rowan::Language for CianeLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        // SAFETY: Every `u16` produced by `kind_to_raw` is a valid `SyntaxKind`
        // discriminant.  We never store arbitrary raw kinds in the tree.
        assert!(raw.0 <= SyntaxKind::ErrorNode as u16);
        // SAFETY: repr(u16) and the assert above guarantee a valid discriminant.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// A rowan syntax node parameterised for the `ciane` language.
pub type SyntaxNode = rowan::SyntaxNode<CianeLanguage>;
/// A rowan syntax token parameterised for the `ciane` language.
pub type SyntaxToken = rowan::SyntaxToken<CianeLanguage>;
/// A rowan syntax element (node or token) parameterised for the `ciane` language.
pub type SyntaxElement = rowan::SyntaxElement<CianeLanguage>;
