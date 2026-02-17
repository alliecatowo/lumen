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
//! ```text
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
use std::collections::BTreeMap;
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
            let mut source = File::open(path)
                .map_err(|e| format!("failed to open {}: {}", path.display(), e))?;
            let mut buffer = [0; 8192];
            loop {
                let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            }
        } else {
            let mut resp = self
                .client
                .get(&full_url)
                .send()
                .map_err(|e| e.to_string())?;
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
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
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
// Cloudflare R2 Registry Client
// =============================================================================

/// Error types for R2 registry operations.
#[derive(Debug, thiserror::Error)]
pub enum R2Error {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Content hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("R2 API error: {status} - {message}")]
    ApiError { status: u16, message: String },
}

impl From<reqwest::Error> for R2Error {
    fn from(e: reqwest::Error) -> Self {
        R2Error::Http(e.to_string())
    }
}

impl From<serde_json::Error> for R2Error {
    fn from(e: serde_json::Error) -> Self {
        R2Error::Serialization(e.to_string())
    }
}

/// Result type for R2 operations.
pub type R2Result<T> = Result<T, R2Error>;

/// Configuration for Cloudflare R2 registry client.
#[derive(Debug, Clone)]
pub struct R2Config {
    /// Cloudflare account ID.
    pub account_id: String,
    /// R2 access key ID.
    pub access_key_id: String,
    /// R2 secret access key.
    pub secret_access_key: String,
    /// R2 bucket name (defaults to "lumen-registry").
    pub bucket: String,
    /// Custom public URL for CDN access (optional).
    /// If not provided, uses the R2.dev subdomain.
    pub public_url: Option<String>,
    /// Timeout for requests in seconds.
    pub timeout_secs: u64,
    /// Region (defaults to "auto" for Cloudflare R2).
    pub region: String,
}

impl R2Config {
    /// Create a new R2 configuration.
    pub fn new(
        account_id: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            bucket: "lumen-registry".to_string(),
            public_url: None,
            timeout_secs: 60,
            region: "auto".to_string(),
        }
    }

    /// Set the bucket name.
    pub fn with_bucket(mut self, bucket: impl Into<String>) -> Self {
        self.bucket = bucket.into();
        self
    }

    /// Set the custom public URL for CDN access.
    pub fn with_public_url(mut self, url: impl Into<String>) -> Self {
        self.public_url = Some(url.into());
        self
    }

    /// Set the timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Get the S3-compatible endpoint URL.
    pub fn endpoint(&self) -> String {
        format!("https://{}.r2.cloudflarestorage.com", self.account_id)
    }

    /// Get the public base URL for reading artifacts.
    /// Uses custom public URL if set, otherwise falls back to R2.dev.
    pub fn public_base_url(&self) -> String {
        if let Some(ref url) = self.public_url {
            url.trim_end_matches('/').to_string()
        } else {
            // Default R2.dev public URL format
            format!("https://pub-{}.r2.dev", self.account_id.replace("-", ""))
        }
    }

    /// Build from lumen.toml config if R2 credentials are present.
    pub fn from_lumen_config(config: &crate::config::LumenConfig) -> Option<Self> {
        let registry = config.registry.as_ref()?;
        let (account_id, access_key) = registry.r2_credentials()?;

        // Split the access key into ID and secret
        // Format is typically "access_key_id:secret_access_key"
        let (key_id, secret) = if access_key.contains(':') {
            let parts: Vec<&str> = access_key.splitn(2, ':').collect();
            (parts[0].to_string(), parts[1].to_string())
        } else {
            // If no colon, treat the whole thing as the key ID
            // and expect the secret to be in an env var
            let secret = std::env::var("R2_SECRET_ACCESS_KEY").ok()?;
            (access_key.to_string(), secret)
        };

        Some(Self::new(account_id, key_id, secret))
    }
}

/// Cloudflare R2 registry client.
///
/// This client provides methods to upload and download packages from
/// a Cloudflare R2 bucket using the S3-compatible API for writes and
/// direct HTTP for public reads.
#[derive(Debug, Clone)]
pub struct R2Client {
    config: R2Config,
    http_client: reqwest::blocking::Client,
}

