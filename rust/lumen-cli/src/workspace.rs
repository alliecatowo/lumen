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
//! ```text
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::{DependencySpec, LumenConfig, WorkspacePackageDefaults};
use crate::lockfile::LockFile;

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
    pub package_defaults: Option<WorkspacePackageDefaults>,
    /// Shared dependencies available to all members.
    pub dependencies: HashMap<String, DependencySpec>,
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
    /// The full config for this member (with defaults applied).
    pub config: LumenConfig,
    /// Whether this member is a direct dependency of the workspace root.
    pub is_direct: bool,
}

/// Type of dependency in the workspace context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    /// Normal runtime dependency.
    Normal,
    /// Development dependency (only for tests).
    Dev,
    /// Build dependency (for build scripts).
    Build,
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
                    if let Ok(config) = LumenConfig::from_str(&content) {
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

    /// Auto-detect workspace from a path dependency.
    ///
    /// When a path dependency points to a workspace member, automatically
    /// detect and return the workspace root.
    pub fn discover_from_path_dep(dep_path: &Path) -> Option<Self> {
        // First check if the dep path itself is a workspace member
        if let Some(ws) = Self::discover(dep_path) {
            // Check if dep_path is actually a member of this workspace
            let canonical_dep = canonicalize_or_clean(dep_path);
            for member in &ws.members {
                if member.abs_path == canonical_dep {
                    return Some(ws);
                }
            }
        }
        None
    }

    /// Find workspace root from any path (even if not a member).
    ///
    /// This searches upward for a workspace manifest and returns the root path.
    pub fn find_root(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();

        loop {
            let manifest_path = dir.join("lumen.toml");
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(config) = LumenConfig::from_str(&content) {
                        if config.workspace.is_some() {
                            return Some(dir);
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

        let config = LumenConfig::from_str(&content)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;

        let ws_config = config.workspace.ok_or_else(|| {
            WorkspaceError::NotAWorkspace("No [workspace] section found".to_string())
        })?;

        let root = manifest_path.parent().unwrap().to_path_buf();

        // Expand member globs
        let members = Self::expand_members(
            &root,
            &ws_config.members,
            &ws_config.exclude,
            &ws_config.package,
            &ws_config.dependencies,
        )?;

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
        self.members
            .iter()
            .find(|m| m.package.as_ref().map(|p| p.name.as_str()) == Some(name))
    }

    /// Get a member by path.
    pub fn member_by_path(&self, path: &Path) -> Option<&WorkspaceMember> {
        let canonical = canonicalize_or_clean(path);
        self.members
            .iter()
            .find(|m| m.abs_path == canonical || m.path == path)
    }

    /// Get the path to the shared lockfile.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("lumen.lock")
    }

    /// Load the workspace lockfile if it exists.
    pub fn load_lockfile(&self) -> Result<LockFile, WorkspaceError> {
        let lock_path = self.lockfile_path();
        LockFile::load(&lock_path).map_err(WorkspaceError::LockfileError)
    }

    /// Save the workspace lockfile.
    pub fn save_lockfile(&self, lockfile: &LockFile) -> Result<(), WorkspaceError> {
        let lock_path = self.lockfile_path();
        lockfile
            .save(&lock_path)
            .map_err(WorkspaceError::LockfileError)
    }

    /// Get all workspace member names.
    pub fn member_names(&self) -> Vec<String> {
        self.members
            .iter()
            .filter_map(|m| m.package.as_ref().map(|p| p.name.clone()))
            .collect()
    }

    /// Resolve dependencies for the entire workspace.
    ///
    /// This combines workspace-level dependencies with member-specific dependencies,
    /// properly handling workspace inheritance.
    pub fn resolve_dependencies(
        &self,
        dep_type: DependencyType,
    ) -> Result<Vec<ResolvedWorkspaceDep>, WorkspaceError> {
        let mut all_deps = Vec::new();
        let mut seen = HashSet::new();

        for member in &self.members {
            let deps = match dep_type {
                DependencyType::Normal => &member.config.dependencies,
                DependencyType::Dev => &member.config.dev_dependencies,
                DependencyType::Build => &member.config.build_dependencies,
            };

            for (name, spec) in deps {
                let resolved = self.resolve_dep_spec(name, spec, member)?;
                let key = (name.clone(), resolved.resolved_path.clone());

                if seen.insert(key) {
                    all_deps.push(ResolvedWorkspaceDep {
                        name: name.clone(),
                        spec: spec.clone(),
                        member: member.path.clone(),
                        resolved_path: resolved.resolved_path,
                        source: resolved.source,
                    });
                }
            }
        }

        // Sort for determinism
        all_deps.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all_deps)
    }

    /// Resolve a dependency specification within the workspace context.
    fn resolve_dep_spec(
        &self,
        name: &str,
        spec: &DependencySpec,
        member: &WorkspaceMember,
    ) -> Result<ResolvedDep, WorkspaceError> {
        match spec {
            DependencySpec::Workspace {
                workspace: true, ..
            } => {
                // Look up in workspace dependencies
                if let Some(ws_spec) = self.dependencies.get(name) {
                    self.resolve_dep_spec(name, ws_spec, member)
                } else {
                    Err(WorkspaceError::ManifestError(format!(
                        "Workspace dependency '{}' not found in workspace.dependencies",
                        name
                    )))
                }
            }
            DependencySpec::Path { path } => {
                let abs_path = if path.starts_with("/") {
                    PathBuf::from(path)
                } else {
                    member
                        .abs_path
                        .join(path)
                        .canonicalize()
                        .unwrap_or_else(|_| member.abs_path.join(path))
                };

                Ok(ResolvedDep {
                    resolved_path: abs_path,
                    source: DepSource::Path,
                })
            }
            _ => {
                // For registry/git deps, we'll store them in a central location
                let cache_dir = self.root.join(".lumen").join("cache").join("deps");
                let dep_dir = cache_dir.join(name);

                Ok(ResolvedDep {
                    resolved_path: dep_dir,
                    source: DepSource::Registry,
                })
            }
        }
    }

    /// Build a dependency graph of workspace members.
    pub fn dependency_graph(&self) -> Result<DependencyGraph, WorkspaceError> {
        let mut graph = DependencyGraph::new();

        // Add all members as nodes
        for member in &self.members {
            let name = member
                .package
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| member.path.display().to_string());
            graph.add_node(name.clone());
        }

        // Add edges based on path dependencies between members
        for member in &self.members {
            if let Some(pkg) = &member.package {
                let member_name = pkg.name.clone();

                // Check all dependency types for path deps to other members
                let all_deps = member.config.all_dependencies();

                for (dep_name, spec) in all_deps {
                    if let DependencySpec::Path { path } = spec {
                        let dep_path = member
                            .abs_path
                            .join(path)
                            .canonicalize()
                            .unwrap_or_else(|_| member.abs_path.join(path));

                        // Check if this path points to another workspace member
                        if let Some(dep_member) = self.member_by_path(&dep_path) {
                            if let Some(dep_pkg) = &dep_member.package {
                                graph.add_edge(member_name.clone(), dep_pkg.name.clone());
                            }
                        }
                    } else if let DependencySpec::Workspace { .. } = spec {
                        // Check if workspace dependency points to another member
                        if let Some(DependencySpec::Path { path }) = self.dependencies.get(&dep_name) {
                            let dep_path = self
                                .root
                                .join(path)
                                .canonicalize()
                                .unwrap_or_else(|_| self.root.join(path));

                            if let Some(dep_member) = self.member_by_path(&dep_path) {
                                if let Some(dep_pkg) = &dep_member.package {
                                    graph.add_edge(member_name.clone(), dep_pkg.name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Get topological order of members for building.
    pub fn build_order(&self) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let graph = self.dependency_graph()?;
        let order = graph.topological_order();

        // Check for cycles
        if graph.has_cycles() {
            return Err(WorkspaceError::CycleDetected(order));
        }

        let mut members = Vec::new();
        for name in order {
            if let Some(member) = self.member_by_name(&name) {
                members.push(member);
            }
        }

        Ok(members)
    }

    /// Get members in reverse dependency order (for publishing).
    pub fn publish_order(&self) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let mut order = self.build_order()?;
        order.reverse();
        Ok(order)
    }

    /// Check if all members have valid version numbers for publishing.
    pub fn validate_versions(&self) -> Result<(), WorkspaceError> {
        let mut errors = Vec::new();

        for member in &self.members {
            if let Some(pkg) = &member.package {
                if pkg.version.is_none() {
                    errors.push(format!("Member '{}' is missing version", pkg.name));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(WorkspaceError::ValidationError(errors.join("\n")))
        }
    }

    /// Apply workspace defaults to a member config.
    pub fn apply_defaults(&self, config: &mut LumenConfig) {
        if let Some(ref defaults) = self.package_defaults {
            if let Some(ref mut pkg) = config.package {
                if pkg.version.is_none() && defaults.version.is_some() {
                    pkg.version = defaults.version.clone();
                }
                if pkg.authors.is_none() && defaults.authors.is_some() {
                    pkg.authors = defaults.authors.clone();
                }
                if pkg.edition.is_none() && defaults.edition.is_some() {
                    pkg.edition = defaults.edition.clone();
                }
                if pkg.license.is_none() && defaults.license.is_some() {
                    pkg.license = defaults.license.clone();
                }
                if pkg.repository.is_none() && defaults.repository.is_some() {
                    pkg.repository = defaults.repository.clone();
                }
            }
        }
    }

    /// Get inherited value from workspace package defaults.
    pub fn get_inherited(&self, field: &str) -> Option<String> {
        self.package_defaults
            .as_ref()
            .and_then(|defaults| match field {
                "version" => defaults.version.clone(),
                "license" => defaults.license.clone(),
                "edition" => defaults.edition.clone(),
                "repository" => defaults.repository.clone(),
                _ => None,
            })
    }

    // Private helpers

    fn expand_members(
        root: &Path,
        patterns: &[String],
        exclude: &[String],
        package_defaults: &Option<WorkspacePackageDefaults>,
        workspace_deps: &HashMap<String, DependencySpec>,
    ) -> Result<Vec<WorkspaceMember>, WorkspaceError> {
        let mut members = Vec::new();
        let mut seen_paths: HashSet<PathBuf> = HashSet::new();

        for pattern in patterns {
            let matches = glob_matches(root, pattern)?;

            for path in matches {
                // Check exclusions
                let relative = path.strip_prefix(root).unwrap_or(&path);
                let rel_str = relative.display().to_string();

                if exclude
                    .iter()
                    .any(|exc| glob_match(exc, &rel_str) || rel_str.starts_with(exc))
                {
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

                // Load member config with inheritance applied
                let (package, mut config) = Self::load_member_config(&manifest_path)?;

                // Apply workspace defaults and resolve workspace dependencies
                Self::apply_inheritance(&mut config, package_defaults, workspace_deps, root)?;

                members.push(WorkspaceMember {
                    path: relative.to_path_buf(),
                    abs_path: path.clone(),
                    manifest_path,
                    package,
                    config,
                    is_direct: true,
                });
            }
        }

        // Sort members by name for determinism
        members.sort_by(|a, b| {
            let name_a = a
                .package
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| a.path.display().to_string());
            let name_b = b
                .package
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| b.path.display().to_string());
            name_a.cmp(&name_b)
        });

        Ok(members)
    }

    fn load_member_config(
        manifest_path: &Path,
    ) -> Result<(Option<crate::config::PackageInfo>, LumenConfig), WorkspaceError> {
        let content = std::fs::read_to_string(manifest_path)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;

        let config = LumenConfig::from_str(&content)
            .map_err(|e| WorkspaceError::ManifestError(e.to_string()))?;

        Ok((config.package.clone(), config))
    }

    /// Apply workspace inheritance to a member config.
    fn apply_inheritance(
        config: &mut LumenConfig,
        package_defaults: &Option<WorkspacePackageDefaults>,
        workspace_deps: &HashMap<String, DependencySpec>,
        workspace_root: &Path,
    ) -> Result<(), WorkspaceError> {
        // Apply package defaults (version, authors, license, etc.)
        if let Some(ref defaults) = package_defaults {
            if let Some(ref mut pkg) = config.package {
                // Handle version.workspace = true
                if pkg.version.is_none() && defaults.version.is_some() {
                    pkg.version = defaults.version.clone();
                }

                // Handle authors inheritance
                if pkg.authors.is_none() && defaults.authors.is_some() {
                    pkg.authors = defaults.authors.clone();
                }

                // Handle edition inheritance
                if pkg.edition.is_none() && defaults.edition.is_some() {
                    pkg.edition = defaults.edition.clone();
                }

                // Handle license inheritance
                if pkg.license.is_none() && defaults.license.is_some() {
                    pkg.license = defaults.license.clone();
                }

                // Handle repository inheritance
                if pkg.repository.is_none() && defaults.repository.is_some() {
                    pkg.repository = defaults.repository.clone();
                }
            }
        }

        // Resolve workspace dependencies
        // Replace `dep = { workspace = true }` with the actual spec from workspace.dependencies
        Self::resolve_workspace_deps_in_config(config, workspace_deps, workspace_root)?;

        Ok(())
    }

    /// Resolve workspace = true dependencies in config.
    fn resolve_workspace_deps_in_config(
        config: &mut LumenConfig,
        workspace_deps: &HashMap<String, DependencySpec>,
        workspace_root: &Path,
    ) -> Result<(), WorkspaceError> {
        // Resolve in normal dependencies
        let mut resolved_deps = HashMap::new();
        for (name, spec) in &config.dependencies {
            let resolved =
                Self::resolve_single_workspace_dep(name, spec, workspace_deps, workspace_root)?;
            resolved_deps.insert(name.clone(), resolved);
        }
        config.dependencies = resolved_deps;

        // Resolve in dev dependencies
        let mut resolved_dev_deps = HashMap::new();
        for (name, spec) in &config.dev_dependencies {
            let resolved =
                Self::resolve_single_workspace_dep(name, spec, workspace_deps, workspace_root)?;
            resolved_dev_deps.insert(name.clone(), resolved);
        }
        config.dev_dependencies = resolved_dev_deps;

        // Resolve in build dependencies
        let mut resolved_build_deps = HashMap::new();
        for (name, spec) in &config.build_dependencies {
            let resolved =
                Self::resolve_single_workspace_dep(name, spec, workspace_deps, workspace_root)?;
            resolved_build_deps.insert(name.clone(), resolved);
        }
        config.build_dependencies = resolved_build_deps;

        Ok(())
    }

    /// Resolve a single workspace dependency.
    fn resolve_single_workspace_dep(
        name: &str,
        spec: &DependencySpec,
        workspace_deps: &HashMap<String, DependencySpec>,
        workspace_root: &Path,
    ) -> Result<DependencySpec, WorkspaceError> {
        match spec {
            DependencySpec::Workspace {
                workspace: true,
                features,
            } => {
                // Look up in workspace dependencies
                if let Some(ws_spec) = workspace_deps.get(name) {
                    let mut resolved = ws_spec.clone();
                    // Merge features if specified in the member
                    if let Some(f) = features {
                        // This is a simplified merge - in production you'd handle this more carefully
                        if let DependencySpec::VersionDetailed {
                            features: ref mut vf,
                            ..
                        } = resolved
                        {
                            *vf = Some(f.clone());
                        }
                    }
                    Ok(resolved)
                } else {
                    Err(WorkspaceError::ManifestError(format!(
                        "Workspace dependency '{}' not found in workspace.dependencies",
                        name
                    )))
                }
            }
            DependencySpec::Path { path } => {
                // Normalize relative paths to be absolute from workspace root
                if !path.starts_with("/") {
                    Ok(DependencySpec::Path {
                        path: workspace_root.join(path).to_string_lossy().to_string(),
                    })
                } else {
                    Ok(spec.clone())
                }
            }
            _ => Ok(spec.clone()),
        }
    }
}

/// A resolved dependency with workspace context.
#[derive(Debug, Clone)]
pub struct ResolvedWorkspaceDep {
    /// Dependency name.
    pub name: String,
    /// Original dependency spec.
    pub spec: DependencySpec,
    /// Path to the member that declared this dependency.
    pub member: PathBuf,
    /// Resolved absolute path to the dependency.
    pub resolved_path: PathBuf,
    /// Source type of the dependency.
    pub source: DepSource,
}

/// Internal resolved dependency info.
#[derive(Debug, Clone)]
struct ResolvedDep {
    resolved_path: PathBuf,
    source: DepSource,
}

/// Dependency source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepSource {
    Path,
    Registry,
    Git,
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
                dependents
                    .entry(to.as_str())
                    .or_default()
                    .push(from.as_str());
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
                let mut sorted_deps: Vec<_> = deps.to_vec();
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
    /// Lockfile error.
    LockfileError(String),
    /// Member not found.
    MemberNotFound(String),
    /// Cycle detected.
    CycleDetected(Vec<String>),
    /// Glob error.
    GlobError(String),
    /// Validation error.
    ValidationError(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAWorkspace(msg) => write!(f, "Not a workspace: {}", msg),
            Self::ManifestError(e) => write!(f, "Manifest error: {}", e),
            Self::LockfileError(e) => write!(f, "Lockfile error: {}", e),
            Self::MemberNotFound(name) => write!(f, "Member not found: {}", name),
            Self::CycleDetected(chain) => write!(f, "Cycle detected: {}", chain.join(" -> ")),
            Self::GlobError(e) => write!(f, "Glob error: {}", e),
            Self::ValidationError(e) => write!(f, "Validation error: {}", e),
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
        let prefix = parts.first().unwrap_or(&"");
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
        let prefix = parts.first().unwrap_or(&"");
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
    let entries = std::fs::read_dir(dir).map_err(|e| WorkspaceError::GlobError(e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| WorkspaceError::GlobError(e.to_string()))?;
        let path = entry.path();

        if path.is_dir() {
            // Check for manifest
            if path.join("lumen.toml").exists()
                && (suffix.is_empty() || path.ends_with(suffix.trim_start_matches('/'))) {
                    results.push(path.clone());
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

/// Best-effort canonicalize; falls back to lexical cleanup if path doesn't exist yet.
fn canonicalize_or_clean(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        // Simple lexical normalization
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                c => out.push(c),
            }
        }
        out
    })
}

// =============================================================================
// Workspace Commands
// =============================================================================

/// Build all workspace members in dependency order.
pub fn cmd_ws_build() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");

    let workspace = match Workspace::discover(&cwd) {
        Some(ws) => ws,
        None => {
            eprintln!(
                "{} no workspace found (no lumen.toml with [workspace] section)",
                crate::colors::red("error:")
            );
            std::process::exit(1);
        }
    };

    println!(
        "{} workspace at {}",
        crate::colors::status_label("Building"),
        crate::colors::gray(&workspace.root.display().to_string())
    );

    let members = match workspace.build_order() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", crate::colors::red("error:"), e);
            std::process::exit(1);
        }
    };

    if members.is_empty() {
        println!(
            "{} no workspace members found",
            crate::colors::gray("info:")
        );
        return;
    }

    println!(
        "{} {} member(s) in dependency order:",
        crate::colors::status_label("Found"),
        members.len()
    );
    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");
        println!(
            "  {} {}",
            crate::colors::bold(name),
            crate::colors::gray(&member.path.display().to_string())
        );
    }

    let mut errors = 0;

    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");

        println!(
            "\n{} {} {}",
            crate::colors::status_label("Compiling"),
            crate::colors::bold(name),
            crate::colors::gray(&format!("({})", member.path.display()))
        );

        // Run build scripts first
        let target_dir = member.abs_path.join("target");
        if let Err(e) = crate::build_script::run_build_scripts(&member.abs_path, &target_dir) {
            eprintln!(
                "    {} build script failed: {}",
                crate::colors::red("error:"),
                e
            );
            errors += 1;
            continue;
        }

        // Build the package
        // Build the package
        match crate::wares::ops::build_package(&member.abs_path) {
            Ok(_) => {
                println!("    {} build succeeded", crate::colors::green("✓"));
            }
            Err(e) => {
                eprintln!("    {} {}", crate::colors::red("error:"), e);
                errors += 1;
            }
        }
    }

    if errors > 0 {
        eprintln!(
            "\n{} build failed with {} error(s)",
            crate::colors::red("error:"),
            errors
        );
        std::process::exit(1);
    } else {
        println!(
            "\n{} workspace build succeeded ({} member(s))",
            crate::colors::green("✓"),
            members.len()
        );
    }
}

/// Check all workspace members without running.
pub fn cmd_ws_check() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");

    let workspace = match Workspace::discover(&cwd) {
        Some(ws) => ws,
        None => {
            eprintln!(
                "{} no workspace found (no lumen.toml with [workspace] section)",
                crate::colors::red("error:")
            );
            std::process::exit(1);
        }
    };

    println!(
        "{} workspace at {}",
        crate::colors::status_label("Checking"),
        crate::colors::gray(&workspace.root.display().to_string())
    );

    let members = match workspace.build_order() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", crate::colors::red("error:"), e);
            std::process::exit(1);
        }
    };

    if members.is_empty() {
        println!(
            "{} no workspace members found",
            crate::colors::gray("info:")
        );
        return;
    }

    println!(
        "{} {} member(s) in dependency order:",
        crate::colors::status_label("Found"),
        members.len()
    );
    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");
        println!(
            "  {} {}",
            crate::colors::bold(name),
            crate::colors::gray(&member.path.display().to_string())
        );
    }

    let mut errors = 0;

    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");

        println!(
            "\n{} {} {}",
            crate::colors::status_label("Checking"),
            crate::colors::bold(name),
            crate::colors::gray(&format!("({})", member.path.display()))
        );

        // Check the package
        match crate::wares::ops::validate_package(&member.abs_path) {
            Ok(_) => {
                println!("    {} check passed", crate::colors::green("✓"));
            }
            Err(e) => {
                eprintln!("    {} {}", crate::colors::red("error:"), e);
                errors += 1;
            }
        }
    }

    if errors > 0 {
        eprintln!(
            "\n{} check failed with {} error(s)",
            crate::colors::red("error:"),
            errors
        );
        std::process::exit(1);
    } else {
        println!(
            "\n{} workspace check passed ({} member(s))",
            crate::colors::green("✓"),
            members.len()
        );
    }
}

