//! Build script execution for Lumen packages.
//!
//! ## Design Philosophy
//!
//! **Build scripts transform source files before compilation.**
//!
//! This module implements a robust build script system:
//!
//! - **Caching**: Build scripts are only re-run when inputs change
//! - **Incremental**: Individual steps can be cached independently
//! - **Deterministic**: Same inputs always produce same outputs
//! - **Isolated**: Build scripts run in a clean environment
//!
//! ## Build Script Format (lumen.toml)
//!
//! ```toml
//! # Simple script path
//! [package]
//! name = "my-package"
//! build = "build.lm"
//!
//! # Or inline configuration with pre/post hooks
//! [package.build]
//! pre = [
//!   "echo 'Building...'",
//!   "lumen run generate.lm"
//! ]
//! post = [
//!   "strip target/binary"
//! ]
//!
//! # Or detailed build steps
//! [[build.steps]]
//! name = "generate-parser"
//! command = "lumen-tool"
//! args = ["generate", "grammar.lm"]
//! outputs = ["src/generated_parser.lm"]
//! rerun-if-changed = ["grammar.lm"]
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{BuildConfig, BuildStep, LumenConfig, PackageBuildSpec};

// =============================================================================
// Build Environment
// =============================================================================

/// Environment variables set for build scripts.
#[derive(Debug, Clone)]
pub struct BuildEnvironment {
    /// Directory where build output should be placed.
    pub out_dir: PathBuf,
    /// Target triple being compiled for.
    pub target: String,
    /// Host triple (the machine doing the compiling).
    pub host: String,
    /// Number of parallel jobs to use.
    pub num_jobs: usize,
    /// Path to the package directory.
    pub package_dir: PathBuf,
    /// Package name.
    pub package_name: String,
    /// Package version.
    pub package_version: String,
    /// Profile (debug or release).
    pub profile: String,
    /// Build script directory (for [build] section steps).
    pub build_dir: PathBuf,
    /// Additional environment variables from config.
    pub extra_env: HashMap<String, String>,
}

impl BuildEnvironment {
    /// Create a new build environment for a package.
    pub fn new(package_dir: &Path, config: &LumenConfig, target_dir: &Path) -> Self {
        let pkg_name = config.package_name().unwrap_or("unknown");
        let pkg_version = config
            .package
            .as_ref()
            .and_then(|p| p.version.clone())
            .unwrap_or_else(|| "0.1.0".to_string());
        
        // Compute a unique hash for this package build
        let pkg_hash = compute_package_hash(package_dir, config);
        
        let out_dir = target_dir
            .join("build")
            .join(format!("{}-{}", pkg_name, &pkg_hash[..8]));
        
        let build_dir = target_dir.join("build");
        
        Self {
            out_dir,
            target: std::env::var("TARGET").unwrap_or_else(|_| default_target()),
            host: std::env::var("HOST").unwrap_or_else(|_| default_target()),
            num_jobs: std::env::var("NUM_JOBS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)),
            package_dir: package_dir.to_path_buf(),
            package_name: pkg_name.to_string(),
            package_version: pkg_version,
            profile: std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string()),
            build_dir,
            extra_env: HashMap::new(),
        }
    }

    /// Apply environment variables to a command.
    pub fn apply_to(&self, cmd: &mut Command) {
        cmd.env("OUT_DIR", &self.out_dir)
            .env("TARGET", &self.target)
            .env("HOST", &self.host)
            .env("NUM_JOBS", self.num_jobs.to_string())
            .env("CARGO_MANIFEST_DIR", &self.package_dir)
            .env("CARGO_PKG_NAME", &self.package_name)
            .env("CARGO_PKG_VERSION", &self.package_version)
            .env("PROFILE", &self.profile)
            .env("LUMEN_OUT_DIR", &self.out_dir)
            .env("LUMEN_TARGET_DIR", &self.build_dir);
        
        for (key, value) in &self.extra_env {
            cmd.env(key, value);
        }
    }

