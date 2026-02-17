//! OpenID Connect (OIDC) authentication for the Lumen package registry.
//!
//! This module provides OIDC-based authentication as an alternative to API tokens.
//! It implements the Authorization Code Flow with PKCE (Proof Key for Code Exchange)
//! for secure browser-based authentication.
//!
//! ## Flow
//!
//! 1. Discover the OIDC provider's configuration via `.well-known/openid-configuration`
//! 2. Generate PKCE code verifier and challenge
//! 3. Build authorization URL and open it in the user's browser
//! 4. Receive the authorization code via local callback server
//! 5. Exchange the code for tokens (access_token, id_token, refresh_token)
//! 6. Validate the ID token claims
//! 7. Store the tokens using the existing credential manager

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// =============================================================================
// OIDC Configuration (Discovery Document)
// =============================================================================

/// OIDC provider discovery document from `.well-known/openid-configuration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// The OIDC issuer identifier (e.g., "https://auth.lumen.sh").
    pub issuer: String,

    /// URL of the authorization endpoint.
    pub authorization_endpoint: String,

    /// URL of the token endpoint.
    pub token_endpoint: String,

    /// URL of the userinfo endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userinfo_endpoint: Option<String>,

    /// URL of the JWKS (JSON Web Key Set) endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// Supported response types.
    #[serde(default)]
    pub response_types_supported: Vec<String>,

    /// Supported grant types.
    #[serde(default)]
    pub grant_types_supported: Vec<String>,

    /// Supported scopes.
    #[serde(default)]
    pub scopes_supported: Vec<String>,

    /// Supported token endpoint auth methods.
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,

    /// Supported code challenge methods (for PKCE).
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

impl OidcConfig {
    /// Check if the provider supports PKCE with S256.
    pub fn supports_pkce_s256(&self) -> bool {
        self.code_challenge_methods_supported.is_empty()
            || self
                .code_challenge_methods_supported
                .contains(&"S256".to_string())
    }

    /// Check if the provider supports the authorization_code grant.
    pub fn supports_authorization_code(&self) -> bool {
        self.grant_types_supported.is_empty()
            || self
                .grant_types_supported
                .contains(&"authorization_code".to_string())
    }

    /// Check if the provider supports refresh tokens.
    pub fn supports_refresh_token(&self) -> bool {
        self.grant_types_supported
            .contains(&"refresh_token".to_string())
    }
}

// =============================================================================
// OIDC Tokens
// =============================================================================

/// Token response from the OIDC provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcToken {
    /// The access token for API requests.
    pub access_token: String,

    /// The token type (usually "Bearer").
    pub token_type: String,

    /// When the access token expires (seconds from issuance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// The ID token (JWT) containing user identity claims.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,

    /// Refresh token for obtaining new access tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Granted scopes (space-separated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Timestamp when the token was obtained.
    #[serde(default = "Utc::now")]
    pub obtained_at: DateTime<Utc>,
}

impl OidcToken {
    /// Check if the access token has expired (with a 30-second buffer).
    pub fn is_expired(&self) -> bool {
        match self.expires_in {
            Some(expires_in) => {
                let expiry = self.obtained_at + Duration::seconds(expires_in as i64 - 30);
                Utc::now() > expiry
            }
            None => false, // No expiry info, assume valid
        }
    }

    /// Check if the token has a refresh token available.
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }
}

// =============================================================================
// ID Token Claims
// =============================================================================

/// Claims extracted from a validated ID token (JWT payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer identifier.
    pub iss: String,

    /// Subject (unique user ID).
    pub sub: String,

    /// Audience (must contain our client_id).
    pub aud: StringOrVec,

    /// Expiration time (Unix timestamp).
    pub exp: u64,

    /// Issued-at time (Unix timestamp).
    pub iat: u64,

    /// Auth time (optional, when the user last authenticated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_time: Option<u64>,

    /// Nonce (must match the one we sent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    /// User's email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Whether the email is verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,

    /// User's display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Helper type for the `aud` claim which can be a string or an array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    String(String),
    Vec(Vec<String>),
}

impl StringOrVec {
    /// Check if the audience contains a specific value.
    pub fn contains(&self, value: &str) -> bool {
        match self {
            StringOrVec::String(s) => s == value,
            StringOrVec::Vec(v) => v.iter().any(|s| s == value),
        }
    }
}

