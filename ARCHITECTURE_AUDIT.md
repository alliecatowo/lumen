# Architecture Audit: Lumen & Wares

## Current State (Dueling Systems)

We currently have multiple overlapping systems for package management and registry interaction.

### 1. Old Lumen Registry
- **Implementation**: `src/registry.rs`, `src/pkg.rs`, `src/registry_cmd.rs`
- **Binaries**: `lpm`, `lpx`
- **Auth**: Token-based
- **Features**: Basic R2 client, simple dependency resolution.

### 2. Wares (Secure Registry)
- **Implementation**: `src/wares/check.rs` (wrapper), `src/trust.rs`
- **Binaries**: `wares`, `wrhs`
- **Auth**: OIDC (GitHub/GitLab/Google) + Ephemeral Certs
- **Features**: Sigstore-style signing, Transparency Log, SLSA provenance.

### 3. Server Redundancy
- **Rust Server**: `registry-server/` (Axum, OIDC, CA, R2) - *To be removed*
- **TypeScript Worker**: `workers/registry/` (Cloudflare R2, OIDC) - *Target*

## Target Architecture

We are consolidating to a single client/server model:

- **Client**: `lumen` (language CLI) + `wares` (package manager CLI).
- **Library**: `src/wares/` shared library (Trust, Resolver, Storage).
- **Server**: Cloudflare Worker (`workers/registry`) enhanced with CA.

## Cleanup Actions

1.  **Remove Binaries**: `lpm`, `lpx` are redundant. Use `lumen pkg` or `wares`.
2.  **Remove Rust Server**: Port CA logic to TypeScript Worker, then delete `registry-server/`.
3.  **Consolidate Code**: Move `trust.rs`, `resolver.rs`, `registry.rs` logic into `src/wares/`.
