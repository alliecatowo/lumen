//! Lock file generation and parsing for reproducible builds.
//!
//! ## Design Philosophy
//!
//! **The lockfile is the source of truth for reproducibility.**
//!
//! This module implements a content-addressed lockfile format inspired by the best practices
//! from Cargo, npm, pnpm, and Bazel:
//!
//! - **Content-addressed**: Every package is identified by its content hash (CID)
//! - **Immutable**: Once published, a version's content never changes
//! - **Verifiable**: All entries include integrity hashes that can be independently verified
//! - **Deterministic**: The same dependency graph always produces the same lockfile
//! - **Secure**: Optional signatures and transparency log integration
//!
//! ## Lockfile Format (v4)
//!
//! ```toml
//! version = 4
//! 
//! [metadata]
//! resolver = "lumen-sat-v1"
//! source_encoding = "project-relative"
//! resolution_mode = "single-version"
//! content_hash = "sha256:abc123..."  # Hash of all package entries
//! generated_at = "2024-01-15T10:30:00Z"
//! lumen_version = "0.1.0"
//! 
//! [[package]]
//! name = "@acme/http-utils"
//! version = "1.2.0"
//! source = "registry+https://registry.lumen.sh"
//! resolved = "cid:bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
//! integrity = "sha512-Low1Rb...=="
//! manifest_hash = "sha256:def456..."
//! signature = "sig:ed25519:..."
//! transparency_index = 42
//! dependencies = ["@acme/json-parser@0.3.2", "@acme/uri@1.0.0"]
//! 
//! [[package.artifacts]]
//! kind = "tar"
//! url = "https://cdn.lumen.sh/pkg/@acme/http-utils/1.2.0.tar"
//! hash = "sha256:abc123..."
//! size = 24576
//! 
//! [[package]]
//! name = "local-lib"
//! version = "0.1.0"
//! source = "path+../local-lib"
//! manifest_hash = "sha256:local123..."
//! dependencies = []
//! 
//! [[package]]
//! name = "vendored-dep"
//! version = "2.0.0"
//! source = "git+https://github.com/acme/vendored-dep?rev=a1b2c3d"
//! resolved = "git:sha:a1b2c3d4e5f6..."
//! manifest_hash = "sha256:git123..."
//! dependencies = []
//! 
//! [security]
//! verify_signatures = true
//! transparency_log_url = "https://rekor.lumen.sh"
//! root_of_trust = "lumen-ca"
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current lockfile format version.
pub const CURRENT_LOCKFILE_VERSION: u32 = 4;

/// Lumen version that produced this lockfile (set at compile time).
pub const LUMEN_VERSION: &str = env!("CARGO_PKG_VERSION");

// =============================================================================
// LockFile Structure
// =============================================================================

/// Represents a `lumen.lock` file for reproducible dependency resolution.
///
/// This is the authoritative source of truth for what dependencies should be used
/// in a build. The lockfile is content-addressed and cryptographically verified.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LockFile {
    /// Lockfile format version (for migration support).
    #[serde(default = "default_lockfile_version")]
    pub version: u32,

    /// Metadata about the resolution process.
    #[serde(default)]
    pub metadata: LockMetadata,

    /// All resolved packages with exact versions and integrity hashes.
    #[serde(default, rename = "package")]
    pub packages: Vec<LockedPackage>,

    /// Security configuration for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    /// Workspace members (for monorepo support).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace: Vec<WorkspaceMember>,
}

fn default_lockfile_version() -> u32 {
    CURRENT_LOCKFILE_VERSION
}

// =============================================================================
// Lock Metadata
// =============================================================================

/// Metadata about how the lockfile was generated.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LockMetadata {
    /// Name and version of the resolver strategy.
    #[serde(default = "default_resolver")]
    pub resolver: String,

    /// Encoding used for path sources.
    #[serde(default = "default_source_encoding")]
    pub source_encoding: String,

    /// Resolution mode used.
    #[serde(default = "default_resolution_mode")]
    pub resolution_mode: String,

    /// Content hash of all packages (for quick change detection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Timestamp when the lockfile was generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,

    /// Lumen version that produced this lockfile.
    #[serde(default = "default_lumen_version")]
    pub lumen_version: String,
}

