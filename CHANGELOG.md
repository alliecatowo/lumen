# Changelog

All notable changes to the Lumen project will be documented in this file.

## [0.1.10] - 2026-02-14

### Changed
- Switched HTTP providers to use `rustls-tls` to fix cross-compilation linking issues with `openssl`.
- Improved VS Code extension packaging to include platform-specific LSP binaries.
- Updated extension publishing to use `npx ovsx` with Node 20.

### Fixed
- Fixed critical bug in `lower.rs` where `where` clause record constraints were overwriting registers.
- Resolved hardcoded API key security vulnerability in Gemini provider.
- Whitelisted `out/` directory in `.vscodeignore` to ensure compiled JavaScript is included in VSIX.
- Fixed MUSL build issues by using `cross` and static linking for `openssl`.

## [0.1.0] - 2026-02-12

### Added
- Initial release of Lumen compiler, LSP, and CLI.
- AI-native primitives: tools, grants, and processes.
- Markdown-native source format support.
