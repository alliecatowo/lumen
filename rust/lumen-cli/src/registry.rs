//! Static Registry Infrastructure for Lumen packages.
//!
//! ## Design Philosophy
//!
//! **The registry is content-addressed, signed, and CDN-ready.**
//!
//! This module implements a world-class static registry architecture:
//!
//! - **Content-addressed**: All artifacts are stored by hash (CID)
//! - **Static hosting**: Entire registry can be served from S3/CDN
//! - **Signed metadata**: Package metadata is cryptographically signed
//! - **Transparency log**: Optional integration with Rekor/sigstore
//! - **Namespace support**: `@scope/package` for organizational clarity
//!
//! ## Registry Structure
//!
//! ```
//! registry/
//! ├── index.json                    # Global package index
//! ├── packages/
//! │   └── @scope/
//! │       └── package-name/
//! │           ├── index.json        # Package version index
//! │           ├── 1.0.0.json        # Version metadata (signed)
//! │           ├── 1.0.1.json
//! │           └── latest.json -> 1.0.1.json
//! ├── artifacts/
//! │   ├── sha256/
//! │   │   └── ab/
//! │   │       └── c123...           # Content-addressed tarballs
//! │   └── cid/
//! │       └── bafy...               # IPFS CIDs (optional)
//! ├── signatures/
//! │   └── @scope/
//! │       └── package-name/
//! │           └── 1.0.0.sig.json    # Detached signatures
//! └── transparency/
//!     └── checkpoint.json           # Rekor checkpoint
//! ```
//!
//! ## CDN Compatibility
//!
//! - All URLs are static and cacheable
//! - ETag support for conditional requests
//! - Cache-Control headers for optimal CDN caching
//! - Redirect support for artifact downloads

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

// =============================================================================
// Registry Client
// =============================================================================

/// Client for interacting with a Lumen package registry.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: String,
    client: reqwest::blocking::Client,
    config: ClientConfig,
}