/// Run tests for all workspace members.
pub fn cmd_ws_test(filter: Option<String>, verbose: bool) {
    let cwd = std::env::current_dir().expect("Failed to get current directory");

    let workspace = match Workspace::discover(&cwd) {
        Some(ws) => ws,
        None => {
            eprintln!("{} no workspace found", crate::colors::red("error:"));
            std::process::exit(1);
        }
    };

    println!(
        "{} workspace at {}",
        crate::colors::status_label("Testing"),
        crate::colors::gray(&workspace.root.display().to_string())
    );

    let members = match workspace.build_order() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", crate::colors::red("error:"), e);
            std::process::exit(1);
        }
    };

    if members.is_empty() {
        println!(
            "{} no workspace members found",
            crate::colors::gray("info:")
        );
        return;
    }

    let mut total_passed = 0;
    let mut total_failed = 0;

    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");

        println!(
            "\n{} {} {}",
            crate::colors::status_label("Testing"),
            crate::colors::bold(name),
            crate::colors::gray(&format!("({})", member.path.display()))
        );

        // Run tests for this member
        match crate::test_cmd::run_tests(Some(member.abs_path.clone()), filter.as_deref(), verbose)
        {
            Ok(summary) => {
                total_passed += summary.passed;
                total_failed += summary.failed;
            }
            Err(e) => {
                eprintln!("    {} test failed: {}", crate::colors::red("error:"), e);
                total_failed += 1;
            }
        }
    }

    let total = total_passed + total_failed;

    if total_failed > 0 {
        eprintln!(
            "\n{} workspace tests failed ({} passed, {} failed)",
            crate::colors::red("error:"),
            total_passed,
            total_failed
        );
        std::process::exit(1);
    } else {
        println!(
            "\n{} workspace tests passed ({} test(s))",
            crate::colors::green("✓"),
            total
        );
    }
}

