//! TUF (The Update Framework) for secure package metadata.
//!
//! Implements TUF-style secure metadata for the Lumen package repository,
//! ensuring package integrity via signed, versioned, and expiring metadata.
//!
//! ## TUF Roles
//!
//! - **Root** — contains public keys for all roles and role thresholds
//! - **Targets** — maps package names to their hashes and sizes
//! - **Snapshot** — version numbers of all metadata files
//! - **Timestamp** — current snapshot version, prevents rollback attacks
//!
//! ## Security Properties
//!
//! - **Freshness**: Metadata has expiration dates; stale metadata is rejected
//! - **Rollback protection**: Version numbers must be monotonically increasing
//! - **Threshold signing**: Multiple signatures can be required per role
//! - **Key rotation**: Root metadata supports rotating all keys

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// =============================================================================
// TUF Error Types
// =============================================================================

/// Errors that can occur during TUF metadata verification.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TufError {
    /// A required signature is invalid or verification failed.
    #[error("signature invalid: {0}")]
    SignatureInvalid(String),

    /// Metadata has expired.
    #[error("metadata expired: expired at {0}")]
    MetadataExpired(String),

    /// A rollback attack was detected (version went backwards).
    #[error("rollback detected: current version {current}, got {received}")]
    RollbackDetected { current: u64, received: u64 },

    /// The required signature threshold was not met.
    #[error("threshold not met: need {required}, got {valid}")]
    ThresholdNotMet { required: u32, valid: u32 },

    /// A target package was not found in the targets metadata.
    #[error("target not found: {0}")]
    TargetNotFound(String),
}

// =============================================================================
// Key and Signature Types
// =============================================================================

/// A public key used for verifying TUF metadata signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TufKey {
    /// Unique key identifier (hex-encoded fingerprint).
    pub key_id: String,
    /// Key type (e.g., "ed25519").
    pub key_type: String,
    /// Public key value (base64-encoded).
    pub public_value: String,
}

/// A signature over TUF metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TufSignature {
    /// Key ID that produced this signature.
    pub key_id: String,
    /// Signature value (base64-encoded).
    pub value: String,
}

// =============================================================================
// TUF Role Types
// =============================================================================

/// Identifies which TUF role a metadata document belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TufRole {
    /// Root role: defines keys and thresholds for all roles.
    Root,
    /// Targets role: maps package names to hashes and sizes.
    Targets,
    /// Snapshot role: records versions of all metadata files.
    Snapshot,
    /// Timestamp role: records current snapshot version.
    Timestamp,
}

impl std::fmt::Display for TufRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TufRole::Root => write!(f, "root"),
            TufRole::Targets => write!(f, "targets"),
            TufRole::Snapshot => write!(f, "snapshot"),
            TufRole::Timestamp => write!(f, "timestamp"),
        }
    }
}

// =============================================================================
// Root Metadata
// =============================================================================

/// Threshold configuration for a TUF role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDefinition {
    /// Key IDs authorized for this role.
    pub key_ids: Vec<String>,
    /// Minimum number of valid signatures required.
    pub threshold: u32,
}

/// Root metadata: the trust anchor for the entire TUF repository.
///
/// Contains all public keys and role thresholds. Root metadata is
/// self-signed and can be updated via key rotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// All known public keys, indexed by key ID.
    pub keys: BTreeMap<String, TufKey>,
    /// Role definitions with key IDs and thresholds.
    pub roles: BTreeMap<String, RoleDefinition>,
    /// Specification version (e.g., "1.0.0").
    pub spec_version: String,
}

impl Root {
    /// Get the role definition for a given role.
    pub fn role_definition(&self, role: TufRole) -> Option<&RoleDefinition> {
        self.roles.get(&role.to_string())
    }

    /// Get a key by its ID.
    pub fn key(&self, key_id: &str) -> Option<&TufKey> {
        self.keys.get(key_id)
    }
}

// =============================================================================
// Targets Metadata
// =============================================================================

/// Describes a single target (package artifact).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInfo {
    /// SHA-256 hash of the target content (hex-encoded).
    pub sha256: String,
    /// Size in bytes.
    pub length: u64,
}