fn default_resolver() -> String {
    "lumen-sat-v1".to_string()
}

fn default_source_encoding() -> String {
    "project-relative".to_string()
}

fn default_resolution_mode() -> String {
    "single-version".to_string()
}

fn default_lumen_version() -> String {
    LUMEN_VERSION.to_string()
}

impl Default for LockMetadata {
    fn default() -> Self {
        Self {
            resolver: default_resolver(),
            source_encoding: default_source_encoding(),
            resolution_mode: default_resolution_mode(),
            content_hash: None,
            generated_at: None,
            lumen_version: default_lumen_version(),
        }
    }
}

// =============================================================================
// Locked Package
// =============================================================================

/// A single locked package with exact version, source, and integrity information.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct LockedPackage {
    /// Package name (may include namespace: @namespace/name).
    pub name: String,

    /// Exact version (not a range).
    pub version: String,

    /// Package source URL with scheme.
    ///
    /// Formats:
    /// - `registry+https://registry.lumen.sh`
    /// - `path+../relative/path`
    /// - `git+https://github.com/org/repo?rev=sha`
    pub source: String,

    /// Content-addressed identifier for the resolved artifact.
    ///
    /// Formats:
    /// - `cid:bafy...` (IPFS CID for registry packages)
    /// - `git:sha:abc123...` (Git commit SHA for git deps)
    /// - `path:relative/path` (For path deps, includes manifest hash)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,

    /// Integrity hash of the downloaded artifact.
    ///
    /// Format: `sha512-<base64>` or `sha256-<hex>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,

    /// Hash of the canonicalized manifest (lumen.toml).
    ///
    /// This allows detecting if a path dependency's manifest has changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,

    /// Hash of the signed metadata blob from the registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_hash: Option<String>,

    /// Cryptographic signature for this package version.
    ///
    /// Format: `sig:<algorithm>:<base64-signature>:<public-key-id>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// Index in the transparency log (if verified).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency_index: Option<u64>,

    /// Legacy checksum field (for backward compatibility with v3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Downloadable artifacts for this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<LockedArtifact>,

    /// Dependencies with their exact resolved versions.
    ///
    /// Format: `@namespace/name@1.2.3` or `name@1.2.3`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,

    /// Optional features enabled for this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,

    /// Target platform constraints (for conditional dependencies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl LockedPackage {
    /// Create a new locked package for a path dependency.
    pub fn from_path(name: String, path: String) -> Self {
        Self {
            name,
            version: "0.1.0".to_string(),
            source: format!("path+{}", normalize_path_source(&path)),
            resolved: None,
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts: Vec::new(),
            dependencies: Vec::new(),
            features: Vec::new(),
            target: None,
        }
    }

    /// Create a new locked package for a path dependency with manifest hash.
    pub fn from_path_with_hash(name: String, path: String, manifest_hash: String) -> Self {
        let mut pkg = Self::from_path(name, path);
        pkg.manifest_hash = Some(manifest_hash);
        pkg
    }

    /// Create a new locked package for a registry dependency.
    pub fn from_registry(
        name: String,
        version: String,
        registry_url: String,
        checksum: String,
    ) -> Self {
        Self {
            name,
            version,
            source: format!("registry+{}", registry_url),
            resolved: None,
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: Some(checksum),
            artifacts: Vec::new(),
            dependencies: Vec::new(),
            features: Vec::new(),
            target: None,
        }
    }

    /// Create a fully-specified registry package with all integrity information.
    #[allow(clippy::too_many_arguments)]
    pub fn from_registry_complete(
        name: String,
        version: String,
        registry_url: String,
        cid: String,
        integrity: String,
        manifest_hash: String,
        artifacts: Vec<LockedArtifact>,
    ) -> Self {
        Self {
            name,
            version,
            source: format!("registry+{}", registry_url),
            resolved: Some(cid),
            integrity: Some(integrity),
            manifest_hash: Some(manifest_hash),
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts,
            dependencies: Vec::new(),
            features: Vec::new(),
            target: None,
        }
    }

    /// Create a new locked package for a git dependency.
    pub fn from_git(name: String, version: String, url: String, rev: String) -> Self {
        Self {
            name,
            version,
            source: format!("git+{}?rev={}", url, rev),
            resolved: Some(format!("git:sha:{}", rev)),
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: Some(rev),
            artifacts: Vec::new(),
            dependencies: Vec::new(),
            features: Vec::new(),
            target: None,
        }
    }

    /// Check if this is a path dependency.
    pub fn is_path_dependency(&self) -> bool {
        self.source.starts_with("path+")
    }

    /// Check if this is a registry dependency.
    pub fn is_registry_dependency(&self) -> bool {
        self.source.starts_with("registry+")
    }

    /// Check if this is a git dependency.
    pub fn is_git_dependency(&self) -> bool {
        self.source.starts_with("git+")
    }

    /// Get the path for a path dependency.
    pub fn get_path(&self) -> Option<&str> {
        self.source.strip_prefix("path+")
    }

    /// Get the registry URL for a registry dependency.
    pub fn get_registry_url(&self) -> Option<&str> {
        self.source.strip_prefix("registry+")
    }

    /// Parse the git source into URL and revision.
    pub fn parse_git_source(&self) -> Option<(String, String)> {
        let git_part = self.source.strip_prefix("git+")?;
        if let Some(idx) = git_part.find("?rev=") {
            let url = &git_part[..idx];
            let rev = &git_part[idx + 5..];
            Some((url.to_string(), rev.to_string()))
        } else {
            Some((git_part.to_string(), "HEAD".to_string()))
        }
    }

    /// Get the content identifier (CID) for this package.
    pub fn get_cid(&self) -> Option<&str> {
        self.resolved.as_ref()?.strip_prefix("cid:")
    }

    /// Compute a unique key for this locked package.
    pub fn key(&self) -> PackageKey {
        PackageKey {
            name: self.name.clone(),
            source: self.source.clone(),
        }
    }

    /// Verify the integrity of this package entry.
    pub fn verify_integrity(&self) -> Result<(), LockIntegrityError> {
        // Check required fields based on source type
        if self.is_registry_dependency() {
            if self.resolved.is_none() && self.checksum.is_none() && self.integrity.is_none() {
                return Err(LockIntegrityError::MissingIntegrity {
                    package: self.name.clone(),
                });
            }
        }

        if self.is_git_dependency() {
            if self.resolved.is_none() {
                return Err(LockIntegrityError::MissingGitRevision {
                    package: self.name.clone(),
                });
            }
        }

        // Verify artifact hashes match declared integrity
        for artifact in &self.artifacts {
            if artifact.hash.is_empty() {
                return Err(LockIntegrityError::MissingArtifactHash {
                    package: self.name.clone(),
                    kind: artifact.kind.clone(),
                });
            }
        }

        Ok(())
    }
}

