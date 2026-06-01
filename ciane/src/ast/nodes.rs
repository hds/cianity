use smol_str::SmolStr;

use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::traits::{AstNode, HasAttrList, HasName};

// ─── macro helpers ───────────────────────────────────────────────────────────

/// Declare a typed AST node struct that wraps a `SyntaxNode` of a specific kind.
macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == SyntaxKind::$kind {
                    Some(Self(node))
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

// ─── node types ──────────────────────────────────────────────────────────────

ast_node!(Root, Root);
ast_node!(WorkflowDef, WorkflowDef);
ast_node!(WorkflowBody, WorkflowBody);
ast_node!(UseDecl, UseDecl);
ast_node!(AttrList, AttrList);
ast_node!(Attr, Attr);
ast_node!(AttrValue, AttrValue);
ast_node!(RefList, RefList);
ast_node!(Ref, Ref);
ast_node!(Stage, Stage);
ast_node!(StageBody, StageBody);
ast_node!(Job, Job);
ast_node!(TemplateDef, TemplateDef);
ast_node!(JobBodyInline, JobBodyInline);
ast_node!(JobBodySteps, JobBodySteps);
ast_node!(Step, Step);
ast_node!(StepsKeyword, StepsKeyword);

// ─── impl HasName / HasAttrList ───────────────────────────────────────────────

impl HasName for WorkflowDef {}
impl HasName for Stage {}
impl HasName for Job {}
impl HasName for TemplateDef {}
impl HasName for Step {}
impl HasName for UseDecl {}

impl HasAttrList for WorkflowDef {}
impl HasAttrList for Stage {}
impl HasAttrList for Job {}
impl HasAttrList for TemplateDef {}
impl HasAttrList for UseDecl {}

// ─── Root ─────────────────────────────────────────────────────────────────────

impl Root {
    /// All `WorkflowDef` children.
    pub fn workflow_defs(&self) -> impl Iterator<Item = WorkflowDef> + '_ {
        self.0.children().filter_map(WorkflowDef::cast)
    }

    /// All `UseDecl` nodes across every workflow in this file.
    pub fn use_decls(&self) -> impl Iterator<Item = UseDecl> + '_ {
        self.workflow_defs().flat_map(|wd| {
            wd.body()
                .map_or_else(Vec::new, |b| b.use_decls().collect::<Vec<_>>())
        })
    }

    /// All `Stage` nodes across every workflow in this file.
    pub fn stages(&self) -> impl Iterator<Item = Stage> + '_ {
        self.workflow_defs().flat_map(|wd| {
            wd.body()
                .map_or_else(Vec::new, |b| b.stages().collect::<Vec<_>>())
        })
    }

    /// All top-level `TemplateDef` nodes (outside any stage) across every workflow.
    pub fn templates(&self) -> impl Iterator<Item = TemplateDef> + '_ {
        self.workflow_defs().flat_map(|wd| {
            wd.body()
                .map_or_else(Vec::new, |b| b.templates().collect::<Vec<_>>())
        })
    }
}

// ─── WorkflowDef ─────────────────────────────────────────────────────────────

impl WorkflowDef {
    /// The `WorkflowBody` child.
    #[must_use]
    pub fn body(&self) -> Option<WorkflowBody> {
        self.0.children().find_map(WorkflowBody::cast)
    }

    /// The `strategy` attribute value, if present.
    #[must_use]
    pub fn strategy(&self) -> Option<SmolStr> {
        self.attr_list()?
            .attrs()
            .find(|a| a.key_text().as_deref() == Some("strategy"))
            .and_then(|a| a.value_text())
    }
}

// ─── WorkflowBody ─────────────────────────────────────────────────────────────

impl WorkflowBody {
    /// All `UseDecl` children.
    pub fn use_decls(&self) -> impl Iterator<Item = UseDecl> + '_ {
        self.0.children().filter_map(UseDecl::cast)
    }

    /// All `Stage` children.
    pub fn stages(&self) -> impl Iterator<Item = Stage> + '_ {
        self.0.children().filter_map(Stage::cast)
    }

    /// All `TemplateDef` children (outside any stage).
    pub fn templates(&self) -> impl Iterator<Item = TemplateDef> + '_ {
        self.0.children().filter_map(TemplateDef::cast)
    }

    /// `true` if this body is wrapped in `{` `}` braces.
    #[must_use]
    pub fn is_braced(&self) -> bool {
        self.0
            .children_with_tokens()
            .any(|e| e.kind() == SyntaxKind::LBrace)
    }
}

// ─── UseDecl ──────────────────────────────────────────────────────────────────

impl UseDecl {
    /// The `path` attribute value, if present.
    #[must_use]
    pub fn path(&self) -> Option<SmolStr> {
        self.attr_list()?
            .attrs()
            .find(|a| a.key_text().as_deref() == Some("path"))
            .and_then(|a| a.value_text())
    }
}

// ─── AttrList ─────────────────────────────────────────────────────────────────

impl AttrList {
    /// All `Attr` children.
    pub fn attrs(&self) -> impl Iterator<Item = Attr> + '_ {
        self.0.children().filter_map(Attr::cast)
    }
}