/// Targets metadata: maps package names to their hashes and sizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Targets {
    /// Map of target name to target info.
    pub targets: BTreeMap<String, TargetInfo>,
}

// =============================================================================
// Snapshot Metadata
// =============================================================================

/// Version info for a metadata file in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetaVersion {
    /// Version number of the metadata.
    pub version: u64,
}

/// Snapshot metadata: records version numbers of all other metadata files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Map of metadata filename to version info.
    pub meta: BTreeMap<String, MetaVersion>,
}

// =============================================================================
// Timestamp Metadata
// =============================================================================

/// Timestamp metadata: contains the current snapshot hash and version.
///
/// This is the entry point for TUF updates and prevents rollback attacks
/// by recording the latest snapshot version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamp {
    /// Hash of the current snapshot metadata (SHA-256, hex-encoded).
    pub snapshot_hash: String,
    /// Version of the current snapshot metadata.
    pub snapshot_version: u64,
}

// =============================================================================
// Signed Metadata Wrapper
// =============================================================================

/// A signed TUF metadata document.
///
/// All TUF metadata is wrapped in this structure, which provides
/// the role, version, expiration, and signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TufMetadata<T> {
    /// Which role this metadata belongs to.
    pub role: TufRole,
    /// Monotonically increasing version number.
    pub version: u64,
    /// When this metadata expires (UTC).
    pub expires: DateTime<Utc>,
    /// The role-specific payload.
    pub body: T,
    /// Signatures over the canonical body.
    pub signatures: Vec<TufSignature>,
}

impl<T: Serialize> TufMetadata<T> {
    /// Compute the canonical bytes of the signed portion (role + version + expires + body).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // We sign over a canonical JSON representation of (role, version, expires, body).
        let canonical = serde_json::json!({
            "role": self.role,
            "version": self.version,
            "expires": self.expires.to_rfc3339(),
            "body": serde_json::to_value(&self.body).unwrap_or(serde_json::Value::Null),
        });
        serde_json::to_vec(&canonical).unwrap_or_default()
    }
}

// =============================================================================
// Signature Verification
// =============================================================================

/// Verify that a single signature is valid for the given data and public key.
///
/// Uses Ed25519 when the `ed25519` feature is enabled; otherwise falls back
/// to HMAC-SHA256 comparison (suitable for testing only).
fn verify_signature(key: &TufKey, data: &[u8], signature: &TufSignature) -> bool {
    #[cfg(feature = "ed25519")]
    {
        use base64::Engine;
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let public_bytes = match base64::engine::general_purpose::STANDARD.decode(&key.public_value)
        {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(&signature.value) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let vk_bytes: [u8; 32] = match public_bytes.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_arr: [u8; 64] = match sig_bytes.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&vk_bytes) else {
            return false;
        };
        let sig = Signature::from_bytes(&sig_arr);
        verifying_key.verify(data, &sig).is_ok()
    }

    #[cfg(not(feature = "ed25519"))]
    {
        // Fallback for testing: HMAC-SHA256 where key.public_value is the shared secret.
        // The signature value is hex(HMAC-SHA256(key, data)).
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<Sha256>;

        let Ok(mut mac) = HmacSha256::new_from_slice(key.public_value.as_bytes()) else {
            return false;
        };
        mac.update(data);
        let expected = hex::encode(mac.finalize().into_bytes());
        expected == signature.value
    }
}