impl R2Client {
    /// Create a new R2 client with the given configuration.
    pub fn new(config: R2Config) -> R2Result<Self> {
        let http_client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| R2Error::Http(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Create an R2 client from lumen.toml configuration.
    pub fn from_config(config: &crate::config::LumenConfig) -> R2Result<Self> {
        let r2_config = R2Config::from_lumen_config(config)
            .ok_or_else(|| R2Error::InvalidConfig(
                "R2 credentials not found in config. Set r2_account_id and r2_access_key in [registry] section.".to_string()
            ))?;
        Self::new(r2_config)
    }

    // =========================================================================
    // Artifact Operations
    // =========================================================================

    /// Upload a package artifact to R2.
    ///
    /// The artifact is stored using content-addressing based on its SHA-256 hash.
    /// Returns the content hash (CID) of the uploaded artifact.
    ///
    /// # Arguments
    ///
    /// * `data` - The artifact data to upload
    /// * `content_type` - Optional MIME type (defaults to application/gzip for tarballs)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let artifact_data = std::fs::read("package.tar.gz")?;
    /// let cid = client.upload_artifact(&artifact_data, None)?;
    /// println!("Uploaded to: {}", cid); // e.g., "sha256:abc123..."
    /// ```
    pub fn upload_artifact(&self, data: &[u8], content_type: Option<&str>) -> R2Result<String> {
        // Compute SHA-256 hash for content addressing
        let hash = compute_sha256(data);
        let cid = format!("sha256:{}", hash);

        // Build the storage path: artifacts/sha256/{first2}/{remaining}
        let key = artifact_path(&hash);

        // Determine content type
        let content_type = content_type.unwrap_or("application/gzip");

        // Upload to R2 using S3-compatible API
        self.upload_to_s3(&key, data, content_type)?;

        Ok(cid)
    }

    /// Download an artifact by its content hash (CID).
    ///
    /// For public reads, this uses the direct HTTP CDN URL which is
    /// faster and doesn't require authentication.
    ///
    /// # Arguments
    ///
    /// * `cid` - Content identifier (e.g., "sha256:abc123...")
    /// * `verify` - Whether to verify the downloaded content matches the CID
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let data = client.download_artifact("sha256:abc123...", true)?;
    /// std::fs::write("package.tar.gz", data)?;
    /// ```
    pub fn download_artifact(&self, cid: &str, verify: bool) -> R2Result<Vec<u8>> {
        // Extract the hash from the CID
        let hash = extract_hash_from_cid(cid)?;

        // Build the URL for public access
        let key = artifact_path(&hash);
        let url = format!("{}/{}", self.config.public_base_url(), key);

        // Download via HTTP
        let response = self.http_client.get(&url).send()?;

        if !response.status().is_success() {
            return Err(R2Error::ApiError {
                status: response.status().as_u16(),
                message: format!("Failed to download artifact: {}", response.status()),
            });
        }

        let data = response.bytes()?.to_vec();

        // Verify content hash if requested
        if verify {
            let actual_hash = compute_sha256(&data);
            if actual_hash != hash {
                return Err(R2Error::HashMismatch {
                    expected: hash,
                    actual: actual_hash,
                });
            }
        }

        Ok(data)
    }

    /// Check if an artifact exists in the registry.
    pub fn artifact_exists(&self, cid: &str) -> R2Result<bool> {
        let hash = extract_hash_from_cid(cid)?;
        let key = artifact_path(&hash);

        // Use HEAD request to check existence without downloading
        let url = format!("{}/{}", self.config.public_base_url(), key);

        let response = self.http_client.head(&url).send()?;

        match response.status() {
            reqwest::StatusCode::OK => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            status => Err(R2Error::ApiError {
                status: status.as_u16(),
                message: format!("Unexpected status checking artifact: {}", status),
            }),
        }
    }

    // =========================================================================
    // Index Operations
    // =========================================================================

    /// Update the global package index.
    ///
    /// This uploads a new `index.json` to the root of the registry.
    pub fn update_global_index(&self, index: &GlobalIndex) -> R2Result<()> {
        let data = serde_json::to_vec_pretty(index)?;
        self.upload_to_s3("index.json", &data, "application/json")?;
        Ok(())
    }

    /// Fetch the global package index.
    ///
    /// Uses the public HTTP endpoint for fast CDN-backed reads.
    pub fn fetch_global_index(&self) -> R2Result<GlobalIndex> {
        let url = format!("{}/index.json", self.config.public_base_url());

        let response = self.http_client.get(&url).send()?;

        if !response.status().is_success() {
            return Err(R2Error::ApiError {
                status: response.status().as_u16(),
                message: format!("Failed to fetch global index: {}", response.status()),
            });
        }

        let index: GlobalIndex = response.json()?;
        Ok(index)
    }

    /// Update a package's version index.
    ///
    /// This uploads a new `index.json` to the package directory.
    pub fn update_package_index(&self, name: &str, index: &RegistryPackageIndex) -> R2Result<()> {
        let key = package_index_path(name);
        let data = serde_json::to_vec_pretty(index)?;
        self.upload_to_s3(&key, &data, "application/json")?;
        Ok(())
    }

    /// Fetch a package's version index.
    pub fn fetch_package_index(&self, name: &str) -> R2Result<RegistryPackageIndex> {
        let key = package_index_path(name);
        let url = format!("{}/{}", self.config.public_base_url(), key);

        let response = self.http_client.get(&url).send()?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(R2Error::ApiError {
                status: 404,
                message: format!("Package '{}' not found", name),
            });
        }

        if !response.status().is_success() {
            return Err(R2Error::ApiError {
                status: response.status().as_u16(),
                message: format!("Failed to fetch package index: {}", response.status()),
            });
        }

        let index: RegistryPackageIndex = response.json()?;
        Ok(index)
    }

    /// Upload version metadata for a package.
    ///
    /// This creates the `{version}.json` file for a specific package version.
    pub fn upload_version_metadata(
        &self,
        name: &str,
        version: &str,
        metadata: &RegistryVersionMetadata,
    ) -> R2Result<()> {
        let key = version_metadata_path(name, version);
        let data = serde_json::to_vec_pretty(metadata)?;
        self.upload_to_s3(&key, &data, "application/json")?;
        Ok(())
    }

    /// Fetch version metadata for a package.
    pub fn fetch_version_metadata(
        &self,
        name: &str,
        version: &str,
    ) -> R2Result<RegistryVersionMetadata> {
        let key = version_metadata_path(name, version);
        let url = format!("{}/{}", self.config.public_base_url(), key);

        let response = self.http_client.get(&url).send()?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(R2Error::ApiError {
                status: 404,
                message: format!("Version '{}@{}' not found", name, version),
            });
        }

        if !response.status().is_success() {
            return Err(R2Error::ApiError {
                status: response.status().as_u16(),
                message: format!("Failed to fetch version metadata: {}", response.status()),
            });
        }

        let metadata: RegistryVersionMetadata = response.json()?;
        Ok(metadata)
    }

