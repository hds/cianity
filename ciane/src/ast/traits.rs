use smol_str::SmolStr;

use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

/// The common interface for all typed AST nodes.
///
/// Every AST node is a thin, zero-cost wrapper around a [`SyntaxNode`].
pub trait AstNode: Sized {
    /// Attempt to cast a raw `SyntaxNode` into this typed node.
    ///
    /// Returns `None` if the node kind does not match.
    fn cast(node: SyntaxNode) -> Option<Self>;

    /// The underlying `SyntaxNode`.
    fn syntax(&self) -> &SyntaxNode;
}

/// An AST node that has a name (an `Ident` token wrapped in a `Name` node).
pub trait HasName: AstNode {
    /// Return the identifier token that is the item's name, if present.
    fn name_token(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::Name)
            .and_then(|n| {
                n.children_with_tokens()
                    .find_map(rowan::NodeOrToken::into_token)
            })
    }

    /// The name as a string slice, if present.
    fn name(&self) -> Option<SmolStr> {
        self.name_token().map(|t| t.text().into())
    }
}

/// An AST node that may carry an attribute list.
pub trait HasAttrList: AstNode {
    /// Return the `AttrList` child node, if present.
    fn attr_list(&self) -> Option<super::nodes::AttrList> {
        self.syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::AttrList)
            .and_then(super::nodes::AttrList::cast)
    }
}
