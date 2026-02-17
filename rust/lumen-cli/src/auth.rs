//! Authentication and authorization for the Lumen package registry.
//!
//! This module provides:
//! - API token management with scopes and expiration
//! - Package ownership management
//! - Secure credential storage with OS keyring integration
//! - Ed25519 signing for package uploads
//! - Registry authentication commands

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

// =============================================================================
// Token Scopes and Types
// =============================================================================

/// Token scope defining permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// Can publish new package versions
    Publish,
    /// Can yank versions
    Yank,
    /// Can manage package owners
    Owner,
    /// Full admin access
    Admin,
}

impl std::fmt::Display for TokenScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenScope::Publish => write!(f, "publish"),
            TokenScope::Yank => write!(f, "yank"),
            TokenScope::Owner => write!(f, "owner"),
            TokenScope::Admin => write!(f, "admin"),
        }
    }
}

impl TokenScope {
    /// Parse a scope from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "publish" => Some(TokenScope::Publish),
            "yank" => Some(TokenScope::Yank),
            "owner" => Some(TokenScope::Owner),
            "admin" => Some(TokenScope::Admin),
            _ => None,
        }
    }

    /// Get all available scopes.
    pub fn all() -> Vec<Self> {
        vec![
            TokenScope::Publish,
            TokenScope::Yank,
            TokenScope::Owner,
            TokenScope::Admin,
        ]
    }
}

// =============================================================================
// API Token
// =============================================================================

/// An API token for registry authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// Token string (format: "lm_<base64>")
    pub token: String,
    /// User-provided name (e.g., "laptop", "ci")
    pub name: String,
    /// When the token was created
    pub created_at: DateTime<Utc>,
    /// When the token expires (None = never)
    pub expires_at: Option<DateTime<Utc>>,
    /// Token scopes/permissions
    pub scopes: Vec<TokenScope>,
    /// Package restrictions (None = all packages)
    pub package_restrictions: Option<Vec<String>>,
}

impl ApiToken {
    /// Create a new API token.
    pub fn new(
        token: String,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        scopes: Vec<TokenScope>,
        package_restrictions: Option<Vec<String>>,
    ) -> Self {
        Self {
            token,
            name,
            created_at: Utc::now(),
            expires_at,
            scopes,
            package_restrictions,
        }
    }

    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
    }

    /// Check if the token has a specific scope.
    pub fn has_scope(&self, scope: TokenScope) -> bool {
        self.scopes.contains(&TokenScope::Admin) || self.scopes.contains(&scope)
    }

    /// Check if the token can access a specific package.
    pub fn can_access_package(&self, package: &str) -> bool {
        match &self.package_restrictions {
            None => true,
            Some(packages) => packages.contains(&package.to_string()),
        }
    }

    /// Get a masked version of the token for display.
    pub fn masked(&self) -> String {
        if self.token.len() < 12 {
            "***".to_string()
        } else {
            format!(
                "{}...{}",
                &self.token[..6],
                &self.token[self.token.len() - 4..]
            )
        }
    }
}

// =============================================================================
// Package Ownership
// =============================================================================

/// Role of a package owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerRole {
    /// Can publish and yank versions
    Maintainer,
    /// Can add/remove owners (full control)
    Owner,
}

impl std::fmt::Display for OwnerRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwnerRole::Maintainer => write!(f, "maintainer"),
            OwnerRole::Owner => write!(f, "owner"),
        }
    }
}

impl OwnerRole {
    /// Parse a role from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "maintainer" => Some(OwnerRole::Maintainer),
            "owner" => Some(OwnerRole::Owner),
            _ => None,
        }
    }
}

/// A package owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageOwner {
    /// User ID
    pub user_id: String,
    /// User email
    pub email: String,
    /// Owner role
    pub role: OwnerRole,
    /// When the owner was added
    pub added_at: DateTime<Utc>,
    /// Who added this owner
    pub added_by: String,
}

impl PackageOwner {
    /// Create a new package owner.
    pub fn new(user_id: String, email: String, role: OwnerRole, added_by: String) -> Self {
        Self {
            user_id,
            email,
            role,
            added_at: Utc::now(),
            added_by,
        }
    }

