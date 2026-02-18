# lumen-provider-crypto

**Cryptographic operations provider for the Lumen tool system.**

## Overview

`lumen-provider-crypto` implements the `ToolProvider` trait to expose cryptographic operations as Lumen tools. It provides pure-Rust implementations of common cryptographic primitives without requiring external libraries or system dependencies, making it suitable for both native and WebAssembly targets.

The provider offers three categories of operations: **hashing** (SHA-256, SHA-512, MD5), **encoding** (Base64), **random generation** (UUID v4, random integers), **message authentication** (HMAC-SHA256), and **digital signatures** (Ed25519 keypair generation, signing, and verification). All operations return deterministic results for the same inputs (except UUID and random integer generation, which use secure randomness).

This crate is part of Lumen's modular tool provider architecture, where each provider implements a specific domain of functionality. The crypto provider is commonly used for data integrity verification, secure token generation, and authentication workflows in AI-native applications.

## Architecture

The crate is organized into two main modules:

| Module | Path | Purpose |
|--------|------|---------|
| **Main provider** | `src/lib.rs` | Core crypto operations (hashing, encoding, HMAC, random) |
| **Ed25519 module** | `src/ed25519.rs` | Digital signature operations (keygen, sign, verify) |

**Key design decisions:**
- **Pure Rust**: Uses `sha2`, `md-5`, `base64`, `uuid`, `hmac`, and `ed25519-dalek` crates for zero-dependency cryptography
- **Hex encoding**: All hash outputs are lowercase hex strings for consistency
- **Base64 standard**: Uses standard Base64 encoding (RFC 4648) without padding variations
- **Tool-per-operation**: Each cryptographic operation is exposed as a separate tool with its own schema
- **Error safety**: Invalid inputs (malformed base64, wrong key lengths) produce clear error messages

## Key Types

### CryptoProvider

The main provider struct implementing `ToolProvider`:

```rust
pub struct CryptoProvider {
    tool: CryptoTool,
    schema: ToolSchema,
}
```

**Factory methods:**
- `CryptoProvider::sha256()` — SHA-256 hashing
- `CryptoProvider::sha512()` — SHA-512 hashing
- `CryptoProvider::md5()` — MD5 hashing
- `CryptoProvider::base64_encode()` — Base64 encoding
- `CryptoProvider::base64_decode()` — Base64 decoding
- `CryptoProvider::uuid()` — UUID v4 generation
- `CryptoProvider::random_int()` — Random integer in range
- `CryptoProvider::hmac_sha256()` — HMAC-SHA256 authentication

### Ed25519Provider

Digital signature provider for Ed25519 operations:

```rust
pub struct Ed25519Provider {
    tool: Ed25519Tool,
    schema: ToolSchema,
}
```

**Factory methods:**
- `Ed25519Provider::keygen()` — Generate Ed25519 keypair
- `Ed25519Provider::sign()` — Sign message with secret key
- `Ed25519Provider::verify()` — Verify signature with public key

## Usage

### Registering Providers

Register crypto providers with the tool registry:

```rust
use lumen_provider_crypto::{CryptoProvider, Ed25519Provider};
use lumen_rt::services::tools::ToolRegistry;

let mut registry = ToolRegistry::new();

// Hash operations
registry.register(Box::new(CryptoProvider::sha256()));
registry.register(Box::new(CryptoProvider::sha512()));
registry.register(Box::new(CryptoProvider::md5()));

// Encoding
registry.register(Box::new(CryptoProvider::base64_encode()));
registry.register(Box::new(CryptoProvider::base64_decode()));

// Random generation
registry.register(Box::new(CryptoProvider::uuid()));
registry.register(Box::new(CryptoProvider::random_int()));

// Authentication
registry.register(Box::new(CryptoProvider::hmac_sha256()));

// Digital signatures
registry.register(Box::new(Ed25519Provider::keygen()));
registry.register(Box::new(Ed25519Provider::sign()));
registry.register(Box::new(Ed25519Provider::verify()));
```

### Calling Tools

#### Hashing

```rust
use serde_json::json;

let provider = CryptoProvider::sha256();
let input = json!({"input": "hello world"});
let result = provider.call(input)?;
// Returns: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
```

#### Base64 Encoding/Decoding

```rust
let encoder = CryptoProvider::base64_encode();
let input = json!({"input": "hello world"});
let encoded = encoder.call(input)?;
// Returns: "aGVsbG8gd29ybGQ="

let decoder = CryptoProvider::base64_decode();
let decode_input = json!({"input": "aGVsbG8gd29ybGQ="});
let decoded = decoder.call(decode_input)?;
// Returns: "hello world"
```

#### UUID Generation

```rust
let provider = CryptoProvider::uuid();
let result = provider.call(json!({}))?;
// Returns: "550e8400-e29b-41d4-a716-446655440000" (random UUID v4)
```

#### Random Integer

```rust
let provider = CryptoProvider::random_int();
let input = json!({"min": 1, "max": 100});
let result = provider.call(input)?;
// Returns: random integer between 1 and 100 (inclusive)
```

#### HMAC-SHA256

```rust
let provider = CryptoProvider::hmac_sha256();
let input = json!({"message": "hello", "key": "secret"});
let result = provider.call(input)?;
// Returns: hex-encoded HMAC (64 characters, deterministic for same inputs)
```

#### Ed25519 Digital Signatures