    /// Create the output directory if it doesn't exist.
    pub fn ensure_out_dir(&self) -> Result<(), BuildScriptError> {
        std::fs::create_dir_all(&self.out_dir)
            .map_err(|e| BuildScriptError::IoError(format!("Failed to create OUT_DIR: {}", e)))?;
        Ok(())
    }
}

fn default_target() -> String {
    // Use rustc's default target or a sensible fallback
    std::env::var("TARGET").unwrap_or_else(|_| {
        // Try to detect from rustc
        if let Ok(output) = Command::new("rustc").args(["-vV"]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(target) = line.strip_prefix("host: ") {
                    return target.to_string();
                }
            }
        }
        "x86_64-unknown-linux-gnu".to_string()
    })
}

fn compute_package_hash(package_dir: &Path, config: &LumenConfig) -> String {
    let mut hasher = Sha256::new();
    
    // Hash package name and version
    if let Some(name) = config.package_name() {
        hasher.update(name.as_bytes());
    }
    if let Some(version) = config.package.as_ref().and_then(|p| p.version.as_ref()) {
        hasher.update(version.as_bytes());
    }
    
    // Hash the package directory path for uniqueness
    hasher.update(package_dir.to_string_lossy().as_bytes());
    
    hex_encode(&hasher.finalize())[..16].to_string()
}

// =============================================================================
// Build Script Configuration
// =============================================================================

/// A build script for legacy/simple format.
#[derive(Debug, Clone)]
pub struct BuildScript {
    /// Command to run.
    pub command: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Files generated by the script.
    pub outputs: Vec<String>,
    /// File patterns that trigger rebuild.
    pub rerun_if_changed: Vec<String>,
}

/// Cache entry for a build step.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct BuildCacheEntry {
    /// Name of the step.
    name: String,
    /// Input hash at the time of last successful run.
    input_hash: String,
    /// Output files and their hashes.
    output_hashes: HashMap<String, String>,
    /// Timestamp of last run.
    timestamp: u64,
    /// Environment variables that were set.
    env: HashMap<String, String>,
}

/// Build cache for a package.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct BuildCache {
    /// Version of the cache format.
    version: u32,
    /// Cache entries for each step.
    entries: HashMap<String, BuildCacheEntry>,
}

const CACHE_VERSION: u32 = 2; // Bumped for env tracking
const CACHE_FILENAME: &str = ".build-cache.json";

// =============================================================================
// Build Script Execution
// =============================================================================

/// Run all build scripts for a package.
///
/// This runs:
/// 1. Pre-build hooks from [package.build] section
/// 2. Build script from [package.build] or [build] section
/// 3. Build steps from [[build.steps]]
///
/// Returns Ok(()) if all scripts succeeded or if there are no scripts.
pub fn run_build_scripts(package_dir: &Path, target_dir: &Path) -> Result<(), BuildScriptError> {
    let config_path = package_dir.join("lumen.toml");
    
    if !config_path.exists() {
        return Ok(());
    }
    
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to read lumen.toml: {}", e)))?;
    
    let config: LumenConfig = toml::from_str(&content)
        .map_err(|e| BuildScriptError::ConfigError(format!("Invalid lumen.toml: {}", e)))?;
    
    // Set up build environment
    let build_env = BuildEnvironment::new(package_dir, &config, target_dir);
    build_env.ensure_out_dir()?;
    
    // Load cache
    let cache = load_build_cache(package_dir)?;
    let mut new_cache = cache.clone();
    
    // Run pre-build hooks if defined in [package.build]
    if let Some(ref pkg_build) = config.package.as_ref().and_then(|p| p.build.as_ref()) {
        run_pre_build_hooks(package_dir, pkg_build, &build_env, &cache, &mut new_cache)?;
    }
    
    // Run the main build script/steps
    let build_ran = if let Some(ref pkg_build) = config.package.as_ref().and_then(|p| p.build.as_ref()) {
        // Run package-level build script if specified
        if let Some(script_path) = pkg_build.script_path() {
            run_build_script_file(package_dir, script_path, &build_env, &cache, &mut new_cache)?;
            true
        } else {
            false
        }
    } else {
        false
    };
    
    // Also check for [build] section (legacy/alternative config)
    let build_section_ran = if let Some(build_config) = parse_build_config(&content) {
        run_build_section_steps(package_dir, &build_config, &build_env, &cache, &mut new_cache)?;
        true
    } else {
        false
    };
    
    if !build_ran && !build_section_ran {
        // No build scripts configured
    }
    
    // Run post-build hooks if defined in [package.build]
    if let Some(ref pkg_build) = config.package.as_ref().and_then(|p| p.build.as_ref()) {
        run_post_build_hooks(package_dir, pkg_build, &build_env, &cache, &mut new_cache)?;
    }
    
    // Save updated cache
    save_build_cache(package_dir, &new_cache)?;
    
    Ok(())
}

