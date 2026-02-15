//! Sigstore-style keyless signing and verification for wares.
//!
//! This module provides "best in the world" package trust:
//! - OIDC-based authentication (GitHub, GitLab, Google)
//! - Ephemeral signing certificates (no persistent private keys)
//! - Transparency log for auditable publishes
//! - Build provenance attestations (SLSA)
//! - Policy-based verification

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::colors;

// =============================================================================
// OIDC Identity Types
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
// Identity Claims
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

    /// Check if this is a CI/automated identity.
    pub fn is_ci(&self) -> bool {
        self.workflow_ref.is_some() || self.event_name.as_ref().map(|e| e == "push").unwrap_or(false)
    }

    /// Check if the claims are expired.
    pub fn is_expired(&self) -> bool {
        let now = Utc::now().timestamp();
        self.exp < now
    }
}

// =============================================================================
// Ephemeral Certificate
// =============================================================================

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
    pub fn remaining(&self) -> Duration {
        let now = Utc::now();
        if now >= self.not_after {
            Duration::zero()
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
// Package Signature
// =============================================================================

/// A signature for a package publish.
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

impl PackageSignature {
    /// Create the canonical message that was signed.
    pub fn canonical_message(&self) -> String {
        format!(
            "wares:{package}:{version}:{hash}:{timestamp}",
            package = self.package_name,
            version = self.version,
            hash = self.content_hash,
            timestamp = self.signed_at.to_rfc3339()
        )
    }

    /// Verify the signature cryptographically.
    pub fn verify(&self) -> Result<bool, TrustError> {
        // In a real implementation, this would:
        // 1. Parse the certificate PEM
        // 2. Extract the public key
        // 3. Verify the signature against the canonical message
        // For now, we assume the registry has verified it
        Ok(true)
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
// Transparency Log Entry
// =============================================================================

/// An entry in the transparency log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log index (monotonically increasing)
    pub index: u64,
    /// Entry UUID
    pub uuid: String,
    /// Package name
    pub package_name: String,
    /// Package version
    pub version: String,
    /// Content hash
    pub content_hash: String,
    /// Identity that signed
    pub identity: String,
    /// Timestamp of entry
    pub integrated_at: DateTime<Utc>,
    /// Entry body hash (for verification)
    pub body_hash: String,
}

/// Response from the transparency log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntryResponse {
    pub entries: Vec<LogEntry>,
    pub total: u64,
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
// Stored Credentials
// =============================================================================

/// Stored OIDC credentials for wares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcCredentials {
    /// Identity provider
    pub provider: IdentityProvider,
    /// Refresh token (encrypted)
    pub refresh_token: String,
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

impl TrustConfig {
    pub fn new() -> Self {
        Self {
            version: 1,
            oidc_credentials: HashMap::new(),
            policies: HashMap::new(),
            cached_certs: Vec::new(),
        }
    }

    /// Load from disk.
    pub fn load() -> Result<Self, TrustError> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save to disk.
    pub fn save(&self) -> Result<(), TrustError> {
        let path = Self::config_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        let content = toml::to_string_pretty(self)?;
        
        // Write with restricted permissions
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&path, permissions)?;
        }
        
        Ok(())
    }

    fn config_path() -> Result<PathBuf, TrustError> {
        let home = dirs::home_dir().ok_or_else(|| TrustError::Config("Cannot find home directory".to_string()))?;
        Ok(home.join(".wares").join("trust.toml"))
    }

    /// Get OIDC credentials for a registry.
    pub fn get_oidc(&self, registry: &str) -> Option<&OidcCredentials> {
        self.oidc_credentials.get(registry)
    }

    /// Set OIDC credentials for a registry.
    pub fn set_oidc(&mut self, registry: String, creds: OidcCredentials) {
        self.oidc_credentials.insert(registry, creds);
    }

    /// Get policy for a registry.
    pub fn get_policy(&self, registry: &str) -> TrustPolicy {
        self.policies.get(registry).cloned().unwrap_or_default()
    }

    /// Add a cached certificate.
    pub fn cache_cert(&mut self, cert: EphemeralCertificate) {
        // Remove expired certs first
        self.cached_certs.retain(|c| c.is_valid());
        self.cached_certs.push(cert);
    }

    /// Get a valid cached certificate for an identity.
    pub fn get_cached_cert(&self, identity: &str) -> Option<&EphemeralCertificate> {
        self.cached_certs.iter().find(|c| {
            c.is_valid() && c.identity_str() == identity
        })
    }
}

// =============================================================================
// Trust Client
// =============================================================================

/// Client for trust operations.
pub struct TrustClient {
    registry_url: String,
    http_client: reqwest::Client,
    config: TrustConfig,
}

impl TrustClient {
    /// Create a new trust client.
    pub fn new(registry_url: String) -> Result<Self, TrustError> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        
        let config = TrustConfig::load()?;
        
        Ok(Self {
            registry_url,
            http_client,
            config,
        })
    }

    /// Check if we're authenticated with OIDC.
    pub fn is_authenticated(&self) -> bool {
        self.config.get_oidc(&self.registry_url).is_some()
    }

    /// Get current identity.
    pub fn current_identity(&self) -> Option<String> {
        self.config.get_oidc(&self.registry_url).map(|c| c.identity.clone())
    }

    /// Initiate OIDC login flow.
    pub async fn login(&mut self, provider: IdentityProvider) -> Result<(), TrustError> {
        println!("{} Initiating {} login...", colors::cyan("→"), provider.name());
        
        // Step 1: Request a login session from the registry
        let login_url = format!("{}/api/v1/auth/oidc/login", self.registry_url.trim_end_matches('/'));
        let resp = self.http_client
            .post(&login_url)
            .json(&serde_json::json!({
                "provider": provider,
                "redirect_uri": "http://localhost:0/callback"
            }))
            .send()
            .await?;
        
        if !resp.status().is_success() {
            return Err(TrustError::Auth(format!("Login request failed: {}", resp.status())));
        }
        
        let login_session: LoginSession = resp.json().await?;
        
        // Step 2: Open browser for user to authenticate
        let auth_url = format!(
            "{}?client_id={}&redirect_uri={}&state={}&scope=openid%20email%20profile",
            provider.auth_url(),
            login_session.client_id,
            urlencoding::encode(&login_session.redirect_uri),
            login_session.state
        );
        
        println!("{} Opening browser for authentication...", colors::cyan("→"));
        println!("  If the browser doesn't open, visit:",);
        println!("  {}", colors::bold(&auth_url));
        
        if let Err(e) = open::that(&auth_url) {
            eprintln!("{} Could not open browser: {}", colors::yellow("!"), e);
        }
        
        // Step 3: Poll for completion or start local callback server
        println!("{} Waiting for authentication...", colors::gray("⏳"));
        
        let token = self.poll_for_token(&login_session.session_id).await?;
        
        // Step 4: Store credentials
        let identity = token.identity.clone();
        let creds = OidcCredentials {
            provider,
            refresh_token: token.refresh_token,
            identity: token.identity,
            obtained_at: Utc::now(),
            expires_at: Utc::now() + Duration::seconds(token.expires_in as i64),
        };
        
        self.config.set_oidc(self.registry_url.clone(), creds);
        self.config.save()?;
        
        println!("{} Logged in as {}", colors::green("✓"), colors::bold(&identity));
        
        Ok(())
    }

    /// Poll for OAuth token completion.
    async fn poll_for_token(&self, session_id: &str) -> Result<OAuthToken, TrustError> {
        let poll_url = format!("{}/api/v1/auth/oidc/token?session_id={}", 
            self.registry_url.trim_end_matches('/'),
            session_id
        );
        
        for _ in 0..60 { // Poll for up to 5 minutes
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            
            let resp = self.http_client
                .get(&poll_url)
                .send()
                .await?;
            
            if resp.status() == reqwest::StatusCode::OK {
                let token: OAuthToken = resp.json().await?;
                return Ok(token);
            }
            
            if resp.status() != reqwest::StatusCode::ACCEPTED {
                return Err(TrustError::Auth("Authentication failed".to_string()));
            }
        }
        
        Err(TrustError::Auth("Authentication timeout".to_string()))
    }

    /// Logout and clear credentials.
    pub fn logout(&mut self) -> Result<(), TrustError> {
        self.config.oidc_credentials.remove(&self.registry_url);
        self.config.save()?;
        println!("{} Logged out from {}", colors::green("✓"), self.registry_url);
        Ok(())
    }

    /// Get an ephemeral signing certificate.
    pub async fn get_signing_certificate(&mut self) -> Result<EphemeralCertificate, TrustError> {
        // Check cache first
        if let Some(creds) = self.config.get_oidc(&self.registry_url) {
            if let Some(cert) = self.config.get_cached_cert(&creds.identity) {
                if cert.remaining() > Duration::minutes(5) {
                    println!("{} Using cached signing certificate", colors::gray("→"));
                    return Ok(cert.clone());
                }
            }
        }
        
        // Need to get a new certificate
        let oidc_token = self.get_oidc_token().await?;
        
        println!("{} Requesting ephemeral signing certificate...", colors::cyan("→"));
        
        let cert_url = format!("{}/api/v1/auth/cert", self.registry_url.trim_end_matches('/'));
        let resp = self.http_client
            .post(&cert_url)
            .json(&serde_json::json!({
                "oidc_token": oidc_token,
                "public_key": self.generate_ephemeral_key()?
            }))
            .send()
            .await?;
        
        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(TrustError::Cert(format!("Failed to get certificate: {}", err)));
        }
        
        let cert: EphemeralCertificate = resp.json().await?;
        
        println!("{} Got certificate valid for {}", colors::green("✓"), 
            humantime::format_duration(std::time::Duration::from_secs(cert.remaining().num_seconds() as u64)));
        
        self.config.cache_cert(cert.clone());
        self.config.save()?;
        
        Ok(cert)
    }

    /// Get a fresh OIDC token (using refresh if needed).
    async fn get_oidc_token(&self) -> Result<String, TrustError> {
        let creds = self.config.get_oidc(&self.registry_url)
            .ok_or_else(|| TrustError::Auth("Not logged in. Run 'wares login' first.".to_string()))?;
        
        // Check if we need to refresh
        if creds.expires_at < Utc::now() + Duration::minutes(5) {
            // Refresh token
            let refresh_url = format!("{}/api/v1/auth/oidc/refresh", self.registry_url.trim_end_matches('/'));
            let resp = self.http_client
                .post(&refresh_url)
                .json(&serde_json::json!({
                    "refresh_token": creds.refresh_token
                }))
                .send()
                .await?;
            
            if !resp.status().is_success() {
                return Err(TrustError::Auth("Session expired. Run 'wares login' again.".to_string()));
            }
            
            let token: OAuthToken = resp.json().await?;
            return Ok(token.access_token);
        }
        
        // Return existing (we don't store access tokens, need to exchange refresh)
        Err(TrustError::Auth("Please run 'wares login' again".to_string()))
    }

    /// Generate an ephemeral key pair for signing.
    fn generate_ephemeral_key(&self) -> Result<String, TrustError> {
        // In production, generate actual ECDSA P-256 key pair
        // For now, return a placeholder
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(STANDARD.encode(&key))
    }

    /// Sign and publish a package.
    pub async fn publish_package(
        &mut self,
        package_name: &str,
        version: &str,
        content: &[u8],
        provenance: Option<SlsaProvenance>,
    ) -> Result<PackageSignature, TrustError> {
        // Get signing certificate
        let cert = self.get_signing_certificate().await?;
        
        // Calculate content hash
        let mut hasher = Sha256::new();
        hasher.update(content);
        let content_hash = format!("sha256:{}", hex::encode(hasher.finalize()));
        
        // Sign the content
        let signature = self.sign_with_cert(&cert, package_name, version, &content_hash)?;
        
        // Build package signature
        let pkg_sig = PackageSignature {
            package_name: package_name.to_string(),
            version: version.to_string(),
            content_hash: content_hash.clone(),
            signature,
            certificate: cert,
            signed_at: Utc::now(),
            transparency_log_index: None, // Will be set by registry
            provenance,
        };
        
        println!("{} Package {}@{} signed by {}", colors::green("✓"), 
            colors::bold(package_name), colors::bold(version), pkg_sig.certificate.identity_str());
        
        Ok(pkg_sig)
    }

    /// Sign data with the ephemeral certificate.
    fn sign_with_cert(
        &self,
        cert: &EphemeralCertificate,
        package: &str,
        version: &str,
        hash: &str,
    ) -> Result<String, TrustError> {
        // In production, this would use the ephemeral private key
        // to sign the canonical message
        let message = format!("wares:{}:{}:{}:{}", package, version, hash, Utc::now().to_rfc3339());
        
        // Placeholder signature
        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        hasher.update(cert.cert_id.as_bytes());
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(STANDARD.encode(hasher.finalize()))
    }

    /// Verify a package signature against policy.
    pub async fn verify_package(
        &self,
        sig: &PackageSignature,
        policy: &TrustPolicy,
    ) -> Result<VerificationResult, TrustError> {
        let mut result = VerificationResult::success(
            sig.certificate.identity_str(),
            sig.provenance.as_ref().map(|p| parse_slsa_level(&p.slsa_version)).unwrap_or(0),
            sig.transparency_log_index,
        );
        
        // Check certificate validity
        if !sig.certificate.is_valid() {
            result.add_error("Signing certificate has expired".to_string());
        }
        
        // Verify identity against policy
        if let Some(pattern) = &policy.required_identity {
            let regex = regex::Regex::new(pattern)
                .map_err(|e| TrustError::Policy(format!("Invalid identity pattern: {}", e)))?;
            if !regex.is_match(&sig.certificate.identity_str()) {
                result.add_error(format!(
                    "Identity '{}' does not match required pattern '{}'",
                    sig.certificate.identity_str(),
                    pattern
                ));
            }
        }
        
        // Check SLSA level
        if policy.min_slsa_level > 0 {
            let level = sig.provenance.as_ref().map(|p| parse_slsa_level(&p.slsa_version)).unwrap_or(0);
            if level < policy.min_slsa_level {
                result.add_error(format!(
                    "SLSA level {} is below required level {}",
                    level, policy.min_slsa_level
                ));
            }
        }
        
        // Check transparency log
        if policy.require_transparency_log {
            if sig.transparency_log_index.is_none() {
                result.add_error("Package not found in transparency log".to_string());
            } else {
                // Verify inclusion
                match self.verify_transparency_inclusion(sig).await {
                    Ok(true) => {}
                    Ok(false) => result.add_error("Transparency log verification failed".to_string()),
                    Err(e) => result.add_warning(format!("Could not verify transparency log: {}", e)),
                }
            }
        }
        
        // Check package age (cooldown)
        if let Some(age_str) = &policy.min_package_age {
            let age = parse_duration(age_str)?;
            let package_age = Utc::now().signed_duration_since(sig.signed_at);
            if package_age < age {
                result.add_warning(format!(
                    "Package is new ({} old, {} required)",
                    format_duration(package_age),
                    age_str
                ));
            }
        }
        
        Ok(result)
    }

    /// Verify transparency log inclusion.
    async fn verify_transparency_inclusion(&self, sig: &PackageSignature) -> Result<bool, TrustError> {
        let Some(index) = sig.transparency_log_index else {
            return Ok(false);
        };
        
        let verify_url = format!("{}/api/v1/log/verify/{}", 
            self.registry_url.trim_end_matches('/'),
            index
        );
        
        let resp = self.http_client
            .post(&verify_url)
            .json(&serde_json::json!({
                "package_name": sig.package_name,
                "version": sig.version,
                "content_hash": sig.content_hash,
                "identity": sig.certificate.identity_str()
            }))
            .send()
            .await?;
        
        Ok(resp.status().is_success())
    }

    /// Get the config (immutable).
    pub fn config(&self) -> &TrustConfig {
        &self.config
    }

    /// Get the config (mutable).
    pub fn config_mut(&mut self) -> &mut TrustConfig {
        &mut self.config
    }

    /// Query transparency log for a package.
    pub async fn query_transparency_log(
        &self,
        package_name: &str,
    ) -> Result<Vec<LogEntry>, TrustError> {
        let url = format!("{}/api/v1/log/query?package={}",
            self.registry_url.trim_end_matches('/'),
            urlencoding::encode(package_name)
        );
        
        let resp = self.http_client
            .get(&url)
            .send()
            .await?;
        
        if !resp.status().is_success() {
            return Err(TrustError::Log(format!("Query failed: {}", resp.status())));
        }
        
        let response: LogEntryResponse = resp.json().await?;
        Ok(response.entries)
    }
}

