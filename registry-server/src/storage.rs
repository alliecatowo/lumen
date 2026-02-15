use aws_sdk_s3::{Client, Config, primitives::ByteStream};
use aws_config::Region;
use aws_credential_types::Credentials;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use thiserror::Error;

use crate::PackageIndex;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("R2 error: {0}")]
    R2(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    Config(String),
}

pub struct Storage {
    client: Client,
    bucket_name: String,
    cache: Arc<RwLock<HashMap<String, PackageIndex>>>,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("bucket_name", &self.bucket_name)
            .finish()
    }
}

impl Storage {
    pub async fn new(
        r2_access_key: &str,
        r2_secret_key: &str,
        r2_bucket: &str,
        r2_endpoint: &str,
    ) -> Result<Self, StorageError> {
        let config = Config::builder()
            .region(Region::new("auto"))
            .endpoint_url(r2_endpoint)
            .credentials_provider(Credentials::new(
                r2_access_key,
                r2_secret_key,
                None,
                None,
                "lumen-registry",
            ))
            .build();

        let client = Client::from_conf(config);

        Ok(Self {
            client,
            bucket_name: r2_bucket.to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn from_environment() -> Result<Self, StorageError> {
        let r2_access_key = std::env::var("R2_ACCESS_KEY")
            .map_err(|_| StorageError::Config("R2_ACCESS_KEY not set".to_string()))?;
        let r2_secret_key = std::env::var("R2_SECRET_KEY")
            .map_err(|_| StorageError::Config("R2_SECRET_KEY not set".to_string()))?;
        let r2_bucket = std::env::var("R2_BUCKET")
            .map_err(|_| StorageError::Config("R2_BUCKET not set".to_string()))?;
        let r2_endpoint = std::env::var("R2_ENDPOINT")
            .map_err(|_| StorageError::Config("R2_ENDPOINT not set".to_string()))?;

        Self::new(&r2_access_key, &r2_secret_key, &r2_bucket, &r2_endpoint).await
    }

    fn package_path(name: &str) -> String {
        format!("packages/{}/index.json", name)
    }

    fn tarball_path(name: &str, version: &str) -> String {
        format!("packages/{}/{}.tarball", name, version)
    }

    pub async fn get_package_index(&self, name: &str) -> Result<Option<PackageIndex>, StorageError> {
        let path = Self::package_path(name);
        
        if let Some(cached) = self.cache.read().get(name).cloned() {
            return Ok(Some(cached));
        }

        let result = self.client
            .get_object()
            .bucket(&self.bucket_name)
            .key(&path)
            .send()
            .await;

        match result {
            Ok(output) => {
                let body = output.body.collect().await
                    .map_err(|e| StorageError::R2(e.to_string()))?;
                let content = String::from_utf8(body.to_vec())
                    .map_err(|e| StorageError::R2(e.to_string()))?;
                let index: PackageIndex = serde_json::from_str(&content)?;
                
                self.cache.write().insert(name.to_string(), index.clone());
                Ok(Some(index))
            }
            Err(e) => {
                if e.to_string().contains("NoSuchKey") || e.to_string().contains("404") {
                    Ok(None)
                } else {
                    Err(StorageError::R2(e.to_string()))
                }
            }
        }
    }

    pub async fn save_package_index(&self, name: &str, index: &PackageIndex) -> Result<(), StorageError> {
        let path = Self::package_path(name);
        let content = serde_json::to_string_pretty(index)?;

        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&path)
            .body(ByteStream::from(content.into_bytes()))
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| StorageError::R2(e.to_string()))?;

        self.cache.write().insert(name.to_string(), index.clone());

        Ok(())
    }

    pub async fn upload_tarball(
        &self,
        name: &str,
        version: &str,
        tarball_data: Vec<u8>,
    ) -> Result<String, StorageError> {
        let path = Self::tarball_path(name, version);
        
        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&path)
            .body(ByteStream::from(tarball_data))
            .content_type("application/gzip")
            .send()
            .await
            .map_err(|e| StorageError::R2(e.to_string()))?;

        Ok(format!("/{}", path))
    }

    pub async fn get_tarball(&self, name: &str, version: &str) -> Result<Vec<u8>, StorageError> {
        let path = Self::tarball_path(name, version);
        
        let result = self.client
            .get_object()
            .bucket(&self.bucket_name)
            .key(&path)
            .send()
            .await
            .map_err(|e| StorageError::R2(e.to_string()))?;

        let body = result.body.collect().await
            .map_err(|e| StorageError::R2(e.to_string()))?;
        
        Ok(body.to_vec())
    }

    pub async fn list_packages(&self) -> Result<Vec<String>, StorageError> {
        let mut packages = Vec::new();
        let mut continuation_token = None;

        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&self.bucket_name)
                .prefix("packages/")
                .delimiter("/");

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let result = request.send().await
                .map_err(|e| StorageError::R2(e.to_string()))?;

            for prefix in result.common_prefixes() {
                if let Some(prefix) = prefix.prefix() {
                    let name = prefix.trim_start_matches("packages/")
                        .trim_end_matches('/')
                        .to_string();
                    if !name.is_empty() {
                        packages.push(name);
                    }
                }
            }

            if result.next_continuation_token().is_none() {
                break;
            }
            continuation_token = result.next_continuation_token().map(String::from);
        }

        Ok(packages)
    }

    pub async fn invalidate_cache(&self, paths: &[String]) {
        // CloudFront cache invalidation would go here
        // Requires aws-sdk-cloudfront and distribution ID
        tracing::info!("Cache invalidation requested for: {:?}", paths);
    }
}
