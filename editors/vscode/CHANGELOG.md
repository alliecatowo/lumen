# Changelog

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
