# Lumen CLI Crate

Command-line interface, package manager, and security infrastructure.

## Quick Reference
- **Entry point**: `bin/lumen.rs` with Clap subcommands
- **Test command**: `cargo test -p lumen-cli`
- **Key commands**: check, run, emit, repl, fmt, pkg, trace, cache, build wasm, lang-ref

## Module Map
| File | Purpose |
|------|---------|
| `bin/lumen.rs` | Clap entry point |
| `module_resolver.rs` | File-based import resolution (.lm.md → .lm → .lumen) |
| `repl.rs` | Interactive REPL (rustyline) |
| `fmt.rs` | AST-based code formatter |
| `wares/` | Package manager ("Wares") |
| `registry.rs` | Content-addressed registry client (R2) |
| `workspace.rs` | Multi-package workspace resolver |
| `binary_cache.rs` | Content-addressable LRU build cache |
| `service_template.rs` | Service project scaffolding |
| `auth.rs` | Ed25519 signing, API tokens |
| `oidc.rs` | OpenID Connect verification |
| `tuf.rs` | TUF 4-role metadata verification |
| `transparency.rs` | Merkle tree transparency log |
| `audit.rs` | Structured audit logging |
| `error_chain.rs` | Structured error chain formatting |
| `ci.rs` | CI configuration (Miri, coverage, sanitizers) |
| `bindgen.rs` | C-to-Lumen bindgen |