/// Run pre-build hooks defined in [package.build].pre
fn run_pre_build_hooks(
    package_dir: &Path,
    pkg_build: &PackageBuildSpec,
    build_env: &BuildEnvironment,
    cache: &BuildCache,
    new_cache: &mut BuildCache,
) -> Result<(), BuildScriptError> {
    let hooks = pkg_build.pre_hooks();
    if hooks.is_empty() {
        return Ok(());
    }
    
    for (idx, hook) in hooks.iter().enumerate() {
        let step_name = format!("pre-build-{}", idx);
        let step = BuildStep::from_shell_command(hook);
        
        println!("{} {}", 
            crate::colors::status_label("Pre-build"), 
            crate::colors::cyan(&step_name));
        
        run_build_step(package_dir, &step, build_env, cache, new_cache)?;
    }
    
    Ok(())
}

/// Run post-build hooks defined in [package.build].post
fn run_post_build_hooks(
    package_dir: &Path,
    pkg_build: &PackageBuildSpec,
    build_env: &BuildEnvironment,
    cache: &BuildCache,
    new_cache: &mut BuildCache,
) -> Result<(), BuildScriptError> {
    let hooks = pkg_build.post_hooks();
    if hooks.is_empty() {
        return Ok(());
    }
    
    for (idx, hook) in hooks.iter().enumerate() {
        let step_name = format!("post-build-{}", idx);
        let step = BuildStep::from_shell_command(hook);
        
        println!("{} {}", 
            crate::colors::status_label("Post-build"), 
            crate::colors::cyan(&step_name));
        
        run_build_step(package_dir, &step, build_env, cache, new_cache)?;
    }
    
    Ok(())
}

/// Run a build script file (e.g., build.lm or build.sh).
fn run_build_script_file(
    package_dir: &Path,
    script_path: &str,
    build_env: &BuildEnvironment,
    cache: &BuildCache,
    new_cache: &mut BuildCache,
) -> Result<(), BuildScriptError> {
    let script_full_path = package_dir.join(script_path);
    
    if !script_full_path.exists() {
        return Err(BuildScriptError::ConfigError(
            format!("Build script not found: {}", script_path)));
    }
    
    // Determine how to run the script based on extension
    let step = if script_path.ends_with(".lm") || script_path.ends_with(".lumen") {
        BuildStep {
            name: Some("build-script".to_string()),
            command: "lumen".to_string(),
            args: vec!["run".to_string(), script_path.to_string()],
            env: HashMap::new(),
            outputs: vec![],
            rerun_if_changed: vec![script_path.to_string()],
            working_dir: None,
        }
    } else {
        // Shell script or other executable
        BuildStep {
            name: Some("build-script".to_string()),
            command: script_full_path.to_string_lossy().to_string(),
            args: vec![],
            env: HashMap::new(),
            outputs: vec![],
            rerun_if_changed: vec![script_path.to_string()],
            working_dir: None,
        }
    };
    
    run_build_step(package_dir, &step, build_env, cache, new_cache)
}

