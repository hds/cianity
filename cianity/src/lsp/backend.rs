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
        Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, InitializeParams, InitializeResult, Position, Range, ServerInfo,
        Uri,
    },
};

use super::capabilities::server_capabilities;

pub struct Backend {
    client: Client,
    documents: Arc<Mutex<HashMap<String, Parse>>>,
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
        let parse = parse(&text);
        let lsp_diagnostics = collect_diagnostics(&parse, &text);

        {
            let mut docs = self.documents.lock().await;
            docs.insert(uri.to_string(), parse);
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
    let start = offset_to_position(source, span.start);
    let end = offset_to_position(source, span.end);
    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        message: message.to_owned(),
        ..Diagnostic::default()
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let clamped = offset.min(source.len());
    let prefix = &source[..clamped];
    let line = prefix.bytes().filter(|&b| b == b'\n').count();
    let col = prefix.rfind('\n').map_or(clamped, |nl| clamped - nl - 1);
    Position::new(
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}
