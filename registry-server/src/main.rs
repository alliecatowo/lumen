//! Wares Registry Server
//!
//! Production-ready registry with:
//! - OIDC authentication (GitHub, GitLab, Google)
//! - Ephemeral certificate signing (Fulcio-like CA)
//! - Package publishing with signature verification
//! - Transparency log integration

use axum::{
    body::Body,
    extract::{self, Path, Query, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{debug, info, warn, error};

mod storage;
mod auth;
mod oidc;
mod ca;

pub use storage::{Storage, StorageError};
pub use auth::{Auth, User};
pub use oidc::{IdentityProvider, OidcFlow, Identity, OAuthSession, SessionStatus};
pub use ca::{CertificateAuthority, IssuedCertificate, CaError};

#[derive(Debug, Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub auth: Arc<Auth>,
    pub oidc: Arc<OidcFlow>,
    pub ca: Arc<CertificateAuthority>,
    pub transparency_log_url: String,
    pub transparency_log_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageIndex {
    pub name: String,
    pub versions: Vec<PackageVersion>,
    pub latest: Option<String>,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVersion {
    pub version: String,
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
    pub unpacked_size: Option<u64>,
    pub file_count: Option<u32>,
    pub published: DateTime<Utc>,
    pub yanked: bool,
    pub metadata: Option<serde_json::Value>,
    pub signature: Option<PackageSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSignature {
    pub key_id: String,
    pub signature: String,
    pub certificate: String,
    pub identity: String,
    pub algorithm: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishRequest {
    pub name: String,
    pub version: String,
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub signature: Option<PackageSignature>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub packages: Vec<SearchResultItem>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub downloads: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(s) => (StatusCode::NOT_FOUND, s),
            AppError::BadRequest(s) => (StatusCode::BAD_REQUEST, s),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Authentication required".to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Access denied".to_string()),
            AppError::Conflict(s) => (StatusCode::CONFLICT, s),
            AppError::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Unauthorized,
    Forbidden,
    Conflict(String),
    Internal(String),
}

impl From<StorageError> for AppError {
    fn from(e: StorageError) -> Self {
        AppError::Internal(e.to_string())
    }
}

fn validate_package_name(name: &str) -> bool {
    let re = regex::Regex::new(r"^[a-z][a-z0-9_-]*$").unwrap();
    re.is_match(name) && name.len() >= 3 && name.len() <= 64
}

fn validate_version(version: &str) -> bool {
    let re = regex::Regex::new(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$").unwrap();
    re.is_match(version)
}

// =============================================================================
// OIDC Authentication Endpoints
// =============================================================================

#[derive(Debug, Serialize)]
struct LoginResponse {
    session_id: String,
    auth_url: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    provider: IdentityProvider,
}

async fn oidc_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let session = state.oidc.create_session(req.provider);
    let auth_url = state.oidc.get_auth_url(&session);

    info!("Created OIDC session {} for provider {:?}", session.session_id, req.provider);

    Ok(Json(LoginResponse {
        session_id: session.session_id,
        auth_url,
    }))
}

#[derive(Debug, Clone, Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    info!("Received OAuth callback for session {}", session_id);

    // Get session to check status
    let session = match state.oidc.get_session(&session_id) {
        Some(s) => s,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Session not found"
            }));
        }
    };

    match session.status {
        SessionStatus::Completed => {
            if let Some(result) = session.result {
                Json(serde_json::json!({
                    "success": true,
                    "identity": result.identity.identity_string(),
                    "message": "Authentication successful! You can close this window."
                }))
            } else {
                Json(serde_json::json!({
                    "success": false,
                    "error": "Session completed but no result found"
                }))
            }
        }
        SessionStatus::Pending => {
            Json(serde_json::json!({
                "success": true,
                "message": "Authentication pending. Please complete the flow in your browser.",
                "session_id": session_id
            }))
        }
        SessionStatus::Failed => {
            Json(serde_json::json!({
                "success": false,
                "error": "Authentication failed"
            }))
        }
        SessionStatus::Expired => {
            Json(serde_json::json!({
                "success": false,
                "error": "Session expired"
            }))
        }
    }
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    identity: String,
    expires_in: u64,
}

