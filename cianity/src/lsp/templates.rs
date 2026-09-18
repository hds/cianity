//! Resolution of the template references written in `inherit` attributes.
//!
//! A reference is `[import/][stage.]name`, where `import` is the name of a
//! `use` import and `stage` is the stage the template is defined in. An
//! unqualified name is a template in the same stage, or failing that a
//! top-level one, which is how the workflow is built.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use ciane::{
    ast::{AstNode, Attr, AttrList, HasAttrList, HasName, Ref, Root, Stage, TemplateDef},
    parse,
    syntax::{SyntaxKind, SyntaxToken},
};
use rowan::{TextRange, TextSize};

/// The file a template reference points into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TemplateFile {
    /// The file the reference itself is written in.
    Current,
    /// Another file, reached through a `use` import.
    Imported(PathBuf),
}

/// The template an `inherit` reference names.
///
/// Two references naming the same template have equal ids, however they are
/// written, which is what makes "find references" and rename work across
/// stages and files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TemplateId {
    pub file: TemplateFile,
    /// The stage the template is defined in; `None` for a top-level template.
    pub stage: Option<String>,
    pub name: String,
}

/// One `inherit` reference, with the ranges of the parts that name things.
pub(super) struct TemplateRef {
    pub id: TemplateId,
    /// Just the template name, e.g. `base` of `dep/build.base`.
    pub name_range: TextRange,
    /// Just the stage name, when the reference is qualified with one.
    pub stage_range: Option<TextRange>,
    /// The import the reference goes through, e.g. `dep` of `dep/build.base`.
    pub import: Option<String>,
    /// Just the import name's range.
    pub import_range: Option<TextRange>,
}

/// Where a template is defined.
pub(super) struct TemplateDefLocation {
    /// The file, when it isn't the one the reference was written in.
    pub path: Option<PathBuf>,
    /// The source of that file, for turning `name_range` into an LSP range.
    pub source: String,
    pub name_range: TextRange,
}

/// The parts of a `[import/][stage.]name` reference, with the byte offset of
/// each name within the text it was split from.
struct RefParts<'a> {
    import: Option<&'a str>,
    stage: Option<(usize, &'a str)>,
    name: (usize, &'a str),
}

fn split_ref(text: &str) -> RefParts<'_> {
    let (import, rest_at) = match text.find('/') {
        Some(slash) => (Some(&text[..slash]), slash + 1),
        None => (None, 0),
    };
    let rest = &text[rest_at..];
    match rest.find('.') {
        Some(dot) => RefParts {
            import,
            stage: Some((rest_at, &rest[..dot])),
            name: (rest_at + dot + 1, &rest[dot + 1..]),
        },
        None => RefParts {
            import,
            stage: None,
            name: (rest_at, rest),
        },
    }
}

/// The `inherit` reference that `token` is part of, if any.
pub(super) fn ref_at(token: &SyntaxToken, root: &Root, file_path: &Path) -> Option<TemplateRef> {
    match token.kind() {
        SyntaxKind::BareValue => {
            let attr = attr_of_value(token)?;
            if attr.key_text().as_deref() != Some("inherit") {
                return None;
            }
            let context = context_stage(token);
            bare_value_ref(token, root, file_path, context.as_deref())
        }
        SyntaxKind::Ident => {
            let ref_node = token.parent().filter(|n| n.kind() == SyntaxKind::Ref)?;
            let list = ref_node
                .parent()
                .filter(|n| n.kind() == SyntaxKind::RefList)?;
            let attr = Attr::cast(list.parent()?.parent()?)?;
            if attr.key_text().as_deref() != Some("inherit") {
                return None;
            }
            let context = context_stage(token);
            list_ref(&Ref::cast(ref_node)?, root, file_path, context.as_deref())
        }
        _ => None,
    }
}

/// Every `inherit` reference in the file.
pub(super) fn all_refs(root: &Root, file_path: &Path) -> Vec<TemplateRef> {
    let mut refs = Vec::new();
    for (stage_name, attr) in inherit_attrs(root) {
        let Some(value) = attr.value() else {
            continue;
        };
        let context = stage_name.as_deref();
        if let Some(token) = value
            .syntax()
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::BareValue)
        {
            refs.extend(bare_value_ref(&token, root, file_path, context));
        } else if let Some(list) = value.ref_list() {
            for item in list.refs() {
                refs.extend(list_ref(&item, root, file_path, context));
            }
        }
    }
    refs
}

