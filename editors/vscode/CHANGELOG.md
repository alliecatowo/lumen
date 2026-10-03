# Changelog

## Unreleased
- The bundled `lumen-lsp` is now actually used: `lumen.lspPath` no longer defaults to `lumen-lsp`, so the order is explicit setting, bundled server, PATH.
- `lumen.executablePath`, `lumen.lspPath` and `lumen.binPath` are machine-scoped and restricted in untrusted workspaces.
- Commands run as tasks without a shell (file names with quotes, `$()` or backticks are safe); `Lumen: Format File` uses the language server and `lumen.formatOnSave` / `lumen.lintOnSave` are removed (use `editor.formatOnSave`; diagnostics come from the server).
- Requires VS Code 1.91 or newer (needed by vscode-languageclient 10).

## 0.6.0
- Version aligned with the Lumen 0.6.0 toolchain; bundled `lumen-lsp` is built from the 0.6.0 workspace (4-crate layout).
- CI now type-checks the extension (`tsc --noEmit`).

## 0.4.0
- Extension moved to the `alliecatowo` Open VSX namespace; new ID is `alliecatowo.lumen` (was `lumen-lang.lumen-lang`).
- Slimmer package: no sourcemaps, sources or lockfile.

## 0.1.10
- Switched to platform-specific LSP bundling.
- Fixed extension entry point resolution issues during packaging.
- Improved syntax highlighting for markdown-embedded Lumen code.

## 0.1.0
- Initial release with syntax highlighting and LSP support.
