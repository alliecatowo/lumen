//! Configuration file parsing for `lumen.toml`.
//!
//! Searches current directory then ancestors, falling back to
//! `~/.config/lumen/lumen.toml` if no project-level file is found.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[allow(dead_code)]
pub struct LumenConfig {
    #[serde(default)]
    pub providers: ProviderSection,
    #[serde(default)]
    pub dependencies: HashMap<String, DependencySpec>,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub package: Option<PackageInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Option<Vec<String>>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub readme: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Simple string version: `pkg = "^1.0"`
    Version(String),
    /// Path dependency: `pkg = { path = "../pkg" }`
    Path { path: String },
    /// Version with registry: `pkg = { version = "^1.0", registry = "..." }`
    VersionDetailed {
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        registry: Option<String>,
    },
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[allow(dead_code)]
pub struct ProviderSection {
    /// Tool name -> provider type mapping (e.g., "llm.chat" = "openai-compatible")
    #[serde(flatten)]
    pub tools: HashMap<String, toml::Value>,

    /// Provider-specific configuration
    #[serde(default)]
    pub config: HashMap<String, ProviderConfig>,

    /// MCP server configurations
    #[serde(default)]
    pub mcp: HashMap<String, McpConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct ProviderConfig {
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub default_model: Option<String>,
    /// Additional provider-specific settings
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct McpConfig {
    pub uri: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

impl LumenConfig {
    /// Load config from `lumen.toml`, searching current dir then parents.
    /// Returns `Default` when no file is found.
    pub fn load() -> Self {
        Self::find_and_load()
            .map(|(_path, cfg)| cfg)
            .unwrap_or_default()
    }

    /// Load config and return the path to the config file that was found.
    pub fn load_with_path() -> Option<(PathBuf, Self)> {
        Self::find_and_load()
    }

    /// Load config from a specific file path.
    pub fn load_from(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| format!("invalid toml in '{}': {}", path.display(), e))
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

    /// Parse a TOML string directly (useful for testing and embedding).
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Generate a default `lumen.toml` template.
    #[allow(dead_code)]
    pub fn default_template() -> &'static str {
        r#"# Lumen Configuration
# See https://lumen-lang.org/docs/config for details

# Map tool names to provider implementations
[providers]
# "llm.chat" = "openai-compatible"
# "http.get" = "builtin-http"
# "http.post" = "builtin-http"

# Provider-specific configuration
# [providers.config.openai-compatible]
# base_url = "https://api.openai.com/v1"
# api_key_env = "OPENAI_API_KEY"
# default_model = "gpt-4"

# MCP server bridges (every MCP server = automatic Lumen tools)
# [providers.mcp.github]
# uri = "npx -y @modelcontextprotocol/server-github"
# tools = ["github.create_issue", "github.search_repos"]
"#
    }
}

fn dirs_or_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config_with_providers() {
        let toml_str = r#"
[providers]
"llm.chat" = "openai-compatible"
"http.get" = "builtin-http"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(
            cfg.providers.tools.get("llm.chat").and_then(|v| v.as_str()),
            Some("openai-compatible")
        );
        assert_eq!(
            cfg.providers.tools.get("http.get").and_then(|v| v.as_str()),
            Some("builtin-http")
        );
    }

    #[test]
    fn parse_config_with_mcp() {
        let toml_str = r#"
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        let gh = cfg.providers.mcp.get("github").expect("github mcp entry");
        assert!(gh.uri.contains("server-github"));
        assert_eq!(gh.tools.len(), 2);
        assert_eq!(gh.tools[0], "github.create_issue");
    }

    #[test]
    fn parse_config_with_provider_config() {
        let toml_str = r#"
[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        let pc = cfg
            .providers
            .config
            .get("openai-compatible")
            .expect("provider config");
        assert_eq!(pc.base_url.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(pc.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(pc.default_model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn empty_string_returns_default() {
        let cfg: LumenConfig = toml::from_str("").expect("empty toml is valid");
        assert!(cfg.providers.tools.is_empty());
        assert!(cfg.providers.config.is_empty());
        assert!(cfg.providers.mcp.is_empty());
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result: Result<LumenConfig, _> = toml::from_str("[broken");
        assert!(result.is_err());
    }

    #[test]
    fn default_template_round_trips() {
        let template = LumenConfig::default_template();
        let result: Result<LumenConfig, _> = toml::from_str(template);
        assert!(result.is_ok(), "default template must be valid toml");
    }

    #[test]
    fn full_config_round_trip() {
        let toml_str = r#"
[providers]
"llm.chat" = "openai-compatible"
"http.get" = "builtin-http"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
"#;
        let cfg = LumenConfig::from_str(toml_str).expect("full config should parse");
        assert_eq!(cfg.providers.tools.len(), 2);
        assert!(cfg.providers.config.contains_key("openai-compatible"));
        assert!(cfg.providers.mcp.contains_key("github"));
    }

    #[test]
    fn parse_package_info() {
        let toml_str = r#"
[package]
name = "my-app"
version = "0.1.0"
description = "A cool app"
authors = ["Alice", "Bob"]
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        let pkg = cfg.package.expect("package should be present");
        assert_eq!(pkg.name, "my-app");
        assert_eq!(pkg.version.as_deref(), Some("0.1.0"));
        assert_eq!(pkg.description.as_deref(), Some("A cool app"));
        assert_eq!(pkg.authors.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn parse_package_minimal() {
        let toml_str = r#"
[package]
name = "minimal"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        let pkg = cfg.package.expect("package should be present");
        assert_eq!(pkg.name, "minimal");
        assert!(pkg.version.is_none());
        assert!(pkg.description.is_none());
        assert!(pkg.authors.is_none());
    }

    #[test]
    fn parse_path_dependency() {
        let toml_str = r#"
[dependencies]
mathlib = { path = "../mathlib" }
utils = { path = "./libs/utils" }
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.dependencies.len(), 2);
        match &cfg.dependencies["mathlib"] {
            DependencySpec::Path { path } => assert_eq!(path, "../mathlib"),
            _ => panic!("expected path spec"),
        }
        match &cfg.dependencies["utils"] {
            DependencySpec::Path { path } => assert_eq!(path, "./libs/utils"),
            _ => panic!("expected path spec"),
        }
    }

    #[test]
    fn parse_full_package_config() {
        let toml_str = r#"
[package]
name = "demo-app"
version = "0.1.0"

[dependencies]
mathlib = { path = "../mathlib" }

[providers]
"llm.chat" = "openai-compatible"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.package.as_ref().unwrap().name, "demo-app");
        assert_eq!(cfg.dependencies.len(), 1);
        assert_eq!(cfg.providers.tools.len(), 1);
    }

    #[test]
    fn no_package_section_is_fine() {
        let toml_str = r#"
[providers]
"llm.chat" = "openai-compatible"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert!(cfg.package.is_none());
        assert!(cfg.dependencies.is_empty());
    }

    #[test]
    fn parse_package_with_all_fields() {
        let toml_str = r#"
[package]
name = "http-utils"
version = "1.2.0"
description = "HTTP utilities for Lumen"
license = "MIT"
authors = ["Alice <alice@example.com>"]
repository = "https://github.com/lumen/http-utils"
keywords = ["http", "network"]
readme = "README.md"
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        let pkg = cfg.package.expect("package should be present");
        assert_eq!(pkg.name, "http-utils");
        assert_eq!(pkg.version.as_deref(), Some("1.2.0"));
        assert_eq!(pkg.description.as_deref(), Some("HTTP utilities for Lumen"));
        assert_eq!(pkg.license.as_deref(), Some("MIT"));
        assert_eq!(
            pkg.repository.as_deref(),
            Some("https://github.com/lumen/http-utils")
        );
        assert_eq!(pkg.keywords.as_ref().unwrap().len(), 2);
        assert_eq!(pkg.readme.as_deref(), Some("README.md"));
    }

    #[test]
    fn parse_version_dependencies() {
        let toml_str = r#"
[dependencies]
http-utils = "^1.2"
json-parser = "~0.3"
logging = { version = ">=1.0, <2.0" }
mathlib = { path = "../mathlib" }
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.dependencies.len(), 4);

        match &cfg.dependencies["http-utils"] {
            DependencySpec::Version(v) => assert_eq!(v, "^1.2"),
            _ => panic!("expected version spec"),
        }

        match &cfg.dependencies["json-parser"] {
            DependencySpec::Version(v) => assert_eq!(v, "~0.3"),
            _ => panic!("expected version spec"),
        }

        match &cfg.dependencies["logging"] {
            DependencySpec::VersionDetailed { version, .. } => assert_eq!(version, ">=1.0, <2.0"),
            _ => panic!("expected version detailed spec"),
        }

        match &cfg.dependencies["mathlib"] {
            DependencySpec::Path { path } => assert_eq!(path, "../mathlib"),
            _ => panic!("expected path spec"),
        }
    }

    #[test]
    fn parse_dev_dependencies() {
        let toml_str = r#"
[dev-dependencies]
test-utils = "^0.1"
mock-server = { path = "../test/mock" }
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.dev_dependencies.len(), 2);

        match &cfg.dev_dependencies["test-utils"] {
            DependencySpec::Version(v) => assert_eq!(v, "^0.1"),
            _ => panic!("expected version spec"),
        }
    }
}