// =============================================================================
// PKCE (Proof Key for Code Exchange)
// =============================================================================

/// PKCE code verifier and challenge pair.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The code verifier (random string, kept secret).
    pub verifier: String,
    /// The code challenge (S256 hash of verifier, sent to authorization endpoint).
    pub challenge: String,
    /// The challenge method ("S256").
    pub method: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair using S256.
    pub fn generate() -> Self {
        // Generate 32 random bytes for the verifier
        let random_bytes: [u8; 32] = rand_bytes();
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes);

        // S256: SHA-256 hash of verifier, base64url-encoded
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);

        Self {
            verifier,
            challenge,
            method: "S256".to_string(),
        }
    }
}

// =============================================================================
// OIDC Error
// =============================================================================

/// Errors that can occur during OIDC authentication.
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Provider does not support required features: {0}")]
    UnsupportedProvider(String),

    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),

    #[error("Token refresh failed: {0}")]
    RefreshFailed(String),

    #[error("ID token validation failed: {0}")]
    ValidationFailed(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Timeout waiting for callback: {0}")]
    Timeout(String),

    #[error("User cancelled authentication")]
    Cancelled,
}

// =============================================================================
// OIDC Client Configuration
// =============================================================================

/// Client configuration for OIDC authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcClientConfig {
    /// The OIDC issuer URL (used for discovery).
    pub issuer_url: String,

    /// Client ID registered with the OIDC provider.
    pub client_id: String,

    /// Client secret (optional, not needed for public clients with PKCE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// Redirect URI for the callback (default: http://localhost:8765/callback).
    #[serde(default = "default_redirect_uri")]
    pub redirect_uri: String,

    /// Scopes to request.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_redirect_uri() -> String {
    "http://localhost:8765/callback".to_string()
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

// =============================================================================
// Core OIDC Functions
// =============================================================================

/// Discover the OIDC provider configuration from the issuer URL.
///
/// Fetches the `.well-known/openid-configuration` document from the issuer.
pub fn discover(issuer_url: &str) -> Result<OidcConfig, OidcError> {
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| OidcError::Http(e.to_string()))?;

    let response = client.get(&discovery_url).send().map_err(|e| {
        OidcError::DiscoveryFailed(format!("Failed to fetch discovery document: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(OidcError::DiscoveryFailed(format!(
            "Discovery endpoint returned HTTP {}",
            response.status()
        )));
    }

    let config: OidcConfig = response.json().map_err(|e| {
        OidcError::DiscoveryFailed(format!("Failed to parse discovery document: {}", e))
    })?;

    // Validate the issuer matches
    if config.issuer.trim_end_matches('/') != issuer_url.trim_end_matches('/') {
        return Err(OidcError::DiscoveryFailed(format!(
            "Issuer mismatch: expected '{}', got '{}'",
            issuer_url, config.issuer
        )));
    }

    // Validate required fields
    if config.authorization_endpoint.is_empty() {
        return Err(OidcError::DiscoveryFailed(
            "Missing authorization_endpoint in discovery document".to_string(),
        ));
    }

    if config.token_endpoint.is_empty() {
        return Err(OidcError::DiscoveryFailed(
            "Missing token_endpoint in discovery document".to_string(),
        ));
    }

    Ok(config)
}

/// Build the authorization URL for the OIDC flow.
///
/// Returns the URL to redirect the user to, along with the state and PKCE verifier
/// that must be verified when the callback is received.
pub fn build_auth_url(
    config: &OidcConfig,
    client_config: &OidcClientConfig,
    state: &str,
    nonce: &str,
    pkce: &PkceChallenge,
) -> Result<String, OidcError> {
    // Validate provider capabilities
    if !config.supports_authorization_code() {
        return Err(OidcError::UnsupportedProvider(
            "Provider does not support authorization_code grant".to_string(),
        ));
    }

    if !config.supports_pkce_s256() {
        return Err(OidcError::UnsupportedProvider(
            "Provider does not support PKCE S256".to_string(),
        ));
    }

    let scopes = client_config.scopes.join(" ");

    let params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", &client_config.client_id),
        ("redirect_uri", &client_config.redirect_uri),
        ("scope", &scopes),
        ("state", state),
        ("nonce", nonce),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", &pkce.method),
    ];

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    Ok(format!("{}?{}", config.authorization_endpoint, query))
}