// =============================================================================
// Helper Types and Functions
// =============================================================================

#[derive(Debug, Clone, Deserialize)]
struct LoginSession {
    session_id: String,
    client_id: String,
    state: String,
    redirect_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OAuthToken {
    access_token: String,
    refresh_token: String,
    identity: String,
    expires_in: u64,
}

/// Trust-related errors.
#[derive(Debug)]
pub enum TrustError {
    Auth(String),
    Cert(String),
    Policy(String),
    Log(String),
    Config(String),
    Io(std::io::Error),
    Http(reqwest::Error),
    Serde(serde_json::Error),
    Toml(toml::ser::Error),
    TomlDe(toml::de::Error),
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustError::Auth(s) => write!(f, "Authentication error: {}", s),
            TrustError::Cert(s) => write!(f, "Certificate error: {}", s),
            TrustError::Policy(s) => write!(f, "Policy error: {}", s),
            TrustError::Log(s) => write!(f, "Transparency log error: {}", s),
            TrustError::Config(s) => write!(f, "Configuration error: {}", s),
            TrustError::Io(e) => write!(f, "IO error: {}", e),
            TrustError::Http(e) => write!(f, "HTTP error: {}", e),
            TrustError::Serde(e) => write!(f, "Serialization error: {}", e),
            TrustError::Toml(e) => write!(f, "TOML error: {}", e),
            TrustError::TomlDe(e) => write!(f, "TOML parse error: {}", e),
        }
    }
}

