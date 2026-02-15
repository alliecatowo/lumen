use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use chrono::{DateTime, Utc};

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
    #[serde(default)]
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

/// Package signature (Unified).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSignature {
    /// Package name
    pub package_name: String,
    /// Package version
    pub version: String,
    /// Content hash (SHA-256 of the package archive)
    pub content_hash: String,
    /// Signature (base64)
    pub signature: String,
    /// Certificate used to sign
    pub certificate: EphemeralCertificate,
    /// Timestamp of signing
    pub signed_at: DateTime<Utc>,
    /// Transparency log entry ID
    pub transparency_log_index: Option<u64>,
    /// Optional build provenance (SLSA)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<SlsaProvenance>,
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
// Trust / Identity Types (moved from trust.rs)
// =============================================================================

/// OIDC identity claims from the identity provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityClaims {
    /// Subject (user ID or workflow ref)
    pub sub: String,
    /// Issuer (e.g., https://github.com)
    pub iss: String,
    /// Audience (our registry)
    pub aud: String,
    /// Email (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Name (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Repository (for GitHub Actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Workflow reference (for GitHub Actions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_ref: Option<String>,
    /// Event name (push, pull_request, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
}

impl IdentityClaims {
    /// Get the identity string for display.
    pub fn identity(&self) -> String {
        if let Some(repo) = &self.repository {
            if let Some(workflow) = &self.workflow_ref {
                return format!("{}/{}", repo, workflow);
            }
            return repo.clone();
        }
        self.sub.clone()
    }
}

/// A short-lived signing certificate issued by the Fulcio-like CA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralCertificate {
    /// Certificate ID (UUID)
    pub cert_id: String,
    /// PEM-encoded certificate
    pub certificate_pem: String,
    /// Public key (base64)
    pub public_key: String,
    /// Identity claims that were verified
    pub identity: IdentityClaims,
    /// Certificate validity start
    pub not_before: DateTime<Utc>,
    /// Certificate validity end
    pub not_after: DateTime<Utc>,
    /// Transparency log index where this cert is recorded
    pub log_index: Option<u64>,
}

impl EphemeralCertificate {
    /// Check if the certificate is still valid.
    pub fn is_valid(&self) -> bool {
        let now = Utc::now();
        self.not_before <= now && now < self.not_after
    }

    /// Get remaining validity duration.
    pub fn remaining(&self) -> chrono::Duration {
        let now = Utc::now();
        if now >= self.not_after {
            chrono::Duration::zero()
        } else {
            self.not_after.signed_duration_since(now)
        }
    }

    /// Get the identity string.
    pub fn identity_str(&self) -> String {
        self.identity.identity()
    }
}

// =============================================================================
// SLSA Provenance
// =============================================================================

/// SLSA build provenance attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlsaProvenance {
    /// SLSA version (always "v1")
    pub slsa_version: String,
    /// Build type (e.g., "https://github.com/slsa-framework/slsa-github-generator/container@v1")
    pub build_type: String,
    /// Builder ID (the trusted build system)
    pub builder_id: String,
    /// Build invocation metadata
    pub invocation: BuildInvocation,
    /// Source repository
    pub source: SourceInfo,
    /// Build metadata
    pub metadata: BuildMetadata,
}

/// Build invocation details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInvocation {
    /// Config source (e.g., workflow file)
    pub config_source: ConfigSource,
    /// Environment variables (sanitized)
    pub environment: HashMap<String, String>,
}

/// Config source for build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSource {
    /// URI to the config file
    pub uri: String,
    /// Digest of the config file
    pub digest: HashMap<String, String>,
    /// Entry point (e.g., workflow name)
    pub entry_point: String,
}

/// Source repository info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Source URI
    pub uri: String,
    /// Git digest (commit SHA)
    pub digest: HashMap<String, String>,
}

/// Build metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    /// Build start time
    pub started_on: DateTime<Utc>,
    /// Build completion time
    pub finished_on: DateTime<Utc>,
}

// =============================================================================
// Auth / Login Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginSession {
    pub session_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub identity: String,
    pub expires_in: u64,
}

// =============================================================================
// Identity Provider Enum
// =============================================================================

/// Supported OIDC identity providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentityProvider {
    GitHub,
    GitLab,
    Google,
}

