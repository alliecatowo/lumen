use super::types::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
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

// Helpers

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
