//! Configuration file parsing for `lumen.toml` (package manifest).
//!
//! ## Design Philosophy
//!
//! **The manifest is a contract between the package author and the build system.**
//!
//! This module implements a comprehensive manifest format inspired by the best practices
//! from Cargo, npm, and Go modules:
//!
//! - **Namespaced packages**: `@namespace/package-name` for organizational clarity
//! - **Toolchain constraints**: Pin compiler versions for reproducibility
//! - **Feature flags**: Conditional compilation with feature dependencies
//! - **Exports**: Define public API surface and library entry points
//! - **Build profiles**: Debug, release, and custom profiles
//! - **Conditional dependencies**: Platform-specific and feature-gated deps
//!
//! ## Manifest Format
//!
//! ```toml
//! [package]
//! name = "@acme/http-utils"
//! version = "1.2.0"
//! description = "HTTP utilities for Lumen"
//! authors = ["Alice <alice@acme.com>"]
//! license = "MIT"
//! repository = "https://github.com/acme/lumen-http-utils"
//! keywords = ["http", "network", "client"]
//! categories = ["networking", "web"]
//! readme = "README.md"
//! documentation = "https://docs.acme.com/http-utils"
//! homepage = "https://acme.com"
//! edition = "2024"
//! rust-version = "1.70"  # Minimum Lumen compiler version
//!
//! [package.exports]
//! default = "client"     # Default export when imported
//! client = { path = "src/client.lm.md" }
//! server = { path = "src/server.lm.md" }
//! types = { path = "src/types.lm.md" }
//!
//! [toolchain]
//! lumen = ">=0.1.0 <0.2.0"
//! lumen = { version = "0.1.0", channel = "stable" }
//!
//! [features]
//! default = ["json", "tls"]
//! json = []
//! tls = ["native-tls"]
//! async = []
//! native-tls = []
//! rustls = []
//!
//! [dependencies]
//! @acme/uri = "^1.0"
//! @acme/json-parser = { version = "^0.3", features = ["streaming"] }
//! logging = { version = ">=1.0, <2.0", optional = true }
//!
//! [dev-dependencies]
//! @acme/test-helpers = "^0.1"
//! mock-server = { path = "../test/mock" }
//!
//! [build-dependencies]
//! codegen = "^0.2"
//!
//! [target.'target(os == "linux")'.dependencies]
//! epoll = "^1.0"
//!
//! [target.'target(os == "windows")'.dependencies]
//! iocp = "^1.0"
//!
//! [profile.dev]
//! opt-level = 0
//! debug = true
//!
//! [profile.release]
//! opt-level = 3
//! lto = true
//! strip = true
//!
//! [workspace]
//! members = ["crates/*"]
//! exclude = ["examples/*"]
//!
//! [providers]
//! "llm.chat" = "openai-compatible"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::str::FromStr;

// =============================================================================
// Core Config Structure
// =============================================================================

/// The root configuration structure for `lumen.toml`.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct LumenConfig {
    /// Package metadata (required for publishable packages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageInfo>,

    /// Toolchain constraints for reproducibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<ToolchainSpec>,

    /// Feature flags for conditional compilation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, FeatureDef>,

    /// Runtime dependencies.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, DependencySpec>,

    /// Development dependencies (tests, benchmarks).
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, DependencySpec>,

    /// Build-time dependencies (build scripts, codegen).
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "build-dependencies")]
    pub build_dependencies: HashMap<String, DependencySpec>,

    /// Target-specific dependencies.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub target: HashMap<String, TargetDeps>,

    /// Build profiles.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub profile: HashMap<String, BuildProfile>,

    /// Workspace configuration (for monorepos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceConfig>,

    /// Provider configuration for tool integrations.
    #[serde(default)]
    pub providers: ProviderSection,
}

// =============================================================================
// Package Information
// =============================================================================