    /// Publish a complete package version.
    ///
    /// This uploads the artifact and updates all necessary index files.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Package version metadata
    /// * `artifact_data` - The package tarball data
    ///
    /// # Returns
    ///
    /// The CID of the uploaded artifact.
    pub fn publish_version(
        &self,
        metadata: &RegistryVersionMetadata,
        artifact_data: &[u8],
    ) -> R2Result<String> {
        let name = &metadata.name;
        let version = &metadata.version;

        // 1. Upload the artifact
        let cid = self.upload_artifact(artifact_data, Some("application/gzip"))?;

        // 2. Upload version metadata
        self.upload_version_metadata(name, version, metadata)?;

        // 3. Update or create package index
        let mut pkg_index = match self.fetch_package_index(name) {
            Ok(index) => index,
            Err(R2Error::ApiError { status: 404, .. }) => RegistryPackageIndex {
                name: name.clone(),
                versions: Vec::new(),
                latest: None,
                yanked: BTreeMap::new(),
                prereleases: Vec::new(),
                description: metadata.description.clone(),
                categories: Vec::new(),
                downloads: Some(0),
            },
            Err(e) => return Err(e),
        };

        // Add version if not already present
        if !pkg_index.versions.contains(&version.to_string()) {
            pkg_index.versions.push(version.to_string());
            pkg_index.versions.sort();
        }

        // Update latest version (simple semver comparison)
        if !version.contains('-') {
            // Not a prerelease
            pkg_index.latest = Some(version.to_string());
        }

        self.update_package_index(name, &pkg_index)?;

        // 4. Update global index
        let mut global_index = match self.fetch_global_index() {
            Ok(index) => index,
            Err(_) => GlobalIndex {
                name: "lumen-registry".to_string(),
                version: "1.0.0".to_string(),
                updated_at: Some(chrono::Utc::now().to_rfc3339()),
                package_count: Some(0),
                packages: Vec::new(),
                checkpoint: None,
            },
        };

        // Check if package is already in index
        let entry_exists = global_index.packages.iter().any(|p| p.name == *name);
        if !entry_exists {
            global_index.packages.push(IndexEntry {
                name: name.clone(),
                latest: Some(version.to_string()),
                description: metadata.description.clone(),
                updated_at: Some(chrono::Utc::now().to_rfc3339()),
            });
            global_index.package_count = Some(global_index.packages.len() as u64);
        } else {
            // Update existing entry
            for entry in &mut global_index.packages {
                if entry.name == *name {
                    entry.latest = Some(version.to_string());
                    entry.description = metadata.description.clone();
                    entry.updated_at = Some(chrono::Utc::now().to_rfc3339());
                }
            }
        }

        global_index.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self.update_global_index(&global_index)?;

        Ok(cid)
    }

    // =========================================================================
    // S3-Compatible API Methods
    // =========================================================================

    /// Upload data to a specific key in R2 using S3-compatible API with AWS Signature V4.
    ///
    /// This is the low-level upload method. For content-addressed uploads, use
    /// [`upload_artifact`] instead.
    pub fn put_object(&self, key: &str, data: &[u8], content_type: &str) -> R2Result<()> {
        self.upload_to_s3(key, data, content_type)
    }

    /// Upload data to R2 using S3-compatible API with AWS Signature V4.
    fn upload_to_s3(&self, key: &str, data: &[u8], content_type: &str) -> R2Result<()> {
        let endpoint = self.config.endpoint();
        let url = format!("{}/{}/{}", endpoint, self.config.bucket, key);

        // Generate AWS Signature V4 headers
        let headers = self.sign_s3_request("PUT", &url, data, content_type)?;

        let response = self
            .http_client
            .put(&url)
            .headers(headers)
            .body(data.to_vec())
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(R2Error::ApiError {
                status: status.as_u16(),
                message: format!("S3 upload failed: {} - {}", status, body),
            });
        }