```rust
// Generate keypair
let keygen = Ed25519Provider::keygen();
let keys = keygen.call(json!({}))?;
// Returns: {"public_key": "base64...", "secret_key": "base64..."}

let public_key = keys["public_key"].as_str().unwrap();
let secret_key = keys["secret_key"].as_str().unwrap();

// Sign message
let signer = Ed25519Provider::sign();
let signature = signer.call(json!({
    "message": "important message",
    "secret_key": secret_key
}))?;
// Returns: "base64-encoded-signature..."

// Verify signature
let verifier = Ed25519Provider::verify();
let valid = verifier.call(json!({
    "message": "important message",
    "signature": signature.as_str().unwrap(),
    "public_key": public_key
}))?;
// Returns: true (signature is valid)
```

### Using from Lumen Code

```lumen
use tool crypto.sha256 as Hash
use tool crypto.ed25519_keygen as Keygen
use tool crypto.ed25519_sign as Sign

grant Hash timeout_ms 5000
grant Keygen timeout_ms 5000
grant Sign timeout_ms 5000

cell verify_content(data: String) -> String / {crypto}
  let hash = Hash(input: data)
  return hash
end

cell sign_document(doc: String) -> String / {crypto}
  let keys = Keygen()
  let signature = Sign(message: doc, secret_key: keys.secret_key)
  return signature
end
```

## Tool Schemas

### Hashing Tools

All hash tools (`crypto.sha256`, `crypto.sha512`, `crypto.md5`) share the same schema:

**Input:**
```json
{
  "type": "object",
  "required": ["input"],
  "properties": {
    "input": { "type": "string", "description": "Input string to hash" }
  }
}
```

**Output:**
```json
{
  "type": "string",
  "description": "Hex-encoded hash"
}
```

### Base64 Tools

**Encoding input:**
```json
{
  "type": "object",
  "required": ["input"],
  "properties": {
    "input": { "type": "string", "description": "Input string to encode" }
  }
}
```

**Decoding input:**
```json
{
  "type": "object",
  "required": ["input"],
  "properties": {
    "input": { "type": "string", "description": "Base64 string to decode" }
  }
}
```

### UUID Tool

**Input:** Empty object `{}`

**Output:** UUID v4 string (36 characters with hyphens)

### Random Integer Tool

**Input:**
```json
{
  "type": "object",
  "required": ["min", "max"],
  "properties": {
    "min": { "type": "number", "description": "Minimum value (inclusive)" },
    "max": { "type": "number", "description": "Maximum value (inclusive)" }
  }
}
```

**Output:** Random integer in range [min, max]

### HMAC-SHA256 Tool

**Input:**
```json
{
  "type": "object",
  "required": ["message", "key"],
  "properties": {
    "message": { "type": "string", "description": "Message to authenticate" },
    "key": { "type": "string", "description": "Secret key" }
  }
}
```

**Output:** Hex-encoded HMAC (64 characters)

### Ed25519 Tools

**Keygen output:**
```json
{
  "type": "object",
  "properties": {
    "public_key": { "type": "string", "description": "Base64-encoded public key (32 bytes)" },
    "secret_key": { "type": "string", "description": "Base64-encoded secret key (32 bytes)" }
  }
}
```

**Sign input:**
```json
{
  "type": "object",
  "required": ["message", "secret_key"],
  "properties": {
    "message": { "type": "string", "description": "Message to sign" },
    "secret_key": { "type": "string", "description": "Base64-encoded Ed25519 secret key" }
  }
}
```

**Verify input:**
```json
{
  "type": "object",
  "required": ["message", "signature", "public_key"],
  "properties": {
    "message": { "type": "string", "description": "Original message" },
    "signature": { "type": "string", "description": "Base64-encoded Ed25519 signature" },
    "public_key": { "type": "string", "description": "Base64-encoded Ed25519 public key" }
  }
}
```

## Testing

Run the test suite:

```bash
# All crypto provider tests
cargo test -p lumen-provider-crypto

# Test with output
cargo test -p lumen-provider-crypto -- --nocapture

# Specific test
cargo test -p lumen-provider-crypto -- sha256_hash
```

The test suite includes:
- **Correctness tests**: Verifies hashes match known test vectors
- **Roundtrip tests**: Base64 encode/decode, Ed25519 sign/verify
- **Error handling**: Invalid base64, wrong key lengths, invalid signatures
- **Determinism**: HMAC and Ed25519 signatures produce consistent results
- **Security**: Different keys/messages produce different outputs

## Security Considerations

- **MD5 is insecure**: Provided for legacy compatibility only; use SHA-256 or SHA-512 for security-critical applications
- **Ed25519 keys**: Secret keys must be kept confidential; public keys can be shared freely
- **HMAC keys**: Secret keys should be high-entropy and kept secure
- **Random generation**: Uses OS-provided CSPRNG via `OsRng` (cryptographically secure)
- **Constant-time operations**: Ed25519 implementation uses constant-time operations to prevent timing attacks

## Related Crates

- **[lumen-rt](../lumen-rt/)** — Provides `ToolProvider` trait and tool registry
- **[lumen-provider-http](../lumen-provider-http/)** — HTTP client provider
- **[lumen-provider-env](../lumen-provider-env/)** — Environment variable access
- **[lumen-cli](../lumen-cli/)** — Uses crypto for package signing and TUF metadata verification