/// Package metadata section.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PackageInfo {
    /// Package name, optionally namespaced: `@namespace/name` or `name`.
    pub name: String,

    /// Semantic version.
    pub version: Option<String>,

    /// Brief description for registry listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Author names and/or emails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,

    /// SPDX license identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Path to license file (defaults to LICENSE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_file: Option<String>,

    /// Source repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// Homepage URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,

    /// Search keywords (max 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    /// Registry categories for browsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,

    /// Path to README file (defaults to README.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,

    /// Include patterns for packaging (defaults to src/**, lumen.toml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,

    /// Exclude patterns for packaging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,

    /// Links to native libraries (for FFI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<String>,

    /// Whether this is a procedural macro plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc_macro: Option<bool>,

    /// Edition/year for language feature gates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,

    /// Minimum compiler version required.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "lumen-version")]
    pub lumen_version: Option<String>,

    /// Exports defining the public API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<ExportsSection>,

    /// Binary entry points.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub bin: HashMap<String, BinaryTarget>,

    /// Library entry point (if different from src/lib.lm.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<LibTarget>,

    /// Published status (true = publishable, false = private, array = restricted registries).
    #[serde(default = "default_publish", skip_serializing_if = "is_default_publish")]
    pub publish: Option<PublishPolicy>,
}

impl Default for PackageInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: None,
            description: None,
            authors: None,
            license: None,
            license_file: None,
            repository: None,
            homepage: None,
            documentation: None,
            keywords: None,
            categories: None,
            readme: None,
            include: None,
            exclude: None,
            links: None,
            proc_macro: None,
            edition: None,
            lumen_version: None,
            exports: None,
            bin: HashMap::new(),
            lib: None,
            publish: None,
        }
    }
}

fn default_publish() -> Option<PublishPolicy> {
    Some(PublishPolicy::Enabled(true))
}

fn is_default_publish(value: &Option<PublishPolicy>) -> bool {
    matches!(value, Some(PublishPolicy::Enabled(true)) | None)
}

/// Publish policy for a package.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum PublishPolicy {
    /// Simple boolean: true = publishable, false = private.
    Enabled(bool),
    /// List of allowed registry names.
    Registries(Vec<String>),
}

// =============================================================================
// Exports Section
// =============================================================================

/// Exports defining the public API surface.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ExportsSection {
    /// Default export when the package is imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// Named exports.
    #[serde(flatten)]
    pub named: BTreeMap<String, ExportEntry>,
}

/// A single export entry.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ExportEntry {
    /// Simple path string.
    Path(String),
    /// Detailed export configuration.
    Detailed {
        /// Path to the module.
        path: String,
        /// Required features to use this export.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        required_features: Vec<String>,
        /// Documentation URL override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
    },
}

// =============================================================================
// Toolchain Specification
// =============================================================================

/// Toolchain constraints for reproducibility.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ToolchainSpec {
    /// Lumen compiler version constraint.
    pub lumen: ToolchainVersion,

    /// Rust toolchain for native extensions (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust: Option<ToolchainVersion>,

    /// LLVM version for codegen (if relevant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llvm: Option<ToolchainVersion>,
}

/// A version constraint for a toolchain component.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ToolchainVersion {
    /// Simple version string (treated as semver range).
    Simple(String),
    /// Detailed version with channel.
    Detailed {
        /// Version constraint.
        version: String,
        /// Release channel: stable, beta, nightly.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
        /// Target triple override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
}

// =============================================================================
// Feature Flags
// =============================================================================

/// Definition of a feature flag.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum FeatureDef {
    /// List of other features and optional dependencies this feature enables.
    Simple(Vec<String>),
    /// Detailed feature definition.
    Detailed {
        /// Features and dependencies this enables.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        enables: Vec<String>,
        /// Documentation for the feature.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Whether this feature is in the default set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
}

// =============================================================================
// Dependency Specification
// =============================================================================

/// Specification for a dependency.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Simple string version: `pkg = "^1.0"`
    Version(String),
    /// Path dependency: `pkg = { path = "../pkg" }`
    Path { path: String },
    /// Version with options: `pkg = { version = "^1.0", registry = "...", features = [...] }`
    VersionDetailed {
        version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        registry: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optional: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_features: Option<bool>,
    },
    /// Git dependency: `pkg = { git = "...", branch = "..." }`
    Git {
        git: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optional: Option<bool>,
    },
    /// Workspace dependency (inherited from workspace).
    Workspace {
        workspace: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<Vec<String>>,
    },
}

// =============================================================================
// Target-Specific Dependencies
// =============================================================================

