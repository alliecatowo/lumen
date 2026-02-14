"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
let terminal;
function getTerminal() {
    if (!terminal) {
        terminal = vscode_1.window.createTerminal("Lumen");
    }
    return terminal;
}
function getLumenPath() {
    const config = vscode_1.workspace.getConfiguration("lumen");
    return config.get("executablePath") || config.get("binPath") || "lumen";
}
function runLumenCommand(args) {
    const editor = vscode_1.window.activeTextEditor;
    if (!editor) {
        vscode_1.window.showErrorMessage("No active file");
        return;
    }
    const document = editor.document;
    const isLumenMarkdown = document.languageId === "markdown" &&
        document.uri.fsPath.endsWith(".lm.md");
    if (document.languageId !== "lumen" && !isLumenMarkdown) {
        vscode_1.window.showErrorMessage("Not a Lumen file");
        return;
    }
    const filePath = document.uri.fsPath;
    const lumenPath = getLumenPath();
    const command = `${lumenPath} ${args.join(" ")} "${filePath}"`;
    const term = getTerminal();
    term.show();
    term.sendText(command);
}
// ── Lumen syntax highlighter for markdown preview ──
const CONTROL_KEYWORDS = new Set([
    "cell", "end", "let", "if", "else", "for", "in", "match", "return",
    "break", "continue", "while", "loop", "when", "fn", "async", "await",
    "try", "catch", "halt", "yield", "with", "then", "as", "mut", "self",
    "spawn", "finally",
]);
const DECL_KEYWORDS = new Set([
    "record", "enum", "use", "tool", "grant", "bind", "effect", "handler",
    "handle", "agent", "pipeline", "orchestration", "machine", "memory",
    "guardrail", "eval", "pattern", "process", "trait", "impl", "type",
    "const", "import", "from", "pub", "mod", "macro", "extern", "union",
    "state", "transition", "stage", "to",
]);
const DECL_WITH_TYPE = new Set([
    "record", "enum", "process", "trait", "impl", "type", "const",
    "import", "from", "pub", "mod", "macro", "extern", "union",
]);
const AI_KEYWORDS = new Set([
    "role", "expect", "schema", "emit", "parallel", "race", "vote",
    "select", "timeout", "observe", "approve", "checkpoint", "escalate",
    "comptime", "where", "step", "guard",
]);
const LOGICAL_OPERATORS = new Set(["and", "or", "not", "is"]);
const CONSTANTS = new Set(["true", "false", "null"]);
const BUILTIN_TYPES = new Set([
    "Int", "Float", "String", "Bool", "Bytes", "Json", "Null", "Any", "Void",
    "list", "map", "set", "tuple", "result", "int", "float", "string",
    "bool", "bytes", "json", "ok", "err",
]);
function escapeHtml(text) {
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}
function highlightLumen(code) {
    const lines = code.split("\n");
    const result = [];
    for (const line of lines) {
        // Handle role blocks: role (system|user|assistant): rest of line is string
        const roleMatch = line.match(/^(\s*)(role)\s+(system|user|assistant)\s*:\s*(.*)/);
        if (roleMatch) {
            const [, indent, keyword, roleName, content] = roleMatch;
            // Highlight interpolations in content
            const contentHtml = content.replace(/\{([^}]*)\}/g, (_m, inner) => {
                return `<span class="lm-interp">{${escapeHtml(inner)}}</span>`;
            }).replace(/([^{]*?)(?=\{|$)/g, (m) => {
                if (m && !m.startsWith("{")) {
                    return `<span class="lm-role-text">${escapeHtml(m)}</span>`;
                }
                return m;
            });
            result.push(`${escapeHtml(indent)}<span class="lm-ai">${keyword}</span> <span class="lm-role-name">${roleName}</span>: ${highlightRoleContent(content)}`);
            continue;
        }
        result.push(highlightLine(line));
    }
    return result.join("\n");
}
function highlightRoleContent(content) {
    let out = "";
    let i = 0;
    while (i < content.length) {
        if (content[i] === "{") {
            let j = i + 1;
            while (j < content.length && content[j] !== "}") {
                j++;
            }
            const inner = content.slice(i + 1, j);
            out += `<span class="lm-interp">{${escapeHtml(inner)}}</span>`;
            i = j + 1;
        }
        else {
            let j = i;
            while (j < content.length && content[j] !== "{") {
                j++;
            }
            out += `<span class="lm-role-text">${escapeHtml(content.slice(i, j))}</span>`;
            i = j;
        }
    }
    return out;
}
function highlightLine(line) {
    let highlighted = "";
    let i = 0;
    let prevWord = "";
    while (i < line.length) {
        // Comments
        if (line[i] === "#") {
            highlighted += `<span class="lm-comment">${escapeHtml(line.slice(i))}</span>`;
            break;
        }
        // Directives
        if (line[i] === "@" && i + 1 < line.length && /[a-zA-Z]/.test(line[i + 1])) {
            let j = i + 1;
            while (j < line.length && /[a-zA-Z0-9_]/.test(line[j])) {
                j++;
            }
            highlighted += `<span class="lm-directive">${escapeHtml(line.slice(i, j))}</span>`;
            i = j;
            continue;
        }
        // Strings
        if (line[i] === '"') {
            let j = i + 1;
            let str = '"';
            while (j < line.length && line[j] !== '"') {
                if (line[j] === "\\") {
                    str += line[j] + (line[j + 1] || "");
                    j += 2;
                    continue;
                }
                if (line[j] === "{") {
                    // String interpolation
                    highlighted += `<span class="lm-string">${escapeHtml(str)}</span>`;
                    str = "";
                    let k = j + 1;
                    while (k < line.length && line[k] !== "}") {
                        k++;
                    }
                    highlighted += `<span class="lm-interp">{${escapeHtml(line.slice(j + 1, k))}}</span>`;
                    j = k + 1;
                    continue;
                }
                str += line[j];
                j++;
            }
            str += '"';
            j = Math.min(j + 1, line.length);
            highlighted += `<span class="lm-string">${escapeHtml(str)}</span>`;
            i = j;
            continue;
        }
        // Numbers
        if (/[0-9]/.test(line[i]) && (i === 0 || /[\s(,\[=<>+\-*/%!:]/.test(line[i - 1]))) {
            let j = i;
            if (line.slice(i, i + 2) === "0x") {
                j = i + 2;
                while (j < line.length && /[0-9a-fA-F_]/.test(line[j])) {
                    j++;
                }
            }
            else if (line.slice(i, i + 2) === "0b") {
                j = i + 2;
                while (j < line.length && /[01_]/.test(line[j])) {
                    j++;
                }
            }
            else {
                while (j < line.length && /[0-9_.]/.test(line[j])) {
                    j++;
                }
            }
            highlighted += `<span class="lm-number">${escapeHtml(line.slice(i, j))}</span>`;
            i = j;
            continue;
        }
        // Property access: .name (not followed by `(`)
        if (line[i] === "." && i + 1 < line.length && /[a-z_]/.test(line[i + 1])) {
            let j = i + 1;
            while (j < line.length && /[a-zA-Z0-9_]/.test(line[j])) {
                j++;
            }
            const name = line.slice(i + 1, j);
            // Check if followed by ( => method call
            let k = j;
            while (k < line.length && line[k] === " ") {
                k++;
            }
            if (k < line.length && line[k] === "(") {
                highlighted += `.<span class="lm-method">${escapeHtml(name)}</span>`;
            }
            else {
                highlighted += `.<span class="lm-property">${escapeHtml(name)}</span>`;
            }
            i = j;
            continue;
        }
        // Words (keywords, types, identifiers)
        if (/[a-zA-Z_]/.test(line[i])) {
            let j = i;
            while (j < line.length && /[a-zA-Z0-9_]/.test(line[j])) {
                j++;
            }
            const word = line.slice(i, j);
            // Look ahead for `(` to detect function/constructor calls
            let k = j;
            while (k < line.length && line[k] === " ") {
                k++;
            }
            const followedByParen = k < line.length && line[k] === "(";
            // Look ahead for `:` to detect named params
            let m = j;
            while (m < line.length && line[m] === " ") {
                m++;
            }
            const followedByColon = m < line.length && line[m] === ":";
            if (CONTROL_KEYWORDS.has(word)) {
                // Special handling: cell followed by name
                if (word === "cell") {
                    highlighted += `<span class="lm-keyword">${word}</span>`;
                    // Look ahead for function name
                    let n = j;
                    while (n < line.length && line[n] === " ") {
                        n++;
                    }
                    if (n < line.length && /[a-zA-Z_]/.test(line[n])) {
                        let p = n;
                        while (p < line.length && /[a-zA-Z0-9_]/.test(line[p])) {
                            p++;
                        }
                        const funcName = line.slice(n, p);
                        if (!CONTROL_KEYWORDS.has(funcName) && !DECL_KEYWORDS.has(funcName)) {
                            highlighted += escapeHtml(line.slice(j, n));
                            highlighted += `<span class="lm-func-def">${escapeHtml(funcName)}</span>`;
                            i = p;
                            prevWord = funcName;
                            continue;
                        }
                    }
                }
                else if (word === "let") {
                    highlighted += `<span class="lm-keyword">${word}</span>`;
                    // Look ahead for variable name
                    let n = j;
                    while (n < line.length && line[n] === " ") {
                        n++;
                    }
                    // Handle `mut`
                    if (line.slice(n, n + 3) === "mut" && (n + 3 >= line.length || /\s/.test(line[n + 3]))) {
                        highlighted += escapeHtml(line.slice(j, n));
                        highlighted += `<span class="lm-keyword">mut</span>`;
                        let q = n + 3;
                        while (q < line.length && line[q] === " ") {
                            q++;
                        }
                        if (q < line.length && /[a-zA-Z_]/.test(line[q])) {
                            let p = q;
                            while (p < line.length && /[a-zA-Z0-9_]/.test(line[p])) {
                                p++;
                            }
                            highlighted += escapeHtml(line.slice(n + 3, q));
                            highlighted += `<span class="lm-var-decl">${escapeHtml(line.slice(q, p))}</span>`;
                            i = p;
                            prevWord = word;
                            continue;
                        }
                        i = n + 3;
                        prevWord = word;
                        continue;
                    }
                    if (n < line.length && /[a-zA-Z_]/.test(line[n])) {
                        let p = n;
                        while (p < line.length && /[a-zA-Z0-9_]/.test(line[p])) {
                            p++;
                        }
                        highlighted += escapeHtml(line.slice(j, n));
                        highlighted += `<span class="lm-var-decl">${escapeHtml(line.slice(n, p))}</span>`;
                        i = p;
                        prevWord = word;
                        continue;
                    }
                }
                else if (word === "for") {
                    highlighted += `<span class="lm-keyword">${word}</span>`;
                    // Look ahead for loop variable
                    let n = j;
                    while (n < line.length && line[n] === " ") {
                        n++;
                    }
                    if (n < line.length && /[a-zA-Z_]/.test(line[n])) {
                        let p = n;
                        while (p < line.length && /[a-zA-Z0-9_]/.test(line[p])) {
                            p++;
                        }
                        const varName = line.slice(n, p);
                        if (varName !== "in") {
                            highlighted += escapeHtml(line.slice(j, n));
                            highlighted += `<span class="lm-var-decl">${escapeHtml(varName)}</span>`;
                            i = p;
                            prevWord = word;
                            continue;
                        }
                    }
                }
                else {
                    highlighted += `<span class="lm-keyword">${word}</span>`;
                }
                i = j;
                prevWord = word;
                continue;
            }
            if (DECL_KEYWORDS.has(word)) {
                highlighted += `<span class="lm-decl">${word}</span>`;
                // Look ahead for type name (uppercase start)
                if (DECL_WITH_TYPE.has(word)) {
                    let n = j;
                    while (n < line.length && line[n] === " ") {
                        n++;
                    }
                    if (n < line.length && /[A-Z]/.test(line[n])) {
                        let p = n;
                        while (p < line.length && /[a-zA-Z0-9_]/.test(line[p])) {
                            p++;
                        }
                        highlighted += escapeHtml(line.slice(j, n));
                        highlighted += `<span class="lm-type-def">${escapeHtml(line.slice(n, p))}</span>`;
                        i = p;
                        prevWord = word;
                        continue;
                    }
                }
                i = j;
                prevWord = word;
                continue;
            }
            if (AI_KEYWORDS.has(word)) {
                highlighted += `<span class="lm-ai">${word}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            if (LOGICAL_OPERATORS.has(word)) {
                highlighted += `<span class="lm-operator">${word}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            if (CONSTANTS.has(word)) {
                highlighted += `<span class="lm-constant">${word}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            if (BUILTIN_TYPES.has(word)) {
                highlighted += `<span class="lm-type">${word}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            // Uppercase word followed by ( => type constructor
            if (word[0] >= "A" && word[0] <= "Z" && followedByParen) {
                highlighted += `<span class="lm-constructor">${escapeHtml(word)}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            // Uppercase word => type annotation
            if (word[0] >= "A" && word[0] <= "Z") {
                highlighted += `<span class="lm-type-ann">${escapeHtml(word)}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            // Lowercase word followed by ( => function call
            if (followedByParen && !CONTROL_KEYWORDS.has(word) && !DECL_KEYWORDS.has(word)) {
                highlighted += `<span class="lm-func-call">${escapeHtml(word)}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            // Lowercase word followed by : => named parameter
            if (followedByColon && prevWord !== "match") {
                highlighted += `<span class="lm-param-label">${escapeHtml(word)}</span>`;
                i = j;
                prevWord = word;
                continue;
            }
            // Regular identifier
            highlighted += escapeHtml(word);
            i = j;
            prevWord = word;
            continue;
        }
        // Multi-char operators
        if (i + 1 < line.length) {
            const two = line.slice(i, i + 2);
            if (two === "=>" || two === "->" || two === "|>" || two === "==" || two === "!=" || two === "<=" || two === ">=" || two === "+=" || two === "-=" || two === "*=" || two === "/=" || two === "%=") {
                highlighted += `<span class="lm-operator">${escapeHtml(two)}</span>`;
                i += 2;
                continue;
            }
        }
        highlighted += escapeHtml(line[i]);
        i++;
    }
    return highlighted;
}
// ── Activate ──
function activate(context) {
    // Register commands
    context.subscriptions.push(vscode_1.commands.registerCommand("lumen.check", () => {
        runLumenCommand(["check"]);
    }));
    context.subscriptions.push(vscode_1.commands.registerCommand("lumen.run", () => {
        runLumenCommand(["run"]);
    }));
    context.subscriptions.push(vscode_1.commands.registerCommand("lumen.fmt", () => {
        runLumenCommand(["fmt"]);
    }));
    context.subscriptions.push(vscode_1.commands.registerCommand("lumen.lint", () => {
        runLumenCommand(["check"]);
    }));
    context.subscriptions.push(vscode_1.commands.registerCommand("lumen.repl", () => {
        const lumenPath = getLumenPath();
        const term = getTerminal();
        term.show();
        term.sendText(`${lumenPath} repl`);
    }));
    // Format on save
    context.subscriptions.push(vscode_1.workspace.onWillSaveTextDocument((event) => {
        const config = vscode_1.workspace.getConfiguration("lumen");
        const formatOnSave = config.get("formatOnSave", false);
        if (!formatOnSave) {
            return;
        }
        const document = event.document;
        const isLumenMarkdown = document.languageId === "markdown" &&
            document.uri.fsPath.endsWith(".lm.md");
        if (document.languageId === "lumen" || isLumenMarkdown) {
            const lumenPath = getLumenPath();
            const filePath = document.uri.fsPath;
            const term = getTerminal();
            term.sendText(`${lumenPath} fmt "${filePath}"`, false);
        }
    }));
    // Lint on save
    context.subscriptions.push(vscode_1.workspace.onDidSaveTextDocument((document) => {
        const config = vscode_1.workspace.getConfiguration("lumen");
        const lintOnSave = config.get("lintOnSave", true);
        if (!lintOnSave) {
            return;
        }
        const isLumenMarkdown = document.languageId === "markdown" &&
            document.uri.fsPath.endsWith(".lm.md");
        if (document.languageId === "lumen" || isLumenMarkdown) {
            const lumenPath = getLumenPath();
            const filePath = document.uri.fsPath;
            const term = getTerminal();
            term.sendText(`${lumenPath} check "${filePath}"`, false);
        }
    }));
    // Look for `lumen-lsp` binary.
    // 1. Check workspace configuration
    // 2. Check bundled binary in `server/` directory
    // 3. Fallback to PATH ("lumen-lsp")
    const config = vscode_1.workspace.getConfiguration("lumen");
    let serverPath = config.get("lspPath");
    if (!serverPath) {
        const platform = process.platform;
        const isWindows = platform === "win32";
        const binaryName = isWindows ? "lumen-lsp.exe" : "lumen-lsp";
        const bundledPath = context.asAbsolutePath(`server/${binaryName}`);
        try {
            const fs = require("fs");
            if (fs.existsSync(bundledPath)) {
                serverPath = bundledPath;
            }
        }
        catch (e) {
            // Ignore error, fall back to PATH
        }
    }
    if (!serverPath) {
        serverPath = "lumen-lsp";
    }
    const serverOptions = {
        command: serverPath,
        args: [],
    };
    const outputChannel = vscode_1.window.createOutputChannel("Lumen Language Server");
    outputChannel.appendLine("Lumen extension activating...");
    const clientOptions = {
        documentSelector: [
            { scheme: "file", language: "lumen" },
            { scheme: "file", language: "markdown", pattern: "**/*.lm.md" },
        ],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher("**/*.lm.md"),
        },
        outputChannel,
        revealOutputChannelOn: 3, // Error only
    };
    client = new node_1.LanguageClient("lumenLsp", "Lumen Language Server", serverOptions, clientOptions);
    client.start();
    outputChannel.appendLine(`LSP client started (server: ${serverPath})`);
    // Markdown preview integration — highlight ```lumen fenced blocks
    return {
        extendMarkdownIt(md) {
            const defaultFence = md.renderer.rules.fence ||
                function (tokens, idx, options, _env, self) {
                    return self.renderToken(tokens, idx, options);
                };
            md.renderer.rules.fence = (tokens, idx, options, env, self) => {
                const token = tokens[idx];
                const info = (token.info || "").trim().toLowerCase();
                if (info === "lumen") {
                    const highlighted = highlightLumen(token.content);
                    return `<pre class="lumen-preview"><code>${highlighted}</code></pre>`;
                }
                return defaultFence(tokens, idx, options, env, self);
            };
            return md;
        },
    };
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
//# sourceMappingURL=extension.js.map