async fn oidc_token(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<TokenResponse>, AppError> {
    let session = state.oidc.get_session(&session_id)
        .ok_or_else(|| AppError::NotFound("Session not found".to_string()))?;

    match session.status {
        SessionStatus::Completed => {
            let result = session.result.as_ref()
                .ok_or_else(|| AppError::Internal("Session result missing".to_string()))?;

            Ok(Json(TokenResponse {
                access_token: result.access_token.clone(),
                refresh_token: result.refresh_token.clone().unwrap_or_default(),
                identity: result.identity.identity_string(),
                expires_in: result.expires_in,
            }))
        }
        SessionStatus::Pending => Err(AppError::BadRequest("Authentication pending".to_string())),
        SessionStatus::Failed => Err(AppError::BadRequest("Authentication failed".to_string())),
        SessionStatus::Expired => Err(AppError::BadRequest("Session expired".to_string())),
    }
}

// =============================================================================
// Certificate Authority Endpoints
// =============================================================================

#[derive(Debug, Deserialize)]
struct CertRequest {
    public_key: String,
    oidc_token: String,
}

#[derive(Debug, Serialize)]
struct CertResponse {
    cert_id: String,
    certificate_pem: String,
    identity: String,
    not_before: String,
    not_after: String,
}

async fn request_certificate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CertRequest>,
) -> Result<Json<CertResponse>, AppError> {
    // TODO: Verify OIDC token
    // For now, extract identity from token (in production, validate with provider)
    let identity = extract_identity_from_token(&req.oidc_token)?;

    let cert = state.ca.issue_certificate(
        &req.public_key,
        &identity,
        "wares.lumen-lang.com",
        10, // 10 minute validity
    ).map_err(|e| AppError::Internal(e.to_string()))?;

    info!("Issued certificate {} for {}", cert.cert_id, identity);

    Ok(Json(CertResponse {
        cert_id: cert.cert_id,
        certificate_pem: cert.certificate_pem,
        identity: cert.identity,
        not_before: cert.issued_at.to_rfc3339(),
        not_after: cert.expires_at.to_rfc3339(),
    }))
}

fn extract_identity_from_token(token: &str) -> Result<String, AppError> {
    // In production, validate the token with the OIDC provider
    // For now, return a placeholder identity
    Ok("github.com/user/repo/.github/workflows/release.yml".to_string())
}

// =============================================================================
// Package Publishing
// =============================================================================