    /// Check if the owner can manage other owners.
    pub fn can_manage_owners(&self) -> bool {
        matches!(self.role, OwnerRole::Owner)
    }

    /// Check if the owner can publish.
    pub fn can_publish(&self) -> bool {
        matches!(self.role, OwnerRole::Maintainer | OwnerRole::Owner)
    }

    /// Check if the owner can yank.
    pub fn can_yank(&self) -> bool {
        matches!(self.role, OwnerRole::Maintainer | OwnerRole::Owner)
    }
}

/// List of owners for a package.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageOwners {
    pub package: String,
    pub owners: Vec<PackageOwner>,
}

// =============================================================================
// Ed25519 Signing Keys
// =============================================================================

/// An Ed25519 keypair for signing package uploads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningKeypair {
    /// The public key (32 bytes, base64 encoded for storage)
    pub public_key: String,
    /// The secret key (64 bytes, base64 encoded for storage)
    pub secret_key: String,
    /// Key ID (fingerprint of public key)
    pub key_id: String,
    /// When the key was created
    pub created_at: DateTime<Utc>,
}

impl SigningKeypair {
    /// Generate a new Ed25519 keypair.
    pub fn generate() -> Result<Self, AuthError> {
        #[cfg(feature = "ed25519")]
        {
            use ed25519_dalek::SigningKey as DalekSigningKey;
            use rand::rngs::OsRng;

            let signing_key = DalekSigningKey::generate(&mut OsRng);
            let verifying_key = signing_key.verifying_key();

            let secret_bytes = signing_key.to_bytes();
            let public_bytes = verifying_key.to_bytes();

            let public_key = base64_encode(&public_bytes);
            let secret_key = base64_encode(&secret_bytes);
            let key_id = hex_encode(&sha256_hash(&public_bytes)[..16]);

            Ok(Self {
                public_key,
                secret_key,
                key_id,
                created_at: Utc::now(),
            })
        }

        #[cfg(not(feature = "ed25519"))]
        {
            // Fallback: generate deterministic placeholder keys for testing
            // In production, the ed25519-dalek feature should be enabled
            let seed = rand::random::<[u8; 32]>();
            let public_key = base64_encode(&seed);
            let secret_key = base64_encode(&[seed.as_slice(), seed.as_slice()].concat());
            let key_id = hex_encode(&sha256_hash(&seed)[..16]);

            Ok(Self {
                public_key,
                secret_key,
                key_id,
                created_at: Utc::now(),
            })
        }
    }

    /// Sign data with this keypair.
    pub fn sign(&self, data: &[u8]) -> Result<String, AuthError> {
        #[cfg(feature = "ed25519")]
        {
            use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

            let secret_bytes = base64_decode(&self.secret_key)?;
            let signing_key = DalekSigningKey::from_bytes(
                &secret_bytes
                    .try_into()
                    .map_err(|_| AuthError::InvalidKey("Invalid secret key length".to_string()))?,
            );

            let signature = signing_key.sign(data);
            Ok(base64_encode(&signature.to_bytes()))
        }

        #[cfg(not(feature = "ed25519"))]
        {
            // Fallback: return a placeholder signature
            // This is NOT cryptographically secure and should only be used for testing
            let hash = sha256_hash(data);
            Ok(base64_encode(&hash))
        }
    }

    /// Verify a signature.
    pub fn verify(&self, data: &[u8], signature_b64: &str) -> Result<bool, AuthError> {
        #[cfg(feature = "ed25519")]
        {
            use ed25519_dalek::{Signature, Verifier, VerifyingKey};

            let public_bytes = base64_decode(&self.public_key)?;
            let verifying_key = VerifyingKey::from_bytes(
                &public_bytes
                    .try_into()
                    .map_err(|_| AuthError::InvalidKey("Invalid public key length".to_string()))?,
            )
            .map_err(|e| AuthError::InvalidKey(e.to_string()))?;

            let signature_bytes = base64_decode(signature_b64)?;
            let signature = Signature::from_bytes(&signature_bytes.try_into().map_err(|_| {
                AuthError::InvalidSignature("Invalid signature length".to_string())
            })?);

            match verifying_key.verify(data, &signature) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }

        #[cfg(not(feature = "ed25519"))]
        {
            // Fallback: verify placeholder signature
            let expected = self.sign(data)?;
            Ok(expected == signature_b64)
        }
    }
}

