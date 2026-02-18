//! Integration tests for registry signature verification.
//!
//! These tests verify that the Ed25519 signature verification implementation
//! correctly validates package signatures and rejects tampered or invalid signatures.

#[cfg(feature = "ed25519")]
mod signature_tests {
    use lumen_cli::registry::{
        ArtifactInfo, PackageSignature, RegistryClient, RegistryVersionMetadata,
    };
    use std::collections::BTreeMap;

    #[test]
    fn test_valid_signature_verification() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        // Generate a keypair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Create test metadata
        let mut metadata = RegistryVersionMetadata {
            name: "@security-test/package".to_string(),
            version: "1.0.0".to_string(),
            deps: BTreeMap::new(),
            optional_deps: BTreeMap::new(),
            artifacts: vec![ArtifactInfo {
                kind: "tar".to_string(),
                url: Some("artifacts/test.tar".to_string()),
                hash: "sha256:abcdef1234567890".to_string(),
                size: Some(2048),
                arch: None,
                os: None,
            }],
            integrity: None,
            signature: None,
            transparency: None,
            yanked: false,
            yank_reason: None,
            published_at: Some("2024-01-15T10:00:00Z".to_string()),
            publisher: None,
            license: Some("MIT".to_string()),
            description: Some("Test package for signature verification".to_string()),
            readme: None,
            documentation: None,
            repository: None,
            keywords: vec!["test".to_string(), "security".to_string()],
        };

        // Create canonical message (without signature field)
        let message = serde_json::to_string(&metadata).unwrap();

        // Sign the message
        let signature = signing_key.sign(message.as_bytes());