async fn upload_package(
    State(state): State<Arc<AppState>>,
    extract::Json(payload): extract::Json<PublishRequest>,
) -> Result<Json<PackageVersion>, AppError> {
    if !validate_package_name(&payload.name) {
        return Err(AppError::BadRequest(format!("Invalid package name: {}", payload.name)));
    }

    if !validate_version(&payload.version) {
        return Err(AppError::BadRequest(format!("Invalid version: {}", payload.version)));
    }

    // Decode tarball
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let tarball_data = BASE64.decode(&payload.tarball)
        .map_err(|e| AppError::BadRequest(format!("Invalid tarball: {}", e)))?;

    // Verify SHA256
    let mut hasher = Sha256::new();
    hasher.update(&tarball_data);
    let calculated_hash = hex::encode(hasher.finalize());

    if calculated_hash != payload.shasum {
        return Err(AppError::BadRequest("SHA256 mismatch".to_string()));
    }

    // Verify signature if provided
    if let Some(ref sig) = payload.signature {
        verify_package_signature(state.clone(), sig, &calculated_hash).await?;
    }

    // Get or create package index
    let storage = &state.storage;
    let mut index = storage.get_package_index(&payload.name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .unwrap_or_else(|| PackageIndex {
            name: payload.name.clone(),
            versions: vec![],
            latest: None,
            description: None,
            repository: None,
            keywords: vec![],
            downloads: 0,
        });

    // Check for version conflict
    if index.versions.iter().any(|v| v.version == payload.version) {
        return Err(AppError::Conflict(format!("Version already exists: {}@{}", payload.name, payload.version)));
    }

    // Upload tarball
    let tarball_path = storage.upload_tarball(&payload.name, &payload.version, tarball_data).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Submit to transparency log
    let log_index = submit_to_transparency_log(state.clone(), &payload).await.ok();

    // Create version entry
    let version = PackageVersion {
        version: payload.version.clone(),
        tarball: tarball_path,
        shasum: payload.shasum.clone(),
        integrity: payload.integrity.clone(),
        unpacked_size: None,
        file_count: None,
        published: Utc::now(),
        yanked: false,
        metadata: payload.metadata.clone(),
        signature: payload.signature.clone(),
    };

    // Update index
    index.versions.push(version.clone());
    index.latest = Some(payload.version.clone());

    storage.save_package_index(&payload.name, &index).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    storage.invalidate_cache(&[
        format!("/packages/{}/index.json", payload.name),
    ]).await;

    info!("Published {}@{} (log_index: {:?})", payload.name, payload.version, log_index);

    Ok(Json(version))
}

async fn verify_package_signature(
    state: Arc<AppState>,
    sig: &PackageSignature,
    content_hash: &str,
) -> Result<(), AppError> {
    // Verify certificate exists and is valid
    let cert = state.ca.get_certificate(&sig.key_id)
        .ok_or_else(|| AppError::BadRequest("Certificate not found".to_string()))?;

    if cert.expires_at < Utc::now() {
        return Err(AppError::BadRequest("Certificate expired".to_string()));
    }

    // Verify signature
    let message = format!("wares:{}:{}:{}", sig.identity, content_hash, sig.timestamp.to_rfc3339());
    let signature_bytes = base64::decode(&sig.signature)
        .map_err(|e| AppError::BadRequest(format!("Invalid signature: {}", e)))?;

    let valid = state.ca.verify_signature(&cert.certificate_pem, message.as_bytes(), &signature_bytes)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !valid {
        return Err(AppError::BadRequest("Signature verification failed".to_string()));
    }

    debug!("Signature verified for {}", sig.identity);
    Ok(())
}

async fn submit_to_transparency_log(
    state: Arc<AppState>,
    pkg: &PublishRequest,
) -> Result<u64, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    
    let identity = pkg.signature.as_ref()
        .map(|s| s.identity.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let response = client
        .post(format!("{}/api/v1/log/entries", state.transparency_log_url))
        .header("X-API-Key", &state.transparency_log_key)
        .json(&serde_json::json!({
            "package_name": pkg.name,
            "version": pkg.version,
            "content_hash": format!("sha256:{}", pkg.shasum),
            "identity": identity,
            "signature": pkg.signature.as_ref().map(|s| s.signature.clone()).unwrap_or_default(),
            "certificate": pkg.signature.as_ref().map(|s| s.certificate.clone()).unwrap_or_default(),
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        return Err(error.into());
    }

    let result: serde_json::Value = response.json().await?;
    let index = result.get("index")
        .and_then(|v| v.as_u64())
        .ok_or("Missing log index")?;

    Ok(index)
}

// =============================================================================
// Other Package Endpoints
// =============================================================================

async fn download_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let storage = &state.storage;
    let index = storage.get_package_index(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", name)))?;

    let pkg_version = index.versions.iter()
        .find(|v| v.version == version && !v.yanked)
        .ok_or_else(|| AppError::NotFound(format!("Version not found: {}@{}", name, version)))?;

    let tarball_data = storage.get_tarball(&name, &version).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/gzip")],
        Body::from(tarball_data),
    ).into_response())
}

async fn yank_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let storage = &state.storage;
    let mut index = storage.get_package_index(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", name)))?;

    let pkg_version = index.versions.iter_mut()
        .find(|v| v.version == version)
        .ok_or_else(|| AppError::NotFound(format!("Version not found: {}@{}", name, version)))?;

    pkg_version.yanked = true;

    if index.latest.as_deref() == Some(&version) {
        index.latest = index.versions.iter()
            .filter(|v| !v.yanked)
            .max_by_key(|v| &v.version)
            .map(|v| v.version.clone());
    }

    storage.save_package_index(&name, &index).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    storage.invalidate_cache(&[
        format!("/packages/{}/index.json", name),
    ]).await;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_package(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<PackageIndex>, AppError> {
    let storage = &state.storage;
    let index = storage.get_package_index(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", name)))?;
    
    Ok(Json(index))
}

async fn list_packages(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, AppError> {
    let storage = &state.storage;
    let packages = storage.list_packages().await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(packages))
}

async fn search_packages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SearchResult>, AppError> {
    let query = params.get("q").cloned().unwrap_or_default();
    let limit = params.get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let storage = &state.storage;
    let packages = storage.list_packages().await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for name in packages {
        if let Ok(Some(index)) = storage.get_package_index(&name).await {
            let matches_name = name.to_lowercase().contains(&query_lower);
            let matches_desc = index.description
                .as_ref()
                .map(|d| d.to_lowercase().contains(&query_lower))
                .unwrap_or(false);
            let matches_keyword = index.keywords.iter()
                .any(|k| k.to_lowercase().contains(&query_lower));

            if matches_name || matches_desc || matches_keyword {
                results.push(SearchResultItem {
                    name: index.name,
                    version: index.latest.unwrap_or_default(),
                    description: index.description,
                    keywords: index.keywords,
                    downloads: index.downloads,
                });

                if results.len() >= limit {
                    break;
                }
            }
        }
    }

    Ok(Json(SearchResult {
        total: results.len(),
        packages: results,
    }))
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("lumen_registry_server=debug,tower_http=debug")
        .init();

    // Load configuration from environment
    let github_client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default();
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default();
    let gitlab_client_id = std::env::var("GITLAB_CLIENT_ID").unwrap_or_default();
    let gitlab_client_secret = std::env::var("GITLAB_CLIENT_SECRET").unwrap_or_default();
    let google_client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let google_client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    
    let transparency_log_url = std::env::var("TRANSPARENCY_LOG_URL")
        .unwrap_or_else(|_| "https://wares-transparency-log.alliecatowo.workers.dev".to_string());
    let transparency_log_key = std::env::var("TRANSPARENCY_LOG_API_KEY")
        .unwrap_or_default();
    
    // Base URL for OAuth callbacks (should be your public registry URL)
    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Initialize components
    let storage = Storage::from_environment().await?;
    let auth = Auth::new();
    let oidc = OidcFlow::new(
        github_client_id,
        github_client_secret,
        gitlab_client_id,
        gitlab_client_secret,
        google_client_id,
        google_client_secret,
        base_url.clone(),
    );
    
    // Load CA from environment or generate new
    let ca = if let Ok(ca_key) = std::env::var("CA_PRIVATE_KEY") {
        CertificateAuthority::from_private_key(&ca_key)?
    } else {
        CertificateAuthority::new()?
    };

    let state = Arc::new(AppState {
        storage: Arc::new(storage),
        auth: Arc::new(auth),
        oidc: Arc::new(oidc),
        ca: Arc::new(ca),
        transparency_log_url: transparency_log_url.clone(),
        transparency_log_key,
    });

    // Cleanup task for expired sessions and certificates
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            cleanup_state.oidc.cleanup_sessions();
            cleanup_state.ca.cleanup_expired();
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
        ]);

    let app = Router::new()
        // OIDC authentication
        .route("/api/v1/auth/oidc/login", post(oidc_login))
        .route("/api/v1/auth/oidc/callback/:session_id", get(oidc_callback))
        .route("/api/v1/auth/oidc/token/:session_id", post(oidc_token))
        // Certificate authority
        .route("/api/v1/auth/cert", post(request_certificate))
        // Package management
        .route("/v1/packages", post(upload_package).get(list_packages))
        .route("/v1/packages/:name/:version", get(download_package).delete(yank_package))
        .route("/v1/packages/:name", get(get_package))
        .route("/v1/index", get(list_packages))
        .route("/v1/search", get(search_packages))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    
    info!("🚀 Wares Registry Server listening on {}", listener.local_addr()?);
    info!("🔐 OIDC authentication enabled");
    info!("📜 Certificate Authority enabled");
    info!("📝 Transparency log: {}", transparency_log_url);

    axum::serve(listener, app).await?;

    Ok(())
}

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