/// Run build steps from the [build] section.
fn run_build_section_steps(
    package_dir: &Path,
    build_config: &BuildConfig,
    build_env: &BuildEnvironment,
    cache: &BuildCache,
    new_cache: &mut BuildCache,
) -> Result<(), BuildScriptError> {
    // Handle simple script format
    if let Some(script) = &build_config.script {
        run_build_script_file(package_dir, script, build_env, cache, new_cache)?;
    }
    
    // Handle detailed steps
    for step in &build_config.steps {
        run_build_step(package_dir, step, build_env, cache, new_cache)?;
    }
    
    Ok(())
}

/// Run a single build step, using cache if possible.
fn run_build_step(
    package_dir: &Path,
    step: &BuildStep,
    build_env: &BuildEnvironment,
    cache: &BuildCache,
    new_cache: &mut BuildCache,
) -> Result<(), BuildScriptError> {
    let step_name = step.name.as_deref().unwrap_or(&step.command);
    
    // Compute input hash
    let input_hash = compute_step_input_hash(package_dir, step, build_env)?;
    
    // Check if we can use cached result
    if let Some(cached) = cache.entries.get(step_name) {
        if cached.input_hash == input_hash {
            // Check if environment variables match
            let env_matches = step.env.iter().all(|(k, v)| {
                cached.env.get(k) == Some(v)
            });
            
            if env_matches {
                // Check if outputs still exist and match
                let mut outputs_valid = true;
                for (output, expected_hash) in &cached.output_hashes {
                    let output_path = package_dir.join(output);
                    if !output_path.exists() {
                        outputs_valid = false;
                        break;
                    }
                    let current_hash = hash_file(&output_path)?;
                    if current_hash != *expected_hash {
                        outputs_valid = false;
                        break;
                    }
                }
                
                if outputs_valid {
                    // Cache hit - skip this step
                    println!("{} {} (cached)", 
                        crate::colors::status_label("Skipping"), 
                        crate::colors::cyan(step_name));
                    new_cache.entries.insert(step_name.to_string(), cached.clone());
                    return Ok(());
                }
            }
        }
    }
    
    // Need to run the step
    println!("{} {}", 
        crate::colors::status_label("Building"), 
        crate::colors::cyan(step_name));
    
    let working_dir = if let Some(ref wd) = step.working_dir {
        package_dir.join(wd)
    } else {
        package_dir.to_path_buf()
    };
    
    let mut cmd = Command::new(&step.command);
    cmd.args(&step.args)
       .current_dir(&working_dir);
    
    // Set up build environment
    build_env.apply_to(&mut cmd);
    
    // Set step-specific environment variables
    for (key, value) in &step.env {
        cmd.env(key, value);
    }
    
    // Run the command
    let output = cmd.output()
        .map_err(|e| BuildScriptError::ExecutionError {
            step: step_name.to_string(),
            message: format!("Failed to execute '{}': {}", step.command, e),
        })?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BuildScriptError::ExecutionError {
            step: step_name.to_string(),
            message: format!("Build script failed: {}", stderr),
        });
    }
    
    // Print stdout if there's any
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.is_empty() {
        for line in stdout.lines() {
            println!("    {}", line);
        }
    }
    
    // Verify outputs exist
    let mut output_hashes = HashMap::new();
    for output in &step.outputs {
        let output_path = package_dir.join(output);
        if !output_path.exists() {
            return Err(BuildScriptError::MissingOutput {
                step: step_name.to_string(),
                output: output.clone(),
            });
        }
        output_hashes.insert(output.clone(), hash_file(&output_path)?);
    }
    
    // Update cache
    let entry = BuildCacheEntry {
        name: step_name.to_string(),
        input_hash,
        output_hashes,
        timestamp: current_timestamp(),
        env: step.env.clone(),
    };
    new_cache.entries.insert(step_name.to_string(), entry);
    
    Ok(())
}