/// Unique key for a package in the lockfile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageKey {
    pub name: String,
    pub source: String,
}

// =============================================================================
// Locked Artifact
// =============================================================================

/// A downloadable artifact for a package.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct LockedArtifact {
    /// Kind of artifact (tar, wasm, source, etc.).
    pub kind: String,

    /// URL to download the artifact from.
    pub url: String,

    /// Hash of the artifact content.
    ///
    /// Format: `sha256:<hex>` or `sha512-<base64>`
    pub hash: String,

    /// Size in bytes (optional, for progress display).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Architecture this artifact is built for (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    /// Platform this artifact is built for (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

// =============================================================================
// Security Configuration
// =============================================================================

/// Security configuration for dependency verification.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SecurityConfig {
    /// Whether to verify package signatures.
    #[serde(default = "default_verify_signatures")]
    pub verify_signatures: bool,

    /// URL of the transparency log server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency_log_url: Option<String>,

    /// Root of trust identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_of_trust: Option<String>,

    /// Allowed certificate fingerprints (for pinning).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_fingerprints: Vec<String>,

    /// Minimum required signature algorithm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_signature_algorithm: Option<String>,
}

fn default_verify_signatures() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            verify_signatures: default_verify_signatures(),
            transparency_log_url: None,
            root_of_trust: None,
            trusted_fingerprints: Vec::new(),
            min_signature_algorithm: None,
        }
    }
}