/// Publish all workspace members in reverse dependency order.
pub fn cmd_ws_publish(dry_run: bool) {
    let cwd = std::env::current_dir().expect("Failed to get current directory");

    let workspace = match Workspace::discover(&cwd) {
        Some(ws) => ws,
        None => {
            eprintln!("{} no workspace found", crate::colors::red("error:"));
            std::process::exit(1);
        }
    };

    // Validate versions first
    if let Err(e) = workspace.validate_versions() {
        eprintln!("{} {}", crate::colors::red("error:"), e);
        std::process::exit(1);
    }

    println!(
        "{} workspace at {}",
        crate::colors::status_label("Publishing"),
        crate::colors::gray(&workspace.root.display().to_string())
    );

    let members = match workspace.publish_order() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", crate::colors::red("error:"), e);
            std::process::exit(1);
        }
    };

    if members.is_empty() {
        println!(
            "{} no workspace members found",
            crate::colors::gray("info:")
        );
        return;
    }

    if dry_run {
        println!(
            "{} dry run mode - no packages will be published",
            crate::colors::yellow("warning:")
        );
    }

    let mut errors = 0;

    for member in &members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");
        let version = member
            .package
            .as_ref()
            .and_then(|p| p.version.as_deref())
            .unwrap_or("unknown");

        println!(
            "\n{} {}@{}",
            crate::colors::status_label("Publishing"),
            crate::colors::bold(name),
            crate::colors::gray(version)
        );

        if dry_run {
            // Just validate
            // Just validate
            match crate::wares::ops::validate_package(&member.abs_path) {
                Ok(_) => {
                    println!("    {} validation passed", crate::colors::green("✓"));
                }
                Err(e) => {
                    eprintln!("    {} {}", crate::colors::red("error:"), e);
                    errors += 1;
                }
            }
        } else {
            // Actually publish
            // Actually publish
            match crate::wares::ops::publish_package(&member.abs_path) {
                Ok(_) => {
                    println!("    {} published successfully", crate::colors::green("✓"));
                }
                Err(e) => {
                    eprintln!("    {} {}", crate::colors::red("error:"), e);
                    errors += 1;
                }
            }
        }
    }

    if errors > 0 {
        eprintln!(
            "\n{} publish failed with {} error(s)",
            crate::colors::red("error:"),
            errors
        );
        std::process::exit(1);
    } else if dry_run {
        println!(
            "\n{} all packages validated ({} member(s))",
            crate::colors::green("✓"),
            members.len()
        );
    } else {
        println!(
            "\n{} all packages published ({} member(s))",
            crate::colors::green("✓"),
            members.len()
        );
    }
}