        Ok(())
    }

    /// Generate AWS Signature V4 headers for S3-compatible requests.
    fn sign_s3_request(
        &self,
        method: &str,
        url: &str,
        payload: &[u8],
        content_type: &str,
    ) -> R2Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, HOST};

        let url = reqwest::Url::parse(url).map_err(|e| R2Error::InvalidConfig(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| R2Error::InvalidConfig("No host in URL".to_string()))?;

        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Compute payload hash
        let payload_hash = compute_sha256(payload);

        // Build canonical request
        let canonical_uri = url.path();
        let canonical_querystring = url.query().unwrap_or("");

        let canonical_headers = format!(
            "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            host, payload_hash, amz_date
        );
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_querystring,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        // Create string to sign
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, self.config.region);
        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm,
            amz_date,
            credential_scope,
            compute_sha256(canonical_request.as_bytes())
        );

        // Calculate signature
        let signature = self.calculate_signature(&date_stamp, &string_to_sign)?;

        // Build authorization header
        let authorization_header = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, self.config.access_key_id, credential_scope, signed_headers, signature
        );

        // Build header map
        let mut headers = HeaderMap::new();
        headers.insert(
            HOST,
            HeaderValue::from_str(host).map_err(|e| R2Error::Http(e.to_string()))?,
        );
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(content_type).map_err(|e| R2Error::Http(e.to_string()))?,
        );
        headers.insert(
            "x-amz-date",
            HeaderValue::from_str(&amz_date).map_err(|e| R2Error::Http(e.to_string()))?,
        );
        headers.insert(
            "x-amz-content-sha256",
            HeaderValue::from_str(&payload_hash).map_err(|e| R2Error::Http(e.to_string()))?,
        );
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&authorization_header)
                .map_err(|e| R2Error::Http(e.to_string()))?,
        );

        Ok(headers)
    }

    /// Calculate AWS Signature V4 signature.
    fn calculate_signature(&self, date_stamp: &str, string_to_sign: &str) -> R2Result<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        // Derive signing key
        let secret = format!("AWS4{}", self.config.secret_access_key);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| R2Error::Authentication(e.to_string()))?;
        mac.update(date_stamp.as_bytes());
        let date_key = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&date_key)
            .map_err(|e| R2Error::Authentication(e.to_string()))?;
        mac.update(self.config.region.as_bytes());
        let date_region_key = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&date_region_key)
            .map_err(|e| R2Error::Authentication(e.to_string()))?;
        mac.update(b"s3");
        let date_region_service_key = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&date_region_service_key)
            .map_err(|e| R2Error::Authentication(e.to_string()))?;
        mac.update(b"aws4_request");
        let signing_key = mac.finalize().into_bytes();

        // Sign the string
        let mut mac = HmacSha256::new_from_slice(&signing_key)
            .map_err(|e| R2Error::Authentication(e.to_string()))?;
        mac.update(string_to_sign.as_bytes());
        let signature = mac.finalize().into_bytes();

        Ok(hex_encode(&signature))
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get the R2 configuration.
    pub fn config(&self) -> &R2Config {
        &self.config
    }

    /// Get the public URL for an artifact by CID.
    pub fn artifact_url(&self, cid: &str) -> R2Result<String> {
        let hash = extract_hash_from_cid(cid)?;
        let key = artifact_path(&hash);
        Ok(format!("{}/{}", self.config.public_base_url(), key))
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
        let entry = self
            .packages
            .entry(name.to_string())
            .or_insert_with(|| PackageBuilder {
                versions: BTreeMap::new(),
                description: None,
                categories: Vec::new(),
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
                output_dir
                    .join("packages")
                    .join(format!("@{}", s))
                    .join(pkg_name)
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
        if let Some(stripped) = scope.strip_prefix('@') {
            (Some(stripped), &name[idx + 1..])
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
    std::fs::write(path, content).map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

/// Compute SHA-256 hash of data.
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

/// Extract the hex hash from a CID.
fn extract_hash_from_cid(cid: &str) -> R2Result<String> {
    if cid.starts_with("sha256:") {
        Ok(cid.strip_prefix("sha256:").unwrap().to_string())
    } else if cid.starts_with("cid:sha256:") {
        Ok(cid.strip_prefix("cid:sha256:").unwrap().to_string())
    } else {
        Err(R2Error::InvalidConfig(format!(
            "Unsupported CID format: {}. Expected sha256:... or cid:sha256:...",
            cid
        )))
    }
}

/// Build the storage path for an artifact based on its hash.
/// Follows the content-addressed structure: artifacts/sha256/{first2}/{remaining}
fn artifact_path(hash: &str) -> String {
    if hash.len() < 4 {
        return format!("artifacts/sha256/{}/{}", &hash[..2.min(hash.len())], hash);
    }
    format!("artifacts/sha256/{}/{}", &hash[..2], &hash[2..])
}

/// Build the storage path for a package index.
fn package_index_path(name: &str) -> String {
    let (scope, pkg) = parse_package_name(name);
    if let Some(s) = scope {
        format!("packages/@{}/{}/index.json", s, pkg)
    } else {
        format!("packages/{}/index.json", pkg)
    }
}

/// Build the storage path for version metadata.
fn version_metadata_path(name: &str, version: &str) -> String {
    let (scope, pkg) = parse_package_name(name);
    if let Some(s) = scope {
        format!("packages/@{}/{}/{}.json", s, pkg, version)
    } else {
        format!("packages/{}/{}.json", pkg, version)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_name() {
        // Bare names still parse (validation is separate) — returns (None, name)
        assert_eq!(parse_package_name("simple"), (None, "simple"));
        // Namespaced names extract scope and local name
        assert_eq!(parse_package_name("@scope/name"), (Some("scope"), "name"));
        assert_eq!(
            parse_package_name("@alliecatowo/lumen-utils"),
            (Some("alliecatowo"), "lumen-utils")
        );
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00]), "00");
        assert_eq!(hex_encode(&[0xff]), "ff");
        assert_eq!(hex_encode(&[0xab, 0xcd]), "abcd");
    }

    #[test]
    fn test_cid_to_hash() {
        assert_eq!(cid_to_hash("sha256:abc123").unwrap(), "sha256:abc123");
        assert_eq!(cid_to_hash("cid:sha256:abc123").unwrap(), "sha256:abc123");
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

    #[test]
    fn test_compute_sha256() {
        let data = b"hello world";
        let hash = compute_sha256(data);
        assert_eq!(hash.len(), 64); // SHA-256 hex string is 64 chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_extract_hash_from_cid() {
        assert_eq!(extract_hash_from_cid("sha256:abc123").unwrap(), "abc123");
        assert_eq!(
            extract_hash_from_cid("cid:sha256:def456").unwrap(),
            "def456"
        );
        assert!(extract_hash_from_cid("invalid").is_err());
    }

    #[test]
    fn test_artifact_path() {
        let hash = "abcdef1234567890";
        let path = artifact_path(hash);
        assert_eq!(path, "artifacts/sha256/ab/cdef1234567890");
    }

    #[test]
    fn test_package_index_path() {
        assert_eq!(
            package_index_path("simple-pkg"),
            "packages/simple-pkg/index.json"
        );
        assert_eq!(
            package_index_path("@scope/name"),
            "packages/@scope/name/index.json"
        );
    }

    #[test]
    fn test_version_metadata_path() {
        assert_eq!(
            version_metadata_path("simple-pkg", "1.0.0"),
            "packages/simple-pkg/1.0.0.json"
        );
        assert_eq!(
            version_metadata_path("@scope/name", "2.1.0"),
            "packages/@scope/name/2.1.0.json"
        );
    }

    #[test]
    fn test_r2_config() {
        let config = R2Config::new("account123", "key_id", "secret_key")
            .with_bucket("my-bucket")
            .with_public_url("https://cdn.example.com");

        assert_eq!(config.account_id, "account123");
        assert_eq!(config.access_key_id, "key_id");
        assert_eq!(config.bucket, "my-bucket");
        assert_eq!(
            config.public_url,
            Some("https://cdn.example.com".to_string())
        );
        assert_eq!(
            config.endpoint(),
            "https://account123.r2.cloudflarestorage.com"
        );
        assert_eq!(config.public_base_url(), "https://cdn.example.com");
    }

    #[test]
    fn test_r2_config_from_lumen_config() {
        let toml = r#"
[package]
name = "test"

[registry]
r2_account_id = "my_account"
r2_access_key = "key_id:secret_key"
"#;
        let lumen_config: crate::config::LumenConfig = toml::from_str(toml).unwrap();
        let r2_config = R2Config::from_lumen_config(&lumen_config);

        assert!(r2_config.is_some());
        let config = r2_config.unwrap();
        assert_eq!(config.account_id, "my_account");
        assert_eq!(config.access_key_id, "key_id");
        assert_eq!(config.secret_access_key, "secret_key");
    }
}

// =============================================================================
// Deployment Registry Infrastructure (T092)
// =============================================================================

/// HTTP method for registry requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMethod {
    /// HTTP GET
    Get,
    /// HTTP PUT
    Put,
    /// HTTP Post
    Post,
    /// HTTP DELETE
    Delete,
    /// HTTP HEAD
    Head,
}

impl fmt::Display for RequestMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestMethod::Get => write!(f, "GET"),
            RequestMethod::Put => write!(f, "PUT"),
            RequestMethod::Post => write!(f, "POST"),
            RequestMethod::Delete => write!(f, "DELETE"),
            RequestMethod::Head => write!(f, "HEAD"),
        }
    }
}