/// Verify a TUF metadata document against a trusted root.
///
/// Checks:
/// 1. The metadata has not expired.
/// 2. Enough valid signatures meet the role's threshold.
pub fn verify_metadata<T: Serialize>(
    metadata: &TufMetadata<T>,
    trusted_root: &Root,
) -> Result<(), TufError> {
    // 1. Check expiry
    if metadata.expires < Utc::now() {
        return Err(TufError::MetadataExpired(metadata.expires.to_rfc3339()));
    }

    // 2. Look up role definition
    let role_def = trusted_root.role_definition(metadata.role).ok_or_else(|| {
        TufError::SignatureInvalid(format!("no role definition for '{}'", metadata.role))
    })?;

    // 3. Compute canonical bytes
    let data = metadata.canonical_bytes();

    // 4. Count valid signatures from authorized keys
    let mut valid_count: u32 = 0;
    for sig in &metadata.signatures {
        // Signature key must be in the role's authorized key list
        if !role_def.key_ids.contains(&sig.key_id) {
            continue;
        }
        // Look up the key
        let Some(key) = trusted_root.key(&sig.key_id) else {
            continue;
        };
        if verify_signature(key, &data, sig) {
            valid_count += 1;
        }
    }

    // 5. Check threshold
    if valid_count < role_def.threshold {
        return Err(TufError::ThresholdNotMet {
            required: role_def.threshold,
            valid: valid_count,
        });
    }

    Ok(())
}

// =============================================================================
// TUF Repository
// =============================================================================

/// A TUF repository client that tracks trusted state and validates updates.
///
/// Maintains the current trusted root and metadata versions, providing
/// rollback detection and signature verification.
pub struct TufRepository {
    /// The currently trusted root metadata.
    trusted_root: Root,
    /// Current root version.
    root_version: u64,
    /// Current snapshot version (for rollback detection).
    snapshot_version: Option<u64>,
    /// Current targets metadata (if loaded).
    targets: Option<Targets>,
    /// Expiration of the most recently validated timestamp metadata.
    timestamp_expires: Option<DateTime<Utc>>,
}

impl TufRepository {
    /// Create a new TUF repository with a trusted root.
    pub fn new(root: Root) -> Self {
        Self {
            trusted_root: root,
            root_version: 1,
            snapshot_version: None,
            targets: None,
            timestamp_expires: None,
        }
    }

    /// Update the trusted root via root rotation.
    ///
    /// The new root must:
    /// - Have a version strictly greater than the current root version
    /// - Be valid according to the **current** trusted root (cross-signed)
    /// - Not be expired
    pub fn update_root(&mut self, new_root: &TufMetadata<Root>) -> Result<(), TufError> {
        // Rollback check
        if new_root.version <= self.root_version {
            return Err(TufError::RollbackDetected {
                current: self.root_version,
                received: new_root.version,
            });
        }

        // Verify against current trusted root
        verify_metadata(new_root, &self.trusted_root)?;

        // Accept the new root
        self.trusted_root = new_root.body.clone();
        self.root_version = new_root.version;

        Ok(())
    }

    /// Update timestamp metadata.
    ///
    /// Verifies signatures and checks for rollback of the snapshot version.
    pub fn update_timestamp(&mut self, ts: &TufMetadata<Timestamp>) -> Result<(), TufError> {
        verify_metadata(ts, &self.trusted_root)?;

        // Rollback check on snapshot version
        if let Some(current_sv) = self.snapshot_version {
            if ts.body.snapshot_version < current_sv {
                return Err(TufError::RollbackDetected {
                    current: current_sv,
                    received: ts.body.snapshot_version,
                });
            }
        }

        self.snapshot_version = Some(ts.body.snapshot_version);
        self.timestamp_expires = Some(ts.expires);
        Ok(())
    }

    /// Update snapshot metadata.
    ///
    /// Verifies signatures and checks version against timestamp.
    pub fn update_snapshot(&mut self, snap: &TufMetadata<Snapshot>) -> Result<(), TufError> {
        verify_metadata(snap, &self.trusted_root)?;

        // Version must match what timestamp expects
        if let Some(expected) = self.snapshot_version {
            if snap.version != expected {
                return Err(TufError::RollbackDetected {
                    current: expected,
                    received: snap.version,
                });
            }
        }

        Ok(())
    }

    /// Update targets metadata.
    ///
    /// Verifies signatures and stores the targets for later verification.
    pub fn update_targets(&mut self, targets: &TufMetadata<Targets>) -> Result<(), TufError> {
        verify_metadata(targets, &self.trusted_root)?;
        self.targets = Some(targets.body.clone());
        Ok(())
    }

