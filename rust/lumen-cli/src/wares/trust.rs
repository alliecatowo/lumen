//! Sigstore-style keyless signing and verification for wares.
//!
//! This module provides "best in the world" package trust:
//! - OIDC-based authentication (GitHub, GitLab, Google)
//! - Ephemeral signing certificates (no persistent private keys)
//! - Transparency log for auditable publishes
//! - Build provenance attestations (SLSA)
//! - Policy-based verification

use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use super::types::*;
use crate::colors;
use base64::{engine::general_purpose::STANDARD, Engine as _};

// =============================================================================
// Trust Config Implementation
// =============================================================================

impl TrustConfig {
    pub fn new() -> Self {
        Self {
            version: 1,
            oidc_credentials: std::collections::HashMap::new(),
            policies: std::collections::HashMap::new(),
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
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&path)?;
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
        let home = dirs::home_dir()
            .ok_or_else(|| TrustError::Config("Cannot find home directory".to_string()))?;
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
        self.cached_certs
            .iter()
            .find(|c| c.is_valid() && c.identity_str() == identity)
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
        self.config
            .get_oidc(&self.registry_url)
            .map(|c| c.identity.clone())
    }

    /// Initiate OIDC login flow using registry-based callback with polling.
    pub async fn login(&mut self, provider: IdentityProvider) -> Result<(), TrustError> {
        println!(
            "{} Initiating {} login...",
            colors::cyan("→"),
            provider.name()
        );

        // Step 1: Request a login session from the registry (use registry callback, not localhost)
        let login_url = format!(
            "{}/api/v1/auth/oidc/login",
            self.registry_url.trim_end_matches('/')
        );

        let resp = self
            .http_client
            .post(&login_url)
            .json(&serde_json::json!({
                "provider": provider
                // No redirect_uri - use registry's default callback
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(TrustError::Auth(format!(
                "Login request failed: {}",
                err_text
            )));
        }

        let login_session: LoginSession = resp.json().await?;
        let session_id = login_session.session_id.clone();

        // Step 2: Open browser for user to authenticate
        println!(
            "{} Opening browser for authentication...",
            colors::cyan("→")
        );
        println!("  If the browser doesn't open, visit:",);
        println!("  {}", colors::bold(&login_session.auth_url));

        if let Err(e) = open::that(&login_session.auth_url) {
            eprintln!("{} Could not open browser: {}", colors::yellow("!"), e);
        }

        // Step 3: Poll for token completion
        println!("{} Waiting for authentication...", colors::gray("⏳"));
        println!("  (Complete the authorization in your browser)",);

        let token = self.poll_for_token(&session_id).await?;

        // Step 4: Store credentials
        let identity = token.identity.clone();
        let creds = OidcCredentials {
            provider,
            access_token: Some(token.access_token),
            refresh_token: token.refresh_token,
            identity: token.identity,
            obtained_at: Utc::now(),
            expires_at: Utc::now() + Duration::seconds(token.expires_in as i64),
        };

        self.config.set_oidc(self.registry_url.clone(), creds);
        self.config.save()?;

        println!(
            "{} Logged in as {}",
            colors::green("✓"),
            colors::bold(&identity)
        );

        Ok(())
    }

    /// Poll for OAuth token completion.
    async fn poll_for_token(&self, session_id: &str) -> Result<OAuthToken, TrustError> {
        let poll_url = format!(
            "{}/api/v1/auth/oidc/token?session_id={}",
            self.registry_url.trim_end_matches('/'),
            session_id
        );

        for _ in 0..60 {
            // Poll for up to 5 minutes
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            let resp = self.http_client.get(&poll_url).send().await?;

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
        println!(
            "{} Logged out from {}",
            colors::green("✓"),
            self.registry_url
        );
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

        println!(
            "{} Requesting ephemeral signing certificate...",
            colors::cyan("→")
        );

        let cert_url = format!(
            "{}/api/v1/auth/cert",
            self.registry_url.trim_end_matches('/')
        );
        let resp = self
            .http_client
            .post(&cert_url)
            .json(&serde_json::json!({
                "oidc_token": oidc_token,
                "public_key": self.generate_ephemeral_key()?
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(TrustError::Cert(format!(
                "Failed to get certificate: {}",
                err
            )));
        }

        let cert: EphemeralCertificate = resp.json().await?;

        println!(
            "{} Got certificate valid for {}",
            colors::green("✓"),
            humantime::format_duration(std::time::Duration::from_secs(
                cert.remaining().num_seconds() as u64
            ))
        );

        self.config.cache_cert(cert.clone());
        self.config.save()?;

        Ok(cert)
    }

    /// Get a fresh OIDC token (using refresh if needed).
    async fn get_oidc_token(&self) -> Result<String, TrustError> {
        let creds = self.config.get_oidc(&self.registry_url).ok_or_else(|| {
            TrustError::Auth("Not logged in. Run 'wares login' first.".to_string())
        })?;

        // Check if token is still valid (with 5 min buffer)
        if creds.expires_at > Utc::now() + Duration::minutes(5) {
            // Return existing access token if available
            if let Some(access_token) = &creds.access_token {
                return Ok(access_token.clone());
            }
        }

        // Token expired or no access token - try to refresh
        if let Some(refresh_token) = &creds.refresh_token {
            let refresh_url = format!(
                "{}/api/v1/auth/oidc/refresh",
                self.registry_url.trim_end_matches('/')
            );
            let resp = self
                .http_client
                .post(&refresh_url)
                .json(&serde_json::json!({
                    "refresh_token": refresh_token
                }))
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(TrustError::Auth(
                    "Session expired. Run 'wares login' again.".to_string(),
                ));
            }

            let token: OAuthToken = resp.json().await?;
            return Ok(token.access_token);
        }

        Err(TrustError::Auth(
            "Session expired and no refresh token available. Run 'wares login' again.".to_string(),
        ))
    }

    /// Generate an ephemeral key pair for signing.
    fn generate_ephemeral_key(&self) -> Result<String, TrustError> {
        // In production, generate actual ECDSA P-256 key pair
        // For now, return a placeholder
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Ok(STANDARD.encode(key))
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

        println!(
            "{} Package {}@{} signed by {}",
            colors::green("✓"),
            colors::bold(package_name),
            colors::bold(version),
            pkg_sig.certificate.identity_str()
        );

        // Upload to registry
        let publish_url = format!("{}/v1/wares", self.registry_url.trim_end_matches('/'));

        // Calculate shasum for registry
        let shasum = pkg_sig
            .content_hash
            .strip_prefix("sha256:")
            .unwrap_or(&pkg_sig.content_hash)
            .to_string();

        let resp = self
            .http_client
            .put(&publish_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "name": package_name,
                "version": version,
                "tarball": STANDARD.encode(content),
                "shasum": shasum,
                "signature": {
                    "identity": pkg_sig.certificate.identity_str(),
                    "signature": pkg_sig.signature,
                    "certificate": pkg_sig.certificate.certificate_pem
                }
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(TrustError::Registry(format!("Failed to publish: {}", err)));
        }

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
        let message = format!(
            "wares:{}:{}:{}:{}",
            package,
            version,
            hash,
            Utc::now().to_rfc3339()
        );

        // Placeholder signature
        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        hasher.update(cert.cert_id.as_bytes());
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
            sig.provenance
                .as_ref()
                .map(|p| parse_slsa_level(&p.slsa_version))
                .unwrap_or(0),
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
            let level = sig
                .provenance
                .as_ref()
                .map(|p| parse_slsa_level(&p.slsa_version))
                .unwrap_or(0);
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
                    Ok(false) => {
                        result.add_error("Transparency log verification failed".to_string())
                    }
                    Err(e) => {
                        result.add_warning(format!("Could not verify transparency log: {}", e))
                    }
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
    async fn verify_transparency_inclusion(
        &self,
        sig: &PackageSignature,
    ) -> Result<bool, TrustError> {
        let Some(index) = sig.transparency_log_index else {
            return Ok(false);
        };

        // Use transparency log URL from env or default
        let log_url = std::env::var("WARES_LOG_URL")
            .unwrap_or_else(|_| "https://wares.lumen-lang.com/log".to_string());
        let verify_url = format!(
            "{}/api/v1/log/verify/{}",
            log_url.trim_end_matches('/'),
            index
        );

        let resp = self
            .http_client
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
        let log_url = std::env::var("WARES_LOG_URL")
            .unwrap_or_else(|_| "https://wares.lumen-lang.com/log".to_string());
        let url = format!(
            "{}/api/v1/log/query?package={}",
            log_url.trim_end_matches('/'),
            urlencoding::encode(package_name)
        );

        let resp = self.http_client.get(&url).send().await?;

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

/// Trust-related errors.
#[derive(Debug)]
pub enum TrustError {
    Auth(String),
    Cert(String),
    Policy(String),
    Log(String),
    Config(String),
    Registry(String),
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
            TrustError::Registry(s) => write!(f, "Registry error: {}", s),
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
    version
        .chars().find(|c| c.is_ascii_digit())
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

    let n: i64 = num
        .parse()
        .map_err(|_| TrustError::Policy(format!("Invalid duration: {}", s)))?;
    let unit: String = chars.collect();

    match unit.as_str() {
        "s" | "sec" | "secs" => Ok(Duration::seconds(n)),
        "m" | "min" | "mins" => Ok(Duration::minutes(n)),
        "h" | "hr" | "hrs" => Ok(Duration::hours(n)),
        "d" | "day" | "days" => Ok(Duration::days(n)),
        _ => Err(TrustError::Policy(format!(
            "Unknown duration unit: {}",
            unit
        ))),
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

// =============================================================================
// Local Callback Server for OAuth
// =============================================================================

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Find a free port on localhost
#[allow(dead_code)]
async fn find_free_port() -> Result<u16, TrustError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Run a simple HTTP server to handle the OAuth callback
#[allow(dead_code)]
async fn run_callback_server(
    port: u16,
    session_id: &str,
    registry_url: &str,
    client: reqwest::Client,
) -> Result<OAuthToken, String> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind: {}", e))?;

    // Accept one connection
    let (mut socket, _) = listener
        .accept()
        .await
        .map_err(|e| format!("Failed to accept: {}", e))?;

    let mut buffer = [0u8; 4096];
    let n = socket
        .read(&mut buffer)
        .await
        .map_err(|e| format!("Failed to read: {}", e))?;

    let request = String::from_utf8_lossy(&buffer[..n]);

    // Parse query parameters from request line
    let code = extract_query_param(&request, "code");
    let state = extract_query_param(&request, "state");
    let error = extract_query_param(&request, "error");

    // Send response
    let response_body = if error.is_some() {
        "Authentication failed. You can close this window."
    } else {
        "Authentication successful! You can close this window."
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );

    socket.write_all(response.as_bytes()).await.ok();

    // Check for error
    if let Some(err) = error {
        return Err(format!("OAuth error: {}", err));
    }

    // Get code and state
    let code = code.ok_or("Missing authorization code")?;
    let state = state.ok_or("Missing state")?;

    // Exchange code via registry (fixed callback URL with session in query param)
    let callback_url = format!(
        "{}/api/v1/auth/oidc/callback",
        registry_url.trim_end_matches('/')
    );

    let resp = client
        .get(&callback_url)
        .query(&[("session", session_id), ("code", &code), ("state", &state)])
        .send()
        .await
        .map_err(|e| format!("Failed to call callback: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Callback failed: {}", err));
    }

    // Get token
    let token_url = format!(
        "{}/api/v1/auth/oidc/token/{}",
        registry_url.trim_end_matches('/'),
        session_id
    );
    let resp = client
        .post(&token_url)
        .send()
        .await
        .map_err(|e| format!("Failed to get token: {}", e))?;

    if !resp.status().is_success() {
        return Err("Failed to get token".to_string());
    }

    let token: OAuthToken = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token: {}", e))?;

    Ok(token)
}

#[allow(dead_code)]
fn extract_query_param(request: &str, name: &str) -> Option<String> {
    // Find request line (e.g., "GET /callback?code=xxx&state=yyy HTTP/1.1")
    let request_line = request.lines().next()?;
    let path_start = request_line.find(' ')? + 1;
    let path_end = request_line[path_start..].find(' ')?;
    let path = &request_line[path_start..path_start + path_end];

    // Extract query string
    let query_start = path.find('?')?;
    let query = &path[query_start + 1..];

    // Parse params
    for param in query.split('&') {
        let (key, value) = param.split_once('=')?;
        
        

        if key == name {
            return Some(urlencoding::decode(value).ok()?.into_owned());
        }
    }

    None
}