/// A request specification for registry API calls.
///
/// Instead of making actual HTTP requests, the deployment registry client
/// produces `RegistryRequest` values describing the intended API call.
/// This enables testing, offline usage, and custom transport layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRequest {
    /// HTTP method.
    pub method: RequestMethod,
    /// Full URL for the request.
    pub url: String,
    /// Request headers as key-value pairs.
    pub headers: Vec<(String, String)>,
    /// Optional request body.
    pub body: Option<Vec<u8>>,
    /// Description of what this request does (for logging).
    pub description: String,
}

impl RegistryRequest {
    /// Create a new GET request.
    pub fn get(url: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            method: RequestMethod::Get,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            description: description.into(),
        }
    }

    /// Create a new PUT request with a body.
    pub fn put(url: impl Into<String>, body: Vec<u8>, description: impl Into<String>) -> Self {
        Self {
            method: RequestMethod::Put,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body),
            description: description.into(),
        }
    }

    /// Create a new POST request with a body.
    pub fn post(url: impl Into<String>, body: Vec<u8>, description: impl Into<String>) -> Self {
        Self {
            method: RequestMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body),
            description: description.into(),
        }
    }

    /// Create a new DELETE request.
    pub fn delete(url: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            method: RequestMethod::Delete,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            description: description.into(),
        }
    }

    /// Create a new HEAD request.
    pub fn head(url: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            method: RequestMethod::Head,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            description: description.into(),
        }
    }

    /// Add a header to the request.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }
}

/// Semver version comparator for the deployment registry.
///
/// Supports common version constraint operators. This is a standalone
/// implementation for the deployment registry that does not depend on
/// the `semver` module's `Constraint` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionComparator {
    /// Exact match: `=1.2.3`
    Exact { major: u64, minor: u64, patch: u64 },
    /// Caret (compatible): `^1.2.3` — same major if major > 0
    Caret { major: u64, minor: u64, patch: u64 },
    /// Tilde (approximate): `~1.2.3` — same major.minor
    Tilde { major: u64, minor: u64, patch: u64 },
    /// Greater than or equal: `>=1.2.3`
    Gte { major: u64, minor: u64, patch: u64 },
    /// Less than: `<2.0.0`
    Lt { major: u64, minor: u64, patch: u64 },
    /// Any version
    Any,
}

