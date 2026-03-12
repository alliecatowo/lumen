//! Ed25519 digital signature operations for the Lumen crypto provider.
//!
//! Provides tool-provider implementations for:
//! - `crypto.ed25519_keygen` — Generate a new Ed25519 keypair
//! - `crypto.ed25519_sign` — Sign a message with an Ed25519 secret key
//! - `crypto.ed25519_verify` — Verify an Ed25519 signature

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use rand::rngs::OsRng;
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Ed25519Tool enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ed25519Tool {
    Keygen,
    Sign,
    Verify,
}

impl Ed25519Tool {
    fn tool_name(&self) -> &'static str {
        match self {
            Ed25519Tool::Keygen => "crypto.ed25519_keygen",
            Ed25519Tool::Sign => "crypto.ed25519_sign",
            Ed25519Tool::Verify => "crypto.ed25519_verify",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Ed25519Tool::Keygen => {
                "Generate a new Ed25519 keypair (returns base64 public and secret keys)"
            }
            Ed25519Tool::Sign => {
                "Sign a message with an Ed25519 secret key (returns base64 signature)"
            }
            Ed25519Tool::Verify => "Verify an Ed25519 signature (returns boolean)",
        }
    }
}

// ---------------------------------------------------------------------------
// Ed25519Provider implementation
// ---------------------------------------------------------------------------

/// Ed25519 digital signature provider implementing the `ToolProvider` trait.
pub struct Ed25519Provider {
    tool: Ed25519Tool,
    schema: ToolSchema,
}

impl Ed25519Provider {
    /// Create a new Ed25519 provider for the given tool.
    fn new(tool: Ed25519Tool) -> Self {
        let (input_schema, output_schema) = match tool {
            Ed25519Tool::Keygen => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "object",
                    "properties": {
                        "public_key": { "type": "string", "description": "Base64-encoded public key (32 bytes)" },
                        "secret_key": { "type": "string", "description": "Base64-encoded secret key (32 bytes)" }
                    }
                }),
            ),
            Ed25519Tool::Sign => (
                json!({
                    "type": "object",
                    "required": ["message", "secret_key"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to sign"
                        },
                        "secret_key": {
                            "type": "string",
                            "description": "Base64-encoded Ed25519 secret key"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Base64-encoded Ed25519 signature (64 bytes)"
                }),
            ),
            Ed25519Tool::Verify => (
                json!({
                    "type": "object",
                    "required": ["message", "signature", "public_key"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Original message"
                        },
                        "signature": {
                            "type": "string",
                            "description": "Base64-encoded Ed25519 signature"
                        },
                        "public_key": {
                            "type": "string",
                            "description": "Base64-encoded Ed25519 public key"
                        }
                    }
                }),
                json!({
                    "type": "boolean",
                    "description": "Whether the signature is valid"
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

    /// Create a keygen provider.
    pub fn keygen() -> Self {
        Self::new(Ed25519Tool::Keygen)
    }

    /// Create a sign provider.
    pub fn sign() -> Self {
        Self::new(Ed25519Tool::Sign)
    }

    /// Create a verify provider.
    pub fn verify() -> Self {
        Self::new(Ed25519Tool::Verify)
    }

    /// Execute the Ed25519 operation.
    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.tool {
            Ed25519Tool::Keygen => {
                let signing_key = SigningKey::generate(&mut OsRng);
                let verifying_key = signing_key.verifying_key();

                let secret_b64 = b64_encode(&signing_key.to_bytes());
                let public_b64 = b64_encode(&verifying_key.to_bytes());

                Ok(json!({
                    "public_key": public_b64,
                    "secret_key": secret_b64
                }))
            }
            Ed25519Tool::Sign => {
                #[derive(Deserialize)]
                struct SignInput {
                    message: String,
                    secret_key: String,
                }
                let input: SignInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidArgs(format!("Invalid input format: {}", e)))?;

                let secret_bytes = b64_decode(&input.secret_key).map_err(|e| {
                    ToolError::InvalidArgs(format!("Invalid base64 secret key: {}", e))
                })?;
                let key_bytes: [u8; 32] = secret_bytes.try_into().map_err(|_| {
                    ToolError::InvalidArgs("Secret key must be exactly 32 bytes".to_string())
                })?;
                let signing_key = SigningKey::from_bytes(&key_bytes);
                let signature = signing_key.sign(input.message.as_bytes());

                Ok(json!(b64_encode(&signature.to_bytes())))
            }
            Ed25519Tool::Verify => {
                #[derive(Deserialize)]
                struct VerifyInput {
                    message: String,
                    signature: String,
                    public_key: String,
                }
                let input: VerifyInput = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidArgs(format!("Invalid input format: {}", e)))?;

                let public_bytes = b64_decode(&input.public_key).map_err(|e| {
                    ToolError::InvalidArgs(format!("Invalid base64 public key: {}", e))
                })?;
                let pub_key_bytes: [u8; 32] = public_bytes.try_into().map_err(|_| {
                    ToolError::InvalidArgs("Public key must be exactly 32 bytes".to_string())
                })?;
                let verifying_key = VerifyingKey::from_bytes(&pub_key_bytes).map_err(|e| {
                    ToolError::InvalidArgs(format!("Invalid Ed25519 public key: {}", e))
                })?;

                let sig_bytes = b64_decode(&input.signature).map_err(|e| {
                    ToolError::InvalidArgs(format!("Invalid base64 signature: {}", e))
                })?;
                let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| {
                    ToolError::InvalidArgs("Signature must be exactly 64 bytes".to_string())
                })?;
                let signature = Signature::from_bytes(&sig_arr);

                let valid = verifying_key
                    .verify(input.message.as_bytes(), &signature)
                    .is_ok();

                Ok(json!(valid))
            }
        }
    }
}