/// Configuration for the registry client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Whether to verify signatures.
    pub verify_signatures: bool,
    /// Whether to check transparency log.
    pub check_transparency: bool,
    /// Timeout for requests in seconds.
    pub timeout_secs: u64,
    /// Custom CA certificate path (for private registries).
    pub ca_cert_path: Option<std::path::PathBuf>,
    /// API key for authenticated requests (optional).
    pub api_key: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            verify_signatures: true,
            check_transparency: false,
            timeout_secs: 30,
            ca_cert_path: None,
            api_key: None,
        }
    }
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::blocking::Client::new(),
            config: ClientConfig::default(),
        }
    }

    /// Create a client with custom configuration.
    pub fn with_config(base_url: impl Into<String>, config: ClientConfig) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::blocking::Client::new(),
            config,
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetch the global package index.
    pub fn fetch_global_index(&self) -> Result<GlobalIndex, String> {
        let url = format!("{}/index.json", self.base_url);
        self.fetch_json(&url)
    }

    /// Fetch the index for a specific package.
    pub fn fetch_package_index(&self, name: &str) -> Result<RegistryPackageIndex, String> {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!("{}/packages/@{}/{}/index.json", self.base_url, s, pkg)
        } else {
            format!("{}/packages/{}/index.json", self.base_url, pkg)
        };
        self.fetch_json(&url)
    }

    /// Fetch metadata for a specific version.
    pub fn fetch_version_metadata(
        &self,
        name: &str,
        version: &str,
    ) -> Result<RegistryVersionMetadata, String> {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!("{}/packages/@{}/{}/{}.json", self.base_url, s, pkg, version)
        } else {
            format!("{}/packages/{}/{}.json", self.base_url, pkg, version)
        };

        let metadata: RegistryVersionMetadata = self.fetch_json(&url)?;

        // Verify signature if configured
        if self.config.verify_signatures {
            if let Some(ref sig) = metadata.signature {
                self.verify_signature(&metadata, sig)?;
            }
        }

        Ok(metadata)
    }

    /// Download an artifact by URL or CID.
    pub fn download_artifact(
        &self,
        url: &str,
        output_path: &Path,
        expected_hash: Option<&str>,
    ) -> Result<(), String> {
        let full_url = if url.contains("://") {
            url.to_string()
        } else {
            let base = self.base_url.trim_end_matches('/');
            let rel = url.trim_start_matches('/');
            format!("{}/{}", base, rel)
        };

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut file = File::create(output_path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();

        if full_url.starts_with("file://") {
            let path = Path::new(full_url.strip_prefix("file://").unwrap());
            let mut source =
                File::open(path).map_err(|e| format!("failed to open {}: {}", path.display(), e))?;
            let mut buffer = [0; 8192];
            loop {
                let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                file.write_all(&buffer[..n])
                    .map_err(|e| e.to_string())?;
            }
        } else {
            let mut resp = self.client.get(&full_url).send().map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!(
                    "failed to download '{}': {}",
                    full_url,
                    resp.status()
                ));
            }
            let mut buffer = [0; 8192];
            loop {
                let n = resp.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                file.write_all(&buffer[..n])
                    .map_err(|e| e.to_string())?;
            }
        }

        if let Some(expected) = expected_hash {
            let actual = format!("sha256:{}", hex_encode(&hasher.finalize()));
            if actual != expected {
                return Err(format!(
                    "artifact checksum mismatch: expected {}, got {}",
                    expected, actual
                ));
            }
        }

        Ok(())
    }

    /// Download by content hash (CID).
    pub fn download_by_cid(&self, cid: &str, output_path: &Path) -> Result<(), String> {
        let url = self.cid_to_url(cid)?;
        self.download_artifact(&url, output_path, Some(&cid_to_hash(cid)?))
    }

    /// Convert a CID to a registry URL.
    fn cid_to_url(&self, cid: &str) -> Result<String, String> {
        // Support both IPFS CIDs and our sha256-based CIDs
        if cid.starts_with("bafy") || cid.starts_with("bafk") {
            // IPFS CIDv1
            Ok(format!("{}/artifacts/cid/{}", self.base_url, cid))
        } else if cid.starts_with("sha256:") || cid.starts_with("cid:sha256:") {
            // Our content-addressed format
            let hash = cid.strip_prefix("cid:sha256:").unwrap_or(cid);
            let hash = hash.strip_prefix("sha256:").unwrap_or(hash);
            if hash.len() < 4 {
                return Err("Invalid hash: too short".to_string());
            }
            let prefix = &hash[..2];
            let rest = &hash[2..];
            Ok(format!(
                "{}/artifacts/sha256/{}/{}",
                self.base_url, prefix, rest
            ))
        } else {
            Err(format!("Unsupported CID format: {}", cid))
        }
    }

    // Helper methods

    fn fetch_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, String> {
        if url.starts_with("file://") {
            let path = Path::new(url.strip_prefix("file://").unwrap());
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
            serde_json::from_str(&content).map_err(|e| format!("invalid JSON: {}", e))
        } else {
            let mut req = self.client.get(url);
            if let Some(ref key) = self.config.api_key {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            let resp = req.send().map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("request failed: {}", resp.status()));
            }
            resp.json().map_err(|e| format!("invalid JSON: {}", e))
        }
    }

    fn verify_signature(
        &self,
        _metadata: &RegistryVersionMetadata,
        _sig: &PackageSignature,
    ) -> Result<(), String> {
        // TODO: Implement actual signature verification
        // For v0, we just check that a signature exists
        Ok(())
    }
}

// =============================================================================
// Registry Data Types
// =============================================================================

/// Global package index (top-level registry listing).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalIndex {
    /// Registry name.
    pub name: String,
    /// Registry version.
    pub version: String,
    /// Timestamp of last update.
    pub updated_at: Option<String>,
    /// Total package count.
    pub package_count: Option<u64>,
    /// Packages included in this index (optional - for full mirroring).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<IndexEntry>,
    /// Transparency log checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<CheckpointInfo>,
}

/// Entry in the global index.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexEntry {
    /// Package name.
    pub name: String,
    /// Latest version.
    pub latest: Option<String>,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Last update timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Package index (per-package version listing).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryPackageIndex {
    /// Package name.
    pub name: String,
    /// Available versions.
    pub versions: Vec<String>,
    /// Latest stable version.
    pub latest: Option<String>,
    /// Yanked versions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub yanked: BTreeMap<String, String>,
    /// Prerelease versions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prereleases: Vec<String>,
    /// Package description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Package categories.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// Total download count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
}

