//! OIDC Authentication Flow for Sigstore-style keyless signing
//!
//! This module implements the OAuth2/OIDC flow for authenticating users
//! via GitHub, GitLab, and other identity providers.

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Supported OIDC identity providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentityProvider {
    GitHub,
    GitLab,
    Google,
}

impl IdentityProvider {
    /// Get the OAuth authorization URL
    pub fn auth_url(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://github.com/login/oauth/authorize",
            IdentityProvider::GitLab => "https://gitlab.com/oauth/authorize",
            IdentityProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
        }
    }

    /// Get the OAuth token URL
    pub fn token_url(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://github.com/login/oauth/access_token",
            IdentityProvider::GitLab => "https://gitlab.com/oauth/token",
            IdentityProvider::Google => "https://oauth2.googleapis.com/token",
        }
    }

    /// Get the userinfo URL
    pub fn userinfo_url(&self) -> &'static str {
        match self {
            IdentityProvider::GitHub => "https://api.github.com/user",
            IdentityProvider::GitLab => "https://gitlab.com/api/v4/user",
            IdentityProvider::Google => "https://openidconnect.googleapis.com/v1/userinfo",
        }
    }

    /// Get the required scopes
    pub fn scopes(&self) -> Vec<&'static str> {
        match self {
            IdentityProvider::GitHub => vec!["read:user", "user:email"],
            IdentityProvider::GitLab => vec!["read_user", "openid"],
            IdentityProvider::Google => vec!["openid", "email", "profile"],
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

/// An active OAuth session
#[derive(Debug, Clone)]
pub struct OAuthSession {
    pub session_id: String,
    pub provider: IdentityProvider,
    pub state: String,
    pub pkce_verifier: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub result: Option<OAuthResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Pending,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone)]
pub struct OAuthResult {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub identity: Identity,
    pub expires_in: u64,
}

/// OIDC Identity claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub repository: Option<String>,
    pub workflow_ref: Option<String>,
    pub event_name: Option<String>,
}

impl Identity {
    /// Get the display identity string
    pub fn identity_string(&self) -> String {
        if let Some(repo) = &self.repository {
            if let Some(workflow) = &self.workflow_ref {
                return format!("{}/{}", repo, workflow);
            }
            return repo.clone();
        }
        if let Some(username) = &self.preferred_username {
            return format!("{}/{}", self.iss.trim_start_matches("https://"), username);
        }
        self.sub.clone()
    }

    /// Check if this is a CI/automated identity
    pub fn is_ci(&self) -> bool {
        self.workflow_ref.is_some() || self.event_name.as_ref().map(|e| e == "push").unwrap_or(false)
    }
}

/// OIDC Flow Manager
#[derive(Debug)]
pub struct OidcFlow {
    sessions: Arc<RwLock<HashMap<String, OAuthSession>>>,
    http_client: reqwest::Client,
    github_client_id: String,
    github_client_secret: String,
    gitlab_client_id: String,
    gitlab_client_secret: String,
    google_client_id: String,
    google_client_secret: String,
    base_url: String,
}