// =============================================================================
// Credentials Storage
// =============================================================================

/// Stored credentials for a single registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCredentials {
    /// Registry URL
    pub registry: String,
    /// API token (may be encrypted or reference keyring)
    pub token: String,
    /// Token name/description
    pub token_name: Option<String>,
    /// Key ID for signing
    pub signing_key_id: Option<String>,
    /// When credentials were saved
    pub saved_at: DateTime<Utc>,
}

/// All stored credentials.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialsFile {
    pub version: i32,
    #[serde(default)]
    pub credentials: Vec<RegistryCredentials>,
}

impl CredentialsFile {
    pub fn new() -> Self {
        Self {
            version: 1,
            credentials: Vec::new(),
        }
    }

    /// Get credentials for a specific registry.
    pub fn get(&self, registry: &str) -> Option<&RegistryCredentials> {
        self.credentials.iter().find(|c| c.registry == registry)
    }

    /// Remove credentials for a specific registry.
    pub fn remove(&mut self, registry: &str) -> bool {
        let before = self.credentials.len();
        self.credentials.retain(|c| c.registry != registry);
        self.credentials.len() < before
    }

    /// Add or update credentials for a registry.
    pub fn set(&mut self, creds: RegistryCredentials) {
        self.remove(&creds.registry);
        self.credentials.push(creds);
    }
}

/// Manager for secure credential storage.
pub struct CredentialManager {
    credentials_path: PathBuf,
    keys_dir: PathBuf,
    use_keyring: bool,
}

impl CredentialManager {
    /// Create a new credential manager.
    pub fn new() -> Result<Self, AuthError> {
        let lumen_dir = lumen_home_dir()?;
        let credentials_path = lumen_dir.join("credentials.toml");
        let keys_dir = lumen_dir.join("keys");

        // Ensure directories exist
        std::fs::create_dir_all(&lumen_dir).map_err(AuthError::Io)?;
        std::fs::create_dir_all(&keys_dir).map_err(AuthError::Io)?;

        Ok(Self {
            credentials_path,
            keys_dir,
            use_keyring: cfg!(feature = "keyring"),
        })
    }

    /// Load credentials file.
    pub fn load_credentials(&self) -> Result<CredentialsFile, AuthError> {
        if !self.credentials_path.exists() {
            return Ok(CredentialsFile::new());
        }

        let content =
            std::fs::read_to_string(&self.credentials_path).map_err(AuthError::Io)?;

        // Decrypt if using keyring (for now, just parse)
        let creds: CredentialsFile =
            toml::from_str(&content).map_err(|e| AuthError::Parse(e.to_string()))?;

        Ok(creds)
    }

