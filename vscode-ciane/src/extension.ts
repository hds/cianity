import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const serverOptions: ServerOptions = {
    command: 'cianity',
    args: ['lsp'],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'ciane' }],
  };

  client = new LanguageClient(
    'cianity',
    'Cianity Language Server',
    serverOptions,
    clientOptions,
  );

  context.subscriptions.push(client);
  client.start();
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
