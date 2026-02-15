//! Git dependency support for Lumen packages.
//!
//! ## Design Philosophy
//!
//! **Git dependencies are resolved immutably by commit hash.**
//!
//! This module implements world-class git dependency handling:
//!
//! - **Commit resolution**: Branch/tag/ref is resolved to a specific SHA
//! - **Content hashing**: Git tree hashes for integrity verification
//! - **Submodule support**: Recursive checkout of submodules
//! - **Caching**: Local cache of git dependencies by commit hash
//! - **Lockfile integration**: Resolved commits are locked for reproducibility
//!
//! ## Git Dependency Formats
//!
//! ```toml
//! [dependencies]
//! # Simple git URL (defaults to main branch)
//! my-lib = { git = "https://github.com/org/repo" }
//!
//! # Specific branch
//! my-lib = { git = "https://github.com/org/repo", branch = "develop" }
//!
//! # Specific tag
//! my-lib = { git = "https://github.com/org/repo", tag = "v1.0.0" }
//!
//! # Specific commit
//! my-lib = { git = "https://github.com/org/repo", rev = "a1b2c3d" }
//!
//! # With features
//! my-lib = { git = "https://github.com/org/repo", features = ["async"] }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// =============================================================================
// Git Resolution Types
// =============================================================================

/// A resolved git dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGit {
    /// Repository URL.
    pub url: String,
    /// Resolved commit SHA (full 40-character hex).
    pub commit_sha: String,
    /// Tree hash of the resolved commit (for content verification).
    pub tree_hash: Option<String>,
    /// The ref that was requested (branch/tag/rev).
    pub requested_ref: GitRef,
    /// Path within the repository (for monorepo support).
    pub subdirectory: Option<String>,
}

/// A git reference (branch, tag, or commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    /// Default branch (HEAD).
    Default,
    /// Named branch.
    Branch(String),
    /// Named tag.
    Tag(String),
    /// Specific commit SHA (full or partial).
    Commit(String),
}

impl std::fmt::Display for GitRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "HEAD"),
            Self::Branch(b) => write!(f, "branch:{}", b),
            Self::Tag(t) => write!(f, "tag:{}", t),
            Self::Commit(c) => write!(f, "commit:{}", &c[..8.min(c.len())]),
        }
    }
}

// =============================================================================
// Git Resolver
// =============================================================================

/// Resolver for git dependencies.
#[derive(Debug, Clone)]
pub struct GitResolver {
    /// Cache directory for git clones.
    cache_dir: PathBuf,
    /// Whether to use shallow clones for performance.
    shallow: bool,
    /// Whether to recurse into submodules.
    recurse_submodules: bool,
}