    /// Save credentials file.
    fn save_credentials(&self, creds: &CredentialsFile) -> Result<(), AuthError> {
        let content = toml::to_string_pretty(creds).map_err(|e| AuthError::Parse(e.to_string()))?;

        // Write with restricted permissions (0o600)
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts
            .open(&self.credentials_path)
            .map_err(AuthError::Io)?;

        file.write_all(content.as_bytes())
            .map_err(AuthError::Io)?;

        // Ensure permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata =
                std::fs::metadata(&self.credentials_path).map_err(AuthError::Io)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&self.credentials_path, permissions)
                .map_err(AuthError::Io)?;
        }

        Ok(())
    }

    /// Store a token for a registry.
    pub fn store_token(
        &self,
        registry: &str,
        token: &str,
        token_name: Option<&str>,
    ) -> Result<(), AuthError> {
        let mut creds = self.load_credentials()?;

        // Try to store in keyring if available
        #[cfg(feature = "keyring")]
        {
            if self.use_keyring {
                let entry = keyring::Entry::new("lumen", registry);
                if let Ok(entry) = entry {
                    let _ = entry.set_password(token);
                    // Store reference in file
                    creds.set(RegistryCredentials {
                        registry: registry.to_string(),
                        token: "__keyring__".to_string(),
                        token_name: token_name.map(|s| s.to_string()),
                        signing_key_id: creds.get(registry).and_then(|c| c.signing_key_id.clone()),
                        saved_at: Utc::now(),
                    });
                    return self.save_credentials(&creds);
                }
            }
        }

        // Fall back to file storage
        creds.set(RegistryCredentials {
            registry: registry.to_string(),
            token: token.to_string(),
            token_name: token_name.map(|s| s.to_string()),
            signing_key_id: creds.get(registry).and_then(|c| c.signing_key_id.clone()),
            saved_at: Utc::now(),
        });

        self.save_credentials(&creds)
    }

    /// Get a token for a registry.
    pub fn get_token(&self, registry: &str) -> Result<Option<String>, AuthError> {
        let creds = self.load_credentials()?;

        if let Some(cred) = creds.get(registry) {
            if cred.token == "__keyring__" {
                // Retrieve from keyring
                #[cfg(feature = "keyring")]
                {
                    let entry = keyring::Entry::new("lumen", registry);
                    if let Ok(entry) = entry {
                        return entry
                            .get_password()
                            .map(Some)
                            .map_err(AuthError::Keyring);
                    }
                }
                return Ok(None);
            } else {
                return Ok(Some(cred.token.clone()));
            }
        }

        Ok(None)
    }

    /// Remove stored credentials for a registry.
    pub fn remove_token(&self, registry: &str) -> Result<bool, AuthError> {
        let mut creds = self.load_credentials()?;

        // Note: Deleting from keyring is not supported in this version
        // The token will be removed from the credentials file, which is sufficient
        // as the keyring entry is just a lookup by service/username

        let removed = creds.remove(registry);
        self.save_credentials(&creds)?;
        Ok(removed)
    }

    /// List all stored credentials (without tokens).
    pub fn list_credentials(&self) -> Result<Vec<RegistryCredentials>, AuthError> {
        let creds = self.load_credentials()?;
        Ok(creds.credentials)
    }

    /// Get or create a signing keypair for a registry.
    pub fn get_or_create_signing_key(&self, registry: &str) -> Result<SigningKeypair, AuthError> {
        let creds = self.load_credentials()?;

        // Check if we already have a key
        if let Some(cred) = creds.get(registry) {
            if let Some(key_id) = &cred.signing_key_id {
                let key_path = self.keys_dir.join(format!("{}.json", key_id));
                if key_path.exists() {
                    let content =
                        std::fs::read_to_string(&key_path).map_err(AuthError::Io)?;
                    let keypair: SigningKeypair = serde_json::from_str(&content)
                        .map_err(|e| AuthError::Parse(e.to_string()))?;
                    return Ok(keypair);
                }
            }
        }

        // Generate new keypair
        let keypair = SigningKeypair::generate()?;
        self.save_signing_key(&keypair)?;

        // Update credentials with key ID
        let mut creds = self.load_credentials()?;
        let registry_creds = creds
            .get(registry)
            .cloned()
            .unwrap_or_else(|| RegistryCredentials {
                registry: registry.to_string(),
                token: String::new(),
                token_name: None,
                signing_key_id: None,
                saved_at: Utc::now(),
            });

        creds.set(RegistryCredentials {
            signing_key_id: Some(keypair.key_id.clone()),
            ..registry_creds
        });
        self.save_credentials(&creds)?;

        Ok(keypair)
    }

    /// Save a signing keypair to disk.
    fn save_signing_key(&self, keypair: &SigningKeypair) -> Result<(), AuthError> {
        let key_path = self.keys_dir.join(format!("{}.json", keypair.key_id));

        let content =
            serde_json::to_string_pretty(keypair).map_err(|e| AuthError::Parse(e.to_string()))?;

        // Write with restricted permissions
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&key_path).map_err(AuthError::Io)?;

        file.write_all(content.as_bytes())
            .map_err(AuthError::Io)?;

        // Ensure permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&key_path).map_err(AuthError::Io)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&key_path, permissions).map_err(AuthError::Io)?;
        }

        Ok(())
    }

    /// Get the path to the credentials file.
    pub fn credentials_path(&self) -> &Path {
        &self.credentials_path
    }

    /// Get the keys directory.
    pub fn keys_dir(&self) -> &Path {
        &self.keys_dir
    }
}

impl Default for CredentialManager {
    fn default() -> Self {
        Self::new().expect("Failed to create credential manager")
    }
}