/// Dependencies specific to a target platform.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TargetDeps {
    /// Runtime dependencies for this target.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, DependencySpec>,

    /// Dev dependencies for this target.
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, DependencySpec>,

    /// Build dependencies for this target.
    #[serde(default, skip_serializing_if = "HashMap::is_empty", rename = "build-dependencies")]
    pub build_dependencies: HashMap<String, DependencySpec>,
}

// =============================================================================
// Build Profiles
// =============================================================================

/// A build profile configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct BuildProfile {
    /// Optimization level (0-3, "s", "z").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opt_level: Option<String>,

    /// Include debug info.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,

    /// Link-time optimization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lto: Option<bool>,

    /// Strip symbols from binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<bool>,

    /// Code generation units (affects parallelism vs optimization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codegen_units: Option<u32>,

    /// Enable debug assertions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_assertions: Option<bool>,

    /// Enable overflow checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_checks: Option<bool>,

    /// Panic strategy ("unwind" or "abort").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panic: Option<String>,

    /// Incremental compilation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incremental: Option<bool>,

    /// Custom environment variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

impl Default for BuildProfile {
    fn default() -> Self {
        Self {
            opt_level: None,
            debug: None,
            lto: None,
            strip: None,
            codegen_units: None,
            debug_assertions: None,
            overflow_checks: None,
            panic: None,
            incremental: None,
            env: HashMap::new(),
        }
    }
}

// =============================================================================
// Workspace Configuration
// =============================================================================

/// Workspace (monorepo) configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceConfig {
    /// Member package paths (supports glob patterns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,

    /// Paths to exclude from member discovery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    /// Default package metadata for all members.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<WorkspacePackageDefaults>,

    /// Dependencies shared by all workspace members.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, DependencySpec>,

    /// Workspace root directory (relative to manifest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// Default package metadata for workspace members.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspacePackageDefaults {
    pub version: Option<String>,
    pub authors: Option<Vec<String>>,
    pub edition: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

// =============================================================================
// Binary and Library Targets
// =============================================================================

/// A binary target configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BinaryTarget {
    /// Path to the entry point (defaults to src/bin/<name>.lm.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Required features to build this binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_features: Option<Vec<String>>,
}

/// A library target configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LibTarget {
    /// Path to the library entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Library type: lib, dylib, staticlib, cdylib.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_type: Option<String>,
}

// =============================================================================
// Provider Section
// =============================================================================

/// Provider configuration for tool integrations.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ProviderSection {
    /// Tool name -> provider type mapping.
    #[serde(flatten)]
    pub tools: HashMap<String, toml::Value>,

    /// Provider-specific configuration.
    #[serde(default)]
    pub config: HashMap<String, ProviderConfig>,

    /// MCP server configurations.
    #[serde(default)]
    pub mcp: HashMap<String, McpConfig>,
}

/// Configuration for a specific provider.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderConfig {
    /// Base URL for API calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Environment variable name for API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Default model to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Additional provider-specific settings.
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

/// MCP server configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpConfig {
    /// URI or command to start the MCP server.
    pub uri: String,
    /// Tools to expose from this server.
    #[serde(default)]
    pub tools: Vec<String>,
}

// =============================================================================
// Implementation
// =============================================================================

impl LumenConfig {
    /// Load config from `lumen.toml`, searching current dir then parents.
    pub fn load() -> Self {
        Self::find_and_load()
            .map(|(_path, cfg)| cfg)
            .unwrap_or_default()
    }

    /// Load config and return the path to the config file.
    pub fn load_with_path() -> Option<(PathBuf, Self)> {
        Self::find_and_load()
    }

    /// Load config from a specific file path.
    pub fn load_from(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
        toml::from_str(&content)
            .map_err(|e| format!("invalid toml in '{}': {}", path.display(), e))
    }

    fn find_and_load() -> Option<(PathBuf, Self)> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let config_path = dir.join("lumen.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path).ok()?;
                let cfg: Self = toml::from_str(&content).ok()?;
                return Some((config_path, cfg));
            }
            if !dir.pop() {
                break;
            }
        }
        // Try global config
        if let Some(home) = dirs_or_home() {
            let global = home.join(".config").join("lumen").join("lumen.toml");
            if global.exists() {
                let content = std::fs::read_to_string(&global).ok()?;
                let cfg: Self = toml::from_str(&content).ok()?;
                return Some((global, cfg));
            }
        }
        None
    }

    /// Parse a TOML string directly.
    pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Generate a default template.
    pub fn default_template() -> &'static str {
        r#"# Lumen Package Manifest
