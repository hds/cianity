use std::path::{Path, PathBuf};

use ciane::{
    ast::{AstNode, Attr, AttrValue, HasName, Root, Stage, WorkflowImport},
    parse,
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::{Location, Position, Range, Uri};

use super::util::{range_to_lsp, token_at};

#[must_use]
pub(super) fn resolve(
    parse: &Parse,
    source: &str,
    offset: usize,
    file_path: &Path,
    current_uri: &Uri,
) -> Option<Location> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    let root = Root::cast(root_node)?;
    resolve_inherit(&token, source, file_path, current_uri, &root)
        .or_else(|| resolve_dependency(&token, &root, source, current_uri))
        .or_else(|| resolve_location_attr(&token, file_path))
        .or_else(|| resolve_import_name(&token, file_path))
}

// ─── inherit ──────────────────────────────────────────────────────────────────

fn resolve_inherit(
    token: &SyntaxToken,
    source: &str,
    file_path: &Path,
    current_uri: &Uri,
    root: &Root,
) -> Option<Location> {
    if token.kind() != SyntaxKind::BareValue {
        return None;
    }
    let attr_value = AttrValue::cast(token.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("inherit") {
        return None;
    }
    let template_ref = attr.value_text()?;

    if let Some((import_name, template_name)) = template_ref.split_once('/') {
        // Cross-file: resolve the import, read the target file, find the template.
        let import_path = resolve_import_path(root, import_name, file_path)?;
        let target_source = std::fs::read_to_string(&import_path).ok()?;
        let target_parse = parse(&target_source);
        let target_root = Root::cast(target_parse.syntax())?;
        let tmpl = target_root.stages().find_map(|s| {
            s.body()?
                .templates()
                .find(|t| t.name().as_deref() == Some(template_name))
        })?;
        let range = range_to_lsp(&target_source, tmpl.name_token()?.text_range());
        let uri = Uri::from_file_path(&import_path)?;
        Some(Location { uri, range })
    } else {
        // Local: template must be in the same stage.
        let stage = ancestor_stage(attr.syntax())?;
        let tmpl = stage
            .body()?
            .templates()
            .find(|t| t.name().as_deref() == Some(template_ref.as_str()))?;
        let range = range_to_lsp(source, tmpl.name_token()?.text_range());
        Some(Location {
            uri: current_uri.clone(),
            range,
        })
    }
}

// ─── dependencies ─────────────────────────────────────────────────────────────

fn resolve_dependency(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    current_uri: &Uri,
) -> Option<Location> {
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    let ref_node = token.parent()?;
    if ref_node.kind() != SyntaxKind::Ref {
        return None;
    }
    let ref_list = ref_node.parent()?;
    if ref_list.kind() != SyntaxKind::RefList {
        return None;
    }
    let attr_value = AttrValue::cast(ref_list.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("dependencies") {
        return None;
    }
    let ref_text = ref_node.text().to_string();
    let (stage_name, job_name) = ref_text.split_once('.')?;
    let stage = root
        .stages()
        .find(|s| s.name().as_deref() == Some(stage_name))?;
    let job = stage
        .body()?
        .jobs()
        .find(|j| j.name().as_deref() == Some(job_name))?;
    let range = range_to_lsp(source, job.name_token()?.text_range());
    Some(Location {
        uri: current_uri.clone(),
        range,
    })
}

// ─── use block location ───────────────────────────────────────────────────────

fn resolve_location_attr(token: &SyntaxToken, file_path: &Path) -> Option<Location> {
    if token.kind() != SyntaxKind::BareValue {
        return None;
    }
    let attr_value = AttrValue::cast(token.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("location") {
        return None;
    }
    // Confirm we are inside a WorkflowImport (AttrList → WorkflowImport).
    let _ = WorkflowImport::cast(attr.syntax().parent()?.parent()?)?;

    let base = file_path.parent().unwrap_or(Path::new("."));
    let target = base.join(token.text());
    if !target.exists() {
        return None;
    }
    let uri = Uri::from_file_path(&target)?;
    let range = Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    };
    Some(Location { uri, range })
}

// ─── use block name ──────────────────────────────────────────────────────────

fn resolve_import_name(token: &SyntaxToken, file_path: &Path) -> Option<Location> {
    if token.kind() != SyntaxKind::BareValue {
        return None;
    }
    let attr_value = AttrValue::cast(token.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("name") {
        return None;
    }
    let import = WorkflowImport::cast(attr.syntax().parent()?.parent()?)?;
    let location_val = import.location()?;
    let base = file_path.parent().unwrap_or(Path::new("."));
    let target = base.join(location_val.as_str());
    if !target.exists() {
        return None;
    }
    let uri = Uri::from_file_path(&target)?;
    let range = Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    };
    Some(Location { uri, range })
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn resolve_import_path(root: &Root, import_name: &str, file_path: &Path) -> Option<PathBuf> {
    let base = file_path.parent().unwrap_or(Path::new("."));
    for ub in root.use_blocks() {
        for imp in ub.imports() {
            if imp.name().as_deref() == Some(import_name)
                && let Some(loc) = imp.location()
            {
                return Some(base.join(loc.as_str()));
            }
        }
    }
    None
}

fn ancestor_stage(node: &SyntaxNode) -> Option<Stage> {
    node.ancestors().find_map(Stage::cast)
}