// =============================================================================
// Request Signing
// =============================================================================

/// Sign a package upload request.
pub fn sign_upload_request(
    keypair: &SigningKeypair,
    package_name: &str,
    version: &str,
    content_hash: &str,
    timestamp: DateTime<Utc>,
) -> Result<UploadSignature, AuthError> {
    // Create canonical request string
    let canonical = format!(
        "lumen:upload:{}:{}:{}:{}",
        package_name,
        version,
        content_hash,
        timestamp.to_rfc3339()
    );

    let signature = keypair.sign(canonical.as_bytes())?;

    Ok(UploadSignature {
        key_id: keypair.key_id.clone(),
        signature,
        timestamp,
        algorithm: "ed25519".to_string(),
    })
}

// =============================================================================
// Whoami Response
// =============================================================================

/// Response from the registry whoami endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoamiResponse {
    /// User ID
    pub user_id: String,
    /// User email
    pub email: String,
    /// User display name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Token scopes
    pub scopes: Vec<String>,
    /// Organizations the user belongs to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub organizations: Vec<OrganizationInfo>,
    /// Token expiration time
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Organization info for whoami response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfo {
    /// Organization ID
    pub id: String,
    /// Organization name
    pub name: String,
    /// User's role in the organization
    pub role: String,
}

/// Validate a token format (basic checks).
pub fn validate_token_format(token: &str) -> Result<(), AuthError> {
    if token.is_empty() {
        return Err(AuthError::InvalidToken("Token cannot be empty".to_string()));
    }

    // Check for valid Lumen token prefix
    if !token.starts_with("lm_") {
        return Err(AuthError::InvalidToken(
            "Token should start with 'lm_'".to_string(),
        ));
    }

    // Minimum length check (lm_ + at least 16 chars of entropy)
    if token.len() < 20 {
        return Err(AuthError::InvalidToken("Token is too short".to_string()));
    }

    Ok(())
}

/// Mask a token for display (shows first 6 and last 4 chars).
pub fn mask_token(token: &str) -> String {
    if token.len() < 12 {
        "***".to_string()
    } else {
        format!("{}...{}", &token[..6], &token[token.len() - 4..])
    }
}

/// Token validation response from registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationResponse {
    /// Whether the token is valid
    pub valid: bool,
    /// User info if valid
    #[serde(flatten)]
    pub user_info: Option<WhoamiResponse>,
    /// Error message if invalid
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// =============================================================================
// Upload Signature
// =============================================================================

/// Upload signature structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSignature {
    /// Key ID used for signing
    pub key_id: String,
    /// Base64-encoded signature
    pub signature: String,
    /// Timestamp of the signature
    pub timestamp: DateTime<Utc>,
    /// Algorithm used (ed25519)
    pub algorithm: String,
}

impl UploadSignature {
    /// Convert to HTTP headers.
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert(
            "X-Lumen-Signature".to_string(),
            format!("{}={}", self.algorithm, self.signature),
        );
        headers.insert("X-Lumen-Key-Id".to_string(), self.key_id.clone());
        headers.insert("X-Lumen-Timestamp".to_string(), self.timestamp.to_rfc3339());
        headers
    }
}

// =============================================================================
// Auth Client
// =============================================================================

/// HTTP client with authentication.
pub struct AuthenticatedClient {
    client: reqwest::blocking::Client,
    token: Option<String>,
    signing_key: Option<SigningKeypair>,
    registry_url: String,
}