/// Metadata for a specific package version.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryVersionMetadata {
    /// Package name (with namespace).
    pub name: String,
    /// Version string.
    pub version: String,
    /// Dependencies (name -> constraint).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deps: BTreeMap<String, String>,
    /// Optional dependencies grouped by feature.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_deps: BTreeMap<String, Vec<String>>,
    /// Downloadable artifacts.
    pub artifacts: Vec<ArtifactInfo>,
    /// Integrity information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<IntegrityInfo>,
    /// Cryptographic signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignature>,
    /// Transparency log entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency: Option<TransparencyEntry>,
    /// Whether this version is yanked.
    #[serde(default)]
    pub yanked: bool,
    /// Yank reason (if yanked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yank_reason: Option<String>,
    /// Publication timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Publisher information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<PublisherInfo>,
    /// License identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Readme URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    /// Documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// Artifact download information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArtifactInfo {
    /// Kind of artifact (tar, wasm, source, etc.).
    pub kind: String,
    /// Download URL (relative or absolute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Content hash (sha256:... or cid:...).
    pub hash: String,
    /// File size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Target architecture (for platform-specific artifacts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// Target OS (for platform-specific artifacts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
}

/// Integrity information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IntegrityInfo {
    /// Hash of the manifest.
    pub manifest_hash: String,
    /// Hash of the metadata JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_hash: Option<String>,
    /// Hash of all source files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Package signature.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageSignature {
    /// Signature algorithm (ed25519, rsa-pss, etc.).
    pub algorithm: String,
    /// Base64-encoded signature.
    pub signature: String,
    /// Key identifier (fingerprint or key ID).
    pub key_id: String,
    /// Timestamp of signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_at: Option<String>,
    /// Rekor bundle (for sigstore integration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rekor_bundle: Option<String>,
}

/// Transparency log entry.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransparencyEntry {
    /// Log index.
    pub log_index: u64,
    /// Log ID.
    pub log_id: String,
    /// Entry timestamp.
    pub timestamp: String,
    /// Tree size at time of inclusion.
    pub tree_size: u64,
    /// Root hash at time of inclusion.
    pub root_hash: String,
}

/// Checkpoint information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointInfo {
    /// Checkpoint timestamp.
    pub timestamp: String,
    /// Tree size.
    pub tree_size: u64,
    /// Root hash.
    pub root_hash: String,
    /// Checkpoint signature.
    pub signature: String,
}

/// Publisher information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublisherInfo {
    /// Publisher name or ID.
    pub name: Option<String>,
    /// Publisher email (optional).
    pub email: Option<String>,
    /// Verification status.
    pub verified: bool,
}

// =============================================================================
// Registry Builder (for creating registry files)
// =============================================================================

/// Builder for creating registry metadata files.
#[derive(Debug, Default)]
pub struct RegistryBuilder {
    packages: BTreeMap<String, PackageBuilder>,
}

#[derive(Debug)]
struct PackageBuilder {
    versions: BTreeMap<String, RegistryVersionMetadata>,
    description: Option<String>,
    categories: Vec<String>,
}

impl RegistryBuilder {
    /// Create a new registry builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a package version.
    pub fn add_version(
        &mut self,
        name: &str,
        version: &str,
        deps: BTreeMap<String, String>,
        artifacts: Vec<ArtifactInfo>,
    ) -> &mut Self {
        let entry = self.packages.entry(name.to_string()).or_insert_with(|| {
            PackageBuilder {
                versions: BTreeMap::new(),
                description: None,
                categories: Vec::new(),
            }
        });

        entry.versions.insert(
            version.to_string(),
            RegistryVersionMetadata {
                name: name.to_string(),
                version: version.to_string(),
                deps,
                optional_deps: BTreeMap::new(),
                artifacts,
                integrity: None,
                signature: None,
                transparency: None,
                yanked: false,
                yank_reason: None,
                published_at: None,
                publisher: None,
                license: None,
                description: None,
                readme: None,
                documentation: None,
                repository: None,
                keywords: Vec::new(),
            },
        );

        self
    }

    /// Set package description.
    pub fn set_description(&mut self, name: &str, description: &str) -> &mut Self {
        if let Some(pkg) = self.packages.get_mut(name) {
            pkg.description = Some(description.to_string());
        }
        self
    }