/// The template declared at `token`, if it is a `template` name.
pub(super) fn def_at(token: &SyntaxToken) -> Option<TemplateId> {
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    let name_node = token.parent().filter(|n| n.kind() == SyntaxKind::Name)?;
    let def = TemplateDef::cast(name_node.parent()?)?;
    Some(TemplateId {
        file: TemplateFile::Current,
        stage: def
            .syntax()
            .ancestors()
            .find_map(Stage::cast)
            .and_then(|s| s.name())
            .map(|n| n.to_string()),
        name: token.text().to_owned(),
    })
}

/// Find where `id` is defined, reading the file it lives in when needed.
pub(super) fn find_def(id: &TemplateId, root: &Root, source: &str) -> Option<TemplateDefLocation> {
    match &id.file {
        TemplateFile::Current => {
            let def = find_in_root(root, id.stage.as_deref(), &id.name)?;
            Some(TemplateDefLocation {
                path: None,
                source: source.to_owned(),
                name_range: def.name_token()?.text_range(),
            })
        }
        TemplateFile::Imported(path) => {
            let target_source = std::fs::read_to_string(path).ok()?;
            let target_root = Root::cast(parse(&target_source).syntax())?;
            let def = find_in_root(&target_root, id.stage.as_deref(), &id.name)?;
            let name_range = def.name_token()?.text_range();
            Some(TemplateDefLocation {
                path: Some(path.clone()),
                source: target_source,
                name_range,
            })
        }
    }
}

/// The references that can be written in an `inherit` inside `context_stage`,
/// including templates in other stages and in imported files.
pub(super) fn visible_refs(
    root: &Root,
    file_path: &Path,
    context_stage: Option<&str>,
) -> Vec<String> {
    let mut refs = Vec::new();
    let mut local: HashSet<String> = HashSet::new();

    if let Some(stage_name) = context_stage {
        for name in stage_template_names(root, stage_name) {
            local.insert(name.clone());
            refs.push(name);
        }
    }

    // A template in the stage shadows a top-level one of the same name, so
    // the top-level one can't be named from here at all.
    for def in root.templates() {
        if let Some(name) = def.name()
            && !local.contains(name.as_str())
        {
            refs.push(name.to_string());
        }
    }

    for stage in root.stages() {
        let Some(stage_name) = stage.name() else {
            continue;
        };
        if Some(stage_name.as_str()) == context_stage {
            continue;
        }
        for name in stage_template_names(root, stage_name.as_str()) {
            refs.push(format!("{stage_name}.{name}"));
        }
    }

    let base = file_path.parent().unwrap_or(Path::new("."));
    for import in root.use_decls() {
        let Some((import_name, location)) = import.name().zip(import.path()) else {
            continue;
        };
        let Ok(source) = std::fs::read_to_string(base.join(location.as_str())) else {
            continue;
        };
        let Some(imported) = Root::cast(parse(&source).syntax()) else {
            continue;
        };
        for def in imported.templates() {
            if let Some(name) = def.name() {
                refs.push(format!("{import_name}/{name}"));
            }
        }
        for stage in imported.stages() {
            let Some(stage_name) = stage.name() else {
                continue;
            };
            for name in stage_template_names(&imported, stage_name.as_str()) {
                refs.push(format!("{import_name}/{stage_name}.{name}"));
            }
        }
    }

    refs
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn bare_value_ref(
    token: &SyntaxToken,
    root: &Root,
    file_path: &Path,
    context_stage: Option<&str>,
) -> Option<TemplateRef> {
    let text = token.text();
    let parts = split_ref(text);
    let start = token.text_range().start();
    let range_at = |(at, part): (usize, &str)| -> TextRange {
        let at = TextSize::try_from(at).unwrap_or_default();
        TextRange::at(start + at, TextSize::of(part))
    };
    Some(TemplateRef {
        id: make_id(
            root,
            file_path,
            parts.import,
            parts.stage.map(|(_, stage)| stage),
            parts.name.1,
            context_stage,
        )?,
        name_range: range_at(parts.name),
        stage_range: parts.stage.map(range_at),
        import: parts.import.map(ToOwned::to_owned),
        import_range: parts.import.map(|import| range_at((0, import))),
    })
}

fn list_ref(
    item: &Ref,
    root: &Root,
    file_path: &Path,
    context_stage: Option<&str>,
) -> Option<TemplateRef> {
    let text = item.text();
    let parts = split_ref(&text);
    let idents: Vec<SyntaxToken> = item
        .syntax()
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident)
        .collect();
    let name_range = idents.last()?.text_range();
    let stage_range = parts.stage.and_then(|_| {
        let index = idents.len().checked_sub(2)?;
        Some(idents[index].text_range())
    });
    Some(TemplateRef {
        id: make_id(
            root,
            file_path,
            parts.import,
            parts.stage.map(|(_, stage)| stage),
            parts.name.1,
            context_stage,
        )?,
        name_range,
        stage_range,
        import: parts.import.map(ToOwned::to_owned),
        import_range: parts
            .import
            .and_then(|_| Some(idents.first()?.text_range())),
    })
}

