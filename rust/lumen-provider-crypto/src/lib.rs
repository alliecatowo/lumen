//! Cryptography provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose cryptographic operations as tools:
//! - `crypto.sha256` — SHA-256 hash
//! - `crypto.sha512` — SHA-512 hash
//! - `crypto.md5` — MD5 hash
//! - `crypto.base64_encode` — Base64 encoding
//! - `crypto.base64_decode` — Base64 decoding
//! - `crypto.uuid` — Generate UUID v4
//! - `crypto.random_int` — Random integer in range
//! - `crypto.hmac_sha256` — HMAC-SHA256
//! - `crypto.ed25519_keygen` — Generate Ed25519 keypair
//! - `crypto.ed25519_sign` — Sign with Ed25519
//! - `crypto.ed25519_verify` — Verify Ed25519 signature
//!
//! All hash operations return hexadecimal strings.

pub mod ed25519;
pub use ed25519::Ed25519Provider;

use hmac::{Hmac, Mac};
use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use md5::Md5;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha512};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CryptoTool enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CryptoTool {
    Sha256,
    Sha512,
    Md5,
    Base64Encode,
    Base64Decode,
    Uuid,
    RandomInt,
    HmacSha256,
}

impl CryptoTool {
    fn tool_name(&self) -> &'static str {
        match self {
            CryptoTool::Sha256 => "crypto.sha256",
            CryptoTool::Sha512 => "crypto.sha512",
            CryptoTool::Md5 => "crypto.md5",
            CryptoTool::Base64Encode => "crypto.base64_encode",
            CryptoTool::Base64Decode => "crypto.base64_decode",
            CryptoTool::Uuid => "crypto.uuid",
            CryptoTool::RandomInt => "crypto.random_int",
            CryptoTool::HmacSha256 => "crypto.hmac_sha256",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            CryptoTool::Sha256 => "Compute SHA-256 hash (returns hex string)",
            CryptoTool::Sha512 => "Compute SHA-512 hash (returns hex string)",
            CryptoTool::Md5 => "Compute MD5 hash (returns hex string)",
            CryptoTool::Base64Encode => "Encode string to base64",
            CryptoTool::Base64Decode => "Decode base64 string",
            CryptoTool::Uuid => "Generate a random UUID v4",
            CryptoTool::RandomInt => "Generate a random integer in the specified range (inclusive)",
            CryptoTool::HmacSha256 => "Compute HMAC-SHA256 (returns hex string)",
        }
    }
}

// ---------------------------------------------------------------------------
// CryptoProvider implementation
// ---------------------------------------------------------------------------

/// Cryptography provider implementing the `ToolProvider` trait.
pub struct CryptoProvider {
    tool: CryptoTool,
    schema: ToolSchema,
}