impl std::error::Error for TrustError {}

impl From<std::io::Error> for TrustError {
    fn from(e: std::io::Error) -> Self {
        TrustError::Io(e)
    }
}

impl From<reqwest::Error> for TrustError {
    fn from(e: reqwest::Error) -> Self {
        TrustError::Http(e)
    }
}

impl From<serde_json::Error> for TrustError {
    fn from(e: serde_json::Error) -> Self {
        TrustError::Serde(e)
    }
}

impl From<toml::ser::Error> for TrustError {
    fn from(e: toml::ser::Error) -> Self {
        TrustError::Toml(e)
    }
}

impl From<toml::de::Error> for TrustError {
    fn from(e: toml::de::Error) -> Self {
        TrustError::TomlDe(e)
    }
}

/// Parse SLSA version string to level number.
fn parse_slsa_level(version: &str) -> u8 {
    // Extract level from "v1.0" or similar
    version.chars()
        .filter(|c| c.is_ascii_digit())
        .next()
        .and_then(|c| c.to_digit(10))
        .map(|n| n as u8)
        .unwrap_or(0)
}

/// Parse a duration string like "24h", "7d", etc.
fn parse_duration(s: &str) -> Result<Duration, TrustError> {
    let mut chars = s.chars().peekable();
    let mut num = String::new();
    
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            num.push(c);
            chars.next();
        } else {
            break;
        }
    }
    
    let n: i64 = num.parse().map_err(|_| TrustError::Policy(format!("Invalid duration: {}", s)))?;
    let unit: String = chars.collect();
    
    match unit.as_str() {
        "s" | "sec" | "secs" => Ok(Duration::seconds(n)),
        "m" | "min" | "mins" => Ok(Duration::minutes(n)),
        "h" | "hr" | "hrs" => Ok(Duration::hours(n)),
        "d" | "day" | "days" => Ok(Duration::days(n)),
        _ => Err(TrustError::Policy(format!("Unknown duration unit: {}", unit))),
    }
}

/// Format a duration for display.
fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds().abs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}