impl GitResolver {
    /// Create a new git resolver.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            shallow: true,
            recurse_submodules: false,
        }
    }

    /// Create with custom options.
    pub fn with_options(
        cache_dir: impl Into<PathBuf>,
        shallow: bool,
        recurse_submodules: bool,
    ) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            shallow,
            recurse_submodules,
        }
    }

    /// Resolve a git dependency to a specific commit.
    pub fn resolve(&self, url: &str, git_ref: &GitRef) -> Result<ResolvedGit, GitError> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| GitError::CacheError(e.to_string()))?;

        // Create a unique directory for this repo based on URL hash
        let repo_dir = self.repo_cache_dir(url);

        // Clone or fetch the repository
        if repo_dir.exists() {
            self.fetch_repo(&repo_dir)?;
        } else {
            self.clone_repo(url, &repo_dir)?;
        }

        // Resolve the ref to a commit SHA
        let commit_sha = self.resolve_ref(&repo_dir, git_ref)?;

        // Get the tree hash for content verification
        let tree_hash = self.get_tree_hash(&repo_dir, &commit_sha)?;

        Ok(ResolvedGit {
            url: url.to_string(),
            commit_sha,
            tree_hash: Some(tree_hash),
            requested_ref: git_ref.clone(),
            subdirectory: None,
        })
    }

    /// Resolve and checkout a git dependency to a target directory.
    pub fn resolve_and_checkout(
        &self,
        url: &str,
        git_ref: &GitRef,
        target_dir: &Path,
    ) -> Result<ResolvedGit, GitError> {
        let resolved = self.resolve(url, git_ref)?;

        // Create target directory
        std::fs::create_dir_all(target_dir)
            .map_err(|e| GitError::CheckoutError(e.to_string()))?;

        // Get the repo directory
        let repo_dir = self.repo_cache_dir(url);

        // Checkout the specific commit
        self.checkout(&repo_dir, &resolved.commit_sha, target_dir)?;

        Ok(resolved)
    }

    /// Get the version from a git dependency's manifest.
    pub fn get_version(&self, url: &str, git_ref: &GitRef) -> Result<String, GitError> {
        let resolved = self.resolve(url, git_ref)?;
        let repo_dir = self.repo_cache_dir(url);

        // Read the manifest file
        let manifest_path = repo_dir.join("lumen.toml");
        if manifest_path.exists() {
            let content =
                std::fs::read_to_string(&manifest_path).map_err(|e| GitError::ManifestError(e.to_string()))?;
            
            // Parse version from TOML (simple regex approach)
            if let Some(version_line) = content.lines().find(|l| l.trim().starts_with("version")) {
                if let Some(version) = version_line.split('=').nth(1) {
                    let version = version.trim().trim_matches('"').trim_matches('\'');
                    return Ok(version.to_string());
                }
            }
        }

        // Fallback: use commit hash as version
        Ok(format!("0.0.0-git.{}", &resolved.commit_sha[..8]))
    }

    /// List available versions (tags) in a git repository.
    pub fn list_versions(&self, url: &str) -> Result<Vec<String>, GitError> {
        let repo_dir = self.repo_cache_dir(url);

        // Ensure repo exists
        if !repo_dir.exists() {
            self.clone_repo(url, &repo_dir)?;
        }

        // Get all tags
        let output = Command::new("git")
            .args(["tag", "-l"])
            .current_dir(&repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::GitCommandError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let tags: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let tag = line.trim();
                // Filter to semantic version-like tags
                if tag.starts_with('v') || tag.contains('.') {
                    Some(tag.to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(tags)
    }

    // Private helper methods

    fn repo_cache_dir(&self, url: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        self.cache_dir.join(&hash[..16])
    }

    fn clone_repo(&self, url: &str, target_dir: &Path) -> Result<(), GitError> {
        let mut args = vec!["clone"];

        if self.shallow {
            args.extend(["--depth", "1"]);
        }

        if self.recurse_submodules {
            args.push("--recurse-submodules");
        }

        args.push(url);
        args.push(target_dir.to_str().unwrap());

        let output = Command::new("git")
            .args(&args)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::CloneError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    fn fetch_repo(&self, repo_dir: &Path) -> Result<(), GitError> {
        let output = Command::new("git")
            .args(["fetch", "--all"])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::FetchError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    fn resolve_ref(&self, repo_dir: &Path, git_ref: &GitRef) -> Result<String, GitError> {
        let ref_spec = match git_ref {
            GitRef::Default => "HEAD".to_string(),
            GitRef::Branch(b) => format!("origin/{}", b),
            GitRef::Tag(t) => format!("tags/{}", t),
            GitRef::Commit(c) => c.clone(),
        };

        let output = Command::new("git")
            .args(["rev-parse", &ref_spec])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::RefResolutionError(format!(
                "Cannot resolve ref '{}'",
                ref_spec
            )));
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        // Validate SHA format
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GitError::InvalidCommitSha(sha));
        }

        Ok(sha)
    }

    fn get_tree_hash(&self, repo_dir: &Path, commit_sha: &str) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(["rev-parse", &format!("{}^{{tree}}", commit_sha)])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Ok(String::new());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn checkout(&self, repo_dir: &Path, commit_sha: &str, target_dir: &Path) -> Result<(), GitError> {
        // Use git archive piped to tar for extraction
        let archive_path = self.cache_dir.join(format!("{}.tar", &commit_sha[..8]));
        
        // Write archive
        let output = Command::new("git")
            .args(["archive", "-o"])
            .arg(&archive_path)
            .arg(commit_sha)
            .current_dir(repo_dir)
            .output()
            .map_err(|e: std::io::Error| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::CheckoutError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Extract to target
        let extract_output = Command::new("tar")
            .args(["-xf", archive_path.to_str().unwrap(), "-C"])
            .arg(target_dir)
            .output()
            .map_err(|e: std::io::Error| GitError::CheckoutError(e.to_string()))?;

        if !extract_output.status.success() {
            return Err(GitError::CheckoutError(
                String::from_utf8_lossy(&extract_output.stderr).to_string(),
            ));
        }

        // Cleanup archive
        let _ = std::fs::remove_file(&archive_path);

        Ok(())
    }
}

impl Default for GitResolver {
    fn default() -> Self {
        let cache_dir = std::env::temp_dir().join("lumen-git-cache");
        Self::new(cache_dir)
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Errors that can occur during git operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// Failed to access cache directory.
    CacheError(String),
    /// Failed to clone repository.
    CloneError(String),
    /// Failed to fetch updates.
    FetchError(String),
    /// Failed to resolve ref.
    RefResolutionError(String),
    /// Invalid commit SHA.
    InvalidCommitSha(String),
    /// Failed to checkout.
    CheckoutError(String),
    /// Failed to read manifest.
    ManifestError(String),
    /// Git command failed.
    GitCommandError(String),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CacheError(e) => write!(f, "Cache error: {}", e),
            Self::CloneError(e) => write!(f, "Failed to clone: {}", e),
            Self::FetchError(e) => write!(f, "Failed to fetch: {}", e),
            Self::RefResolutionError(e) => write!(f, "Ref resolution failed: {}", e),
            Self::InvalidCommitSha(sha) => write!(f, "Invalid commit SHA: {}", sha),
            Self::CheckoutError(e) => write!(f, "Checkout failed: {}", e),
            Self::ManifestError(e) => write!(f, "Manifest error: {}", e),
            Self::GitCommandError(e) => write!(f, "Git command failed: {}", e),
        }
    }
}

impl std::error::Error for GitError {}

// =============================================================================
// Git URL Parsing
// =============================================================================

/// Parse a git URL with optional ref specification.
pub fn parse_git_url(url: &str) -> (String, GitRef) {
    // Handle URL with fragment (e.g., #branch or #tag)
    if let Some(idx) = url.find('#') {
        let base_url = &url[..idx];
        let ref_part = &url[idx + 1..];
        
        let git_ref = if ref_part.starts_with("branch=") {
            GitRef::Branch(ref_part[7..].to_string())
        } else if ref_part.starts_with("tag=") {
            GitRef::Tag(ref_part[4..].to_string())
        } else if ref_part.starts_with("commit=") || ref_part.starts_with("rev=") {
            let sha = if ref_part.starts_with("commit=") {
                &ref_part[7..]
            } else {
                &ref_part[4..]
            };
            GitRef::Commit(sha.to_string())
        } else {
            // Assume it's a branch name
            GitRef::Branch(ref_part.to_string())
        };
        
        return (base_url.to_string(), git_ref);
    }
    
    (url.to_string(), GitRef::Default)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_url_simple() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Default);
    }

    #[test]
    fn test_parse_git_url_with_branch() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo#develop");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Branch("develop".to_string()));
    }

    #[test]
    fn test_parse_git_url_with_explicit_branch() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo#branch=feature-xyz");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Branch("feature-xyz".to_string()));
    }

    #[test]
    fn test_parse_git_url_with_tag() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo#tag=v1.0.0");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Tag("v1.0.0".to_string()));
    }

    #[test]
    fn test_parse_git_url_with_commit() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo#commit=abc123");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Commit("abc123".to_string()));
    }

    #[test]
    fn test_git_ref_display() {
        assert_eq!(format!("{}", GitRef::Default), "HEAD");
        assert_eq!(format!("{}", GitRef::Branch("main".to_string())), "branch:main");
        assert_eq!(format!("{}", GitRef::Tag("v1.0.0".to_string())), "tag:v1.0.0");
        assert_eq!(
            format!("{}", GitRef::Commit("a1b2c3d4e5f6".to_string())),
            "commit:a1b2c3d4"
        );
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::CloneError("Connection refused".to_string());
        assert!(err.to_string().contains("Failed to clone"));
    }
}