# See https://lumen-lang.org/docs/manifest for details

[package]
name = "my-package"
version = "0.1.0"
description = "A Lumen package"
authors = ["Your Name <you@example.com>"]
license = "MIT"
edition = "2024"

[package.exports]
default = "main"

[toolchain]
lumen = ">=0.1.0"

[features]
default = []

[dependencies]
# @acme/http-utils = "^1.0"

[dev-dependencies]
# @acme/test-helpers = "^0.1"

[profile.release]
opt-level = 3
lto = true

[providers]
# "llm.chat" = "openai-compatible"
"#
    }

    /// Get the package name, with namespace if present.
    pub fn package_name(&self) -> Option<&str> {
        self.package.as_ref().map(|p| p.name.as_str())
    }

    /// Check if the package is namespaced.
    pub fn is_namespaced(&self) -> bool {
        self.package
            .as_ref()
            .map(|p| p.name.contains('/'))
            .unwrap_or(false)
    }

    /// Get the namespace (part before /) if namespaced.
    pub fn namespace(&self) -> Option<&str> {
        self.package_name()?.split('/').next()
    }

    /// Get the local name (part after /) if namespaced.
    pub fn local_name(&self) -> Option<&str> {
        self.package_name()?.split('/').nth(1)
    }

    /// Get all dependencies including dev and build dependencies.
    pub fn all_dependencies(&self) -> HashMap<String, &DependencySpec> {
        let mut all = HashMap::new();
        for (name, spec) in &self.dependencies {
            all.insert(name.clone(), spec);
        }
        for (name, spec) in &self.dev_dependencies {
            all.insert(name.clone(), spec);
        }
        for (name, spec) in &self.build_dependencies {
            all.insert(name.clone(), spec);
        }
        all
    }

    /// Check if a feature is enabled by default.
    pub fn is_default_feature(&self, feature: &str) -> bool {
        self.features
            .get("default")
            .map(|def| match def {
                FeatureDef::Simple(features) => features.contains(&feature.to_string()),
                FeatureDef::Detailed { enables, .. } => {
                    enables.contains(&feature.to_string())
                }
            })
            .unwrap_or(false)
    }

    /// Get all features that should be enabled given a set of requested features.
    pub fn resolve_features(&self, requested: &[String]) -> Vec<String> {
        let mut resolved = std::collections::HashSet::new();

        // Add default features if not explicitly overridden
        if !requested.is_empty() || self.features.contains_key("default") {
            if let Some(def) = self.features.get("default") {
                match def {
                    FeatureDef::Simple(features) => {
                        for f in features {
                            resolved.insert(f.clone());
                        }
                    }
                    FeatureDef::Detailed { enables, .. } => {
                        for f in enables {
                            resolved.insert(f.clone());
                        }
                    }
                }
            }
        }

        // Add requested features and their dependencies
        let mut to_process: Vec<String> = requested.to_vec();
        while let Some(feature) = to_process.pop() {
            if resolved.insert(feature.clone()) {
                if let Some(def) = self.features.get(&feature) {
                    match def {
                        FeatureDef::Simple(features) => {
                            for f in features {
                                if !resolved.contains(f) {
                                    to_process.push(f.clone());
                                }
                            }
                        }
                        FeatureDef::Detailed { enables, .. } => {
                            for f in enables {
                                if !resolved.contains(f) {
                                    to_process.push(f.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut result: Vec<_> = resolved.into_iter().collect();
        result.sort();
        result
    }

    /// Validate the manifest for common errors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check package name
        if let Some(pkg) = &self.package {
            if !is_valid_package_name(&pkg.name) {
                errors.push(format!(
                    "Invalid package name '{}': must be lowercase alphanumeric with hyphens",
                    pkg.name
                ));
            }

            // Check version
            if let Some(version) = &pkg.version {
                if crate::semver::Version::from_str(version).is_err() {
                    errors.push(format!("Invalid version '{}': must be valid semver", version));
                }
            }

            // Check keywords count
            if let Some(keywords) = &pkg.keywords {
                if keywords.len() > 5 {
                    errors.push("Package cannot have more than 5 keywords".to_string());
                }
            }
        }

        // Check for circular feature dependencies
        for (feature, def) in &self.features {
            let deps = match def {
                FeatureDef::Simple(d) => d.clone(),
                FeatureDef::Detailed { enables, .. } => enables.clone(),
            };
            for dep in &deps {
                if dep == feature {
                    errors.push(format!("Feature '{}' depends on itself", feature));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Check if a package name is valid.
fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }

    // Handle namespaced names
    let local_name = if let Some(idx) = name.find('/') {
        let namespace = &name[..idx];
        let local = &name[idx + 1..];

        // Validate namespace
        if !namespace.starts_with('@') || namespace.len() < 2 {
            return false;
        }
        let namespace = &namespace[1..];
        if !is_valid_name_part(namespace) {
            return false;
        }
        local
    } else {
        name
    };

    is_valid_name_part(local_name)
}

fn is_valid_name_part(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') {
        return false;
    }

    let mut prev_dash = false;
    for ch in name.chars() {
        match ch {
            'a'..='z' | '0'..='9' => prev_dash = false,
            '-' => {
                if prev_dash {
                    return false;
                }
                prev_dash = true;
            }
            _ => return false,
        }
    }
    true
}

fn dirs_or_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[package]
name = "test"
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.package.unwrap().name, "test");
    }

    #[test]
    fn parse_namespaced_package() {
        let toml = r#"
[package]
name = "@acme/http-utils"
version = "1.0.0"
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert!(cfg.is_namespaced());
        assert_eq!(cfg.namespace(), Some("@acme"));
        assert_eq!(cfg.local_name(), Some("http-utils"));
    }

    #[test]
    fn parse_toolchain() {
        let toml = r#"
[toolchain]
lumen = ">=0.1.0 <0.2.0"
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert!(cfg.toolchain.is_some());
    }

    #[test]
    fn parse_features() {
        let toml = r#"
[features]
default = ["json", "tls"]
json = []
tls = ["native-tls"]
native-tls = []
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.features.len(), 4);
        assert!(cfg.is_default_feature("json"));
        assert!(!cfg.is_default_feature("native-tls"));
    }

    #[test]
    fn resolve_features() {
        let toml = r#"
[features]
default = ["a"]
a = ["b"]
b = ["c"]
c = []
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        let resolved = cfg.resolve_features(&[]);
        assert!(resolved.contains(&"a".to_string()));
        assert!(resolved.contains(&"b".to_string()));
        assert!(resolved.contains(&"c".to_string()));
    }

    #[test]
    fn parse_exports() {
        let toml = r#"
[package]
name = "test"

[package.exports]
default = "main"
client = { path = "src/client.lm.md" }
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        let exports = cfg.package.unwrap().exports.unwrap();
        assert_eq!(exports.default, Some("main".to_string()));
        assert!(exports.named.contains_key("client"));
    }

    #[test]
    fn parse_build_profile() {
        let toml = r#"
[profile.release]
opt-level = 3
lto = true
strip = true
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        let release = cfg.profile.get("release").unwrap();
        assert_eq!(release.opt_level, Some("3".to_string()));
        assert_eq!(release.lto, Some(true));
    }

    #[test]
    fn parse_workspace() {
        let toml = r#"
[workspace]
members = ["crates/*"]
exclude = ["examples/*"]
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        let ws = cfg.workspace.unwrap();
        assert_eq!(ws.members, vec!["crates/*"]);
        assert_eq!(ws.exclude, vec!["examples/*"]);
    }

    #[test]
    fn validate_package_name() {
        assert!(is_valid_package_name("valid-name"));
        assert!(is_valid_package_name("@namespace/name"));
        assert!(!is_valid_package_name("Invalid"));
        assert!(!is_valid_package_name("invalid--name"));
        assert!(!is_valid_package_name("-invalid"));
        assert!(!is_valid_package_name("invalid-"));
    }

    #[test]
    fn validate_config() {
        let toml = r#"
[package]
name = "@acme/http-utils"
version = "1.0.0"
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_invalid_name() {
        let toml = r#"
[package]
name = "InvalidName"
"#;
        let cfg: LumenConfig = toml::from_str(toml).unwrap();
        assert!(cfg.validate().is_err());
    }
}