        // Create signature object
        let sig = PackageSignature {
            algorithm: "ed25519".to_string(),
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            key_id: base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes()),
            signed_at: Some(chrono::Utc::now().to_rfc3339()),
            rekor_bundle: None,
        };

        // Add signature to metadata
        metadata.signature = Some(sig.clone());

        // Create client and verify
        let client = RegistryClient::new("https://test.registry");
        let result = client.verify_signature(&metadata, &sig);

        assert!(
            result.is_ok(),
            "Valid signature should verify: {:?}",
            result
        );
    }

    #[test]
    fn test_tampered_metadata_fails_verification() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        // Generate a keypair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Create test metadata
        let mut metadata = RegistryVersionMetadata {
            name: "@security-test/tampered".to_string(),
            version: "1.0.0".to_string(),
            deps: BTreeMap::new(),
            optional_deps: BTreeMap::new(),
            artifacts: vec![],
            integrity: None,
            signature: None,
            transparency: None,
            yanked: false,
            yank_reason: None,
            published_at: None,
            publisher: None,
            license: None,
            description: Some("Package to test tampering detection".to_string()),
            readme: None,
            documentation: None,
            repository: None,
            keywords: Vec::new(),
        };

        // Sign the original metadata
        let message = serde_json::to_string(&metadata).unwrap();
        let signature = signing_key.sign(message.as_bytes());

        // Create signature object
        let sig = PackageSignature {
            algorithm: "ed25519".to_string(),
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            key_id: base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes()),
            signed_at: Some(chrono::Utc::now().to_rfc3339()),
            rekor_bundle: None,
        };

        metadata.signature = Some(sig.clone());

        // ATTACK: Tamper with the metadata after signing
        metadata.version = "2.0.0".to_string(); // Version bumped maliciously

        // Create client and verify - should FAIL
        let client = RegistryClient::new("https://test.registry");
        let result = client.verify_signature(&metadata, &sig);

        assert!(result.is_err(), "Tampered metadata must fail verification");
        let error = result.unwrap_err();
        assert!(
            error.contains("Signature verification failed"),
            "Error should indicate verification failure, got: {}",
            error
        );
    }

    #[test]
    fn test_wrong_public_key_fails_verification() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        // Generate TWO different keypairs
        let signing_key = SigningKey::generate(&mut OsRng);
        let attacker_key = SigningKey::generate(&mut OsRng);
        let attacker_verifying_key = attacker_key.verifying_key();

        // Create test metadata
        let mut metadata = RegistryVersionMetadata {
            name: "@security-test/wrong-key".to_string(),
            version: "1.0.0".to_string(),
            deps: BTreeMap::new(),
            optional_deps: BTreeMap::new(),
            artifacts: vec![],
            integrity: None,
            signature: None,
            transparency: None,
            yanked: false,
            yank_reason: None,
            published_at: None,
            publisher: None,
            license: None,
            description: Some("Test wrong key attack".to_string()),
            readme: None,
            documentation: None,
            repository: None,
            keywords: Vec::new(),
        };

        // Sign with legitimate key
        let message = serde_json::to_string(&metadata).unwrap();
        let signature = signing_key.sign(message.as_bytes());

        // ATTACK: Provide attacker's public key instead of legitimate one
        let sig = PackageSignature {
            algorithm: "ed25519".to_string(),
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            key_id: base64::engine::general_purpose::STANDARD
                .encode(attacker_verifying_key.to_bytes()),
            signed_at: Some(chrono::Utc::now().to_rfc3339()),
            rekor_bundle: None,
        };

        metadata.signature = Some(sig.clone());

        // Create client and verify - should FAIL
        let client = RegistryClient::new("https://test.registry");
        let result = client.verify_signature(&metadata, &sig);

        assert!(
            result.is_err(),
            "Signature with wrong public key must fail verification"
        );
    }

    #[test]
    fn test_unsupported_algorithm_rejected() {
        let metadata = RegistryVersionMetadata {
            name: "@security-test/unsupported-algo".to_string(),
            version: "1.0.0".to_string(),
            deps: BTreeMap::new(),
            optional_deps: BTreeMap::new(),
            artifacts: vec![],
            integrity: None,
            signature: None,
            transparency: None,
            yanked: false,
            yank_reason: None,
            published_at: None,
            publisher: None,
            license: None,
            description: Some("Test unsupported algorithm".to_string()),
            readme: None,
            documentation: None,
            repository: None,
            keywords: Vec::new(),
        };

        // ATTACK: Try to use an unsupported algorithm
        let sig = PackageSignature {
            algorithm: "rsa-pss-2048".to_string(), // Not supported
            signature: "dummy_signature_data".to_string(),
            key_id: "dummy_key_id".to_string(),
            signed_at: None,
            rekor_bundle: None,
        };

        let client = RegistryClient::new("https://test.registry");
        let result = client.verify_signature(&metadata, &sig);

        assert!(result.is_err(), "Unsupported algorithm must be rejected");
        let error = result.unwrap_err();
        assert!(
            error.contains("Unsupported signature algorithm"),
            "Error should mention unsupported algorithm, got: {}",
            error
        );
    }

    #[test]
    fn test_malformed_signature_rejected() {
        let metadata = RegistryVersionMetadata {
            name: "@security-test/malformed".to_string(),
            version: "1.0.0".to_string(),
            deps: BTreeMap::new(),
            optional_deps: BTreeMap::new(),
            artifacts: vec![],
            integrity: None,
            signature: None,
            transparency: None,
            yanked: false,
            yank_reason: None,
            published_at: None,
            publisher: None,
            license: None,
            description: Some("Test malformed signature".to_string()),
            readme: None,
            documentation: None,
            repository: None,
            keywords: Vec::new(),
        };

        // ATTACK: Provide malformed base64 data
        let sig = PackageSignature {
            algorithm: "ed25519".to_string(),
            signature: "!@#$%^&*()_+{}[]|\\:;\"'<>,.?/~`".to_string(), // Invalid base64
            key_id: "also!@#invalid$%^base64&*()".to_string(),
            signed_at: None,
            rekor_bundle: None,
        };

        let client = RegistryClient::new("https://test.registry");
        let result = client.verify_signature(&metadata, &sig);

        assert!(result.is_err(), "Malformed signature must be rejected");
        let error = result.unwrap_err();
        assert!(
            error.contains("Failed to decode"),
            "Error should mention decode failure, got: {}",
            error
        );
    }
}