    /// Build the registry files to a directory.
    pub fn build(&self, output_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("cannot create output dir: {}", e))?;

        // Write global index
        let global_index = GlobalIndex {
            name: "local-registry".to_string(),
            version: "1.0.0".to_string(),
            updated_at: None,
            package_count: Some(self.packages.len() as u64),
            packages: self
                .packages
                .iter()
                .map(|(name, pkg)| {
                    let latest = pkg.versions.keys().last().cloned();
                    IndexEntry {
                        name: name.clone(),
                        latest,
                        description: pkg.description.clone(),
                        updated_at: None,
                    }
                })
                .collect(),
            checkpoint: None,
        };

        let global_index_path = output_dir.join("index.json");
        write_json(&global_index_path, &global_index)?;

        // Write per-package files
        for (name, pkg) in &self.packages {
            let (scope, pkg_name) = parse_package_name(name);
            let pkg_dir = if let Some(s) = scope {
                output_dir.join("packages").join(format!("@{}", s)).join(pkg_name)
            } else {
                output_dir.join("packages").join(pkg_name)
            };

            std::fs::create_dir_all(&pkg_dir)
                .map_err(|e| format!("cannot create package dir: {}", e))?;

            // Write version index
            let versions: Vec<String> = pkg.versions.keys().cloned().collect();
            let latest = versions.last().cloned();
            let index = RegistryPackageIndex {
                name: name.clone(),
                versions,
                latest,
                yanked: BTreeMap::new(),
                prereleases: Vec::new(),
                description: pkg.description.clone(),
                categories: pkg.categories.clone(),
                downloads: None,
            };

            write_json(&pkg_dir.join("index.json"), &index)?;

            // Write version metadata files
            for (version, metadata) in &pkg.versions {
                write_json(&pkg_dir.join(format!("{}.json", version)), metadata)?;
            }
        }

        Ok(())
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn parse_package_name(name: &str) -> (Option<&str>, &str) {
    if let Some(idx) = name.find('/') {
        let scope = &name[..idx];
        if scope.starts_with('@') {
            (Some(&scope[1..]), &name[idx + 1..])
        } else {
            (None, name)
        }
    } else {
        (None, name)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(nibble_to_hex(b >> 4));
        s.push(nibble_to_hex(b & 0x0f));
    }
    s
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

fn cid_to_hash(cid: &str) -> Result<String, String> {
    if let Some(hash) = cid.strip_prefix("sha256:") {
        Ok(format!("sha256:{}", hash))
    } else if let Some(hash) = cid.strip_prefix("cid:sha256:") {
        Ok(format!("sha256:{}", hash))
    } else {
        Err(format!("Cannot extract hash from CID: {}", cid))
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize JSON: {}", e))?;
    std::fs::write(path, content)
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_name() {
        assert_eq!(parse_package_name("simple"), (None, "simple"));
        assert_eq!(parse_package_name("@scope/name"), (Some("scope"), "name"));
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00]), "00");
        assert_eq!(hex_encode(&[0xff]), "ff");
        assert_eq!(hex_encode(&[0xab, 0xcd]), "abcd");
    }

    #[test]
    fn test_cid_to_hash() {
        assert_eq!(
            cid_to_hash("sha256:abc123").unwrap(),
            "sha256:abc123"
        );
        assert_eq!(
            cid_to_hash("cid:sha256:abc123").unwrap(),
            "sha256:abc123"
        );
    }

    #[test]
    fn test_registry_builder() {
        let mut builder = RegistryBuilder::new();
        builder
            .add_version(
                "test-pkg",
                "1.0.0",
                BTreeMap::new(),
                vec![ArtifactInfo {
                    kind: "tar".to_string(),
                    url: Some("artifacts/test.tar".to_string()),
                    hash: "sha256:abc123".to_string(),
                    size: Some(1024),
                    arch: None,
                    os: None,
                }],
            )
            .set_description("test-pkg", "A test package");

        let temp_dir = std::env::temp_dir().join("lumen_registry_test");
        builder.build(&temp_dir).unwrap();

        assert!(temp_dir.join("index.json").exists());
        assert!(temp_dir.join("packages/test-pkg/index.json").exists());
        assert!(temp_dir.join("packages/test-pkg/1.0.0.json").exists());
    }
}