/// Exchange an authorization code for tokens.
///
/// This completes the OIDC authorization code flow by exchanging the code
/// received from the callback for access and ID tokens.
pub fn exchange_code(
    config: &OidcConfig,
    client_config: &OidcClientConfig,
    code: &str,
    pkce_verifier: &str,
) -> Result<OidcToken, OidcError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| OidcError::Http(e.to_string()))?;

    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", code);
    params.insert("redirect_uri", &client_config.redirect_uri);
    params.insert("client_id", &client_config.client_id);
    params.insert("code_verifier", pkce_verifier);

    let mut req = client.post(&config.token_endpoint).form(&params);

    // Add client_secret if configured (confidential client)
    if let Some(ref secret) = client_config.client_secret {
        req = req.basic_auth(&client_config.client_id, Some(secret));
    }

    let response = req
        .send()
        .map_err(|e| OidcError::TokenExchangeFailed(format!("Token request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| "<no body>".to_string());
        return Err(OidcError::TokenExchangeFailed(format!(
            "Token endpoint returned HTTP {}: {}",
            status, body
        )));
    }

    let mut token: OidcToken = response.json().map_err(|e| {
        OidcError::TokenExchangeFailed(format!("Failed to parse token response: {}", e))
    })?;

    token.obtained_at = Utc::now();

    Ok(token)
}

/// Validate an ID token's claims.
///
/// This performs structural validation of the ID token claims:
/// - Issuer matches the expected issuer
/// - Audience contains our client_id
/// - Token is not expired
/// - Nonce matches (if provided)
///
/// Note: This does NOT verify the JWT signature. In production, you should
/// also verify the signature using the provider's JWKS endpoint.
pub fn validate_id_token(
    id_token: &str,
    issuer: &str,
    client_id: &str,
    expected_nonce: Option<&str>,
) -> Result<IdTokenClaims, OidcError> {
    // Split the JWT into parts
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err(OidcError::ValidationFailed(
            "ID token is not a valid JWT (expected 3 parts)".to_string(),
        ));
    }

    // Decode the payload (second part)
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| {
            // Try with padding
            base64::engine::general_purpose::URL_SAFE.decode(parts[1])
        })
        .map_err(|e| {
            OidcError::ValidationFailed(format!("Failed to decode ID token payload: {}", e))
        })?;

    let claims: IdTokenClaims = serde_json::from_slice(&payload_bytes).map_err(|e| {
        OidcError::ValidationFailed(format!("Failed to parse ID token claims: {}", e))
    })?;

    // Validate issuer
    if claims.iss.trim_end_matches('/') != issuer.trim_end_matches('/') {
        return Err(OidcError::ValidationFailed(format!(
            "Issuer mismatch: expected '{}', got '{}'",
            issuer, claims.iss
        )));
    }

    // Validate audience
    if !claims.aud.contains(client_id) {
        return Err(OidcError::ValidationFailed(format!(
            "Client ID '{}' not found in token audience",
            client_id
        )));
    }

    // Validate expiration
    let now = Utc::now().timestamp() as u64;
    if claims.exp < now {
        return Err(OidcError::ValidationFailed(
            "ID token has expired".to_string(),
        ));
    }

    // Validate nonce if expected
    if let Some(expected) = expected_nonce {
        match &claims.nonce {
            Some(nonce) if nonce == expected => {}
            Some(nonce) => {
                return Err(OidcError::ValidationFailed(format!(
                    "Nonce mismatch: expected '{}', got '{}'",
                    expected, nonce
                )));
            }
            None => {
                return Err(OidcError::ValidationFailed(
                    "Expected nonce in ID token but none found".to_string(),
                ));
            }
        }
    }

    Ok(claims)
}

