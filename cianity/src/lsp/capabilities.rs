use tower_lsp_server::ls_types::{
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

/// Return the initial server capabilities advertised to the client.
#[must_use]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..ServerCapabilities::default()
    }
}