// =============================================================================
// Caching
// =============================================================================

/// Compute a hash of all inputs for a build step.
fn compute_step_input_hash(
    package_dir: &Path,
    step: &BuildStep,
    build_env: &BuildEnvironment,
) -> Result<String, BuildScriptError> {
    let mut hasher = Sha256::new();
    
    // Hash the command and args
    hasher.update(step.command.as_bytes());
    for arg in &step.args {
        hasher.update(arg.as_bytes());
    }
    
    // Hash environment variables
    let mut env_keys: Vec<_> = step.env.keys().collect();
    env_keys.sort();
    for key in env_keys {
        hasher.update(key.as_bytes());
        hasher.update(step.env[key].as_bytes());
    }
    
    // Hash build environment variables that affect the build
    hasher.update(build_env.target.as_bytes());
    hasher.update(build_env.profile.as_bytes());
    
    // Hash the rerun-if-changed files
    let mut all_patterns = step.rerun_if_changed.clone();
    
    // Also hash the source files that match patterns
    all_patterns.sort();
    for pattern in &all_patterns {
        let pattern_path = package_dir.join(pattern);
        let paths = glob::glob(pattern_path.to_string_lossy().as_ref())
            .map_err(|e| BuildScriptError::IoError(format!("Invalid glob pattern '{}': {}", pattern, e)))?;
        
        let mut file_hashes = Vec::new();
        for entry in paths.flatten() {
            if entry.is_file() {
                let hash = hash_file(&entry)?;
                file_hashes.push((entry, hash));
            }
        }
        
        file_hashes.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, hash) in file_hashes {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(hash.as_bytes());
        }
    }
    
    Ok(format!("sha256:{}", hex_encode(&hasher.finalize())))
}

/// Hash a single file.
fn hash_file(path: &Path) -> Result<String, BuildScriptError> {
    let content = std::fs::read(path)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to read '{}': {}", path.display(), e)))?;
    
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("sha256:{}", hex_encode(&hasher.finalize())))
}

/// Load the build cache for a package.
fn load_build_cache(package_dir: &Path) -> Result<BuildCache, BuildScriptError> {
    let cache_path = package_dir.join(".lumen").join(CACHE_FILENAME);
    
    if !cache_path.exists() {
        return Ok(BuildCache {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        });
    }
    
    let content = std::fs::read_to_string(&cache_path)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to read cache: {}", e)))?;
    
    let cache: BuildCache = serde_json::from_str(&content)
        .map_err(|e| BuildScriptError::IoError(format!("Invalid cache file: {}", e)))?;
    
    // Check version
    if cache.version != CACHE_VERSION {
        // Invalidate cache on version mismatch
        return Ok(BuildCache {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        });
    }
    
    Ok(cache)
}

/// Save the build cache for a package.
fn save_build_cache(package_dir: &Path, cache: &BuildCache) -> Result<(), BuildScriptError> {
    let cache_dir = package_dir.join(".lumen");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to create cache dir: {}", e)))?;
    
    let cache_path = cache_dir.join(CACHE_FILENAME);
    let content = serde_json::to_string_pretty(cache)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to serialize cache: {}", e)))?;
    
    std::fs::write(&cache_path, content)
        .map_err(|e| BuildScriptError::IoError(format!("Failed to write cache: {}", e)))?;
    
    Ok(())
}

// =============================================================================
// Configuration Parsing
// =============================================================================

/// Parse [build] section from TOML content.
fn parse_build_config(content: &str) -> Option<BuildConfig> {
    // Parse the full config and extract the build section
    let config: toml::Value = toml::from_str(content).ok()?;
    let build_table = config.get("build")?;
    
    // Try to deserialize the build config
    build_table.clone().try_into().ok()
}

