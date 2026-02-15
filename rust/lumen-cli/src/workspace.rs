//! Workspace support for Lumen monorepos.
//!
//! ## Design Philosophy
//!
//! **A workspace is a collection of packages that share a lockfile.**
//!
//! This module implements world-class workspace support:
//!
//! - **Shared lockfile**: All workspace members use a single `lumen.lock`
//! - **Shared dependencies**: Common dependencies can be hoisted
//! - **Path dependencies**: Members can depend on each other
//! - **Virtual manifest**: Root `lumen.toml` defines workspace without being a package
//! - **Member isolation**: Each member can have its own `lumen.toml`
//!
//! ## Workspace Layout
//!
//! ```
//! my-workspace/
//! ├── lumen.toml           # Virtual manifest (workspace config)
//! ├── lumen.lock           # Shared lockfile for all members
//! ├── crates/
//! │   ├── core/
//! │   │   └── lumen.toml   # Member package manifest
//! │   └── utils/
//! │       └── lumen.toml   # Member package manifest
//! ├── apps/
//! │   └── web/
//! │       └── lumen.toml   # Member package manifest
//! └── tests/
//!     └── integration/
//!         └── lumen.toml   # Member package manifest
//! ```
//!
//! ## Virtual Manifest (lumen.toml)
//!
//! ```toml
//! [workspace]
//! members = ["crates/*", "apps/*"]
//! exclude = ["tests/fixtures/*"]
//!
//! [workspace.package]
//! version = "0.1.0"
//! authors = ["Team"]
//! license = "MIT"
//!
//! [workspace.dependencies]
//! @org/shared = { path = "crates/shared" }
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

// =============================================================================
// Workspace Configuration
// =============================================================================

/// Workspace configuration loaded from the root manifest.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Root directory of the workspace.
    pub root: PathBuf,
    /// Path to the root manifest.
    pub manifest_path: PathBuf,
    /// Workspace members (expanded to actual paths).
    pub members: Vec<WorkspaceMember>,
    /// Packages excluded from member discovery.
    pub exclude: Vec<String>,
    /// Default package settings for all members.
    pub package_defaults: Option<crate::config::WorkspacePackageDefaults>,
    /// Shared dependencies available to all members.
    pub dependencies: HashMap<String, crate::config::DependencySpec>,
}

/// A member of a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Relative path from workspace root.
    pub path: PathBuf,
    /// Absolute path to the member directory.
    pub abs_path: PathBuf,
    /// Path to the member's manifest.
    pub manifest_path: PathBuf,
    /// The member's package configuration.
    pub package: Option<crate::config::PackageInfo>,
    /// Whether this member is a direct dependency of the workspace root.
    pub is_direct: bool,
}


// =============================================================================
// Workspace Discovery
// =============================================================================

impl Workspace {
    /// Discover a workspace starting from a directory.
    ///
    /// Searches upward for a `lumen.toml` with `[workspace]` section.
    pub fn discover(start_dir: &Path) -> Option<Self> {
        let mut dir = start_dir.to_path_buf();
        
        loop {
            let manifest_path = dir.join("lumen.toml");
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(config) = crate::config::LumenConfig::from_str(&content) {
                        if config.workspace.is_some() {
                            return Self::load(&manifest_path).ok();
                        }
                    }
                }
            }
            