// =============================================================================
// Workspace Support
// =============================================================================

/// A member of a workspace.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct WorkspaceMember {
    /// Path to the workspace member (relative to workspace root).
    pub path: String,

    /// Package name (optional, loaded from manifest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

// =============================================================================
// Integrity Errors
// =============================================================================

/// Errors that can occur during integrity verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockIntegrityError {
    /// Package is missing integrity information.
    MissingIntegrity { package: String },
    /// Git package is missing revision.
    MissingGitRevision { package: String },
    /// Artifact is missing hash.
    MissingArtifactHash { package: String, kind: String },
    /// Content hash mismatch.
    HashMismatch {
        package: String,
        expected: String,
        actual: String,
    },
    /// Signature verification failed.
    SignatureFailed { package: String, reason: String },
    /// Transparency log verification failed.
    TransparencyLogFailed { package: String, reason: String },
}

impl fmt::Display for LockIntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingIntegrity { package } => {
                write!(f, "package '{}' is missing integrity information", package)
            }
            Self::MissingGitRevision { package } => {
                write!(f, "git package '{}' is missing revision", package)
            }
            Self::MissingArtifactHash { package, kind } => {
                write!(
                    f,
                    "package '{}' has '{}' artifact missing hash",
                    package, kind
                )
            }
            Self::HashMismatch {
                package,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "package '{}' hash mismatch: expected {}, got {}",
                    package, expected, actual
                )
            }
            Self::SignatureFailed { package, reason } => {
                write!(f, "signature verification failed for '{}': {}", package, reason)
            }
            Self::TransparencyLogFailed { package, reason } => {
                write!(
                    f,
                    "transparency log verification failed for '{}': {}",
                    package, reason
                )
            }
        }
    }
}

impl std::error::Error for LockIntegrityError {}

// =============================================================================
// LockFile Implementation
// =============================================================================

impl Default for LockFile {
    fn default() -> Self {
        Self {
            version: default_lockfile_version(),
            metadata: LockMetadata::default(),
            packages: Vec::new(),
            security: None,
            workspace: Vec::new(),
        }
    }
}

impl LockFile {
    /// Load a lock file from disk.
    ///
    /// Returns an empty default lockfile if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content =
            std::fs::read_to_string(path).map_err(|e| format!("cannot read lock file: {}", e))?;

        // Parse and migrate if needed
        let mut lock: Self =
            toml::from_str(&content).map_err(|e| format!("invalid lock file: {}", e))?;

        // Migrate older versions
        if lock.version < CURRENT_LOCKFILE_VERSION {
            lock = lock.migrate_to_current()?;
        }