/// Check if a package has build scripts.
pub fn has_build_scripts(package_dir: &Path) -> bool {
    let config_path = package_dir.join("lumen.toml");
    
    if !config_path.exists() {
        return false;
    }
    
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return false;
    };
    
    // Check for [package.build] section
    if let Ok(config) = toml::from_str::<LumenConfig>(&content) {
        if config.package.as_ref().and_then(|p| p.build.as_ref()).is_some() {
            return true;
        }
    }
    
    // Check for [build] section
    parse_build_config(&content).is_some()
}

/// Get the OUT_DIR for a package.
pub fn get_out_dir(package_dir: &Path, target_dir: &Path) -> Option<PathBuf> {
    let config_path = package_dir.join("lumen.toml");
    if !config_path.exists() {
        return None;
    }
    
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: LumenConfig = toml::from_str(&content).ok()?;
    
    let build_env = BuildEnvironment::new(package_dir, &config, target_dir);
    Some(build_env.out_dir)
}

// =============================================================================
// Utilities
// =============================================================================

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

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// =============================================================================
// Errors
// =============================================================================

/// Errors that can occur during build script execution.
#[derive(Debug, Clone)]
pub enum BuildScriptError {
    /// I/O error.
    IoError(String),
    /// Configuration error.
    ConfigError(String),
    /// Execution error.
    ExecutionError { step: String, message: String },
    /// Missing expected output.
    MissingOutput { step: String, output: String },
}

impl std::fmt::Display for BuildScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::ConfigError(e) => write!(f, "Configuration error: {}", e),
            Self::ExecutionError { step, message } => {
                write!(f, "Execution error in '{}': {}", step, message)
            }
            Self::MissingOutput { step, output } => {
                write!(f, "Missing output '{}' from step '{}'", output, step)
            }
        }
    }
}

impl std::error::Error for BuildScriptError {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_dir(prefix: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), stamp));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_parse_simple_build_config() {
        let toml = r#"
[build]
script = "./build.sh"
rerun-if-changed = ["src/grammar.lm"]
"#;
        