fn make_id(
    root: &Root,
    file_path: &Path,
    import: Option<&str>,
    stage: Option<&str>,
    name: &str,
    context_stage: Option<&str>,
) -> Option<TemplateId> {
    let file = match import {
        Some(import_name) => TemplateFile::Imported(import_path(root, import_name, file_path)?),
        None => TemplateFile::Current,
    };
    // An unqualified name in the current file is the stage's own template when
    // there is one, and a top-level template otherwise.
    let stage = match (stage, &file) {
        (Some(stage_name), _) => Some(stage_name.to_owned()),
        (None, TemplateFile::Imported(_)) => None,
        (None, TemplateFile::Current) => context_stage
            .filter(|ctx| stage_template_names(root, ctx).iter().any(|n| n == name))
            .map(ToOwned::to_owned),
    };
    Some(TemplateId {
        file,
        stage,
        name: name.to_owned(),
    })
}

fn import_path(root: &Root, import_name: &str, file_path: &Path) -> Option<PathBuf> {
    let base = file_path.parent().unwrap_or(Path::new("."));
    root.use_decls()
        .find(|imp| imp.name().as_deref() == Some(import_name))
        .and_then(|imp| imp.path())
        .map(|location| normalize(&base.join(location.as_str())))
}

/// Remove `.` and `..` components, so that the path of an import written as
/// `./shared.ci` matches the one the editor uses for that file.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(out.components().next_back(), Some(Component::Normal(_))) =>
            {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn find_in_root(root: &Root, stage: Option<&str>, name: &str) -> Option<TemplateDef> {
    match stage {
        Some(stage_name) => root
            .stages()
            .find(|s| s.name().as_deref() == Some(stage_name))?
            .body()?
            .templates()
            .find(|t| t.name().as_deref() == Some(name)),
        None => root.templates().find(|t| t.name().as_deref() == Some(name)),
    }
}

fn stage_template_names(root: &Root, stage_name: &str) -> Vec<String> {
    root.stages()
        .find(|s| s.name().as_deref() == Some(stage_name))
        .and_then(|s| s.body())
        .map(|body| {
            body.templates()
                .filter_map(|t| t.name())
                .map(|n| n.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The stage a reference is written in, if it isn't at the top level.
pub(super) fn context_stage(token: &SyntaxToken) -> Option<String> {
    token
        .parent()?
        .ancestors()
        .find_map(Stage::cast)
        .and_then(|s| s.name())
        .map(|n| n.to_string())
}

/// Every `inherit` attribute in the file, with the stage it's written in.
fn inherit_attrs(root: &Root) -> Vec<(Option<String>, Attr)> {
    let mut lists: Vec<(Option<String>, AttrList)> = Vec::new();

    for def in root.templates() {
        lists.extend(def.attr_list().map(|list| (None, list)));
    }
    for stage in root.stages() {
        let stage_name = stage.name().map(|n| n.to_string());
        let Some(body) = stage.body() else {
            continue;
        };
        for job in body.jobs() {
            lists.extend(job.attr_list().map(|list| (stage_name.clone(), list)));
        }
        for def in body.templates() {
            lists.extend(def.attr_list().map(|list| (stage_name.clone(), list)));
        }
    }

    lists
        .into_iter()
        .flat_map(|(stage, list)| {
            list.attrs()
                .filter(|attr| attr.key_text().as_deref() == Some("inherit"))
                .map(move |attr| (stage.clone(), attr))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn attr_of_value(token: &SyntaxToken) -> Option<Attr> {
    Attr::cast(token.parent()?.parent()?)
}