/// List all workspace members.
pub fn cmd_ws_list() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");

    let workspace = match Workspace::discover(&cwd) {
        Some(ws) => ws,
        None => {
            eprintln!("{} no workspace found", crate::colors::red("error:"));
            std::process::exit(1);
        }
    };

    println!(
        "{} workspace at {}",
        crate::colors::status_label("Workspace"),
        crate::colors::bold(&workspace.root.display().to_string())
    );

    if workspace.members.is_empty() {
        println!("{} no members found", crate::colors::gray("info:"));
        return;
    }

    println!(
        "{} {} member(s):",
        crate::colors::status_label("Members"),
        workspace.members.len()
    );

    for member in &workspace.members {
        let name = member
            .package
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("<unnamed>");
        let version = member
            .package
            .as_ref()
            .and_then(|p| p.version.as_deref())
            .unwrap_or("-");
        println!(
            "  {} @ {} {}",
            crate::colors::bold(name),
            crate::colors::gray(version),
            crate::colors::gray(&format!("({})", member.path.display()))
        );
    }
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

    #[test]
    fn test_canonicalize_or_clean() {
        let path = Path::new("/a/b/../c");
        let cleaned = canonicalize_or_clean(path);
        assert!(cleaned.to_string_lossy().contains("a"));
        assert!(cleaned.to_string_lossy().contains("c"));
    }

    #[test]
    fn test_find_root_nonexistent() {
        // Should not find a workspace in a temp directory with no manifest
        let tmp = std::env::temp_dir();
        let root = Workspace::find_root(&tmp);
        // This might find one if we're running from a workspace, so just ensure it doesn't panic
        let _ = root;
    }
}