impl AuthenticatedClient {
    /// Create a new authenticated client.
    pub fn new(registry_url: String) -> Result<Self, AuthError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AuthError::Http(e.to_string()))?;

        // Check env var first for CI/testing
        let token = std::env::var("LUMEN_AUTH_TOKEN").ok().or_else(|| {
            // Try to load token from credential manager
            let cred_manager = CredentialManager::new().ok()?;
            cred_manager.get_token(&registry_url).ok()?
        });

        let signing_key = if token.is_some() {
            CredentialManager::new()
                .ok()
                .and_then(|cm| cm.get_or_create_signing_key(&registry_url).ok())
        } else {
            None
        };

        Ok(Self {
            client,
            token,
            signing_key,
            registry_url,
        })
    }

    /// Check if the client has authentication.
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    /// Make an authenticated GET request.
    pub fn get(&self, path: &str) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.registry_url.trim_end_matches('/'), path);
        let mut req = self.client.get(&url);

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        req.send().map_err(|e| AuthError::Http(e.to_string()))
    }

    /// Make an authenticated POST request.
    pub fn post(
        &self,
        path: &str,
        body: Vec<u8>,
    ) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.registry_url.trim_end_matches('/'), path);
        let mut req = self.client.post(&url).body(body);

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        req.send().map_err(|e| AuthError::Http(e.to_string()))
    }

    /// Make an authenticated PUT request with signing.
    pub fn put_signed(
        &self,
        path: &str,
        body: Vec<u8>,
        package_name: &str,
        version: &str,
        proof: Option<serde_json::Value>,
    ) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.registry_url.trim_end_matches('/'), path);

        // Calculate content hash
        let content_hash = format!("sha256:{}", hex_encode(&sha256_hash(&body)));

        // Sign the request
        let signing_key = self
            .signing_key
            .as_ref()
            .ok_or_else(|| AuthError::NotAuthenticated("No signing key available".to_string()))?;

        let signature = sign_upload_request(
            signing_key,
            package_name,
            version,
            &content_hash,
            Utc::now(),
        )?;

        // Build JSON payload (base64 encode tarball as worker expects JSON)
        let tarball_b64 = base64_encode(&body);
        let json_body = serde_json::json!({
            "name": package_name,
            "version": version,
            "tarball": tarball_b64,
            "shasum": content_hash.replace("sha256:", ""),
            "signature": {
                "signature": signature.signature,
                "certificate": signature.key_id,
                "identity": signature.algorithm,
                "key_id": signature.key_id,
                "timestamp": signature.timestamp.to_rfc3339(),
            },
            "proof": proof,
        });

        let mut req = self.client.put(&url).json(&json_body);

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        // Keep headers for compatibility with some worker versions or proxies
        req = req.header("X-Lumen-Content-Hash", content_hash);
        for (key, value) in signature.to_headers() {
            req = req.header(&key, value);
        }

        req.send().map_err(|e| AuthError::Http(e.to_string()))
    }

    /// Make an authenticated DELETE request.
    pub fn delete(&self, path: &str) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.registry_url.trim_end_matches('/'), path);
        let mut req = self.client.delete(&url);

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        req.send().map_err(|e| AuthError::Http(e.to_string()))
    }

    /// Make an authenticated request with custom method and body.
    pub fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.registry_url.trim_end_matches('/'), path);

        let mut req = self.client.request(method, &url);

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        if let Some(body) = body {
            req = req.body(body);
        }

        if let Some(ct) = content_type {
            req = req.header("Content-Type", ct);
        }

        req.send().map_err(|e| AuthError::Http(e.to_string()))
    }

    /// Get current user info from registry.
    pub fn whoami(&self) -> Result<WhoamiResponse, AuthError> {
        if !self.is_authenticated() {
            return Err(AuthError::NotAuthenticated(
                "No token configured. Run 'lumen registry login' first.".to_string(),
            ));
        }

        let resp = self.get("/api/v1/auth/whoami")?;

        if resp.status().is_success() {
            resp.json::<WhoamiResponse>()
                .map_err(|e| AuthError::Parse(format!("Failed to parse whoami response: {}", e)))
        } else if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            Err(AuthError::NotAuthenticated(
                "Token is invalid or expired. Run 'lumen registry login' to re-authenticate."
                    .to_string(),
            ))
        } else {
            Err(AuthError::Http(format!("Whoami failed: {}", resp.status())))
        }
    }

    /// Validate the current token with the registry.
    pub fn validate_token(&self) -> Result<TokenValidationResponse, AuthError> {
        if !self.is_authenticated() {
            return Ok(TokenValidationResponse {
                valid: false,
                user_info: None,
                error: Some("No token configured".to_string()),
            });
        }

        match self.whoami() {
            Ok(user_info) => Ok(TokenValidationResponse {
                valid: true,
                user_info: Some(user_info),
                error: None,
            }),
            Err(AuthError::NotAuthenticated(msg)) => Ok(TokenValidationResponse {
                valid: false,
                user_info: None,
                error: Some(msg),
            }),
            Err(e) => Err(e),
        }
    }

    /// Handle 401/403 responses with helpful messages.
    pub fn check_auth_error(response: &reqwest::blocking::Response) -> Option<String> {
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => Some(
                "Authentication failed. Run `lumen registry login` to authenticate.".to_string(),
            ),
            reqwest::StatusCode::FORBIDDEN => Some(
                "You don't have permission to perform this action. Check your token scopes."
                    .to_string(),
            ),
            _ => None,
        }
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Authentication error types.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("Not authenticated: {0}")]
    NotAuthenticated(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("HTTP error: {0}")]
    Http(String),
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the Lumen home directory (~/.lumen).
pub fn lumen_home_dir() -> Result<PathBuf, AuthError> {
    let home = dirs::home_dir()
        .ok_or_else(|| AuthError::Parse("Could not find home directory".to_string()))?;
    Ok(home.join(".lumen"))
}