impl CryptoProvider {
    /// Create a new crypto provider for the given tool.
    fn new(tool: CryptoTool) -> Self {
        let (input_schema, output_schema) = match tool {
            CryptoTool::Sha256 | CryptoTool::Sha512 | CryptoTool::Md5 => (
                json!({
                    "type": "object",
                    "required": ["input"],
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Input string to hash"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Hex-encoded hash"
                }),
            ),
            CryptoTool::Base64Encode => (
                json!({
                    "type": "object",
                    "required": ["input"],
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Input string to encode"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Base64-encoded string"
                }),
            ),
            CryptoTool::Base64Decode => (
                json!({
                    "type": "object",
                    "required": ["input"],
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Base64 string to decode"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Decoded string"
                }),
            ),
            CryptoTool::Uuid => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "string",
                    "description": "UUID v4 string"
                }),
            ),
            CryptoTool::RandomInt => (
                json!({
                    "type": "object",
                    "required": ["min", "max"],
                    "properties": {
                        "min": {
                            "type": "number",
                            "description": "Minimum value (inclusive)"
                        },
                        "max": {
                            "type": "number",
                            "description": "Maximum value (inclusive)"
                        }
                    }
                }),
                json!({
                    "type": "number",
                    "description": "Random integer in range [min, max]"
                }),
            ),
            CryptoTool::HmacSha256 => (
                json!({
                    "type": "object",
                    "required": ["message", "key"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to authenticate"
                        },
                        "key": {
                            "type": "string",
                            "description": "Secret key"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Hex-encoded HMAC"
                }),
            ),
        };

        let schema = ToolSchema {
            name: tool.tool_name().to_string(),
            description: tool.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["crypto".to_string()],
        };

        Self { tool, schema }
    }

    /// Create a SHA-256 provider.
    pub fn sha256() -> Self {
        Self::new(CryptoTool::Sha256)
    }

    /// Create a SHA-512 provider.
    pub fn sha512() -> Self {
        Self::new(CryptoTool::Sha512)
    }

    /// Create an MD5 provider.
    pub fn md5() -> Self {
        Self::new(CryptoTool::Md5)
    }

    /// Create a Base64 encode provider.
    pub fn base64_encode() -> Self {
        Self::new(CryptoTool::Base64Encode)
    }

    /// Create a Base64 decode provider.
    pub fn base64_decode() -> Self {
        Self::new(CryptoTool::Base64Decode)
    }

    /// Create a UUID provider.
    pub fn uuid() -> Self {
        Self::new(CryptoTool::Uuid)
    }

    /// Create a random int provider.
    pub fn random_int() -> Self {
        Self::new(CryptoTool::RandomInt)
    }

    /// Create an HMAC-SHA256 provider.
    pub fn hmac_sha256() -> Self {
        Self::new(CryptoTool::HmacSha256)
    }

    /// Execute the crypto operation.
    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.tool {
            CryptoTool::Sha256 => {
                #[derive(Deserialize)]
                struct HashInput {
                    input: String,
                }
                let input: HashInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                let mut hasher = Sha256::new();
                hasher.update(input.input.as_bytes());
                let result = hasher.finalize();
                Ok(json!(hex::encode(result)))
            }
            CryptoTool::Sha512 => {
                #[derive(Deserialize)]
                struct HashInput {
                    input: String,
                }
                let input: HashInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                let mut hasher = Sha512::new();
                hasher.update(input.input.as_bytes());
                let result = hasher.finalize();
                Ok(json!(hex::encode(result)))
            }
            CryptoTool::Md5 => {
                #[derive(Deserialize)]
                struct HashInput {
                    input: String,
                }
                let input: HashInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                let mut hasher = Md5::new();
                hasher.update(input.input.as_bytes());
                let result = hasher.finalize();
                Ok(json!(hex::encode(result)))
            }
            CryptoTool::Base64Encode => {
                #[derive(Deserialize)]
                struct EncodeInput {
                    input: String,
                }
                let input: EncodeInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                use base64::Engine;
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(input.input.as_bytes());
                Ok(json!(encoded))
            }
            CryptoTool::Base64Decode => {
                #[derive(Deserialize)]
                struct DecodeInput {
                    input: String,
                }
                let input: DecodeInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(input.input.as_bytes())
                    .map_err(|e| {
                        ToolError::InvocationFailed(format!("Invalid base64 input: {}", e))
                    })?;
                let decoded_str = String::from_utf8(decoded).map_err(|e| {
                    ToolError::InvocationFailed(format!("Decoded data is not valid UTF-8: {}", e))
                })?;
                Ok(json!(decoded_str))
            }
            CryptoTool::Uuid => {
                let uuid = Uuid::new_v4();
                Ok(json!(uuid.to_string()))
            }
            CryptoTool::RandomInt => {
                #[derive(Deserialize)]
                struct RandomInput {
                    min: i64,
                    max: i64,
                }
                let input: RandomInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                if input.min > input.max {
                    return Err(ToolError::InvocationFailed(
                        "min must be less than or equal to max".into(),
                    ));
                }
                let mut rng = rand::thread_rng();
                let value = rng.gen_range(input.min..=input.max);
                Ok(json!(value))
            }
            CryptoTool::HmacSha256 => {
                #[derive(Deserialize)]
                struct HmacInput {
                    message: String,
                    key: String,
                }
                let input: HmacInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                type HmacSha256 = Hmac<Sha256>;
                let mut mac = HmacSha256::new_from_slice(input.key.as_bytes())
                    .map_err(|e| ToolError::InvocationFailed(format!("Invalid HMAC key: {}", e)))?;
                mac.update(input.message.as_bytes());
                let result = mac.finalize();
                Ok(json!(hex::encode(result.into_bytes())))
            }
        }
    }
}

