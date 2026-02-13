import * as path from "path";
import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
  // Look for `lumen-lsp` binary. Prefer a workspace-local build, fall back to PATH.
  const config = workspace.getConfiguration("lumen");
  const serverPath: string =
    config.get("lspPath") || "lumen-lsp";

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "lumen" },
      { scheme: "file", language: "lumen-markdown" },
      { scheme: "file", pattern: "**/*.lm.md" },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.lm.md"),
    },
  };

  client = new LanguageClient(
    "lumenLsp",
    "Lumen Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
