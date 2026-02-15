//! Storage backend for wares (R2/S3-compatible)
//!
//! Moved from src/registry.rs (R2Client)

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2Config {
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    pub public_url: Option<String>,
}

impl R2Config {
    pub fn new(account_id: String, access_key_id: String, secret_access_key: String) -> Self {
        Self {
            account_id,
            access_key_id,
            secret_access_key,
            bucket: "lumen-registry".to_string(), // Default bucket
            public_url: None,
        }
    }

    pub fn with_bucket(mut self, bucket: &str) -> Self {
        self.bucket = bucket.to_string();
        self
    }
}

pub struct R2Client {
    config: R2Config,
    http: Client,
}

impl R2Client {
    pub fn new(config: R2Config) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    /// Upload an artifact to R2
    /// Returns the public URL if successful
    pub fn upload_artifact(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, String> {
        // TODO: Implement AWS Signature V4 signing for R2
        // For now, this is a placeholder structure based on the previous registry.rs
        Err("Not implemented: R2 upload with SigV4".to_string())
    }

    /// Download an artifact from R2
    pub fn download_artifact(&self, key: &str) -> Result<Vec<u8>, String> {
        let url = format!(
            "https://{}.r2.cloudflarestorage.com/{}/{}",
            self.config.account_id, self.config.bucket, key
        );
        
        // This also needs SigV4 if the bucket is private
        // If public, use public_url
        
        Err("Not implemented: R2 download".to_string())
    }
}