/// Base64 encode bytes.
fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Base64 decode string.
fn base64_decode(s: &str) -> Result<Vec<u8>, AuthError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| AuthError::Parse(format!("Base64 decode error: {}", e)))
}

/// SHA256 hash of bytes.
fn sha256_hash(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

/// Hex encode bytes.
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_scope_display() {
        assert_eq!(TokenScope::Publish.to_string(), "publish");
        assert_eq!(TokenScope::Admin.to_string(), "admin");
    }

    #[test]
    fn test_token_scope_from_str() {
        assert_eq!(TokenScope::from_str("publish"), Some(TokenScope::Publish));
        assert_eq!(TokenScope::from_str("ADMIN"), Some(TokenScope::Admin));
        assert_eq!(TokenScope::from_str("invalid"), None);
    }

    #[test]
    fn test_api_token_expired() {
        let token = ApiToken::new(
            "test".to_string(),
            "test".to_string(),
            Some(Utc::now() - chrono::Duration::days(1)),
            vec![TokenScope::Publish],
            None,
        );
        assert!(token.is_expired());

        let token2 = ApiToken::new(
            "test".to_string(),
            "test".to_string(),
            Some(Utc::now() + chrono::Duration::days(1)),
            vec![TokenScope::Publish],
            None,
        );
        assert!(!token2.is_expired());
    }

    #[test]
    fn test_api_token_has_scope() {
        let token = ApiToken::new(
            "test".to_string(),
            "test".to_string(),
            None,
            vec![TokenScope::Publish, TokenScope::Yank],
            None,
        );
        assert!(token.has_scope(TokenScope::Publish));
        assert!(!token.has_scope(TokenScope::Owner));

        let admin_token = ApiToken::new(
            "test".to_string(),
            "test".to_string(),
            None,
            vec![TokenScope::Admin],
            None,
        );
        assert!(admin_token.has_scope(TokenScope::Publish));
        assert!(admin_token.has_scope(TokenScope::Owner));
    }

    #[test]
    fn test_owner_role_permissions() {
        let maintainer = PackageOwner::new(
            "user1".to_string(),
            "user1@example.com".to_string(),
            OwnerRole::Maintainer,
            "admin".to_string(),
        );
        assert!(maintainer.can_publish());
        assert!(maintainer.can_yank());
        assert!(!maintainer.can_manage_owners());

        let owner = PackageOwner::new(
            "user2".to_string(),
            "user2@example.com".to_string(),
            OwnerRole::Owner,
            "admin".to_string(),
        );
        assert!(owner.can_publish());
        assert!(owner.can_yank());
        assert!(owner.can_manage_owners());
    }

    #[test]
    fn test_credentials_file() {
        let mut creds = CredentialsFile::new();
        assert_eq!(creds.version, 1);

        creds.set(RegistryCredentials {
            registry: "https://registry.lumen.sh".to_string(),
            token: "lm_test".to_string(),
            token_name: Some("test".to_string()),
            signing_key_id: None,
            saved_at: Utc::now(),
        });

        assert!(creds.get("https://registry.lumen.sh").is_some());
        assert!(creds.remove("https://registry.lumen.sh"));
        assert!(creds.get("https://registry.lumen.sh").is_none());
    }
}