impl IdentityProvider {
    /// Get the OIDC issuer URL for this provider.
    pub fn issuer(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://token.actions.githubusercontent.com",
            IdentityProvider::GitLab => "https://gitlab.com",
            IdentityProvider::Google => "https://accounts.google.com",
        }
    }

    /// Get the human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "GitHub",
            IdentityProvider::GitLab => "GitLab",
            IdentityProvider::Google => "Google",
        }
    }

    /// Get the OAuth authorization URL.
    pub fn auth_url(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://github.com/login/oauth/authorize",
            IdentityProvider::GitLab => "https://gitlab.com/oauth/authorize",
            IdentityProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
        }
    }

    /// Get the OAuth token URL.
    pub fn token_url(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://github.com/login/oauth/access_token",
            IdentityProvider::GitLab => "https://gitlab.com/oauth/token",
            IdentityProvider::Google => "https://oauth2.googleapis.com/token",
        }
    }
}

impl std::str::FromStr for IdentityProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(IdentityProvider::GitHub),
            "gitlab" => Ok(IdentityProvider::GitLab),
            "google" => Ok(IdentityProvider::Google),
            _ => Err(format!("Unknown identity provider: {}", s)),
        }
    }
}

// =============================================================================
// Trust Configuration (Data)
// =============================================================================

/// Stored OIDC credentials for wares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcCredentials {
    /// Identity provider
    pub provider: IdentityProvider,
    /// Access token (for immediate use)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// Refresh token (encrypted) - may not be provided by all OAuth providers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Identity string
    pub identity: String,
    /// When obtained
    pub obtained_at: DateTime<Utc>,
    /// When expires
    pub expires_at: DateTime<Utc>,
}

/// Trust configuration stored locally.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustConfig {
    pub version: i32,
    /// OIDC credentials per registry
    pub oidc_credentials: HashMap<String, OidcCredentials>,
    /// Trust policies per registry
    pub policies: HashMap<String, TrustPolicy>,
    /// Cached ephemeral certificates
    pub cached_certs: Vec<EphemeralCertificate>,
}

// =============================================================================
// Trust Policy
// =============================================================================

/// Policy for verifying packages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustPolicy {
    /// Required identity pattern (regex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_identity: Option<String>,
    /// Minimum SLSA build level (0-3)
    #[serde(default)]
    pub min_slsa_level: u8,
    /// Require transparency log inclusion
    #[serde(default = "default_true")]
    pub require_transparency_log: bool,
    /// Minimum package age before trust (cooldown period)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_package_age: Option<String>,
    /// Allowed identity providers
    #[serde(default)]
    pub allowed_providers: Vec<String>,
    /// Block install scripts by default
    #[serde(default = "default_true")]
    pub block_install_scripts: bool,
}

fn default_true() -> bool {
    true
}

impl TrustPolicy {
    /// Default permissive policy.
    pub fn permissive() -> Self {
        Self {
            require_transparency_log: false,
            ..Default::default()
        }
    }

    /// Strict policy for high-security environments.
    pub fn strict() -> Self {
        Self {
            required_identity: Some("^https://github.com/[^/]+/[^/]+/.github/workflows/.*$".to_string()),
            min_slsa_level: 2,
            require_transparency_log: true,
            min_package_age: Some("24h".to_string()),
            allowed_providers: vec!["github".to_string()],
            block_install_scripts: true,
        }
    }
}

/// Result of policy verification.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub passed: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub identity: Option<String>,
    pub slsa_level: u8,
    pub log_index: Option<u64>,
}

impl VerificationResult {
    pub fn success(identity: String, slsa_level: u8, log_index: Option<u64>) -> Self {
        Self {
            passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            identity: Some(identity),
            slsa_level,
            log_index,
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            passed: false,
            warnings: Vec::new(),
            errors: vec![error],
            identity: None,
            slsa_level: 0,
            log_index: None,
        }
    }

    pub fn add_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    pub fn add_error(&mut self, msg: String) {
        self.errors.push(msg);
        self.passed = false;
    }
}

// =============================================================================
// Log Types (Added)
// =============================================================================

/// Transparency log entry from query.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEntry {
    /// Log index.
    pub log_index: u64,
    /// The signed entry.
    pub entry: PackageSignature,
    /// Timestamp of inclusion.
    pub timestamp: DateTime<Utc>,
}

/// Response from log query.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEntryResponse {
    /// Matching entries.
    pub entries: Vec<LogEntry>,
}