// ─── Attr ─────────────────────────────────────────────────────────────────────

fn is_kw(kind: SyntaxKind) -> bool {
    matches!(
        kind,
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

impl Attr {
    /// The key token (an `Ident` or keyword-as-name).
    #[must_use]
    pub fn key_token(&self) -> Option<SyntaxToken> {
        self.0.children_with_tokens().find_map(|e| {
            e.into_token()
                .filter(|t| t.kind() == SyntaxKind::Ident || is_kw(t.kind()))
        })
    }

    /// The key text, if present.
    #[must_use]
    pub fn key_text(&self) -> Option<SmolStr> {
        self.key_token().map(|t| t.text().into())
    }

    /// The `AttrValue` child.
    #[must_use]
    pub fn value(&self) -> Option<AttrValue> {
        self.0.children().find_map(AttrValue::cast)
    }

    /// Convenience: get the raw text of the attribute value.
    #[must_use]
    pub fn value_text(&self) -> Option<SmolStr> {
        let av = self.value()?;
        av.syntax()
            .children_with_tokens()
            .find_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::BareValue))
            .map(|t| t.text().into())
    }
}

// ─── AttrValue ────────────────────────────────────────────────────────────────

impl AttrValue {
    /// The `RefList` child, if this is a list value (e.g. `dependencies`).
    #[must_use]
    pub fn ref_list(&self) -> Option<RefList> {
        self.0.children().find_map(RefList::cast)
    }

    /// The `BareValue` token text, if this is a scalar value.
    #[must_use]
    pub fn bare_text(&self) -> Option<SmolStr> {
        self.0
            .children_with_tokens()
            .find_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::BareValue))
            .map(|t| t.text().into())
    }
}

// ─── RefList ─────────────────────────────────────────────────────────────────

impl RefList {
    /// All `Ref` children.
    pub fn refs(&self) -> impl Iterator<Item = Ref> + '_ {
        self.0.children().filter_map(Ref::cast)
    }
}

// ─── Ref ─────────────────────────────────────────────────────────────────────

impl Ref {
    /// The full text of the reference (e.g. `build.build_debug`).
    #[must_use]
    pub fn text(&self) -> String {
        self.0.text().to_string()
    }
}

// ─── Stage ───────────────────────────────────────────────────────────────────

impl Stage {
    /// The `StageBody` child.
    #[must_use]
    pub fn body(&self) -> Option<StageBody> {
        self.0.children().find_map(StageBody::cast)
    }
}

// ─── StageBody ────────────────────────────────────────────────────────────────

impl StageBody {
    /// All `Job` children.
    pub fn jobs(&self) -> impl Iterator<Item = Job> + '_ {
        self.0.children().filter_map(Job::cast)
    }

    /// All `TemplateDef` children.
    pub fn templates(&self) -> impl Iterator<Item = TemplateDef> + '_ {
        self.0.children().filter_map(TemplateDef::cast)
    }
}

// ─── Job ─────────────────────────────────────────────────────────────────────

impl Job {
    /// The inline body (`{ shell }`), if this is a single-step job.
    #[must_use]
    pub fn inline_body(&self) -> Option<JobBodyInline> {
        self.0.children().find_map(JobBodyInline::cast)
    }

    /// The step-list body (`[ step* ]`), if present.
    #[must_use]
    pub fn steps_body(&self) -> Option<JobBodySteps> {
        self.0.children().find_map(JobBodySteps::cast)
    }
}

// ─── JobBodyInline ────────────────────────────────────────────────────────────

impl JobBodyInline {
    /// The raw shell body text, if present.
    #[must_use]
    pub fn shell_text(&self) -> Option<SmolStr> {
        self.0
            .children_with_tokens()
            .find_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::ShellBody))
            .map(|t| t.text().into())
    }
}

// ─── JobBodySteps ─────────────────────────────────────────────────────────────

impl JobBodySteps {
    /// All `Step` children (includes bare step references).
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        self.0.children().filter_map(Step::cast)
    }

    /// All `StepsKeyword` children.
    pub fn steps_keywords(&self) -> impl Iterator<Item = StepsKeyword> + '_ {
        self.0.children().filter_map(StepsKeyword::cast)
    }
}

// ─── Step ─────────────────────────────────────────────────────────────────────

impl Step {
    /// The raw shell body text, if this is a full step (not a reference).
    #[must_use]
    pub fn shell_text(&self) -> Option<SmolStr> {
        self.0
            .children_with_tokens()
            .find_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::ShellBody))
            .map(|t| t.text().into())
    }

    /// `true` if this step has a shell body (is not a bare reference).
    #[must_use]
    pub fn has_body(&self) -> bool {
        self.shell_text().is_some()
    }
}

// ─── TemplateDef ─────────────────────────────────────────────────────────────

impl TemplateDef {
    /// The step-list body.
    #[must_use]
    pub fn body(&self) -> Option<JobBodySteps> {
        self.0.children().find_map(JobBodySteps::cast)
    }
}