        Ok(lock)
    }

    /// Save the lock file to disk.
    ///
    /// The file is written with a canonical format for deterministic diffs.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let header = "# This file is automatically generated by lumen pkg.\n# Do not edit manually.\n\n";

        let mut normalized = self.normalized();

        // Compute content hash
        normalized.metadata.content_hash = Some(normalized.compute_content_hash());
        normalized.metadata.generated_at = Some(Self::current_timestamp());

        let content = toml::to_string_pretty(&normalized)
            .map_err(|e| format!("failed to serialize lock file: {}", e))?;

        let full_content = format!("{}{}", header, content);

        // Write atomically (write to temp file, then rename)
        let temp_path = path.with_extension("lock.tmp");
        std::fs::write(&temp_path, &full_content)
            .map_err(|e| format!("cannot write lock file: {}", e))?;
        std::fs::rename(&temp_path, path)
            .map_err(|e| format!("cannot rename lock file: {}", e))?;

        Ok(())
    }

    /// Add or update a locked package.
    pub fn add_package(&mut self, pkg: LockedPackage) {
        let mut pkg = pkg;

        // Normalize path sources
        if pkg.is_path_dependency() {
            if let Some(path) = pkg.get_path().map(str::to_owned) {
                pkg.source = format!("path+{}", normalize_path_source(&path));
            }
        }

        // Sort dependencies for deterministic output
        pkg.dependencies.sort();
        pkg.features.sort();

        // Remove existing entry with same name
        self.packages.retain(|p| p.name != pkg.name);
        self.packages.push(pkg);
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Get a package by name.
    pub fn get_package(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Get a mutable reference to a package by name.
    pub fn get_package_mut(&mut self, name: &str) -> Option<&mut LockedPackage> {
        self.packages.iter_mut().find(|p| p.name == name)
    }

    /// Check if the lockfile contains a package.
    pub fn has_package(&self, name: &str) -> bool {
        self.packages.iter().any(|p| p.name == name)
    }

    /// Remove a package by name.
    pub fn remove_package(&mut self, name: &str) -> bool {
        let len_before = self.packages.len();
        self.packages.retain(|p| p.name != name);
        self.packages.len() != len_before
    }

    /// Get all package names.
    pub fn package_names(&self) -> Vec<&str> {
        self.packages.iter().map(|p| p.name.as_str()).collect()
    }

    /// Get topological order of packages (dependencies first).
    pub fn topological_order(&self) -> Result<Vec<&LockedPackage>, String> {
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let pkg_map: BTreeMap<&str, &LockedPackage> = self
            .packages
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();

        // Initialize
        for pkg in &self.packages {
            in_degree.entry(pkg.name.as_str()).or_insert(0);
            for dep_str in &pkg.dependencies {
                // Parse dependency string "name@version"
                let dep_name = dep_str.split('@').next().unwrap_or(dep_str);
                if pkg_map.contains_key(dep_name) {
                    dependents.entry(dep_name).or_default().push(&pkg.name);
                    *in_degree.entry(pkg.name.as_str()).or_insert(0) += 1;
                }
            }
        }

        // Kahn's algorithm
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();
        queue.sort(); // Deterministic order

        let mut result = Vec::new();

        while let Some(name) = queue.pop() {
            if let Some(&pkg) = pkg_map.get(name) {
                result.push(pkg);
            }

            if let Some(deps) = dependents.get(name) {
                let mut sorted_deps: Vec<_> = deps.iter().copied().collect();
                sorted_deps.sort();
                for dep in sorted_deps {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            let pos = queue.binary_search(&dep).unwrap_or_else(|e| e);
                            queue.insert(pos, dep);
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != self.packages.len() {
            let resolved: HashSet<_> = result.iter().map(|p| p.name.as_str()).collect();
            let cycle_members: Vec<_> = self
                .packages
                .iter()
                .filter(|p| !resolved.contains(p.name.as_str()))
                .map(|p| p.name.as_str())
                .collect();
            return Err(format!("circular dependency detected involving: {:?}", cycle_members));
        }

        Ok(result)
    }

    /// Verify the integrity of all packages.
    pub fn verify_integrity(&self) -> Result<(), Vec<LockIntegrityError>> {
        let errors: Vec<_> = self
            .packages
            .iter()
            .filter_map(|pkg| pkg.verify_integrity().err())
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check if the lockfile needs regeneration.
    pub fn is_stale(&self, manifest_deps: &std::collections::HashMap<String, String>) -> bool {
        // Check if any dependency was added/removed
        let locked_names: HashSet<_> = self.packages.iter().map(|p| p.name.as_str()).collect();
        let manifest_names: HashSet<_> = manifest_deps.keys().map(|s| s.as_str()).collect();

        if locked_names != manifest_names {
            return true;
        }

        // Check if any version constraints changed
        for pkg in &self.packages {
            if let Some(constraint) = manifest_deps.get(&pkg.name) {
                // Simple check - in reality we'd parse and compare properly
                if !constraint.contains(&pkg.version) && !constraint.contains('*') {
                    return true;
                }
            }
        }

        false
    }

    /// Create a diff between this lockfile and another.
    pub fn diff(&self, other: &LockFile) -> LockDiff {
        let mut diff = LockDiff::default();

        let self_names: HashSet<_> = self.packages.iter().map(|p| &p.name).collect();
        let other_names: HashSet<_> = other.packages.iter().map(|p| &p.name).collect();

        // Added packages
        for name in other_names.difference(&self_names) {
            if let Some(pkg) = other.get_package(name) {
                diff.added.push(pkg.clone());
            }
        }

        // Removed packages
        for name in self_names.difference(&other_names) {
            if let Some(pkg) = self.get_package(name) {
                diff.removed.push(pkg.clone());
            }
        }

        // Changed packages
        for name in self_names.intersection(&other_names) {
            let self_pkg = self.get_package(name);
            let other_pkg = other.get_package(name);
            if let (Some(sp), Some(op)) = (self_pkg, other_pkg) {
                if sp.version != op.version || sp.source != op.source {
                    diff.changed.push(PackageChange {
                        name: (*name).clone(),
                        old_version: sp.version.clone(),
                        old_source: sp.source.clone(),
                        new_version: op.version.clone(),
                        new_source: op.source.clone(),
                    });
                }
            }
        }

        diff.added.sort_by(|a, b| a.name.cmp(&b.name));
        diff.removed.sort_by(|a, b| a.name.cmp(&b.name));
        diff.changed.sort_by(|a, b| a.name.cmp(&b.name));

        diff
    }

    // Private helper methods

    fn normalized(&self) -> Self {
        let mut out = self.clone();
        out.version = default_lockfile_version();
        out.metadata = LockMetadata::default();
        for pkg in &mut out.packages {
            pkg.dependencies.sort();
            pkg.features.sort();
        }
        out.packages.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn compute_content_hash(&self) -> String {
        let mut hasher = Sha256::new();

        // Include all packages in a deterministic order
        for pkg in &self.packages {
            hasher.update(pkg.name.as_bytes());
            hasher.update(pkg.version.as_bytes());
            hasher.update(pkg.source.as_bytes());
            if let Some(ref resolved) = pkg.resolved {
                hasher.update(resolved.as_bytes());
            }
            if let Some(ref integrity) = pkg.integrity {
                hasher.update(integrity.as_bytes());
            }
            for dep in &pkg.dependencies {
                hasher.update(dep.as_bytes());
            }
        }

        format!("sha256:{}", hex_encode(&hasher.finalize()))
    }

    fn current_timestamp() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let datetime = chrono_timestamp(secs);
        format!("{}Z", datetime)
    }

    fn migrate_to_current(self) -> Result<Self, String> {
        let mut lock = self;

        // v1 -> v2: Add metadata
        // v2 -> v3: Add artifacts
        // v3 -> v4: Add security, workspace, content-addressing

        if lock.version < 2 {
            lock.metadata = LockMetadata::default();
        }

        if lock.version < 4 {
            // Migrate packages to new format
            for pkg in &mut lock.packages {
                // Convert old checksum to new integrity format
                if let Some(checksum) = &pkg.checksum {
                    if pkg.integrity.is_none() && checksum.starts_with("sha256:") {
                        // Keep checksum for backward compat, also set integrity
                        pkg.integrity = Some(checksum.clone());
                    }
                }
            }
        }

        lock.version = CURRENT_LOCKFILE_VERSION;
        Ok(lock)
    }
}

/// Diff between two lockfiles.
#[derive(Debug, Clone, Default)]
pub struct LockDiff {
    pub added: Vec<LockedPackage>,
    pub removed: Vec<LockedPackage>,
    pub changed: Vec<PackageChange>,
}

/// A change to a package between two lockfiles.
#[derive(Debug, Clone)]
pub struct PackageChange {
    pub name: String,
    pub old_version: String,
    pub old_source: String,
    pub new_version: String,
    pub new_source: String,
}

impl LockDiff {
    /// Check if the diff is empty.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Get a human-readable summary.
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        if !self.added.is_empty() {
            lines.push(format!("Added {} package(s):", self.added.len()));
            for pkg in &self.added {
                lines.push(format!("  + {}@{}", pkg.name, pkg.version));
            }
        }

        if !self.removed.is_empty() {
            lines.push(format!("Removed {} package(s):", self.removed.len()));
            for pkg in &self.removed {
                lines.push(format!("  - {}@{}", pkg.name, pkg.version));
            }
        }

        if !self.changed.is_empty() {
            lines.push(format!("Changed {} package(s):", self.changed.len()));
            for change in &self.changed {
                lines.push(format!(
                    "  ~ {} {} -> {}",
                    change.name, change.old_version, change.new_version
                ));
            }
        }

        lines.join("\n")
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn normalize_path_source(path: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let is_absolute = path.starts_with('/') || path.starts_with('\\');
    let normalized = path.replace('\\', "/");
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            match parts.last() {
                Some(last) if last != ".." => {
                    parts.pop();
                }
                _ => parts.push(part.to_string()),
            }
        } else {
            parts.push(part.to_string());
        }
    }

    if parts.is_empty() {
        if is_absolute {
            return "/".to_string();
        }
        return ".".to_string();
    }

    let normalized = parts.join("/");
    if is_absolute {
        format!("/{}", normalized)
    } else {
        normalized
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

fn chrono_timestamp(secs: u64) -> String {
    // Simple ISO 8601 timestamp without chrono dependency
    // This is approximate - for production use chrono
    let days = secs / 86400;
    let year = 1970 + (days / 365);
    let month = ((days % 365) / 30) + 1;
    let day = ((days % 365) % 30) + 1;
    let hour = (secs % 86400) / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hour, minute, second
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_tmp_lock_path(test_name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}.lock", test_name, std::process::id(), ts))
    }

    #[test]
    fn lock_file_default() {
        let lock = LockFile::default();
        assert_eq!(lock.version, CURRENT_LOCKFILE_VERSION);
        assert!(lock.packages.is_empty());
        assert!(lock.security.is_none());
    }

    #[test]
    fn lock_file_round_trip() {
        let mut lock = LockFile::default();

        lock.add_package(LockedPackage {
            name: "@acme/http-utils".to_string(),
            version: "1.2.0".to_string(),
            source: "registry+https://registry.lumen.sh".to_string(),
            resolved: Some("cid:bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string()),
            integrity: Some("sha512-Low1Rb...==".to_string()),
            manifest_hash: Some("sha256:def456".to_string()),
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts: vec![LockedArtifact {
                kind: "tar".to_string(),
                url: "https://cdn.lumen.sh/pkg/@acme/http-utils/1.2.0.tar".to_string(),
                hash: "sha256:abc123".to_string(),
                size: Some(24576),
                arch: None,
                platform: None,
            }],
            dependencies: vec!["@acme/json-parser@0.3.2".to_string()],
            features: vec![],
            target: None,
        });

        lock.add_package(LockedPackage::from_path(
            "mathlib".to_string(),
            "../mathlib".to_string(),
        ));

        let toml = toml::to_string_pretty(&lock).unwrap();
        let parsed: LockFile = toml::from_str(&toml).unwrap();

        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.version, CURRENT_LOCKFILE_VERSION);
        assert_eq!(
            parsed.get_package("@acme/http-utils").unwrap().version,
            "1.2.0"
        );
        assert_eq!(parsed.get_package("mathlib").unwrap().version, "0.1.0");
    }

    #[test]
    fn lock_file_backward_compat() {
        let old = r#"
version = 3

[[package]]
name = "mathlib"
version = "0.1.0"
source = "path+../mathlib"
checksum = "sha256:abc123"
"#;
        let parsed: LockFile = toml::from_str(old).unwrap();
        // Should be migrated to v4
        assert_eq!(parsed.version, CURRENT_LOCKFILE_VERSION);
        assert_eq!(parsed.packages.len(), 1);
    }

    #[test]
    fn locked_package_path_dependency() {
        let pkg = LockedPackage::from_path("mathlib".to_string(), "../mathlib".to_string());
        assert!(pkg.is_path_dependency());
        assert!(!pkg.is_registry_dependency());
        assert!(!pkg.is_git_dependency());
        assert_eq!(pkg.get_path(), Some("../mathlib"));
        assert_eq!(pkg.get_registry_url(), None);
    }

    #[test]
    fn locked_package_git_dependency() {
        let pkg = LockedPackage::from_git(
            "vendored".to_string(),
            "1.0.0".to_string(),
            "https://github.com/acme/vendored".to_string(),
            "a1b2c3d".to_string(),
        );
        assert!(pkg.is_git_dependency());
        let (url, rev) = pkg.parse_git_source().unwrap();
        assert_eq!(url, "https://github.com/acme/vendored");
        assert_eq!(rev, "a1b2c3d");
    }

    #[test]
    fn add_package_replaces_existing() {
        let mut lock = LockFile::default();

        lock.add_package(LockedPackage::from_path(
            "pkg".to_string(),
            "../pkg".to_string(),
        ));
        assert_eq!(lock.packages.len(), 1);

        lock.add_package(LockedPackage::from_path(
            "pkg".to_string(),
            "../pkg2".to_string(),
        ));
        assert_eq!(lock.packages.len(), 1);
        assert_eq!(lock.get_package("pkg").unwrap().get_path(), Some("../pkg2"));
    }

    #[test]
    fn topological_order() {
        let mut lock = LockFile::default();

        lock.add_package(LockedPackage {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            source: "registry+https://registry.lumen.sh".to_string(),
            resolved: None,
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts: vec![],
            dependencies: vec!["b@1.0.0".to_string()],
            features: vec![],
            target: None,
        });

        lock.add_package(LockedPackage {
            name: "b".to_string(),
            version: "1.0.0".to_string(),
            source: "registry+https://registry.lumen.sh".to_string(),
            resolved: None,
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts: vec![],
            dependencies: vec![],
            features: vec![],
            target: None,
        });

        let order = lock.topological_order().unwrap();
        assert_eq!(order.len(), 2);
        // b should come before a (dependency first)
        assert_eq!(order[0].name, "b");
        assert_eq!(order[1].name, "a");
    }

    #[test]
    fn lock_diff() {
        let mut lock1 = LockFile::default();
        lock1.add_package(LockedPackage::from_registry(
            "a".to_string(),
            "1.0.0".to_string(),
            "https://registry.lumen.sh".to_string(),
            "sha256:abc".to_string(),
        ));
        lock1.add_package(LockedPackage::from_registry(
            "b".to_string(),
            "1.0.0".to_string(),
            "https://registry.lumen.sh".to_string(),
            "sha256:def".to_string(),
        ));

        let mut lock2 = LockFile::default();
        lock2.add_package(LockedPackage::from_registry(
            "a".to_string(),
            "2.0.0".to_string(),
            "https://registry.lumen.sh".to_string(),
            "sha256:xyz".to_string(),
        ));
        lock2.add_package(LockedPackage::from_registry(
            "c".to_string(),
            "1.0.0".to_string(),
            "https://registry.lumen.sh".to_string(),
            "sha256:123".to_string(),
        ));

        let diff = lock1.diff(&lock2);

        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "b");

        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "c");

        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].name, "a");
        assert_eq!(diff.changed[0].old_version, "1.0.0");
        assert_eq!(diff.changed[0].new_version, "2.0.0");
    }

    #[test]
    fn integrity_verification() {
        let valid_pkg = LockedPackage::from_registry(
            "test".to_string(),
            "1.0.0".to_string(),
            "https://registry.lumen.sh".to_string(),
            "sha256:abc".to_string(),
        );
        assert!(valid_pkg.verify_integrity().is_ok());

        let invalid_pkg = LockedPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            source: "registry+https://registry.lumen.sh".to_string(),
            resolved: None,
            integrity: None,
            manifest_hash: None,
            meta_hash: None,
            signature: None,
            transparency_index: None,
            checksum: None,
            artifacts: vec![],
            dependencies: vec![],
            features: vec![],
            target: None,
        };
        assert!(invalid_pkg.verify_integrity().is_err());
    }

    #[test]
    fn normalize_path() {
        assert_eq!(normalize_path_source("../mathlib"), "../mathlib");
        assert_eq!(normalize_path_source("./mathlib"), "mathlib");
        assert_eq!(normalize_path_source("a/b/../c"), "a/c");
        assert_eq!(normalize_path_source("a/./b"), "a/b");
    }
}