impl ToolProvider for Ed25519Provider {
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
// Helpers
// ---------------------------------------------------------------------------

fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keygen_returns_valid_keys() {
        let provider = Ed25519Provider::keygen();
        let result = provider.call(json!({})).unwrap();
        let obj = result.as_object().unwrap();

        let public_key = obj.get("public_key").unwrap().as_str().unwrap();
        let secret_key = obj.get("secret_key").unwrap().as_str().unwrap();

        // Base64-encoded 32-byte keys
        let pub_bytes = b64_decode(public_key).unwrap();
        let sec_bytes = b64_decode(secret_key).unwrap();
        assert_eq!(pub_bytes.len(), 32);
        assert_eq!(sec_bytes.len(), 32);
    }

    #[test]
    fn keygen_produces_unique_keys() {
        let provider = Ed25519Provider::keygen();
        let r1 = provider.call(json!({})).unwrap();
        let r2 = provider.call(json!({})).unwrap();
        assert_ne!(r1, r2, "Two keygens should produce different keys");
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let public_key = keys["public_key"].as_str().unwrap();
        let secret_key = keys["secret_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let signature = signer
            .call(json!({
                "message": "hello world",
                "secret_key": secret_key
            }))
            .unwrap();
        let sig_str = signature.as_str().unwrap();

        // Signature should be 64 bytes base64-encoded
        let sig_bytes = b64_decode(sig_str).unwrap();
        assert_eq!(sig_bytes.len(), 64);

        let verifier = Ed25519Provider::verify();
        let result = verifier
            .call(json!({
                "message": "hello world",
                "signature": sig_str,
                "public_key": public_key
            }))
            .unwrap();
        assert_eq!(result, json!(true));
    }

    #[test]
    fn verify_rejects_wrong_message() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let public_key = keys["public_key"].as_str().unwrap();
        let secret_key = keys["secret_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let signature = signer
            .call(json!({
                "message": "hello world",
                "secret_key": secret_key
            }))
            .unwrap();
        let sig_str = signature.as_str().unwrap();

        let verifier = Ed25519Provider::verify();
        let result = verifier
            .call(json!({
                "message": "tampered message",
                "signature": sig_str,
                "public_key": public_key
            }))
            .unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn verify_rejects_wrong_key() {
        // Generate two different keypairs
        let keygen = Ed25519Provider::keygen();
        let keys1 = keygen.call(json!({})).unwrap();
        let keys2 = keygen.call(json!({})).unwrap();
        let secret_key_1 = keys1["secret_key"].as_str().unwrap();
        let public_key_2 = keys2["public_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let signature = signer
            .call(json!({
                "message": "hello",
                "secret_key": secret_key_1
            }))
            .unwrap();
        let sig_str = signature.as_str().unwrap();

        // Verify with wrong public key
        let verifier = Ed25519Provider::verify();
        let result = verifier
            .call(json!({
                "message": "hello",
                "signature": sig_str,
                "public_key": public_key_2
            }))
            .unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn sign_rejects_invalid_secret_key() {
        let signer = Ed25519Provider::sign();
        let result = signer.call(json!({
            "message": "hello",
            "secret_key": "not-valid-base64!!!"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn sign_rejects_wrong_length_key() {
        let signer = Ed25519Provider::sign();
        let short_key = b64_encode(&[0u8; 16]); // 16 bytes, not 32
        let result = signer.call(json!({
            "message": "hello",
            "secret_key": short_key
        }));
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_invalid_signature() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let public_key = keys["public_key"].as_str().unwrap();

        let verifier = Ed25519Provider::verify();
        let result = verifier.call(json!({
            "message": "hello",
            "signature": "not-valid-base64!!!",
            "public_key": public_key
        }));
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_wrong_length_signature() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let public_key = keys["public_key"].as_str().unwrap();

        let verifier = Ed25519Provider::verify();
        let short_sig = b64_encode(&[0u8; 32]); // 32 bytes, not 64
        let result = verifier.call(json!({
            "message": "hello",
            "signature": short_sig,
            "public_key": public_key
        }));
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_invalid_public_key() {
        let verifier = Ed25519Provider::verify();
        let result = verifier.call(json!({
            "message": "hello",
            "signature": b64_encode(&[0u8; 64]),
            "public_key": "bad-base64!!!"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn provider_metadata() {
        let providers = vec![
            (Ed25519Provider::keygen(), "crypto.ed25519_keygen"),
            (Ed25519Provider::sign(), "crypto.ed25519_sign"),
            (Ed25519Provider::verify(), "crypto.ed25519_verify"),
        ];

        for (provider, expected_name) in providers {
            assert_eq!(provider.name(), expected_name);
            assert_eq!(provider.version(), "1.0.0");
            assert_eq!(provider.schema().effects, vec!["crypto"]);
        }
    }

    #[test]
    fn sign_deterministic_same_key_same_message() {
        // Ed25519 with dalek is deterministic (RFC 8032 compliant)
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let secret_key = keys["secret_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let sig1 = signer
            .call(json!({ "message": "test", "secret_key": secret_key }))
            .unwrap();
        let sig2 = signer
            .call(json!({ "message": "test", "secret_key": secret_key }))
            .unwrap();
        assert_eq!(sig1, sig2, "Ed25519 signatures should be deterministic");
    }

    #[test]
    fn sign_different_messages_produce_different_signatures() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let secret_key = keys["secret_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let sig1 = signer
            .call(json!({ "message": "message A", "secret_key": secret_key }))
            .unwrap();
        let sig2 = signer
            .call(json!({ "message": "message B", "secret_key": secret_key }))
            .unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn sign_empty_message() {
        let keygen = Ed25519Provider::keygen();
        let keys = keygen.call(json!({})).unwrap();
        let public_key = keys["public_key"].as_str().unwrap();
        let secret_key = keys["secret_key"].as_str().unwrap();

        let signer = Ed25519Provider::sign();
        let signature = signer
            .call(json!({ "message": "", "secret_key": secret_key }))
            .unwrap();
        let sig_str = signature.as_str().unwrap();

        let verifier = Ed25519Provider::verify();
        let result = verifier
            .call(json!({
                "message": "",
                "signature": sig_str,
                "public_key": public_key
            }))
            .unwrap();
        assert_eq!(result, json!(true));
    }
}