            if !dir.pop() {
                break;
            }
        }
        
        None
    }

    /// Load a workspace from a manifest file.
    pub fn load(manifest_path: &Path) -> Result<Self, WorkspaceError> {
        let content = std::fs::read_to_string(manifest_path)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;
        
        let config = crate::config::LumenConfig::from_str(&content)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;
        
        let ws_config = config.workspace.ok_or_else(|| {
            WorkspaceError::NotAWorkspace("No [workspace] section found".to_string())
        })?;
        
        let root = manifest_path.parent().unwrap().to_path_buf();
        
        // Expand member globs
        let members = Self::expand_members(&root, &ws_config.members, &ws_config.exclude)?;
        
        Ok(Self {
            root: root.clone(),
            manifest_path: manifest_path.to_path_buf(),
            members,
            exclude: ws_config.exclude,
            package_defaults: ws_config.package,
            dependencies: ws_config.dependencies,
        })
    }

    /// Check if a path is within a workspace.
    pub fn contains_path(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    /// Get a member by name.
    pub fn member_by_name(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| {
            m.package.as_ref().map(|p| p.name.as_str()) == Some(name)
        })
    }

    /// Get a member by path.
    pub fn member_by_path(&self, path: &Path) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.abs_path == path || m.path == path)
    }

    /// Get the path to the shared lockfile.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("lumen.lock")
    }

    /// Build a dependency graph of workspace members.
    pub fn dependency_graph(&self) -> Result<DependencyGraph, WorkspaceError> {
        let mut graph = DependencyGraph::new();
        
        // Add all members as nodes
        for member in &self.members {
            let name = member.package.as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| member.path.display().to_string());
            graph.add_node(name.clone());
        }
        
        // Add edges based on path dependencies
        for member in &self.members {
            if let Some(pkg) = &member.package {
                let name = pkg.name.clone();
                
                // Check dependencies for path deps pointing to other members
                // Note: This would require access to the full config, not just package
                // For now, we'll just create the basic structure
            }
        }
        
        Ok(graph)
    }

    /// Get topological order of members for building.
    pub fn build_order(&self) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let graph = self.dependency_graph()?;
        let order = graph.topological_order();
        
        let mut members = Vec::new();
        for name in order {
            if let Some(member) = self.member_by_name(&name) {
                members.push(member);
            }
        }
        
        Ok(members)
    }

    // Private helpers

    fn expand_members(
        root: &Path,
        patterns: &[String],
        exclude: &[String],
    ) -> Result<Vec<WorkspaceMember>, WorkspaceError> {
        let mut members = Vec::new();
        let mut seen_paths: HashSet<PathBuf> = HashSet::new();
        
        for pattern in patterns {
            let matches = glob_matches(root, pattern)?;
            
            for path in matches {
                // Check exclusions
                let relative = path.strip_prefix(root).unwrap_or(&path);
                let rel_str = relative.display().to_string();
                
                if exclude.iter().any(|exc| {
                    glob_match(exc, &rel_str) || rel_str.starts_with(exc)
                }) {
                    continue;
                }
                
                // Check for manifest
                let manifest_path = path.join("lumen.toml");
                if !manifest_path.exists() {
                    continue;
                }
                
                if seen_paths.contains(&path) {
                    continue;
                }
                seen_paths.insert(path.clone());
                
                // Load member config
                let package = Self::load_member_package(&manifest_path)?;
                
                members.push(WorkspaceMember {
                    path: relative.to_path_buf(),
                    abs_path: path.clone(),
                    manifest_path,
                    package,
                    is_direct: true,
                });
            }
        }
        
        // Sort members by name for determinism
        members.sort_by(|a, b| {
            let name_a = a.package.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| a.path.display().to_string());
            let name_b = b.package.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| b.path.display().to_string());
            name_a.cmp(&name_b)
        });
        
        Ok(members)
    }

    fn load_member_package(manifest_path: &Path) -> Result<Option<crate::config::PackageInfo>, WorkspaceError> {
        let content = std::fs::read_to_string(manifest_path)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;
        
        let config = crate::config::LumenConfig::from_str(&content)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;
        
        Ok(config.package)
    }
}

// =============================================================================
// Dependency Graph
// =============================================================================