    /// Verify a package target against the stored targets metadata.
    ///
    /// Checks that the target exists in the targets metadata and that
    /// the provided hash and size match.
    pub fn verify_target(&self, name: &str, hash: &[u8], size: u64) -> Result<(), TufError> {
        let targets = self
            .targets
            .as_ref()
            .ok_or_else(|| TufError::TargetNotFound("no targets metadata loaded".to_string()))?;

        let info = targets
            .targets
            .get(name)
            .ok_or_else(|| TufError::TargetNotFound(name.to_string()))?;

        // Check hash
        let hash_hex = hex::encode(hash);
        if hash_hex != info.sha256 {
            return Err(TufError::SignatureInvalid(format!(
                "hash mismatch for '{}': expected {}, got {}",
                name, info.sha256, hash_hex
            )));
        }

        // Check size
        if size != info.length {
            return Err(TufError::SignatureInvalid(format!(
                "size mismatch for '{}': expected {}, got {}",
                name, info.length, size
            )));
        }

        Ok(())
    }

    /// Check if the repository's timestamp metadata has expired.
    pub fn is_expired(&self) -> bool {
        self.timestamp_expires
            .map(|exp| exp < Utc::now())
            .unwrap_or(true) // No timestamp loaded = treat as expired
    }

    /// Get the currently trusted root.
    pub fn trusted_root(&self) -> &Root {
        &self.trusted_root
    }

    /// Get the current root version.
    pub fn root_version(&self) -> u64 {
        self.root_version
    }

    /// Get the stored targets (if any).
    pub fn targets(&self) -> Option<&Targets> {
        self.targets.as_ref()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use sha2::{Digest, Sha256};

    /// A test keypair holding both the signing key and the TUF public key info.
    struct TestKeypair {
        key_id: String,
        tuf_key: TufKey,
        #[cfg(feature = "ed25519")]
        signing_key: ed25519_dalek::SigningKey,
        #[cfg(not(feature = "ed25519"))]
        secret: String,
    }

    /// Generate a test keypair.
    fn gen_test_keypair(key_id: &str) -> TestKeypair {
        #[cfg(feature = "ed25519")]
        {
            use base64::Engine;
            use ed25519_dalek::SigningKey;
            use rand::rngs::OsRng;

            let signing_key = SigningKey::generate(&mut OsRng);
            let verifying_key = signing_key.verifying_key();
            let public_b64 =
                base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

            TestKeypair {
                key_id: key_id.to_string(),
                tuf_key: TufKey {
                    key_id: key_id.to_string(),
                    key_type: "ed25519".to_string(),
                    public_value: public_b64,
                },
                signing_key,
            }
        }

        #[cfg(not(feature = "ed25519"))]
        {
            let secret = format!("secret-for-{}", key_id);
            TestKeypair {
                key_id: key_id.to_string(),
                tuf_key: TufKey {
                    key_id: key_id.to_string(),
                    key_type: "hmac-sha256".to_string(),
                    public_value: secret.clone(),
                },
                secret,
            }
        }
    }

    /// Sign a TUF metadata document with a test keypair.
    fn sign_with<T: Serialize>(metadata: &TufMetadata<T>, kp: &TestKeypair) -> TufSignature {
        let data = metadata.canonical_bytes();

        #[cfg(feature = "ed25519")]
        {
            use base64::Engine;
            use ed25519_dalek::Signer;

            let sig = kp.signing_key.sign(&data);
            TufSignature {
                key_id: kp.key_id.clone(),
                value: base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()),
            }
        }

        #[cfg(not(feature = "ed25519"))]
        {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            type HmacSha256 = Hmac<Sha256>;

            let mut mac = HmacSha256::new_from_slice(kp.secret.as_bytes()).unwrap();
            mac.update(&data);
            TufSignature {
                key_id: kp.key_id.clone(),
                value: hex::encode(mac.finalize().into_bytes()),
            }
        }
    }