impl VersionComparator {
    /// Parse a version comparator from a string.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s == "*" {
            return Ok(VersionComparator::Any);
        }

        if let Some(rest) = s.strip_prefix("^") {
            let (major, minor, patch) = parse_version_triple(rest.trim())?;
            return Ok(VersionComparator::Caret {
                major,
                minor,
                patch,
            });
        }

        if let Some(rest) = s.strip_prefix("~") {
            let (major, minor, patch) = parse_version_triple(rest.trim())?;
            return Ok(VersionComparator::Tilde {
                major,
                minor,
                patch,
            });
        }

        if let Some(rest) = s.strip_prefix(">=") {
            let (major, minor, patch) = parse_version_triple(rest.trim())?;
            return Ok(VersionComparator::Gte {
                major,
                minor,
                patch,
            });
        }

        if let Some(rest) = s.strip_prefix("<") {
            let (major, minor, patch) = parse_version_triple(rest.trim())?;
            return Ok(VersionComparator::Lt {
                major,
                minor,
                patch,
            });
        }

        if let Some(rest) = s.strip_prefix("=") {
            let (major, minor, patch) = parse_version_triple(rest.trim())?;
            return Ok(VersionComparator::Exact {
                major,
                minor,
                patch,
            });
        }

        // Bare version is exact match
        let (major, minor, patch) = parse_version_triple(s)?;
        Ok(VersionComparator::Exact {
            major,
            minor,
            patch,
        })
    }

    /// Check if a version triple matches this comparator.
    pub fn matches(&self, major: u64, minor: u64, patch: u64) -> bool {
        match self {
            VersionComparator::Exact {
                major: m,
                minor: mi,
                patch: p,
            } => major == *m && minor == *mi && patch == *p,

            VersionComparator::Caret {
                major: m,
                minor: mi,
                patch: p,
            } => {
                if *m > 0 {
                    major == *m && (minor > *mi || (minor == *mi && patch >= *p))
                } else if *mi > 0 {
                    major == 0 && minor == *mi && patch >= *p
                } else {
                    major == 0 && minor == 0 && patch == *p
                }
            }

            VersionComparator::Tilde {
                major: m,
                minor: mi,
                patch: p,
            } => major == *m && minor == *mi && patch >= *p,

            VersionComparator::Gte {
                major: m,
                minor: mi,
                patch: p,
            } => {
                if major > *m {
                    true
                } else if major == *m {
                    if minor > *mi {
                        true
                    } else if minor == *mi {
                        patch >= *p
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            VersionComparator::Lt {
                major: m,
                minor: mi,
                patch: p,
            } => {
                if major < *m {
                    true
                } else if major == *m {
                    if minor < *mi {
                        true
                    } else if minor == *mi {
                        patch < *p
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            VersionComparator::Any => true,
        }
    }

    /// Check if a version string (e.g. "1.2.3") matches this comparator.
    pub fn matches_str(&self, version: &str) -> bool {
        match parse_version_triple(version) {
            Ok((major, minor, patch)) => self.matches(major, minor, patch),
            Err(_) => false,
        }
    }
}

impl fmt::Display for VersionComparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionComparator::Exact {
                major,
                minor,
                patch,
            } => write!(f, "={}.{}.{}", major, minor, patch),
            VersionComparator::Caret {
                major,
                minor,
                patch,
            } => write!(f, "^{}.{}.{}", major, minor, patch),
            VersionComparator::Tilde {
                major,
                minor,
                patch,
            } => write!(f, "~{}.{}.{}", major, minor, patch),
            VersionComparator::Gte {
                major,
                minor,
                patch,
            } => write!(f, ">={}.{}.{}", major, minor, patch),
            VersionComparator::Lt {
                major,
                minor,
                patch,
            } => write!(f, "<{}.{}.{}", major, minor, patch),
            VersionComparator::Any => write!(f, "*"),
        }
    }
}

/// A version requirement combining multiple comparators with AND semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    /// The comparators that must all be satisfied.
    pub comparators: Vec<VersionComparator>,
}

impl VersionReq {
    /// Parse a version requirement string.
    ///
    /// Supports space-separated comparators (AND semantics):
    /// - `"^1.2.3"` — single caret comparator
    /// - `">=1.0.0 <2.0.0"` — range via AND of two comparators
    /// - `"*"` — any version
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty version requirement".to_string());
        }

        let mut comparators = Vec::new();
        let mut rest = s;

        while !rest.is_empty() {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }

            // Find where this comparator ends: at the next space followed by an operator
            let end = find_comparator_boundary(rest);
            let part = &rest[..end];
            rest = &rest[end..];

            let comp = VersionComparator::parse(part)?;
            comparators.push(comp);
        }

        if comparators.is_empty() {
            return Err("no comparators found".to_string());
        }

        Ok(VersionReq { comparators })
    }

    /// Check if a version triple matches all comparators.
    pub fn matches(&self, major: u64, minor: u64, patch: u64) -> bool {
        self.comparators
            .iter()
            .all(|c| c.matches(major, minor, patch))
    }

    /// Check if a version string matches all comparators.
    pub fn matches_str(&self, version: &str) -> bool {
        match parse_version_triple(version) {
            Ok((major, minor, patch)) => self.matches(major, minor, patch),
            Err(_) => false,
        }
    }

    /// Any version requirement.
    pub fn any() -> Self {
        VersionReq {
            comparators: vec![VersionComparator::Any],
        }
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.comparators.iter().map(|c| c.to_string()).collect();
        write!(f, "{}", parts.join(" "))
    }
}

/// Find the boundary of the current comparator in a space-separated string.
fn find_comparator_boundary(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    // Skip any leading operator characters
    while i < bytes.len()
        && (bytes[i] == b'^'
            || bytes[i] == b'~'
            || bytes[i] == b'>'
            || bytes[i] == b'<'
            || bytes[i] == b'='
            || bytes[i] == b'!')
    {
        i += 1;
    }
    // Skip the version part (digits and dots and hyphens)
    while i < bytes.len() && bytes[i] != b' ' {
        i += 1;
    }
    i
}