/// Refresh an access token using a refresh token.
pub fn refresh_token(
    config: &OidcConfig,
    client_config: &OidcClientConfig,
    refresh_tok: &str,
) -> Result<OidcToken, OidcError> {
    if !config.supports_refresh_token() {
        return Err(OidcError::RefreshFailed(
            "Provider does not support refresh_token grant".to_string(),
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| OidcError::Http(e.to_string()))?;

    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_tok);
    params.insert("client_id", &client_config.client_id);

    let scopes = client_config.scopes.join(" ");
    params.insert("scope", &scopes);

    let mut req = client.post(&config.token_endpoint).form(&params);

    if let Some(ref secret) = client_config.client_secret {
        req = req.basic_auth(&client_config.client_id, Some(secret));
    }

    let response = req
        .send()
        .map_err(|e| OidcError::RefreshFailed(format!("Refresh token request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| "<no body>".to_string());
        return Err(OidcError::RefreshFailed(format!(
            "Token endpoint returned HTTP {}: {}",
            status, body
        )));
    }

    let mut token: OidcToken = response.json().map_err(|e| {
        OidcError::RefreshFailed(format!("Failed to parse refresh token response: {}", e))
    })?;

    token.obtained_at = Utc::now();

    // Preserve the refresh token if the response didn't include a new one
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh_tok.to_string());
    }

    Ok(token)
}

/// Generate a cryptographically random state parameter.
pub fn generate_state() -> String {
    let bytes: [u8; 16] = rand_bytes();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate a cryptographically random nonce.
pub fn generate_nonce() -> String {
    let bytes: [u8; 16] = rand_bytes();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Build the discovery URL for an issuer.
pub fn discovery_url(issuer_url: &str) -> String {
    format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    )
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Generate random bytes (using sha2 hash of timestamp + counter as fallback).
///
/// In production with the `rand` feature, this uses `rand::random()`.
/// Without `rand`, it uses a simple entropy source (NOT cryptographically secure).
fn rand_bytes<const N: usize>() -> [u8; N] {
    #[cfg(feature = "ed25519")]
    {
        let mut bytes = [0u8; N];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes
    }

    #[cfg(not(feature = "ed25519"))]
    {
        // Fallback: use system time + process id for entropy
        // NOT cryptographically secure - use for testing only
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut hasher = Sha256::new();
        hasher.update(seed.to_le_bytes());
        hasher.update(std::process::id().to_le_bytes());
        let hash = hasher.finalize();
        let mut bytes = [0u8; N];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = hash[i % hash.len()];
        }
        bytes
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // OidcConfig tests
    // -------------------------------------------------------------------------

    fn sample_config() -> OidcConfig {
        OidcConfig {
            issuer: "https://auth.lumen.sh".to_string(),
            authorization_endpoint: "https://auth.lumen.sh/authorize".to_string(),
            token_endpoint: "https://auth.lumen.sh/oauth/token".to_string(),
            userinfo_endpoint: Some("https://auth.lumen.sh/userinfo".to_string()),
            jwks_uri: Some("https://auth.lumen.sh/.well-known/jwks.json".to_string()),
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ],
            scopes_supported: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            token_endpoint_auth_methods_supported: vec!["none".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        }
    }

    fn sample_client_config() -> OidcClientConfig {
        OidcClientConfig {
            issuer_url: "https://auth.lumen.sh".to_string(),
            client_id: "lumen-cli".to_string(),
            client_secret: None,
            redirect_uri: "http://localhost:8765/callback".to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
        }
    }

    #[test]
    fn test_oidc_config_supports_pkce() {
        let config = sample_config();
        assert!(config.supports_pkce_s256());

        // Empty list means all methods supported
        let mut config2 = config.clone();
        config2.code_challenge_methods_supported = vec![];
        assert!(config2.supports_pkce_s256());

        // Only "plain" listed
        let mut config3 = config;
        config3.code_challenge_methods_supported = vec!["plain".to_string()];
        assert!(!config3.supports_pkce_s256());
    }

    #[test]
    fn test_oidc_config_supports_authorization_code() {
        let config = sample_config();
        assert!(config.supports_authorization_code());

        let mut config2 = config.clone();
        config2.grant_types_supported = vec!["client_credentials".to_string()];
        assert!(!config2.supports_authorization_code());

        // Empty list means all grants supported
        let mut config3 = config;
        config3.grant_types_supported = vec![];
        assert!(config3.supports_authorization_code());
    }

    #[test]
    fn test_oidc_config_supports_refresh_token() {
        let config = sample_config();
        assert!(config.supports_refresh_token());

        let mut config2 = config;
        config2.grant_types_supported = vec!["authorization_code".to_string()];
        assert!(!config2.supports_refresh_token());
    }

    // -------------------------------------------------------------------------
    // OidcToken tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_oidc_token_not_expired() {
        let token = OidcToken {
            access_token: "test_access".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            id_token: None,
            refresh_token: None,
            scope: None,
            obtained_at: Utc::now(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oidc_token_expired() {
        let token = OidcToken {
            access_token: "test_access".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(60),
            id_token: None,
            refresh_token: None,
            scope: None,
            obtained_at: Utc::now() - Duration::seconds(120),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_oidc_token_no_expiry() {
        let token = OidcToken {
            access_token: "test_access".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: None,
            id_token: None,
            refresh_token: None,
            scope: None,
            obtained_at: Utc::now() - Duration::days(365),
        };
        // No expiry info => never considered expired
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oidc_token_can_refresh() {
        let mut token = OidcToken {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            id_token: None,
            refresh_token: Some("refresh_me".to_string()),
            scope: None,
            obtained_at: Utc::now(),
        };
        assert!(token.can_refresh());

        token.refresh_token = None;
        assert!(!token.can_refresh());
    }

    // -------------------------------------------------------------------------
    // PKCE tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pkce_challenge_generation() {
        let pkce = PkceChallenge::generate();
        assert!(!pkce.verifier.is_empty());
        assert!(!pkce.challenge.is_empty());
        assert_eq!(pkce.method, "S256");

        // Verify the challenge is the SHA-256 of the verifier
        let mut hasher = Sha256::new();
        hasher.update(pkce.verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pkce.challenge, expected);
    }

    #[test]
    fn test_pkce_uniqueness() {
        let pkce1 = PkceChallenge::generate();
        let pkce2 = PkceChallenge::generate();
        // Very unlikely to be equal (2^256 space)
        assert_ne!(pkce1.verifier, pkce2.verifier);
        assert_ne!(pkce1.challenge, pkce2.challenge);
    }

    // -------------------------------------------------------------------------
    // build_auth_url tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_build_auth_url() {
        let config = sample_config();
        let client_config = sample_client_config();
        let pkce = PkceChallenge::generate();

        let url =
            build_auth_url(&config, &client_config, "test_state", "test_nonce", &pkce).unwrap();

        assert!(url.starts_with("https://auth.lumen.sh/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=lumen-cli"));
        assert!(url.contains("state=test_state"));
        assert!(url.contains("nonce=test_nonce"));
        assert!(url.contains(&format!(
            "code_challenge={}",
            urlencoding::encode(&pkce.challenge)
        )));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("scope=openid"));
    }

    #[test]
    fn test_build_auth_url_unsupported_grant() {
        let mut config = sample_config();
        config.grant_types_supported = vec!["client_credentials".to_string()];
        let client_config = sample_client_config();
        let pkce = PkceChallenge::generate();

        let result = build_auth_url(&config, &client_config, "state", "nonce", &pkce);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OidcError::UnsupportedProvider(_)
        ));
    }

    #[test]
    fn test_build_auth_url_unsupported_pkce() {
        let mut config = sample_config();
        config.code_challenge_methods_supported = vec!["plain".to_string()];
        let client_config = sample_client_config();
        let pkce = PkceChallenge::generate();

        let result = build_auth_url(&config, &client_config, "state", "nonce", &pkce);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // validate_id_token tests
    // -------------------------------------------------------------------------

    fn make_jwt(claims: &serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(claims).unwrap());
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"fake_signature");
        format!("{}.{}.{}", header, payload, signature)
    }

    #[test]
    fn test_validate_id_token_valid() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": "lumen-cli",
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
            "nonce": "test_nonce",
            "email": "user@example.com",
            "email_verified": true,
            "name": "Test User"
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(
            &jwt,
            "https://auth.lumen.sh",
            "lumen-cli",
            Some("test_nonce"),
        );
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.iss, "https://auth.lumen.sh");
        assert_eq!(parsed.sub, "user-123");
        assert_eq!(parsed.email, Some("user@example.com".to_string()));
        assert_eq!(parsed.name, Some("Test User".to_string()));
    }

    #[test]
    fn test_validate_id_token_wrong_issuer() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://evil.example.com",
            "sub": "user-123",
            "aud": "lumen-cli",
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(&jwt, "https://auth.lumen.sh", "lumen-cli", None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OidcError::ValidationFailed(_)
        ));
    }

    #[test]
    fn test_validate_id_token_wrong_audience() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": "wrong-client",
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(&jwt, "https://auth.lumen.sh", "lumen-cli", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_id_token_expired() {
        let past_exp = (Utc::now().timestamp() - 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": "lumen-cli",
            "exp": past_exp,
            "iat": Utc::now().timestamp() as u64 - 7200,
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(&jwt, "https://auth.lumen.sh", "lumen-cli", None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("expired"));
    }

    #[test]
    fn test_validate_id_token_wrong_nonce() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": "lumen-cli",
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
            "nonce": "wrong_nonce"
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(
            &jwt,
            "https://auth.lumen.sh",
            "lumen-cli",
            Some("expected_nonce"),
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Nonce mismatch"));
    }

    #[test]
    fn test_validate_id_token_missing_nonce() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": "lumen-cli",
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(
            &jwt,
            "https://auth.lumen.sh",
            "lumen-cli",
            Some("expected_nonce"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_id_token_array_audience() {
        let future_exp = (Utc::now().timestamp() + 3600) as u64;
        let claims = serde_json::json!({
            "iss": "https://auth.lumen.sh",
            "sub": "user-123",
            "aud": ["lumen-cli", "other-app"],
            "exp": future_exp,
            "iat": Utc::now().timestamp() as u64,
        });

        let jwt = make_jwt(&claims);
        let result = validate_id_token(&jwt, "https://auth.lumen.sh", "lumen-cli", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_id_token_invalid_jwt() {
        let result = validate_id_token("not.a.valid.jwt.with.five.parts", "issuer", "client", None);
        assert!(result.is_err());

        let result = validate_id_token("just_one_part", "issuer", "client", None);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // StringOrVec tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_string_or_vec_contains() {
        let single = StringOrVec::String("foo".to_string());
        assert!(single.contains("foo"));
        assert!(!single.contains("bar"));

        let multi = StringOrVec::Vec(vec!["foo".to_string(), "bar".to_string()]);
        assert!(multi.contains("foo"));
        assert!(multi.contains("bar"));
        assert!(!multi.contains("baz"));
    }

    // -------------------------------------------------------------------------
    // Helper function tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_state() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert!(!s1.is_empty());
        assert!(!s2.is_empty());
        // Should be unique
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_generate_nonce() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert!(!n1.is_empty());
        assert!(!n2.is_empty());
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_discovery_url() {
        assert_eq!(
            discovery_url("https://auth.lumen.sh"),
            "https://auth.lumen.sh/.well-known/openid-configuration"
        );
        // Trailing slash is stripped
        assert_eq!(
            discovery_url("https://auth.lumen.sh/"),
            "https://auth.lumen.sh/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_oidc_error_display() {
        let err = OidcError::DiscoveryFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Discovery failed: timeout");

        let err = OidcError::ValidationFailed("expired".to_string());
        assert_eq!(err.to_string(), "ID token validation failed: expired");

        let err = OidcError::Cancelled;
        assert_eq!(err.to_string(), "User cancelled authentication");
    }

    #[test]
    fn test_oidc_client_config_defaults() {
        let config: OidcClientConfig = serde_json::from_str(
            r#"{
            "issuer_url": "https://auth.example.com",
            "client_id": "test"
        }"#,
        )
        .unwrap();

        assert_eq!(config.redirect_uri, "http://localhost:8765/callback");
        assert_eq!(config.scopes, vec!["openid", "email", "profile"]);
        assert!(config.client_secret.is_none());
    }

    #[test]
    fn test_oidc_config_serialization() {
        let config = sample_config();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: OidcConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.issuer, config.issuer);
        assert_eq!(parsed.authorization_endpoint, config.authorization_endpoint);
        assert_eq!(parsed.token_endpoint, config.token_endpoint);
    }

    #[test]
    fn test_oidc_token_serialization() {
        let token = OidcToken {
            access_token: "access_123".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            id_token: Some("id.token.here".to_string()),
            refresh_token: Some("refresh_456".to_string()),
            scope: Some("openid email".to_string()),
            obtained_at: Utc::now(),
        };

        let json = serde_json::to_string(&token).unwrap();
        let parsed: OidcToken = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "access_123");
        assert_eq!(parsed.token_type, "Bearer");
        assert_eq!(parsed.expires_in, Some(3600));
        assert!(parsed.id_token.is_some());
        assert!(parsed.refresh_token.is_some());
    }
}
