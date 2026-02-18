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
//! - **Authentication**: Support for SSH keys and HTTPS tokens
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

/// Default Lumen home directory for caching.
pub fn lumen_home_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".lumen")
}

/// Get the git cache directory.
pub fn git_cache_dir() -> PathBuf {
    lumen_home_dir().join("git")
}

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

impl GitRef {
    /// Get the ref as a string for git operations.
    pub fn as_ref_str(&self) -> String {
        match self {
            Self::Default => "HEAD".to_string(),
            Self::Branch(b) => format!("origin/{}", b),
            Self::Tag(t) => format!("refs/tags/{}", t),
            Self::Commit(c) => c.clone(),
        }
    }
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
    /// Authentication configuration.
    auth: GitAuthConfig,
}

impl GitResolver {
    /// Create a new git resolver.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            shallow: true,
            recurse_submodules: false,
            auth: GitAuthConfig::from_env(),
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
            auth: GitAuthConfig::from_env(),
        }
    }

    /// Set authentication configuration.
    pub fn with_auth(mut self, auth: GitAuthConfig) -> Self {
        self.auth = auth;
        self
    }

    /// Resolve a git dependency to a specific commit.
    pub fn resolve(&self, url: &str, git_ref: &GitRef) -> Result<ResolvedGit, GitError> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| GitError::CacheError(e.to_string()))?;

        // Detect and configure auth if not already set
        let auth = if matches!(self.auth.auth, GitAuth::None) {
            detect_auth(url)
        } else {
            self.auth.clone()
        };

        // Create a unique directory for this repo based on URL hash
        let repo_dir = self.repo_cache_dir(url);

        // Clone or fetch the repository
        if repo_dir.exists() {
            self.fetch_repo_with_auth(&repo_dir, url, &auth)?;
        } else {
            self.clone_repo_with_auth(url, &repo_dir, &auth)?;
        }

        // Fetch specific ref if needed (for tags and branches)
        self.fetch_ref_if_needed(&repo_dir, git_ref, url, &auth)?;

        // Resolve the ref to a commit SHA
        let commit_sha = resolve_git_ref(&repo_dir, git_ref)?;

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
        std::fs::create_dir_all(target_dir).map_err(|e| GitError::CheckoutError(e.to_string()))?;

        // Get the repo directory
        let repo_dir = self.repo_cache_dir(url);

        // Checkout the specific commit
        checkout_git_commit(&repo_dir, &resolved.commit_sha, target_dir)?;

        Ok(resolved)
    }

    /// Get the version from a git dependency's manifest.
    pub fn get_version(&self, url: &str, git_ref: &GitRef) -> Result<String, GitError> {
        let resolved = self.resolve(url, git_ref)?;
        let repo_dir = self.repo_cache_dir(url);

        // Read the manifest file at the specific commit
        let manifest_content =
            self.show_file_at_commit(&repo_dir, &resolved.commit_sha, "lumen.toml")?;

        if !manifest_content.is_empty() {
            // Parse version from TOML (simple regex approach)
            if let Some(version_line) = manifest_content
                .lines()
                .find(|l| l.trim().starts_with("version"))
            {
                if let Some(version) = version_line.split('=').nth(1) {
                    let version = version.trim().trim_matches('"').trim_matches('\'');
                    return Ok(version.to_string());
                }
            }
        }

        // Fallback: use commit hash as version
        Ok(format!("0.0.0-git.{}", &resolved.commit_sha[..8]))
    }

    /// Get the manifest content at a specific commit.
    pub fn get_manifest_at_ref(
        &self,
        url: &str,
        git_ref: &GitRef,
    ) -> Result<Option<String>, GitError> {
        let resolved = self.resolve(url, git_ref)?;
        let repo_dir = self.repo_cache_dir(url);

        match self.show_file_at_commit(&repo_dir, &resolved.commit_sha, "lumen.toml") {
            Ok(content) => Ok(Some(content)),
            Err(GitError::GitCommandError(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List available versions (tags) in a git repository.
    pub fn list_versions(&self, url: &str) -> Result<Vec<String>, GitError> {
        // First ensure repo is cloned
        let _ = self.resolve(url, &GitRef::Default)?;

        let repo_dir = self.repo_cache_dir(url);

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

    fn clone_repo_with_auth(
        &self,
        url: &str,
        target_dir: &Path,
        auth: &GitAuthConfig,
    ) -> Result<(), GitError> {
        std::fs::create_dir_all(target_dir.parent().unwrap_or(target_dir))
            .map_err(|e| GitError::CacheError(e.to_string()))?;

        let mut args = vec!["clone"];

        if self.shallow {
            args.extend(["--depth", "1"]);
        }

        if self.recurse_submodules {
            args.push("--recurse-submodules");
        }

        // Use authenticated URL for HTTPS
        let authenticated_url = auth.authenticated_url(url);
        args.push(&authenticated_url);
        args.push(target_dir.to_str().unwrap());

        let mut cmd = Command::new("git");
        cmd.args(&args);
        auth.apply(&mut cmd);

        let output = cmd
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Detect authentication errors
            if stderr.contains("Authentication failed")
                || stderr.contains("403")
                || stderr.contains("401")
                || stderr.contains("could not read Username")
            {
                return Err(GitError::AuthError(stderr.to_string()));
            }
            return Err(GitError::CloneError(stderr.to_string()));
        }

        Ok(())
    }

    fn fetch_repo_with_auth(
        &self,
        repo_dir: &Path,
        url: &str,
        auth: &GitAuthConfig,
    ) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.args(["fetch", "origin"]).current_dir(repo_dir);
        auth.apply(&mut cmd);

        let output = cmd
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If fetch fails, try re-cloning (repository might be corrupted)
            if stderr.contains("does not appear to be a git repository") {
                std::fs::remove_dir_all(repo_dir).ok();
                return self.clone_repo_with_auth(url, repo_dir, auth);
            }
            return Err(GitError::FetchError(stderr.to_string()));
        }

        Ok(())
    }

    #[allow(unused_variables)]
    fn fetch_ref_if_needed(
        &self,
        repo_dir: &Path,
        git_ref: &GitRef,
        url: &str,
        auth: &GitAuthConfig,
    ) -> Result<(), GitError> {
        match git_ref {
            GitRef::Branch(branch) => {
                // Fetch the specific branch
                let mut cmd = Command::new("git");
                cmd.args(["fetch", "origin", branch]).current_dir(repo_dir);
                auth.apply(&mut cmd);

                let output = cmd
                    .output()
                    .map_err(|e| GitError::GitCommandError(e.to_string()))?;

                if !output.status.success() {
                    return Err(GitError::FetchError(format!(
                        "Failed to fetch branch '{}': {}",
                        branch,
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            }
            GitRef::Tag(tag) => {
                // Fetch the specific tag
                let mut cmd = Command::new("git");
                cmd.args([
                    "fetch",
                    "origin",
                    &format!("refs/tags/{}:refs/tags/{}", tag, tag),
                ])
                .current_dir(repo_dir);
                auth.apply(&mut cmd);

                let output = cmd
                    .output()
                    .map_err(|e| GitError::GitCommandError(e.to_string()))?;

                if !output.status.success() {
                    return Err(GitError::FetchError(format!(
                        "Failed to fetch tag '{}': {}",
                        tag,
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
            }
            GitRef::Commit(_) => {
                // For specific commits, we might need to fetch more history
                // if it's not available in a shallow clone
                let mut cmd = Command::new("git");
                cmd.args(["fetch", "--unshallow"]).current_dir(repo_dir);
                auth.apply(&mut cmd);

                // This might fail if the repo is not shallow, which is fine
                let _ = cmd.output();
            }
            GitRef::Default => {
                // Just fetch the default branch
                let mut cmd = Command::new("git");
                cmd.args(["fetch", "origin"]).current_dir(repo_dir);
                auth.apply(&mut cmd);

                let _ = cmd.output();
            }
        }
        Ok(())
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

    fn show_file_at_commit(
        &self,
        repo_dir: &Path,
        commit_sha: &str,
        path: &str,
    ) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(["show", &format!("{}:{}", commit_sha, path)])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::GitCommandError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Clone a repository without explicit auth (uses self.auth).
    pub fn clone_repo(&self, url: &str, target_dir: &Path) -> Result<(), GitError> {
        self.clone_repo_with_auth(url, target_dir, &self.auth)
    }

    /// Fetch a repository without explicit auth (uses self.auth).
    pub fn fetch_repo(&self, repo_dir: &Path) -> Result<(), GitError> {
        self.fetch_repo_with_auth(repo_dir, "", &self.auth)
    }
}

impl Default for GitResolver {
    fn default() -> Self {
        Self::new(git_cache_dir())
    }
}

// =============================================================================
// Standalone Functions
// =============================================================================

/// Fetch a git repository to the cache directory.
///
/// This is the main entry point for fetching git dependencies. It handles:
/// - Cloning the repository if it doesn't exist
/// - Fetching updates if it already exists
/// - Resolving the ref to an exact commit SHA
///
/// # Arguments
///
/// * `url` - The git URL (https:// or git@)
/// * `git_ref` - The reference to fetch (branch, tag, or commit)
/// * `cache_dir` - The directory to cache the repository in
///
/// # Returns
///
/// The path to the cached repository directory.
pub fn fetch_git_repo(url: &str, git_ref: &GitRef, cache_dir: &Path) -> Result<PathBuf, GitError> {
    let resolver = GitResolver::new(cache_dir);
    let repo_dir = resolver.repo_cache_dir(url);

    // Clone or fetch
    if repo_dir.exists() {
        resolver.fetch_repo(&repo_dir)?;
    } else {
        std::fs::create_dir_all(&repo_dir).map_err(|e| GitError::CacheError(e.to_string()))?;
        resolver.clone_repo(url, &repo_dir)?;
    }

    // Fetch specific ref if needed
    match git_ref {
        GitRef::Branch(branch) => {
            // Fetch the specific branch
            let output = Command::new("git")
                .args(["fetch", "origin", branch])
                .current_dir(&repo_dir)
                .output()
                .map_err(|e| GitError::GitCommandError(e.to_string()))?;

            if !output.status.success() {
                return Err(GitError::FetchError(format!(
                    "Failed to fetch branch '{}': {}",
                    branch,
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        GitRef::Tag(tag) => {
            // Fetch the specific tag
            let output = Command::new("git")
                .args(["fetch", "origin", &format!("refs/tags/{}", tag)])
                .current_dir(&repo_dir)
                .output()
                .map_err(|e| GitError::GitCommandError(e.to_string()))?;

            if !output.status.success() {
                return Err(GitError::FetchError(format!(
                    "Failed to fetch tag '{}': {}",
                    tag,
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        _ => {}
    }

    Ok(repo_dir)
}

/// Resolve a git ref to an exact commit SHA.
///
/// # Arguments
///
/// * `repo_path` - Path to the git repository
/// * `git_ref` - The reference to resolve
///
/// # Returns
///
/// The full 40-character commit SHA.
pub fn resolve_git_ref(repo_path: &Path, git_ref: &GitRef) -> Result<String, GitError> {
    let ref_spec = match git_ref {
        GitRef::Default => "HEAD".to_string(),
        GitRef::Branch(b) => format!("origin/{}", b),
        GitRef::Tag(t) => format!("refs/tags/{}", t),
        GitRef::Commit(c) => {
            // If it's already a full SHA, return it
            if c.len() == 40 && c.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(c.clone());
            }
            c.clone()
        }
    };

    let output = Command::new("git")
        .args(["rev-parse", &ref_spec])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitError::GitCommandError(e.to_string()))?;

    if !output.status.success() {
        return Err(GitError::RefResolutionError(format!(
            "Cannot resolve ref '{}': {}",
            ref_spec,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Validate SHA format
    if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GitError::InvalidCommitSha(sha));
    }

    Ok(sha)
}

/// Checkout a specific commit to a target directory.
///
/// Uses `git archive` for efficient extraction without full git history.
pub fn checkout_git_commit(
    repo_path: &Path,
    commit_sha: &str,
    target_dir: &Path,
) -> Result<(), GitError> {
    // Ensure target directory exists
    std::fs::create_dir_all(target_dir).map_err(|e| GitError::CheckoutError(e.to_string()))?;

    // Use git archive piped to tar for extraction
    let archive_path = target_dir
        .parent()
        .unwrap_or(target_dir)
        .join(format!("{}.tar", &commit_sha[..8]));

    // Write archive
    let output = Command::new("git")
        .args(["archive", "-o"])
        .arg(&archive_path)
        .arg(commit_sha)
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitError::GitCommandError(e.to_string()))?;

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
        .map_err(|e| GitError::CheckoutError(e.to_string()))?;

    if !extract_output.status.success() {
        return Err(GitError::CheckoutError(
            String::from_utf8_lossy(&extract_output.stderr).to_string(),
        ));
    }

    // Cleanup archive
    let _ = std::fs::remove_file(&archive_path);

    Ok(())
}

/// Update a git dependency by fetching latest changes.
///
/// This is called during `lumen pkg update` for git dependencies.
pub fn update_git_repo(url: &str, cache_dir: &Path) -> Result<(), GitError> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let repo_dir = cache_dir.join(&hash[..16]);

    if !repo_dir.exists() {
        // Repository doesn't exist, clone it
        let resolver = GitResolver::new(cache_dir);
        resolver.clone_repo(url, &repo_dir)?;
    } else {
        // Repository exists, fetch updates
        let output = Command::new("git")
            .args(["fetch", "--all", "--prune"])
            .current_dir(&repo_dir)
            .output()
            .map_err(|e| GitError::GitCommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::FetchError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
    }

    Ok(())
}

// =============================================================================
// Authentication
// =============================================================================

/// Authentication method for git operations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitAuth {
    /// No authentication (public repository).
    #[default]
    None,
    /// SSH key authentication.
    Ssh {
        /// Path to the private key file (defaults to ~/.ssh/id_rsa or id_ed25519).
        key_path: Option<PathBuf>,
        /// Passphrase for the SSH key if encrypted.
        passphrase: Option<String>,
    },
    /// HTTPS token authentication.
    HttpsToken {
        /// The token to use.
        token: String,
    },
    /// Username and password for HTTPS.
    HttpsCredentials {
        /// Username.
        username: String,
        /// Password or personal access token.
        password: String,
    },
}

/// Environment configuration for git authentication.
#[derive(Debug, Clone)]
pub struct GitAuthConfig {
    /// Authentication method.
    pub auth: GitAuth,
    /// Environment variables to set for git commands.
    pub env_vars: HashMap<String, String>,
}

impl GitAuthConfig {
    /// Create a new auth config with no authentication.
    pub fn none() -> Self {
        Self {
            auth: GitAuth::None,
            env_vars: HashMap::new(),
        }
    }

    /// Create a new auth config with SSH key.
    pub fn ssh(key_path: Option<PathBuf>) -> Self {
        let mut env_vars = HashMap::new();

        // Set up SSH command to use the specific key
        if let Some(ref key) = key_path {
            let ssh_cmd = format!(
                "ssh -i '{}' -o StrictHostKeyChecking=accept-new",
                key.display()
            );
            env_vars.insert("GIT_SSH_COMMAND".to_string(), ssh_cmd);
        } else {
            // Use default SSH but with strict host key checking disabled for automation
            env_vars.insert(
                "GIT_SSH_COMMAND".to_string(),
                "ssh -o StrictHostKeyChecking=accept-new".to_string(),
            );
        }

        Self {
            auth: GitAuth::Ssh {
                key_path,
                passphrase: None,
            },
            env_vars,
        }
    }

    /// Create a new auth config with HTTPS token.
    pub fn https_token(token: String) -> Self {
        let mut env_vars = HashMap::new();
        // Git will use this for HTTPS authentication
        env_vars.insert("GIT_ASKPASS".to_string(), "echo".to_string());
        env_vars.insert("GIT_TERMINAL_PROMPT".to_string(), "0".to_string());

        Self {
            auth: GitAuth::HttpsToken { token },
            env_vars,
        }
    }

    /// Create auth config from environment variables.
    pub fn from_env() -> Self {
        // Check for SSH key path
        if let Ok(ssh_key) = std::env::var("LUMEN_GIT_SSH_KEY") {
            return Self::ssh(Some(PathBuf::from(ssh_key)));
        }

        // Check for HTTPS token
        if let Ok(token) = std::env::var("LUMEN_GIT_TOKEN") {
            return Self::https_token(token);
        }

        // Check for GitHub token (common convention)
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            return Self::https_token(token);
        }

        // Check for GitLab token
        if let Ok(token) = std::env::var("GITLAB_TOKEN") {
            return Self::https_token(token);
        }

        Self::none()
    }

    /// Apply authentication to a command.
    pub fn apply(&self, cmd: &mut Command) {
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }
    }

    /// Get the URL with embedded credentials for HTTPS auth.
    pub fn authenticated_url(&self, url: &str) -> String {
        match &self.auth {
            GitAuth::HttpsToken { token } if url.starts_with("https://") => {
                // Embed token in URL: https://token@host/path
                if let Some(rest) = url.strip_prefix("https://") {
                    format!("https://{}@{}", token, rest)
                } else {
                    url.to_string()
                }
            }
            GitAuth::HttpsCredentials { username, password } if url.starts_with("https://") => {
                // Embed credentials in URL: https://user:pass@host/path
                if let Some(rest) = url.strip_prefix("https://") {
                    format!("https://{}:{}@{}", username, password, rest)
                } else {
                    url.to_string()
                }
            }
            _ => url.to_string(),
        }
    }
}

/// Check if a URL is an SSH URL.
pub fn is_ssh_url(url: &str) -> bool {
    url.starts_with("git@") || url.starts_with("ssh://")
}

/// Check if a URL is an HTTPS URL.
pub fn is_https_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Detect the appropriate auth method for a URL.
pub fn detect_auth(url: &str) -> GitAuthConfig {
    let mut config = GitAuthConfig::from_env();

    // For SSH URLs, ensure SSH auth is configured
    if is_ssh_url(url) && matches!(config.auth, GitAuth::None) {
        config = GitAuthConfig::ssh(None);
    }

    config
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
    /// Authentication failed.
    AuthError(String),
    /// Network error.
    NetworkError(String),
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
            Self::AuthError(e) => write!(f, "Authentication failed: {}", e),
            Self::NetworkError(e) => write!(f, "Network error: {}", e),
        }
    }
}

impl std::error::Error for GitError {}

// =============================================================================
// Git URL Parsing
// =============================================================================

/// Parse a git URL with optional ref specification.
///
/// Supports:
/// - `https://github.com/org/repo`
/// - `https://github.com/org/repo#branch`
/// - `https://github.com/org/repo#tag=v1.0.0`
/// - `https://github.com/org/repo#rev=abc123`
pub fn parse_git_url(url: &str) -> (String, GitRef) {
    // Handle URL with fragment (e.g., #branch or #tag)
    if let Some(idx) = url.find('#') {
        let base_url = &url[..idx];
        let ref_part = &url[idx + 1..];

        let git_ref = if let Some(stripped) = ref_part.strip_prefix("branch=") {
            GitRef::Branch(stripped.to_string())
        } else if let Some(stripped) = ref_part.strip_prefix("tag=") {
            GitRef::Tag(stripped.to_string())
        } else if ref_part.starts_with("commit=") || ref_part.starts_with("rev=") {
            let sha = ref_part
                .strip_prefix("commit=")
                .or_else(|| ref_part.strip_prefix("rev="))
                .map(|s| s.to_string())
                .unwrap_or_else(|| ref_part.to_string());
            GitRef::Commit(sha)
        } else {
            // Assume it's a branch name
            GitRef::Branch(ref_part.to_string())
        };

        return (base_url.to_string(), git_ref);
    }

    (url.to_string(), GitRef::Default)
}

/// Convert a DependencySpec::Git into a GitRef.
pub fn dep_spec_to_git_ref(spec: &crate::config::DependencySpec) -> Option<(String, GitRef)> {
    match spec {
        crate::config::DependencySpec::Git {
            git,
            branch,
            tag,
            rev,
            ..
        } => {
            let git_ref = if let Some(r) = rev {
                GitRef::Commit(r.clone())
            } else if let Some(t) = tag {
                GitRef::Tag(t.clone())
            } else if let Some(b) = branch {
                GitRef::Branch(b.clone())
            } else {
                GitRef::Default
            };
            Some((git.clone(), git_ref))
        }
        _ => None,
    }
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
    fn test_parse_git_url_with_rev() {
        let (url, git_ref) = parse_git_url("https://github.com/org/repo#rev=def456");
        assert_eq!(url, "https://github.com/org/repo");
        assert_eq!(git_ref, GitRef::Commit("def456".to_string()));
    }

    #[test]
    fn test_git_ref_display() {
        assert_eq!(format!("{}", GitRef::Default), "HEAD");
        assert_eq!(
            format!("{}", GitRef::Branch("main".to_string())),
            "branch:main"
        );
        assert_eq!(
            format!("{}", GitRef::Tag("v1.0.0".to_string())),
            "tag:v1.0.0"
        );
        assert_eq!(
            format!("{}", GitRef::Commit("a1b2c3d4e5f6".to_string())),
            "commit:a1b2c3d4"
        );
    }

    #[test]
    fn test_git_ref_as_ref_str() {
        assert_eq!(GitRef::Default.as_ref_str(), "HEAD");
        assert_eq!(
            GitRef::Branch("main".to_string()).as_ref_str(),
            "origin/main"
        );
        assert_eq!(
            GitRef::Tag("v1.0.0".to_string()).as_ref_str(),
            "refs/tags/v1.0.0"
        );
        assert_eq!(GitRef::Commit("abc123".to_string()).as_ref_str(), "abc123");
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::CloneError("Connection refused".to_string());
        assert!(err.to_string().contains("Failed to clone"));

        let err = GitError::FetchError("Timeout".to_string());
        assert!(err.to_string().contains("Failed to fetch"));

        let err = GitError::RefResolutionError("Not found".to_string());
        assert!(err.to_string().contains("Ref resolution failed"));

        let err = GitError::InvalidCommitSha("abc123".to_string());
        assert!(err.to_string().contains("Invalid commit SHA"));

        let err = GitError::CheckoutError("Dirty working tree".to_string());
        assert!(err.to_string().contains("Checkout failed"));
    }

    #[test]
    fn test_resolved_git_fields() {
        let resolved = ResolvedGit {
            url: "https://github.com/test/repo".to_string(),
            commit_sha: "a1b2c3d4e5f6g7h8i9j0".repeat(2),
            tree_hash: Some("t1t2t3t4t5t6".to_string()),
            requested_ref: GitRef::Tag("v1.0.0".to_string()),
            subdirectory: None,
        };

        assert_eq!(resolved.url, "https://github.com/test/repo");
        assert_eq!(resolved.commit_sha.len(), 40);
        assert!(resolved.tree_hash.is_some());
    }

    #[test]
    fn test_git_resolver_default() {
        let resolver = GitResolver::default();
        assert!(resolver.shallow); // Default is true for performance
    }

    #[test]
    fn test_git_resolver_with_options() {
        let cache_dir = std::env::temp_dir().join("test_git_cache");
        let resolver = GitResolver::with_options(&cache_dir, false, true);

        assert_eq!(resolver.cache_dir, cache_dir);
        assert!(!resolver.shallow);
        assert!(resolver.recurse_submodules);
    }
}
