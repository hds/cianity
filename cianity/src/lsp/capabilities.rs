use tower_lsp_server::ls_types::{
    CompletionOptions, DocumentFormattingOptions, HoverProviderCapability, OneOf, RenameOptions,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, WorkDoneProgressOptions,
};

/// Return the initial server capabilities advertised to the client.
#[must_use]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Right(DocumentFormattingOptions {
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["(".to_owned(), "=".to_owned(), "[".to_owned()]),
            ..CompletionOptions::default()
        }),
        ..ServerCapabilities::default()
    }
}
