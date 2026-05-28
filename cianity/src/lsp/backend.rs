use std::{collections::HashMap, sync::Arc};

use ciane::{
    ast::{AstNode, Root},
    error::Severity,
    parse,
    parser::Parse,
    validation::validate,
};
use tokio::sync::Mutex;
use tower_lsp_server::{
    Client, LanguageServer,
    jsonrpc::Result,
    ls_types::{
        CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse,
        GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InitializeParams,
        InitializeResult, Location, PrepareRenameResponse, Range, ReferenceParams, RenameParams,
        ServerInfo, TextDocumentPositionParams, TextEdit, Uri, WorkspaceEdit,
    },
};

use super::{
    capabilities::server_capabilities, completion, definition, format, hover, references, rename,
    symbols, util,
};

pub struct Backend {
    client: Client,
    documents: Arc<Mutex<HashMap<String, (Parse, String)>>>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn on_change(&self, uri: Uri, text: String) {
        let parsed = parse(&text);
        let lsp_diagnostics = collect_diagnostics(&parsed, &text);
        {
            let mut docs = self.documents.lock().await;
            docs.insert(uri.to_string(), (parsed, text));
        }
        self.client
            .publish_diagnostics(uri, lsp_diagnostics, None)
            .await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "cianity".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
            offset_encoding: None,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.remove(&params.text_document.uri.to_string());
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        Ok(hover::at(parse, source, offset))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        let range = definition::resolve(parse, source, offset);
        Ok(range.map(|r| GotoDefinitionResponse::Scalar(Location { uri, range: r })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(symbols::collect(
            parse, source,
        ))))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        let items = completion::at(parse, source, offset);
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        Ok(format::edits(parse, source))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        Ok(
            references::find(parse, source, offset, include_declaration).map(|ranges| {
                ranges
                    .into_iter()
                    .map(|range| Location {
                        uri: uri.clone(),
                        range,
                    })
                    .collect()
            }),
        )
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        Ok(rename::prepare(parse, source, offset))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        let docs = self.documents.lock().await;
        let Some((parse, source)) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };
        let offset = util::position_to_offset(source, position);
        Ok(
            rename::edits_for(parse, source, offset, &new_name).map(|edits| {
                let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
                changes.insert(uri, edits);
                WorkspaceEdit {
                    changes: Some(changes),
                    ..WorkspaceEdit::default()
                }
            }),
        )
    }
}

fn collect_diagnostics(parse: &Parse, source: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = parse
        .errors()
        .iter()
        .map(|e| {
            make_diagnostic(
                source,
                e.span.clone(),
                &e.message,
                DiagnosticSeverity::ERROR,
            )
        })
        .collect();

    if let Some(root) = Root::cast(parse.syntax()) {
        for diag in validate(&root) {
            let severity = match diag.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
            };
            out.push(make_diagnostic(source, diag.span, &diag.message, severity));
        }
    }

    out
}

fn make_diagnostic(
    source: &str,
    span: std::ops::Range<usize>,
    message: &str,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    let start = util::offset_to_position(source, span.start);
    let end = util::offset_to_position(source, span.end);
    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        message: message.to_owned(),
        ..Diagnostic::default()
    }
}