    /// Build a root from one or more test keypairs (threshold 1 per role).
    fn build_root(keypairs: &[&TestKeypair]) -> Root {
        let mut keys = BTreeMap::new();
        let mut key_ids = Vec::new();
        for kp in keypairs {
            keys.insert(kp.key_id.clone(), kp.tuf_key.clone());
            key_ids.push(kp.key_id.clone());
        }

        let role_def = RoleDefinition {
            key_ids,
            threshold: 1,
        };

        let mut roles = BTreeMap::new();
        roles.insert("root".to_string(), role_def.clone());
        roles.insert("targets".to_string(), role_def.clone());
        roles.insert("snapshot".to_string(), role_def.clone());
        roles.insert("timestamp".to_string(), role_def);

        Root {
            keys,
            roles,
            spec_version: "1.0.0".to_string(),
        }
    }

    /// Helper: create a test root with one key and return (Root, TestKeypair).
    fn test_root_and_key() -> (Root, TestKeypair) {
        let kp = gen_test_keypair("key1");
        let root = build_root(&[&kp]);
        (root, kp)
    }

    /// Helper: sign metadata with a single keypair.
    fn sign_test<T: Serialize>(mut metadata: TufMetadata<T>, kp: &TestKeypair) -> TufMetadata<T> {
        let sig = sign_with(&metadata, kp);
        metadata.signatures = vec![sig];
        metadata
    }

    #[test]
    fn test_verify_valid_metadata() {
        let (root, kp) = test_root_and_key();
        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let metadata = sign_test(
            TufMetadata {
                role: TufRole::Targets,
                version: 1,
                expires: Utc::now() + Duration::days(1),
                body: targets,
                signatures: vec![],
            },
            &kp,
        );

        assert!(verify_metadata(&metadata, &root).is_ok());
    }

    #[test]
    fn test_verify_expired_metadata() {
        let (root, kp) = test_root_and_key();
        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let metadata = sign_test(
            TufMetadata {
                role: TufRole::Targets,
                version: 1,
                expires: Utc::now() - Duration::days(1),
                body: targets,
                signatures: vec![],
            },
            &kp,
        );

        let err = verify_metadata(&metadata, &root).unwrap_err();
        assert!(matches!(err, TufError::MetadataExpired(_)));
    }

    #[test]
    fn test_verify_invalid_signature() {
        let (root, _kp) = test_root_and_key();
        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let metadata = TufMetadata {
            role: TufRole::Targets,
            version: 1,
            expires: Utc::now() + Duration::days(1),
            body: targets,
            signatures: vec![TufSignature {
                key_id: "key1".to_string(),
                value: "bad-signature".to_string(),
            }],
        };

        let err = verify_metadata(&metadata, &root).unwrap_err();
        assert!(matches!(err, TufError::ThresholdNotMet { .. }));
    }

    #[test]
    fn test_verify_threshold_not_met_no_signatures() {
        let (root, _kp) = test_root_and_key();
        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let metadata = TufMetadata {
            role: TufRole::Targets,
            version: 1,
            expires: Utc::now() + Duration::days(1),
            body: targets,
            signatures: vec![], // No signatures at all
        };

        let err = verify_metadata(&metadata, &root).unwrap_err();
        assert!(
            matches!(
                err,
                TufError::ThresholdNotMet {
                    required: 1,
                    valid: 0
                }
            ),
            "expected ThresholdNotMet, got {:?}",
            err
        );
    }

    #[test]
    fn test_threshold_two_keys() {
        // Require threshold=2, provide two valid signatures.
        let kp1 = gen_test_keypair("k1");
        let kp2 = gen_test_keypair("k2");

        let role_def = RoleDefinition {
            key_ids: vec!["k1".to_string(), "k2".to_string()],
            threshold: 2,
        };

        let mut keys = BTreeMap::new();
        keys.insert("k1".to_string(), kp1.tuf_key.clone());
        keys.insert("k2".to_string(), kp2.tuf_key.clone());

        let mut roles = BTreeMap::new();
        roles.insert("targets".to_string(), role_def);

        let root = Root {
            keys,
            roles,
            spec_version: "1.0.0".to_string(),
        };

        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let mut metadata = TufMetadata {
            role: TufRole::Targets,
            version: 1,
            expires: Utc::now() + Duration::days(1),
            body: targets,
            signatures: vec![],
        };

        let sig1 = sign_with(&metadata, &kp1);
        let sig2 = sign_with(&metadata, &kp2);
        metadata.signatures = vec![sig1, sig2];

        assert!(verify_metadata(&metadata, &root).is_ok());

        // Now remove one signature — should fail threshold
        metadata.signatures.pop();
        let err = verify_metadata(&metadata, &root).unwrap_err();
        assert!(matches!(
            err,
            TufError::ThresholdNotMet {
                required: 2,
                valid: 1
            }
        ));
    }

