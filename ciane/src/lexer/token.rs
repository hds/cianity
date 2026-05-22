use crate::syntax::SyntaxKind;

/// Maps a bare identifier string to a keyword `SyntaxKind`, if it is a reserved word.
#[must_use]
pub fn keyword_kind(text: &str) -> Option<SyntaxKind> {
    match text {
        "use" => Some(SyntaxKind::KwUse),
        "stage" => Some(SyntaxKind::KwStage),
        "job" => Some(SyntaxKind::KwJob),
        "step" => Some(SyntaxKind::KwStep),
        "steps" => Some(SyntaxKind::KwSteps),
        "template" => Some(SyntaxKind::KwTemplate),
        "workflow" => Some(SyntaxKind::KwWorkflow),
        "defaults" => Some(SyntaxKind::KwDefaults),
        _ => None,
    }
}