        let config = parse_build_config(toml).unwrap();
        assert_eq!(config.script, Some("./build.sh".to_string()));
        assert_eq!(config.rerun_if_changed, vec!["src/grammar.lm"]);
    }

    #[test]
    fn test_parse_detailed_build_steps() {
        let toml = r#"
[[build.steps]]
name = "generate-parser"
command = "lumen-tool"
args = ["generate", "grammar.lm"]
outputs = ["src/generated_parser.lm"]
rerun-if-changed = ["grammar.lm"]

[[build.steps]]
name = "compile-protos"
command = "protoc"
args = ["--lumen_out=.", "proto/*.proto"]
"#;
        
        let config = parse_build_config(toml).unwrap();
        assert_eq!(config.steps.len(), 2);
        assert_eq!(config.steps[0].name, Some("generate-parser".to_string()));
        assert_eq!(config.steps[0].command, "lumen-tool");
        assert_eq!(config.steps[0].args, vec!["generate", "grammar.lm"]);
    }

    #[test]
    fn test_parse_package_build_simple() {
        let toml = r#"
[package]
name = "test"
build = "build.lm"
"#;
        
        let config: LumenConfig = toml::from_str(toml).unwrap();
        let pkg_build = config.package.unwrap().build.unwrap();
        
        match pkg_build {
            PackageBuildSpec::Simple(path) => assert_eq!(path, "build.lm"),
            _ => panic!("Expected Simple variant"),
        }
    }

    #[test]
    fn test_parse_package_build_detailed() {
        let toml = r#"
[package]
name = "test"

[package.build]
pre = ["echo 'pre'", "mkdir -p out"]
post = ["echo 'post'"]
script = "build.lm"
"#;
        
        let config: LumenConfig = toml::from_str(toml).unwrap();
        let pkg_build = config.package.unwrap().build.unwrap();
        
        match &pkg_build {
            PackageBuildSpec::Detailed { pre, post, script } => {
                assert_eq!(pre.len(), 2);
                assert_eq!(post.len(), 1);
                assert_eq!(script.as_deref(), Some("build.lm"));
            }
            _ => panic!("Expected Detailed variant"),
        }
        
        assert_eq!(pkg_build.pre_hooks().len(), 2);
        assert_eq!(pkg_build.post_hooks().len(), 1);
        assert_eq!(pkg_build.script_path(), Some("build.lm"));
    }

    #[test]
    fn test_build_step_from_shell_command() {
        let step = BuildStep::from_shell_command("echo hello world");
        assert_eq!(step.command, "echo");
        assert_eq!(step.args, vec!["hello", "world"]);
        
        let step2 = BuildStep::from_shell_command("lumen run build.lm --release");
        assert_eq!(step2.command, "lumen");
        assert_eq!(step2.args, vec!["run", "build.lm", "--release"]);
    }

    #[test]
    fn test_build_cache_save_load() {
        let temp = create_temp_dir("build_cache_test");
        
        let cache = BuildCache {
            version: CACHE_VERSION,
            entries: {
                let mut m = HashMap::new();
                m.insert("test-step".to_string(), BuildCacheEntry {
                    name: "test-step".to_string(),
                    input_hash: "sha256:abc123".to_string(),
                    output_hashes: {
                        let mut h = HashMap::new();
                        h.insert("output.lm".to_string(), "sha256:def456".to_string());
                        h
                    },
                    timestamp: 1234567890,
                    env: HashMap::new(),
                });
                m
            },
        };
        
        save_build_cache(&temp, &cache).unwrap();
        let loaded = load_build_cache(&temp).unwrap();
        
        assert_eq!(loaded.version, cache.version);
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries.contains_key("test-step"));
    }

    #[test]
    fn test_hash_file() {
        let temp = create_temp_dir("hash_file_test");
        let test_file = temp.join("test.txt");
        
        let mut file = std::fs::File::create(&test_file).unwrap();
        file.write_all(b"hello world").unwrap();
        drop(file);
        
        let hash1 = hash_file(&test_file).unwrap();
        let hash2 = hash_file(&test_file).unwrap();
        
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("sha256:"));
    }

    #[test]
    fn test_build_environment() {
        let temp = create_temp_dir("build_env_test");
        let target_dir = create_temp_dir("target_test");
        
        let config = LumenConfig {
            package: Some(crate::config::PackageInfo {
                name: "test-pkg".to_string(),
                version: Some("1.0.0".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        
        let build_env = BuildEnvironment::new(&temp, &config, &target_dir);
        
        assert_eq!(build_env.package_name, "test-pkg");
        assert_eq!(build_env.package_version, "1.0.0");
        assert!(build_env.out_dir.to_string_lossy().contains("test-pkg"));
    }

    #[test]
    fn test_has_build_scripts() {
        let temp = create_temp_dir("has_build_test");
        
        // No build scripts
        let toml_no_build = r#"
[package]
name = "test"
"#;
        std::fs::write(temp.join("lumen.toml"), toml_no_build).unwrap();
        assert!(!has_build_scripts(&temp));
        
        // With [package.build]
        let temp2 = create_temp_dir("has_build_test2");
        let toml_with_build = r#"
[package]
name = "test"
build = "build.lm"
"#;
        std::fs::write(temp2.join("lumen.toml"), toml_with_build).unwrap();
        assert!(has_build_scripts(&temp2));
        
        // With [build] section
        let temp3 = create_temp_dir("has_build_test3");
        let toml_build_section = r#"
[package]
name = "test"

[build]
script = "build.sh"
"#;
        std::fs::write(temp3.join("lumen.toml"), toml_build_section).unwrap();
        assert!(has_build_scripts(&temp3));
    }
}
