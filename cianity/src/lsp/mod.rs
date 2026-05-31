mod backend;
mod capabilities;
mod completion;
mod definition;
mod format;
mod hover;
mod references;
mod rename;
mod symbols;
mod util;

#[cfg(test)]
mod tests;

use tower_lsp_server::{LspService, Server};

use backend::Backend;

/// Start the LSP server, communicating over stdin/stdout.
pub async fn start() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