impl ToolProvider for CryptoProvider {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        self.execute(input)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_metadata() {
        let provider = CryptoProvider::sha256();
        assert_eq!(provider.name(), "crypto.sha256");
        assert_eq!(provider.version(), "1.0.0");
        assert_eq!(provider.schema().name, "crypto.sha256");
        assert_eq!(provider.schema().effects, vec!["crypto"]);
    }

    #[test]
    fn all_tools_have_correct_metadata() {
        let providers = vec![
            (CryptoProvider::sha256(), "crypto.sha256"),
            (CryptoProvider::sha512(), "crypto.sha512"),
            (CryptoProvider::md5(), "crypto.md5"),
            (CryptoProvider::base64_encode(), "crypto.base64_encode"),
            (CryptoProvider::base64_decode(), "crypto.base64_decode"),
            (CryptoProvider::uuid(), "crypto.uuid"),
            (CryptoProvider::random_int(), "crypto.random_int"),
            (CryptoProvider::hmac_sha256(), "crypto.hmac_sha256"),
        ];

        for (provider, expected_name) in providers {
            assert_eq!(provider.name(), expected_name);
            assert_eq!(provider.version(), "1.0.0");
            assert_eq!(provider.schema().effects, vec!["crypto"]);
        }
    }

    #[test]
    fn sha256_hash() {
        let provider = CryptoProvider::sha256();
        let input = json!({"input": "hello"});
        let result = provider.call(input).unwrap();
        // SHA-256 of "hello"
        assert_eq!(
            result.as_str().unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha512_hash() {
        let provider = CryptoProvider::sha512();
        let input = json!({"input": "hello"});
        let result = provider.call(input).unwrap();
        // SHA-512 of "hello" (first 64 chars)
        assert!(result
            .as_str()
            .unwrap()
            .starts_with("9b71d224bd62f3785d96d46ad3ea3d73"));
    }

    #[test]
    fn md5_hash() {
        let provider = CryptoProvider::md5();
        let input = json!({"input": "hello"});
        let result = provider.call(input).unwrap();
        // MD5 of "hello"
        assert_eq!(result.as_str().unwrap(), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn base64_encode_decode() {
        let provider_encode = CryptoProvider::base64_encode();
        let provider_decode = CryptoProvider::base64_decode();

        let input = json!({"input": "hello world"});
        let encoded = provider_encode.call(input).unwrap();
        assert_eq!(encoded.as_str().unwrap(), "aGVsbG8gd29ybGQ=");

        let decode_input = json!({"input": encoded.as_str().unwrap()});
        let decoded = provider_decode.call(decode_input).unwrap();
        assert_eq!(decoded.as_str().unwrap(), "hello world");
    }

    #[test]
    fn base64_decode_invalid() {
        let provider = CryptoProvider::base64_decode();
        let input = json!({"input": "not!valid!base64"});
        let result = provider.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn uuid_generation() {
        let provider = CryptoProvider::uuid();
        let input = json!({});
        let result = provider.call(input).unwrap();
        let uuid_str = result.as_str().unwrap();
        assert_eq!(uuid_str.len(), 36); // UUID v4 format: 8-4-4-4-12
        assert!(uuid_str.contains('-'));
    }

    #[test]
    fn random_int_in_range() {
        let provider = CryptoProvider::random_int();
        let input = json!({"min": 1, "max": 10});
        let result = provider.call(input).unwrap();
        let value = result.as_i64().unwrap();
        assert!((1..=10).contains(&value));
    }

    #[test]
    fn random_int_invalid_range() {
        let provider = CryptoProvider::random_int();
        let input = json!({"min": 10, "max": 1});
        let result = provider.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn hmac_sha256() {
        let provider = CryptoProvider::hmac_sha256();
        let input = json!({"message": "hello", "key": "secret"});
        let result = provider.call(input).unwrap();
        let hmac = result.as_str().unwrap();
        assert_eq!(hmac.len(), 64); // SHA-256 produces 32 bytes = 64 hex chars
                                    // Verify deterministic output
        let result2 = provider
            .call(json!({"message": "hello", "key": "secret"}))
            .unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn hmac_different_key_different_output() {
        let provider = CryptoProvider::hmac_sha256();
        let result1 = provider
            .call(json!({"message": "hello", "key": "secret1"}))
            .unwrap();
        let result2 = provider
            .call(json!({"message": "hello", "key": "secret2"}))
            .unwrap();
        assert_ne!(result1, result2);
    }
}
