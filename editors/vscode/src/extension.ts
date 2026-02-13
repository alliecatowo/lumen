import * as path from "path";
import { workspace, ExtensionContext, commands, window, Terminal } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let terminal: Terminal | undefined;

function getTerminal(): Terminal {
  if (!terminal) {
    terminal = window.createTerminal("Lumen");
  }
  return terminal;
}

function getLumenPath(): string {
  const config = workspace.getConfiguration("lumen");
  return config.get("binPath") || "lumen";
}

function runLumenCommand(args: string[]) {
  const editor = window.activeTextEditor;
  if (!editor) {
    window.showErrorMessage("No active file");
    return;
  }

  const document = editor.document;
  if (document.languageId !== "lumen" && document.languageId !== "lumen-markdown") {
    window.showErrorMessage("Not a Lumen file");
    return;
  }

  const filePath = document.uri.fsPath;
  const lumenPath = getLumenPath();
  const command = `${lumenPath} ${args.join(" ")} "${filePath}"`;

  const term = getTerminal();
  term.show();
  term.sendText(command);
}

export function activate(context: ExtensionContext) {
  // Register commands
  context.subscriptions.push(
    commands.registerCommand("lumen.check", () => {
      runLumenCommand(["check"]);
    })
  );

  context.subscriptions.push(
    commands.registerCommand("lumen.run", () => {
      runLumenCommand(["run"]);
    })
  );

  context.subscriptions.push(
    commands.registerCommand("lumen.fmt", () => {
      runLumenCommand(["fmt"]);
    })
  );

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