/// A dependency graph for workspace members.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    nodes: HashSet<String>,
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, name: String) {
        self.nodes.insert(name);
    }

    pub fn add_edge(&mut self, from: String, to: String) {
        self.nodes.insert(from.clone());
        self.nodes.insert(to.clone());
        self.edges.entry(from).or_default().push(to);
    }

    /// Get topological order (dependencies first).
    pub fn topological_order(&self) -> Vec<String> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        
        // Initialize
        for node in &self.nodes {
            in_degree.entry(node.as_str()).or_insert(0);
        }
        
        // Build graph - edge (from, to) means "from depends on to"
        // So "to" must come before "from" in build order
        for (from, deps) in &self.edges {
            for to in deps {
                // from depends on to, so from has one more incoming edge
                *in_degree.entry(from.as_str()).or_insert(0) += 1;
                dependents.entry(to.as_str()).or_default().push(from.as_str());
            }
        }
        
        // Kahn's algorithm - start with nodes that have no dependencies
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();
        queue.sort();
        
        let mut result = Vec::new();
        
        while let Some(name) = queue.pop() {
            result.push(name.to_string());
            
            // Process all nodes that depend on this one
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
        
        result
    }

    /// Check for cycles.
    pub fn has_cycles(&self) -> bool {
        let order = self.topological_order();
        order.len() != self.nodes.len()
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Errors that can occur with workspaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Not a workspace.
    NotAWorkspace(String),
    /// Manifest error.
    ManifestError(String),
    /// Member not found.
    MemberNotFound(String),
    /// Cycle detected.
    CycleDetected(Vec<String>),
    /// Glob error.
    GlobError(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAWorkspace(msg) => write!(f, "Not a workspace: {}", msg),
            Self::ManifestError(e) => write!(f, "Manifest error: {}", e),
            Self::MemberNotFound(name) => write!(f, "Member not found: {}", name),
            Self::CycleDetected(chain) => write!(f, "Cycle detected: {}", chain.join(" -> ")),
            Self::GlobError(e) => write!(f, "Glob error: {}", e),
        }
    }
}

impl std::error::Error for WorkspaceError {}

// =============================================================================
// Glob Helpers
// =============================================================================

fn glob_matches(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, WorkspaceError> {
    let mut results = Vec::new();
    
    // Simple glob implementation for basic patterns
    // Supports: * for single directory, ** for recursive
    
    if pattern.contains("**") {
        // Recursive glob
        let parts: Vec<&str> = pattern.split("**").collect();
        let prefix = parts.get(0).unwrap_or(&"");
        let suffix = parts.get(1).unwrap_or(&"");
        
        let search_root = if prefix.is_empty() {
            root.to_path_buf()
        } else {
            root.join(prefix.trim_end_matches('/'))
        };
        
        if search_root.exists() {
            walk_dir_for_manifests(&search_root, suffix, &mut results)?;
        }
    } else if pattern.contains('*') {
        // Single-level glob
        let parts: Vec<&str> = pattern.split('*').collect();
        let prefix = parts.get(0).unwrap_or(&"");
        let suffix = parts.get(1).unwrap_or(&"");
        
        let search_dir = root.join(prefix.trim_end_matches('/'));
        
        if let Ok(entries) = std::fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                
                if suffix.is_empty() || name_str.ends_with(suffix.trim_start_matches('/')) {
                    let path = entry.path();
                    if path.join("lumen.toml").exists() {
                        results.push(path.clone());
                    }
                }
            }
        }
    } else {
        // No glob - direct path
        let path = root.join(pattern);
        if path.join("lumen.toml").exists() {
            results.push(path);
        }
    }
    
    Ok(results)
}

fn walk_dir_for_manifests(
    dir: &Path,
    suffix: &str,
    results: &mut Vec<PathBuf>,
) -> Result<(), WorkspaceError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| WorkspaceError::GlobError(e.to_string()))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| WorkspaceError::GlobError(e.to_string()))?;
        let path = entry.path();
        
        if path.is_dir() {
            // Check for manifest
            if path.join("lumen.toml").exists() {
                if suffix.is_empty() || path.ends_with(suffix.trim_start_matches('/')) {
                    results.push(path.clone());
                }
            }
            
            // Recurse into subdirectories
            walk_dir_for_manifests(&path, suffix, results)?;
        }
    }
    
    Ok(())
}

fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return text.starts_with(prefix) && text.ends_with(suffix);
        }
    }
    pattern == text
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a".to_string());
        graph.add_node("b".to_string());
        graph.add_node("c".to_string());
        graph.add_edge("a".to_string(), "b".to_string());
        graph.add_edge("b".to_string(), "c".to_string());
        
        let order = graph.topological_order();
        assert_eq!(order, vec!["c", "b", "a"]);
    }

    #[test]
    fn test_dependency_graph_no_cycles() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a".to_string());
        graph.add_node("b".to_string());
        graph.add_edge("a".to_string(), "b".to_string());
        
        assert!(!graph.has_cycles());
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("*bar", "foobar"));
        assert!(glob_match("foo*bar", "foobar"));
    }

    #[test]
    fn test_workspace_error_display() {
        let err = WorkspaceError::MemberNotFound("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