    #[test]
    fn test_repository_new() {
        let (root, _kp) = test_root_and_key();
        let repo = TufRepository::new(root);
        assert_eq!(repo.root_version(), 1);
        assert!(repo.is_expired()); // No timestamp loaded yet
        assert!(repo.targets().is_none());
    }

    #[test]
    fn test_root_rotation() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root.clone());

        // Create new root v2
        let new_root_meta = sign_test(
            TufMetadata {
                role: TufRole::Root,
                version: 2,
                expires: Utc::now() + Duration::days(365),
                body: root,
                signatures: vec![],
            },
            &kp,
        );

        assert!(repo.update_root(&new_root_meta).is_ok());
        assert_eq!(repo.root_version(), 2);
    }

    #[test]
    fn test_root_rotation_rollback() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root.clone());

        // Try to "rotate" to version 1 (same as current)
        let old_root_meta = sign_test(
            TufMetadata {
                role: TufRole::Root,
                version: 1,
                expires: Utc::now() + Duration::days(365),
                body: root,
                signatures: vec![],
            },
            &kp,
        );

        let err = repo.update_root(&old_root_meta).unwrap_err();
        assert!(
            matches!(
                err,
                TufError::RollbackDetected {
                    current: 1,
                    received: 1
                }
            ),
            "expected RollbackDetected, got {:?}",
            err
        );
    }

    #[test]
    fn test_timestamp_rollback_detection() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root);

        // Load timestamp with snapshot version 5
        let ts1 = sign_test(
            TufMetadata {
                role: TufRole::Timestamp,
                version: 1,
                expires: Utc::now() + Duration::days(1),
                body: Timestamp {
                    snapshot_hash: "abc".to_string(),
                    snapshot_version: 5,
                },
                signatures: vec![],
            },
            &kp,
        );
        assert!(repo.update_timestamp(&ts1).is_ok());

        // Try to go back to snapshot version 3
        let ts2 = sign_test(
            TufMetadata {
                role: TufRole::Timestamp,
                version: 2,
                expires: Utc::now() + Duration::days(1),
                body: Timestamp {
                    snapshot_hash: "def".to_string(),
                    snapshot_version: 3,
                },
                signatures: vec![],
            },
            &kp,
        );
        let err = repo.update_timestamp(&ts2).unwrap_err();
        assert!(
            matches!(
                err,
                TufError::RollbackDetected {
                    current: 5,
                    received: 3
                }
            ),
            "expected RollbackDetected, got {:?}",
            err
        );
    }

    #[test]
    fn test_verify_target() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root);

        let content = b"hello world";
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(content);
            hasher.finalize().to_vec()
        };
        let hash_hex = hex::encode(&hash);

        let mut targets_map = BTreeMap::new();
        targets_map.insert(
            "my-package".to_string(),
            TargetInfo {
                sha256: hash_hex,
                length: content.len() as u64,
            },
        );

        let targets_meta = sign_test(
            TufMetadata {
                role: TufRole::Targets,
                version: 1,
                expires: Utc::now() + Duration::days(30),
                body: Targets {
                    targets: targets_map,
                },
                signatures: vec![],
            },
            &kp,
        );

        assert!(repo.update_targets(&targets_meta).is_ok());

        // Valid target
        assert!(repo
            .verify_target("my-package", &hash, content.len() as u64)
            .is_ok());

        // Wrong hash
        let err = repo
            .verify_target("my-package", b"wrong-hash", content.len() as u64)
            .unwrap_err();
        assert!(matches!(err, TufError::SignatureInvalid(_)));

        // Wrong size
        let err = repo.verify_target("my-package", &hash, 999).unwrap_err();
        assert!(matches!(err, TufError::SignatureInvalid(_)));

        // Not found
        let err = repo
            .verify_target("no-such-pkg", &hash, content.len() as u64)
            .unwrap_err();
        assert!(matches!(err, TufError::TargetNotFound(_)));
    }

    #[test]
    fn test_is_expired_with_valid_timestamp() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root);

        let ts = sign_test(
            TufMetadata {
                role: TufRole::Timestamp,
                version: 1,
                expires: Utc::now() + Duration::days(1),
                body: Timestamp {
                    snapshot_hash: "abc".to_string(),
                    snapshot_version: 1,
                },
                signatures: vec![],
            },
            &kp,
        );
        assert!(repo.update_timestamp(&ts).is_ok());
        assert!(!repo.is_expired());
    }

    #[test]
    fn test_snapshot_version_mismatch() {
        let (root, kp) = test_root_and_key();
        let mut repo = TufRepository::new(root);

        // Set expected snapshot version to 5 via timestamp
        let ts = sign_test(
            TufMetadata {
                role: TufRole::Timestamp,
                version: 1,
                expires: Utc::now() + Duration::days(1),
                body: Timestamp {
                    snapshot_hash: "abc".to_string(),
                    snapshot_version: 5,
                },
                signatures: vec![],
            },
            &kp,
        );
        assert!(repo.update_timestamp(&ts).is_ok());

        // Try snapshot with version 3 (doesn't match)
        let snap = sign_test(
            TufMetadata {
                role: TufRole::Snapshot,
                version: 3,
                expires: Utc::now() + Duration::days(1),
                body: Snapshot {
                    meta: BTreeMap::new(),
                },
                signatures: vec![],
            },
            &kp,
        );
        let err = repo.update_snapshot(&snap).unwrap_err();
        assert!(matches!(
            err,
            TufError::RollbackDetected {
                current: 5,
                received: 3
            }
        ));

        // Correct version
        let snap_ok = sign_test(
            TufMetadata {
                role: TufRole::Snapshot,
                version: 5,
                expires: Utc::now() + Duration::days(1),
                body: Snapshot {
                    meta: BTreeMap::new(),
                },
                signatures: vec![],
            },
            &kp,
        );
        assert!(repo.update_snapshot(&snap_ok).is_ok());
    }

    #[test]
    fn test_tuf_role_display() {
        assert_eq!(TufRole::Root.to_string(), "root");
        assert_eq!(TufRole::Targets.to_string(), "targets");
        assert_eq!(TufRole::Snapshot.to_string(), "snapshot");
        assert_eq!(TufRole::Timestamp.to_string(), "timestamp");
    }

    #[test]
    fn test_tuf_error_display() {
        let e = TufError::SignatureInvalid("bad sig".to_string());
        assert!(e.to_string().contains("bad sig"));

        let e = TufError::MetadataExpired("2026-01-01T00:00:00Z".to_string());
        assert!(e.to_string().contains("2026-01-01"));

        let e = TufError::RollbackDetected {
            current: 5,
            received: 3,
        };
        assert!(e.to_string().contains("5"));
        assert!(e.to_string().contains("3"));

        let e = TufError::ThresholdNotMet {
            required: 2,
            valid: 1,
        };
        assert!(e.to_string().contains("2"));
        assert!(e.to_string().contains("1"));

        let e = TufError::TargetNotFound("my-pkg".to_string());
        assert!(e.to_string().contains("my-pkg"));
    }

    #[test]
    fn test_unknown_key_ignored() {
        // Signature from an unknown key should be silently ignored
        let (root, kp) = test_root_and_key();
        let targets = Targets {
            targets: BTreeMap::new(),
        };
        let mut metadata = TufMetadata {
            role: TufRole::Targets,
            version: 1,
            expires: Utc::now() + Duration::days(1),
            body: targets,
            signatures: vec![],
        };

        // Add a valid signature from key1 and a bogus one from "unknown"
        let valid_sig = sign_with(&metadata, &kp);
        metadata.signatures = vec![
            TufSignature {
                key_id: "unknown-key".to_string(),
                value: "whatever".to_string(),
            },
            valid_sig,
        ];

        assert!(verify_metadata(&metadata, &root).is_ok());
    }
}