impl OidcFlow {
    pub fn new(
        github_client_id: String,
        github_client_secret: String,
        gitlab_client_id: String,
        gitlab_client_secret: String,
        google_client_id: String,
        google_client_secret: String,
        base_url: String,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            http_client,
            github_client_id,
            github_client_secret,
            gitlab_client_id,
            gitlab_client_secret,
            google_client_id,
            google_client_secret,
            base_url,
        }
    }

    /// Create a new OAuth session
    pub fn create_session(&self, provider: IdentityProvider) -> OAuthSession {
        let session_id = generate_random_string(32);
        let state = generate_random_string(32);
        let pkce_verifier = generate_pkce_verifier();
        
        let client_id = match provider {
            IdentityProvider::GitHub => self.github_client_id.clone(),
            IdentityProvider::GitLab => self.gitlab_client_id.clone(),
            IdentityProvider::Google => self.google_client_id.clone(),
        };

        // Build redirect URI from base URL
        let redirect_uri = format!("{}/api/v1/auth/oidc/callback/{}", 
            self.base_url.trim_end_matches('/'), 
            session_id
        );

        let session = OAuthSession {
            session_id: session_id.clone(),
            provider,
            state,
            pkce_verifier,
            redirect_uri,
            client_id,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(10),
            status: SessionStatus::Pending,
            result: None,
        };

        self.sessions.write().insert(session_id, session.clone());
        info!("Created OAuth session: {}", session.session_id);
        
        session
    }

    /// Get the authorization URL for a session
    pub fn get_auth_url(&self, session: &OAuthSession) -> String {
        let pkce_challenge = base64_urlencode(&sha256_hash(&session.pkce_verifier));
        
        let scopes = session.provider.scopes().join(" ");
        
        format!(
            "{}?client_id={}&redirect_uri={}&state={}&scope={}&response_type=code&code_challenge={}&code_challenge_method=S256",
            session.provider.auth_url(),
            urlencoding::encode(&session.client_id),
            urlencoding::encode(&session.redirect_uri),
            urlencoding::encode(&session.state),
            urlencoding::encode(&scopes),
            pkce_challenge
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        session_id: &str,
        code: &str,
        state: &str,
    ) -> Result<Identity, OidcError> {
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(session_id)
            .ok_or(OidcError::SessionNotFound)?;

        if session.status != SessionStatus::Pending {
            return Err(OidcError::SessionInvalid);
        }

        if session.expires_at < Utc::now() {
            session.status = SessionStatus::Expired;
            return Err(OidcError::SessionExpired);
        }

        if session.state != state {
            return Err(OidcError::InvalidState);
        }

        let client_secret = match session.provider {
            IdentityProvider::GitHub => &self.github_client_secret,
            IdentityProvider::GitLab => &self.gitlab_client_secret,
            IdentityProvider::Google => &self.google_client_secret,
        };

        // Exchange code for tokens
        let token_response = self.http_client
            .post(session.provider.token_url())
            .header("Accept", "application/json")
            .form(&[
                ("client_id", session.client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("code", code),
                ("redirect_uri", &session.redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", &session.pkce_verifier),
            ])
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        if !token_response.status().is_success() {
            let error_text = token_response.text().await.unwrap_or_default();
            return Err(OidcError::TokenExchangeFailed(error_text));
        }

        let token_data: TokenResponse = token_response
            .json()
            .await
            .map_err(|e| OidcError::ParseError(e.to_string()))?;

        debug!("Received token response for session {}", session_id);

        // Fetch user info
        let identity = self.fetch_identity(
            session.provider,
            &token_data.access_token,
        ).await?;

        // Store result
        session.result = Some(OAuthResult {
            access_token: token_data.access_token,
            refresh_token: token_data.refresh_token,
            id_token: token_data.id_token,
            identity: identity.clone(),
            expires_in: token_data.expires_in.unwrap_or(3600),
        });
        session.status = SessionStatus::Completed;

        info!("OAuth flow completed for session {}, identity: {}", 
            session_id, identity.identity_string());

        Ok(identity)
    }

    /// Fetch identity from the provider
    async fn fetch_identity(
        &self,
        provider: IdentityProvider,
        access_token: &str,
    ) -> Result<Identity, OidcError> {
        let userinfo_response = self.http_client
            .get(provider.userinfo_url())
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Accept", "application/json")
            .header("User-Agent", "wares-registry/1.0")
            .send()
            .await
            .map_err(|e| OidcError::Http(e.to_string()))?;

        if !userinfo_response.status().is_success() {
            let error_text = userinfo_response.text().await.unwrap_or_default();
            return Err(OidcError::UserInfoFailed(error_text));
        }

        let user_data: serde_json::Value = userinfo_response
            .json()
            .await
            .map_err(|e| OidcError::ParseError(e.to_string()))?;

        debug!("User info response: {:?}", user_data);

        // Parse identity based on provider
        let identity = match provider {
            IdentityProvider::GitHub => self.parse_github_identity(user_data),
            IdentityProvider::GitLab => self.parse_gitlab_identity(user_data),
            IdentityProvider::Google => self.parse_google_identity(user_data),
        };

        Ok(identity)
    }

    fn parse_github_identity(&self, data: serde_json::Value) -> Identity {
        Identity {
            sub: data.get("id").and_then(|v| v.as_i64()).map(|id| id.to_string())
                .or_else(|| data.get("login").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_default(),
            iss: "https://github.com".to_string(),
            aud: "wares.lumen-lang.com".to_string(),
            email: data.get("email").and_then(|v| v.as_str()).map(|s| s.to_string()),
            name: data.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            preferred_username: data.get("login").and_then(|v| v.as_str()).map(|s| s.to_string()),
            repository: None,
            workflow_ref: None,
            event_name: None,
        }
    }

    fn parse_gitlab_identity(&self, data: serde_json::Value) -> Identity {
        Identity {
            sub: data.get("id").and_then(|v| v.as_i64()).map(|id| id.to_string())
                .unwrap_or_default(),
            iss: "https://gitlab.com".to_string(),
            aud: "wares.lumen-lang.com".to_string(),
            email: data.get("email").and_then(|v| v.as_str()).map(|s| s.to_string()),
            name: data.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            preferred_username: data.get("username").and_then(|v| v.as_str()).map(|s| s.to_string()),
            repository: None,
            workflow_ref: None,
            event_name: None,
        }
    }

    fn parse_google_identity(&self, data: serde_json::Value) -> Identity {
        Identity {
            sub: data.get("sub").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default(),
            iss: "https://accounts.google.com".to_string(),
            aud: data.get("aud").and_then(|v| v.as_str()).map(|s| s.to_string())
                .unwrap_or_else(|| "wares.lumen-lang.com".to_string()),
            email: data.get("email").and_then(|v| v.as_str()).map(|s| s.to_string()),
            name: data.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            preferred_username: data.get("email").and_then(|v| v.as_str()).map(|s| s.to_string()),
            repository: None,
            workflow_ref: None,
            event_name: None,
        }
    }

    /// Get session by ID
    pub fn get_session(&self, session_id: &str) -> Option<OAuthSession> {
        self.sessions.read().get(session_id).cloned()
    }

    /// Cleanup expired sessions
    pub fn cleanup_sessions(&self) {
        let mut sessions = self.sessions.write();
        let now = Utc::now();
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.expires_at < now)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in expired {
            if let Some(session) = sessions.remove(&id) {
                if session.status == SessionStatus::Pending {
                    warn!("Cleaned up expired session: {}", id);
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("Session not found")]
    SessionNotFound,
    #[error("Session expired")]
    SessionExpired,
    #[error("Session invalid")]
    SessionInvalid,
    #[error("Invalid state parameter")]
    InvalidState,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("Failed to fetch user info: {0}")]
    UserInfoFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Generate a random string for state/verifier
fn generate_random_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = (0..len)
        .map(|_| {
            const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
            CHARSET[rng.gen_range(0..CHARSET.len())] as char
        })
        .collect();
    chars.into_iter().collect()
}

/// Generate PKCE code verifier
fn generate_pkce_verifier() -> String {
    generate_random_string(128)
}

/// Base64 URL-safe encoding without padding
fn base64_urlencode(data: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(data)
}

/// SHA256 hash
fn sha256_hash(data: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hasher.finalize().to_vec()
}