/// Parse a `"major.minor.patch"` triple. Ignores pre-release/build suffixes.
fn parse_version_triple(s: &str) -> Result<(u64, u64, u64), String> {
    // Strip pre-release/build suffixes
    let core = s.split('-').next().unwrap_or(s);
    let core = core.split('+').next().unwrap_or(core);

    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("expected major.minor.patch, got '{}'", s));
    }
    let major = parts[0]
        .parse::<u64>()
        .map_err(|e| format!("invalid major '{}': {}", parts[0], e))?;
    let minor = parts[1]
        .parse::<u64>()
        .map_err(|e| format!("invalid minor '{}': {}", parts[1], e))?;
    let patch = parts[2]
        .parse::<u64>()
        .map_err(|e| format!("invalid patch '{}': {}", parts[2], e))?;
    Ok((major, minor, patch))
}

/// Configuration for a deployment registry endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentRegistryConfig {
    /// Base URL for the registry API (e.g., `"https://registry.lumen.dev/api/v1"`).
    pub base_url: String,
    /// Optional API token for authenticated requests.
    pub api_token: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Whether to verify TLS certificates.
    pub verify_tls: bool,
    /// Whether to include pre-release versions in resolution.
    pub include_prereleases: bool,
}

impl DeploymentRegistryConfig {
    /// Create a new config with default settings.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_token: None,
            timeout_secs: 30,
            verify_tls: true,
            include_prereleases: false,
        }
    }

    /// Set the API token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }

    /// Set the timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set whether to verify TLS.
    pub fn with_tls_verification(mut self, verify: bool) -> Self {
        self.verify_tls = verify;
        self
    }

    /// Set whether to include prereleases.
    pub fn with_prereleases(mut self, include: bool) -> Self {
        self.include_prereleases = include;
        self
    }
}

impl Default for DeploymentRegistryConfig {
    fn default() -> Self {
        Self::new("https://registry.lumen.dev/api/v1")
    }
}

/// Metadata for a package in the deployment registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentPackageMetadata {
    /// Package name (may include scope, e.g. `"@scope/name"`).
    pub name: String,
    /// Available versions (sorted in ascending order).
    pub versions: Vec<String>,
    /// Latest stable version.
    pub latest: Option<String>,
    /// Description of the package.
    pub description: Option<String>,
    /// License identifier (SPDX).
    pub license: Option<String>,
    /// Repository URL.
    pub repository: Option<String>,
    /// Yanked version set: version -> reason.
    pub yanked: BTreeMap<String, String>,
}

impl DeploymentPackageMetadata {
    /// Create metadata for a new package.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            versions: Vec::new(),
            latest: None,
            description: None,
            license: None,
            repository: None,
            yanked: BTreeMap::new(),
        }
    }

    /// Check if a specific version is yanked.
    pub fn is_yanked(&self, version: &str) -> bool {
        self.yanked.contains_key(version)
    }

    /// Get non-yanked versions.
    pub fn available_versions(&self) -> Vec<&str> {
        self.versions
            .iter()
            .filter(|v| !self.yanked.contains_key(v.as_str()))
            .map(|v| v.as_str())
            .collect()
    }

    /// Resolve the best matching version for a requirement.
    ///
    /// Returns the highest non-yanked version that satisfies the requirement.
    pub fn resolve(&self, req: &VersionReq) -> Option<String> {
        self.available_versions()
            .into_iter()
            .rfind(|v| req.matches_str(v))
            .map(|v| v.to_string())
    }
}

/// Local cache entry for resolved package metadata.
#[derive(Debug, Clone)]
pub struct CachedPackage {
    /// The package metadata.
    pub metadata: DeploymentPackageMetadata,
    /// Unix timestamp when this entry was cached.
    pub cached_at: u64,
    /// ETag from the server (for conditional requests).
    pub etag: Option<String>,
}

/// Deployment registry client that produces request specifications.
///
/// This client does NOT make HTTP requests directly. Instead, it produces
/// `RegistryRequest` objects that describe the API calls to be made.
/// This design enables:
/// - Pure unit testing without network
/// - Custom transport layers (e.g., caching proxies)
/// - Offline operation with cached data
#[derive(Debug)]
pub struct DeploymentRegistryClient {
    config: DeploymentRegistryConfig,
    cache: BTreeMap<String, CachedPackage>,
}

impl DeploymentRegistryClient {
    /// Create a new deployment registry client.
    pub fn new(config: DeploymentRegistryConfig) -> Self {
        Self {
            config,
            cache: BTreeMap::new(),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &DeploymentRegistryConfig {
        &self.config
    }

    /// Build a request to fetch the global package index.
    pub fn fetch_index_request(&self) -> RegistryRequest {
        let url = format!("{}/index.json", self.base_url());
        let mut req = RegistryRequest::get(url, "fetch global package index");
        req = self.apply_auth(req);
        req
    }

    /// Build a request to fetch a package's metadata.
    pub fn fetch_package_request(&self, name: &str) -> RegistryRequest {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!("{}/packages/@{}/{}/index.json", self.base_url(), s, pkg)
        } else {
            format!("{}/packages/{}/index.json", self.base_url(), pkg)
        };
        let mut req = RegistryRequest::get(url, format!("fetch package metadata for '{}'", name));
        req = self.apply_auth(req);

        // Add conditional request header if we have a cached ETag
        if let Some(cached) = self.cache.get(name) {
            if let Some(ref etag) = cached.etag {
                req = req.with_header("If-None-Match", etag.clone());
            }
        }

        req
    }

    /// Build a request to fetch a specific version's metadata.
    pub fn fetch_version_request(&self, name: &str, version: &str) -> RegistryRequest {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!(
                "{}/packages/@{}/{}/{}.json",
                self.base_url(),
                s,
                pkg,
                version
            )
        } else {
            format!("{}/packages/{}/{}.json", self.base_url(), pkg, version)
        };
        let mut req = RegistryRequest::get(
            url,
            format!("fetch version metadata for '{}@{}'", name, version),
        );
        req = self.apply_auth(req);
        req
    }

