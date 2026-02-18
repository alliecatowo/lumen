---
name: security-infra
description: Lumen's security infrastructure - Ed25519 signing, TUF metadata verification, OIDC auth, transparency logs, capability sandboxing
---

# Lumen Security Infrastructure

## Package Supply Chain Security

### Ed25519 Signing (`rust/lumen-cli/src/auth.rs`)
- Real Ed25519 key generation and signing via `ed25519-dalek`
- Used for package publishing, registry auth, TUF metadata
- API token management with secure storage

### TUF Metadata Verification (`rust/lumen-cli/src/tuf.rs`)
Full implementation of The Update Framework:
- Four TUF roles: Root, Targets, Snapshot, Timestamp
- Ed25519 signature verification (HMAC-SHA256 fallback)
- Threshold signing: configurable required signatures per role
- Rollback detection: version numbers must increase monotonically
- Expiration enforcement: stale metadata rejected
- Root rotation: cross-signed new root
- Target verification: content hash and size checked

### OIDC Authentication (`rust/lumen-cli/src/oidc.rs`)
- OpenID Connect token verification for registry auth
- Standard OIDC flows with ID token validation

### Transparency Log (`rust/lumen-cli/src/transparency.rs`)
- Merkle tree append-only log for all published packages
- Tamper-evident record of all package versions
- Inclusion proofs for verification

### Audit Logging (`rust/lumen-cli/src/audit.rs`)
- Structured audit log for security-relevant operations

## Runtime Security

### Capability Sandbox (`rust/lumen-compiler/src/compiler/sandbox.rs`)
- Each cell runs with only explicitly declared grants
- Undeclared tool calls rejected at compile time (resolve phase) AND runtime
- Recursive capability scoping with parent intersection

### Tool Policy Enforcement (`rust/lumen-rt/src/services/tools.rs`)
- `validate_tool_policy()` checks grant constraints before every tool dispatch
- Constraint keys: domain (URL patterns), timeout_ms, max_tokens
- Policy violations produce structured errors

### Cryptography (`rust/lumen-rt/src/services/crypto.rs`)
Pure-Rust implementations:
- SHA-256, BLAKE3
- HMAC-SHA256
- HKDF key derivation
- Ed25519 signatures
- UUID v4 generation
- Base64 encoding/decoding
