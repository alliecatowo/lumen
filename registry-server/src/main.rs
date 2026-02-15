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
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod storage;
mod auth;

pub use storage::Storage;
pub use auth::{Auth, User};

#[derive(Debug, Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub auth: Arc<Auth>,
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishRequest {
    pub name: String,
    pub version: String,
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
    pub metadata: Option<serde_json::Value>,
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

fn validate_package_name(name: &str) -> bool {
    let re = regex::Regex::new(r"^[a-z][a-z0-9_-]*$").unwrap();
    re.is_match(name) && name.len() >= 3 && name.len() <= 64
}

fn validate_version(version: &str) -> bool {
    let re = regex::Regex::new(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$").unwrap();
    re.is_match(version)
}

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

    let tarball_data = BASE64.decode(&payload.tarball)
        .map_err(|e| AppError::BadRequest(format!("Invalid tarball: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&tarball_data);
    let calculated_hash = hex::encode(hasher.finalize());

    if calculated_hash != payload.shasum {
        return Err(AppError::BadRequest("SHA256 mismatch".to_string()));
    }

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

    if index.versions.iter().any(|v| v.version == payload.version) {
        return Err(AppError::Conflict(format!("Version already exists: {}@{}", payload.name, payload.version)));
    }

    let tarball_path = storage.upload_tarball(&payload.name, &payload.version, tarball_data).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

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
    };

    index.versions.push(version.clone());
    index.latest = Some(payload.version.clone());

    storage.save_package_index(&payload.name, &index).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    storage.invalidate_cache(&[
        format!("/packages/{}/index.json", payload.name),
    ]).await;

    Ok(Json(version))
}

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumen_registry_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let storage = Storage::from_environment().await?;
    let auth = Auth::new();

    let state = Arc::new(AppState {
        storage: Arc::new(storage),
        auth: Arc::new(auth),
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
        .route("/v1/packages", post(upload_package))
        .route("/v1/packages/:name/:version", get(download_package).delete(yank_package))
        .route("/v1/packages/:name", get(get_package))
        .route("/v1/index", get(list_packages))
        .route("/v1/search", get(search_packages))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Registry server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