    /// Build a request to download an artifact by its content hash.
    pub fn download_artifact_request(&self, cid: &str) -> Result<RegistryRequest, String> {
        let hash = if let Some(h) = cid.strip_prefix("sha256:") {
            h
        } else if let Some(h) = cid.strip_prefix("cid:sha256:") {
            h
        } else {
            return Err(format!("unsupported CID format: {}", cid));
        };

        if hash.len() < 4 {
            return Err("hash too short".to_string());
        }
        let prefix = &hash[..2];
        let rest = &hash[2..];
        let url = format!("{}/artifacts/sha256/{}/{}", self.base_url(), prefix, rest);
        let mut req = RegistryRequest::get(url, format!("download artifact {}", cid));
        req = self.apply_auth(req);
        Ok(req)
    }

    /// Build a request to publish a package version.
    pub fn publish_request(
        &self,
        name: &str,
        version: &str,
        metadata_json: Vec<u8>,
    ) -> RegistryRequest {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!(
                "{}/packages/@{}/{}/{}.json",
                self.base_url(),
                s,
                pkg,
                version
            )
        } else {
            format!("{}/packages/{}/{}.json", self.base_url(), pkg, version)
        };
        let mut req = RegistryRequest::put(
            url,
            metadata_json,
            format!("publish '{}@{}'", name, version),
        );
        req = self.apply_auth(req);
        req = req.with_header("Content-Type", "application/json");
        req
    }

    /// Build a request to yank a version.
    pub fn yank_request(&self, name: &str, version: &str) -> RegistryRequest {
        let (scope, pkg) = parse_package_name(name);
        let url = if let Some(s) = scope {
            format!(
                "{}/packages/@{}/{}/{}/yank",
                self.base_url(),
                s,
                pkg,
                version
            )
        } else {
            format!("{}/packages/{}/{}/yank", self.base_url(), pkg, version)
        };
        let mut req =
            RegistryRequest::post(url, Vec::new(), format!("yank '{}@{}'", name, version));
        req = self.apply_auth(req);
        req
    }

    /// Build a request to check if an artifact exists (HEAD request).
    pub fn artifact_exists_request(&self, cid: &str) -> Result<RegistryRequest, String> {
        let hash = if let Some(h) = cid.strip_prefix("sha256:") {
            h
        } else if let Some(h) = cid.strip_prefix("cid:sha256:") {
            h
        } else {
            return Err(format!("unsupported CID format: {}", cid));
        };

        if hash.len() < 4 {
            return Err("hash too short".to_string());
        }
        let prefix = &hash[..2];
        let rest = &hash[2..];
        let url = format!("{}/artifacts/sha256/{}/{}", self.base_url(), prefix, rest);
        let mut req = RegistryRequest::head(url, format!("check artifact exists {}", cid));
        req = self.apply_auth(req);
        Ok(req)
    }

    // =========================================================================
    // Cache operations
    // =========================================================================

    /// Insert or update a cached package.
    pub fn cache_package(&mut self, metadata: DeploymentPackageMetadata, etag: Option<String>) {
        let name = metadata.name.clone();
        self.cache.insert(
            name,
            CachedPackage {
                metadata,
                cached_at: 0, // Caller should set this from a clock
                etag,
            },
        );
    }

    /// Get a cached package if available.
    pub fn get_cached(&self, name: &str) -> Option<&CachedPackage> {
        self.cache.get(name)
    }

    /// Clear the local cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get number of cached packages.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    // =========================================================================
    // Version resolution
    // =========================================================================

    /// Resolve the best version from cached metadata.
    ///
    /// Returns `None` if the package is not cached or no version matches.
    pub fn resolve_cached(&self, name: &str, req: &VersionReq) -> Option<String> {
        self.cache
            .get(name)
            .and_then(|cached| cached.metadata.resolve(req))
    }

    /// Resolve versions for multiple packages from cache.
    ///
    /// Returns a map of package name -> resolved version for all packages
    /// that could be resolved from cache.
    pub fn resolve_all_cached(
        &self,
        requirements: &[(String, VersionReq)],
    ) -> BTreeMap<String, String> {
        let mut results = BTreeMap::new();
        for (name, req) in requirements {
            if let Some(version) = self.resolve_cached(name, req) {
                results.insert(name.clone(), version);
            }
        }
        results
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    fn base_url(&self) -> &str {
        self.config.base_url.trim_end_matches('/')
    }

    fn apply_auth(&self, req: RegistryRequest) -> RegistryRequest {
        if let Some(ref token) = self.config.api_token {
            req.with_header("Authorization", format!("Bearer {}", token))
        } else {
            req
        }
    }
}

use std::fmt;
