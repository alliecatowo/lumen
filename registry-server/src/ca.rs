//! Certificate Authority for ephemeral signing certificates
//!
//! Implements a Fulcio-like CA that issues short-lived certificates
//! based on OIDC identity verification.

use chrono::{DateTime, Duration, Utc};
use p256::ecdsa::{SigningKey, Signature, VerifyingKey};
use p256::pkcs8::{EncodePrivateKey, LineEnding};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, info, warn};
// Note: Full X.509 certificate generation would require more dependencies
// For now we use a simplified certificate format
use x509_cert::ext::pkix::{BasicConstraints, KeyUsage, KeyUsages, SubjectKeyIdentifier, AuthorityKeyIdentifier};

/// Certificate Authority
#[derive(Debug)]
pub struct CertificateAuthority {
    signing_key: SigningKey,
    certificate_pem: String,
    issued_certs: Arc<RwLock<HashMap<String, IssuedCertificate>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedCertificate {
    pub cert_id: String,
    pub certificate_pem: String,
    pub public_key: String,
    pub identity: String,
    pub issuer: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub log_index: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    pub public_key: String,
    pub oidc_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub cert_id: String,
    pub certificate_pem: String,
    pub issued_at: String,
    pub expires_at: String,
}

impl CertificateAuthority {
    /// Create a new CA with a generated key pair
    pub fn new() -> Result<Self, CaError> {
        let signing_key = SigningKey::random(&mut OsRng);
        
        // Generate self-signed CA certificate
        let certificate_pem = generate_ca_certificate(&signing_key)?;
        
        info!("Certificate Authority initialized");
        
        Ok(Self {
            signing_key,
            certificate_pem,
            issued_certs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create CA from existing private key
    pub fn from_private_key(private_key_pem: &str) -> Result<Self, CaError> {
        use p256::pkcs8::DecodePrivateKey;
        
        let signing_key = SigningKey::from_pkcs8_pem(private_key_pem)
            .map_err(|e| CaError::InvalidKey(e.to_string()))?;
        
        let certificate_pem = generate_ca_certificate(&signing_key)?;
        
        info!("Certificate Authority initialized from existing key");
        
        Ok(Self {
            signing_key,
            certificate_pem,
            issued_certs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Issue a new ephemeral certificate
    pub fn issue_certificate(
        &self,
        public_key_pem: &str,
        identity: &str,
        issuer: &str,
        validity_minutes: i64,
    ) -> Result<IssuedCertificate, CaError> {
        let cert_id = format!("cert-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
            generate_random_hex(8),
            generate_random_hex(4),
            generate_random_hex(4),
            generate_random_hex(4),
            generate_random_hex(12)
        );

        debug!("Issuing certificate {} for identity: {}", cert_id, identity);

        // Parse the public key
        let public_key = parse_public_key(public_key_pem)?;

        // Build certificate
        let now = Utc::now();
        let expires_at = now + Duration::minutes(validity_minutes);

        let certificate_pem = build_certificate(
            &self.signing_key,
            &public_key,
            identity,
            issuer,
            &cert_id,
            now,
            expires_at,
        )?;

        let cert = IssuedCertificate {
            cert_id: cert_id.clone(),
            certificate_pem: certificate_pem.clone(),
            public_key: public_key_pem.to_string(),
            identity: identity.to_string(),
            issuer: issuer.to_string(),
            issued_at: now,
            expires_at,
            log_index: None,
        };

        self.issued_certs.write().insert(cert_id, cert.clone());
        
        info!("Issued certificate {} valid until {}", cert_id, expires_at);

        Ok(cert)
    }

    /// Get a certificate by ID
    pub fn get_certificate(&self, cert_id: &str) -> Option<IssuedCertificate> {
        self.issued_certs.read().get(cert_id).cloned()
    }

    /// Get CA certificate PEM
    pub fn ca_certificate(&self) -> &str {
        &self.certificate_pem
    }

    /// Verify a signature with a certificate
    pub fn verify_signature(
        &self,
        _cert_pem: &str,
        _message: &[u8],
        _signature: &[u8],
    ) -> Result<bool, CaError> {
        // TODO: Implement proper signature verification
        // For now, return true (in production, extract public key from cert and verify)
        Ok(true)
    }

    /// Cleanup expired certificates
    pub fn cleanup_expired(&self) {
        let mut certs = self.issued_certs.write();
        let now = Utc::now();
        let expired: Vec<String> = certs
            .iter()
            .filter(|(_, c)| c.expires_at < now)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in expired {
            certs.remove(&id);
            debug!("Cleaned up expired certificate: {}", id);
        }
    }
}

/// Generate a self-signed CA certificate
fn generate_ca_certificate(signing_key: &SigningKey) -> Result<String, CaError> {
    // For simplicity, return a placeholder CA cert
    // In production, this would be a proper X.509 CA certificate
    let verifying_key = VerifyingKey::from(signing_key);
    let public_key_bytes = verifying_key.to_sec1_bytes();
    
    // Generate a simple self-signed certificate
    let cert_pem = format!(
        "-----BEGIN CERTIFICATE-----\n\
        MIIBkTCB+wIJAKHBfpE\n\
        (CA Certificate for Wares Registry - Placeholder)\n\
        Subject: CN=Wares Registry CA\n\
        Issuer: CN=Wares Registry CA\n\
        {}\n\
        -----END CERTIFICATE-----",
        base64::encode(&public_key_bytes)
    );
    
    Ok(cert_pem)
}

/// Build an ephemeral certificate
fn build_certificate(
    ca_signing_key: &SigningKey,
    public_key: &VerifyingKey,
    subject_identity: &str,
    issuer: &str,
    serial: &str,
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
) -> Result<String, CaError> {
    // For production, this would use x509-cert crate properly
    // For now, return a structured certificate that includes all the info
    
    let public_key_bytes = public_key.to_sec1_bytes();
    let public_key_b64 = base64::encode(&public_key_bytes);
    
    let cert_data = format!(
        r#"{{
  "cert_id": "{}",
  "subject": "{}",
  "issuer": "{}",
  "not_before": "{}",
  "not_after": "{}",
  "public_key": "{}",
  "key_algorithm": "ECDSA P-256"
}}"#,
        serial,
        subject_identity,
        issuer,
        not_before.to_rfc3339(),
        not_after.to_rfc3339(),
        public_key_b64
    );
    
    // Sign the certificate data
    use p256::ecdsa::signature::Signer;
    let signature: Signature = ca_signing_key.sign(cert_data.as_bytes());
    let signature_b64 = base64::encode(signature.to_der().as_bytes());
    
    let certificate_pem = format!(
        "-----BEGIN WARES CERTIFICATE-----\n{}\n\n-----BEGIN SIGNATURE-----\n{}\n-----END WARES CERTIFICATE-----",
        base64::encode(cert_data),
        signature_b64
    );
    
    Ok(certificate_pem)
}

/// Parse a public key from PEM
fn parse_public_key(pem: &str) -> Result<VerifyingKey, CaError> {
    // Try to parse as PEM
    if pem.contains("BEGIN PUBLIC KEY") {
        use p256::pkcs8::DecodePublicKey;
        VerifyingKey::from_public_key_pem(pem)
            .map_err(|e| CaError::InvalidKey(format!("Failed to parse PEM: {}", e)))
    } else if pem.contains("BEGIN EC PUBLIC KEY") {
        // Handle SEC1 format
        use p256::elliptic_curve::sec1::EncodedPoint;
        // Parse SEC1 format
        let decoded = pem.trim()
            .lines()
            .filter(|l| !l.starts_with("---"))
            .collect::<String>();
        let bytes = base64::decode(&decoded)
            .map_err(|e| CaError::InvalidKey(format!("Base64 decode failed: {}", e)))?;
        
        VerifyingKey::from_sec1_bytes(&bytes)
            .map_err(|e| CaError::InvalidKey(format!("SEC1 parse failed: {}", e)))
    } else {
        // Try base64 raw key
        let bytes = base64::decode(pem.trim())
            .map_err(|e| CaError::InvalidKey(format!("Base64 decode failed: {}", e)))?;
        
        VerifyingKey::from_sec1_bytes(&bytes)
            .map_err(|e| CaError::InvalidKey(format!("SEC1 parse failed: {}", e)))
    }
}

/// Extract public key from certificate
fn extract_public_key_from_cert(cert_pem: &str) -> Result<Vec<u8>, CaError> {
    // Parse the certificate and extract public key
    let lines: Vec<&str> = cert_pem.lines().collect();
    let mut in_cert = false;
    let mut cert_b64 = String::new();
    
    for line in lines {
        if line.contains("BEGIN WARES CERTIFICATE") {
            in_cert = true;
            continue;
        }
        if line.contains("BEGIN SIGNATURE") {
            break;
        }
        if in_cert {
            cert_b64.push_str(line.trim());
        }
    }
    
    let cert_data = base64::decode(&cert_b64)
        .map_err(|e| CaError::InvalidCertificate(format!("Base64 decode failed: {}", e)))?;
    
    let cert_json: serde_json::Value = serde_json::from_slice(&cert_data)
        .map_err(|e| CaError::InvalidCertificate(format!("JSON parse failed: {}", e)))?;
    
    let public_key_pem = cert_json.get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CaError::InvalidCertificate("Missing public_key field".to_string()))?;
    
    // Parse the PEM to get raw bytes
    let key_lines: Vec<&str> = public_key_pem.lines()
        .filter(|l| !l.starts_with("---") && !l.trim().is_empty())
        .collect();
    let key_b64 = key_lines.join("");
    
    base64::decode(&key_b64)
        .map_err(|e| CaError::InvalidKey(format!("Key decode failed: {}", e)))
}

fn generate_random_hex(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum CaError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),
    #[error("Invalid certificate: {0}")]
    InvalidCertificate(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("PKCS8 error: {0}")]
    Pkcs8(String),
    #[error("X509 error: {0}")]
    X509(String),
}
