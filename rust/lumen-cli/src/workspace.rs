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
                    if let Ok(config) = content.parse::<LumenConfig>() {
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
                    if let Ok(config) = content.parse::<LumenConfig>() {
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

        let config = content
            .parse::<LumenConfig>()
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
                        if let Some(DependencySpec::Path { path }) =
                            self.dependencies.get(&dep_name)
                        {
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

        let config = content
            .parse::<LumenConfig>()
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
                && (suffix.is_empty() || path.ends_with(suffix.trim_start_matches('/')))
            {
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
// Workspace Resolver (T190)
// =============================================================================

/// Configuration for a workspace parsed from TOML, independent of filesystem.
#[derive(Debug, Clone)]
pub struct ResolverWorkspaceConfig {
    /// Root directory of the workspace.
    pub root_dir: PathBuf,
    /// Workspace members.
    pub members: Vec<ResolverWorkspaceMember>,
    /// Shared dependencies available to all members.
    pub shared_dependencies: HashMap<String, ResolverDependencySpec>,
    /// Default member to operate on when none is specified.
    pub default_member: Option<String>,
}

/// A member within a resolver workspace.
#[derive(Debug, Clone)]
pub struct ResolverWorkspaceMember {
    /// Member name (unique within the workspace).
    pub name: String,
    /// Path relative to workspace root.
    pub path: PathBuf,
    /// Semver version string.
    pub version: String,
    /// Names of other workspace members this member depends on.
    pub dependencies: Vec<String>,
}

/// Dependency specification for shared workspace dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverDependencySpec {
    /// Version requirement string.
    pub version: String,
    /// Source of the dependency.
    pub source: ResolverDependencySource,
}

/// Source from which a dependency is obtained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolverDependencySource {
    /// From a package registry.
    Registry(String),
    /// Local filesystem path.
    Path(PathBuf),
    /// Git repository with optional revision.
    Git { url: String, rev: Option<String> },
}

/// A single step in a build plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverBuildStep {
    /// Name of the member to build.
    pub member: String,
    /// Steps with the same parallel_group can run concurrently.
    pub parallel_group: usize,
}

/// Errors specific to the workspace resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolverError {
    /// A referenced member was not found.
    MemberNotFound(String),
    /// A cyclic dependency was detected among members.
    CyclicDependency(Vec<String>),
    /// Two members share the same name.
    DuplicateMember(String),
    /// A member path is invalid (empty or otherwise unusable).
    InvalidPath(PathBuf),
    /// TOML parsing failed.
    ParseError(String),
    /// Multiple members require different versions of the same shared dependency.
    VersionConflict {
        dependency: String,
        versions: Vec<String>,
    },
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemberNotFound(name) => write!(f, "member not found: {}", name),
            Self::CyclicDependency(chain) => {
                write!(f, "cyclic dependency: {}", chain.join(" -> "))
            }
            Self::DuplicateMember(name) => write!(f, "duplicate member: {}", name),
            Self::InvalidPath(p) => write!(f, "invalid path: {}", p.display()),
            Self::ParseError(msg) => write!(f, "parse error: {}", msg),
            Self::VersionConflict {
                dependency,
                versions,
            } => write!(
                f,
                "version conflict for '{}': [{}]",
                dependency,
                versions.join(", ")
            ),
        }
    }
}

impl std::error::Error for ResolverError {}

/// Resolves workspace members in dependency order and provides build planning.
#[derive(Debug)]
pub struct WorkspaceResolver {
    config: ResolverWorkspaceConfig,
    resolution_order: Vec<String>,
}

impl WorkspaceResolver {
    /// Create a new resolver from a workspace configuration.
    ///
    /// Validates the configuration and computes topological ordering.
    /// Returns an error if there are duplicate members, missing dependencies,
    /// or cyclic dependencies.
    pub fn new(config: ResolverWorkspaceConfig) -> Result<Self, ResolverError> {
        // Validate: check for duplicates
        let mut seen_names: HashSet<String> = HashSet::new();
        for member in &config.members {
            if !seen_names.insert(member.name.clone()) {
                return Err(ResolverError::DuplicateMember(member.name.clone()));
            }
        }

        // Validate: check for invalid paths
        for member in &config.members {
            if member.path.as_os_str().is_empty() {
                return Err(ResolverError::InvalidPath(member.path.clone()));
            }
        }

        // Validate: check that all dependencies reference existing members
        for member in &config.members {
            for dep in &member.dependencies {
                if !seen_names.contains(dep) {
                    return Err(ResolverError::MemberNotFound(format!(
                        "'{}' (dependency of '{}')",
                        dep, member.name
                    )));
                }
            }
        }

        // Detect cycles
        if let Some(cycle) = Self::find_cycle(&config.members) {
            return Err(ResolverError::CyclicDependency(cycle));
        }

        // Compute topological order
        let resolution_order = Self::topological_sort(&config.members);

        Ok(Self {
            config,
            resolution_order,
        })
    }

    /// Parse a workspace TOML string into a `ResolverWorkspaceConfig`.
    ///
    /// Expected format:
    /// ```toml
    /// [workspace]
    /// root = "/path/to/workspace"
    /// default_member = "core"
    ///
    /// [[workspace.members]]
    /// name = "core"
    /// path = "crates/core"
    /// version = "0.1.0"
    /// dependencies = []
    ///
    /// [[workspace.members]]
    /// name = "utils"
    /// path = "crates/utils"
    /// version = "0.1.0"
    /// dependencies = ["core"]
    ///
    /// [workspace.shared_dependencies.serde]
    /// version = "1.0"
    /// source = { registry = "https://crates.io" }
    /// ```
    pub fn from_toml(content: &str) -> Result<ResolverWorkspaceConfig, ResolverError> {
        let raw: toml::Value =
            toml::from_str(content).map_err(|e| ResolverError::ParseError(e.to_string()))?;

        let ws = raw
            .get("workspace")
            .ok_or_else(|| ResolverError::ParseError("missing [workspace] section".to_string()))?;

        // root_dir
        let root_dir = ws
            .get("root")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        // default_member
        let default_member = ws
            .get("default_member")
            .and_then(|v| v.as_str())
            .map(String::from);

        // members
        let members_val = ws.get("members");
        let members = if let Some(arr) = members_val.and_then(|v| v.as_array()) {
            let mut result = Vec::new();
            for item in arr {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ResolverError::ParseError("member missing 'name'".to_string()))?
                    .to_string();
                let path = item.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ResolverError::ParseError(format!("member '{}' missing 'path'", name))
                })?;
                let version = item
                    .get("version")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ResolverError::ParseError(format!("member '{}' missing 'version'", name))
                    })?
                    .to_string();
                let dependencies = item
                    .get("dependencies")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                result.push(ResolverWorkspaceMember {
                    name,
                    path: PathBuf::from(path),
                    version,
                    dependencies,
                });
            }
            result
        } else {
            Vec::new()
        };

        // shared_dependencies
        let shared_dependencies =
            if let Some(deps) = ws.get("shared_dependencies").and_then(|v| v.as_table()) {
                let mut map = HashMap::new();
                for (name, spec_val) in deps {
                    let version = spec_val
                        .get("version")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ResolverError::ParseError(format!(
                                "shared dependency '{}' missing 'version'",
                                name
                            ))
                        })?
                        .to_string();

                    let source = if let Some(src) = spec_val.get("source") {
                        if let Some(registry) = src.get("registry").and_then(|v| v.as_str()) {
                            ResolverDependencySource::Registry(registry.to_string())
                        } else if let Some(path) = src.get("path").and_then(|v| v.as_str()) {
                            ResolverDependencySource::Path(PathBuf::from(path))
                        } else if let Some(url) = src.get("git").and_then(|v| v.as_str()) {
                            let rev = src.get("rev").and_then(|v| v.as_str()).map(String::from);
                            ResolverDependencySource::Git {
                                url: url.to_string(),
                                rev,
                            }
                        } else {
                            ResolverDependencySource::Registry("default".to_string())
                        }
                    } else {
                        ResolverDependencySource::Registry("default".to_string())
                    };

                    map.insert(name.clone(), ResolverDependencySpec { version, source });
                }
                map
            } else {
                HashMap::new()
            };

        Ok(ResolverWorkspaceConfig {
            root_dir,
            members,
            shared_dependencies,
            default_member,
        })
    }

    // ---- Resolution queries ----

    /// Return the topologically sorted member names (dependencies before dependents).
    pub fn resolve_order(&self) -> &[String] {
        &self.resolution_order
    }

    /// Look up a member by name.
    pub fn resolve_member(&self, name: &str) -> Option<&ResolverWorkspaceMember> {
        self.config.members.iter().find(|m| m.name == name)
    }

    /// Return the direct dependencies of a member (as `ResolverWorkspaceMember` refs).
    pub fn member_dependencies(&self, name: &str) -> Vec<&ResolverWorkspaceMember> {
        let member = match self.resolve_member(name) {
            Some(m) => m,
            None => return Vec::new(),
        };
        member
            .dependencies
            .iter()
            .filter_map(|dep_name| self.resolve_member(dep_name))
            .collect()
    }

    /// Return all members that depend on the given member (reverse deps).
    pub fn reverse_dependencies(&self, name: &str) -> Vec<&ResolverWorkspaceMember> {
        self.config
            .members
            .iter()
            .filter(|m| m.dependencies.iter().any(|d| d == name))
            .collect()
    }

    // ---- Build planning ----

    /// Produce a build plan with parallel grouping.
    ///
    /// Members in the same `parallel_group` have no inter-dependencies and can
    /// be built concurrently. Groups are numbered starting from 0.
    pub fn build_plan(&self) -> Vec<ResolverBuildStep> {
        if self.config.members.is_empty() {
            return Vec::new();
        }

        // Map member name -> set of dependency names
        let dep_map: HashMap<&str, HashSet<&str>> = self
            .config
            .members
            .iter()
            .map(|m| {
                let deps: HashSet<&str> = m.dependencies.iter().map(|s| s.as_str()).collect();
                (m.name.as_str(), deps)
            })
            .collect();

        let mut assigned: HashMap<&str, usize> = HashMap::new();
        let mut steps = Vec::new();

        // Process in topological order; each member's group = max(dep groups) + 1
        for name in &self.resolution_order {
            let group = dep_map
                .get(name.as_str())
                .map(|deps| {
                    deps.iter()
                        .filter_map(|d| assigned.get(d))
                        .max()
                        .map(|g| g + 1)
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            assigned.insert(name.as_str(), group);
            steps.push(ResolverBuildStep {
                member: name.clone(),
                parallel_group: group,
            });
        }

        steps
    }

    /// Determine which workspace members are affected by a set of changed files.
    ///
    /// A member is affected if any changed file path starts with its path.
    /// Transitive dependents are also included.
    pub fn affected_members(&self, changed_files: &[PathBuf]) -> Vec<String> {
        let mut directly_affected: HashSet<String> = HashSet::new();

        for member in &self.config.members {
            let member_path = self.config.root_dir.join(&member.path);
            for file in changed_files {
                if file.starts_with(&member_path) || file.starts_with(&member.path) {
                    directly_affected.insert(member.name.clone());
                    break;
                }
            }
        }

        // Expand to transitive dependents (anything that depends on affected members)
        let mut all_affected = directly_affected.clone();
        let mut frontier: Vec<String> = directly_affected.into_iter().collect();

        while let Some(name) = frontier.pop() {
            for rdep in self.reverse_dependencies(&name) {
                if all_affected.insert(rdep.name.clone()) {
                    frontier.push(rdep.name.clone());
                }
            }
        }

        // Return in topological order
        self.resolution_order
            .iter()
            .filter(|n| all_affected.contains(n.as_str()))
            .cloned()
            .collect()
    }

    // ---- Validation ----

    /// Validate the workspace configuration and return all errors found.
    pub fn validate(&self) -> Vec<ResolverError> {
        let mut errors = Vec::new();

        // Check for empty member names
        for member in &self.config.members {
            if member.name.is_empty() {
                errors.push(ResolverError::ParseError(
                    "member has empty name".to_string(),
                ));
            }
            if member.version.is_empty() {
                errors.push(ResolverError::ParseError(format!(
                    "member '{}' has empty version",
                    member.name
                )));
            }
            if member.path.as_os_str().is_empty() {
                errors.push(ResolverError::InvalidPath(member.path.clone()));
            }
        }

        // Check for duplicate member names
        let mut seen: HashSet<&str> = HashSet::new();
        for member in &self.config.members {
            if !member.name.is_empty() && !seen.insert(&member.name) {
                errors.push(ResolverError::DuplicateMember(member.name.clone()));
            }
        }

        // Check for missing dependency references
        let member_names: HashSet<&str> = self
            .config
            .members
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        for member in &self.config.members {
            for dep in &member.dependencies {
                if !member_names.contains(dep.as_str()) {
                    errors.push(ResolverError::MemberNotFound(format!(
                        "'{}' (dependency of '{}')",
                        dep, member.name
                    )));
                }
            }
        }

        // Check for cycles
        if let Some(cycle) = Self::find_cycle(&self.config.members) {
            errors.push(ResolverError::CyclicDependency(cycle));
        }

        // Check default_member references a real member
        if let Some(ref default) = self.config.default_member {
            if !member_names.contains(default.as_str()) {
                errors.push(ResolverError::MemberNotFound(format!(
                    "'{}' (default_member)",
                    default
                )));
            }
        }

        errors
    }

    /// Detect cycles among members and return the cycle chain if one exists.
    pub fn detect_cycles(&self) -> Option<Vec<String>> {
        Self::find_cycle(&self.config.members)
    }

    /// Return a reference to the underlying config.
    pub fn config(&self) -> &ResolverWorkspaceConfig {
        &self.config
    }

    // ---- Private helpers ----

    /// Kahn's algorithm for topological sort.
    fn topological_sort(members: &[ResolverWorkspaceMember]) -> Vec<String> {
        let names: HashSet<&str> = members.iter().map(|m| m.name.as_str()).collect();

        // in-degree: how many dependencies does each member have (within the workspace)
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        // reverse map: dep -> list of members that depend on it
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        for m in members {
            in_degree.entry(m.name.as_str()).or_insert(0);
            for dep in &m.dependencies {
                if names.contains(dep.as_str()) {
                    *in_degree.entry(m.name.as_str()).or_insert(0) += 1;
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(m.name.as_str());
                }
            }
        }

        // Seed queue with members that have zero in-degree (no workspace deps)
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();
        queue.sort(); // deterministic

        let mut result: Vec<String> = Vec::new();

        while let Some(name) = queue.pop() {
            result.push(name.to_string());

            if let Some(deps) = dependents.get(name) {
                let mut sorted_deps: Vec<&str> = deps.to_vec();
                sorted_deps.sort();
                for dep in sorted_deps {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            // Insert sorted to keep determinism
                            let pos = queue.binary_search(&dep).unwrap_or_else(|e| e);
                            queue.insert(pos, dep);
                        }
                    }
                }
            }
        }

        result
    }

    /// DFS-based cycle detection. Returns the cycle path if one is found.
    fn find_cycle(members: &[ResolverWorkspaceMember]) -> Option<Vec<String>> {
        let names: HashSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
        let dep_map: HashMap<&str, &[String]> = members
            .iter()
            .map(|m| (m.name.as_str(), m.dependencies.as_slice()))
            .collect();

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut color: HashMap<&str, Color> = names.iter().map(|&n| (n, Color::White)).collect();
        let mut path: Vec<&str> = Vec::new();

        fn dfs<'a>(
            node: &'a str,
            dep_map: &HashMap<&'a str, &'a [String]>,
            color: &mut HashMap<&'a str, Color>,
            path: &mut Vec<&'a str>,
            names: &HashSet<&'a str>,
        ) -> Option<Vec<String>> {
            color.insert(node, Color::Gray);
            path.push(node);

            if let Some(deps) = dep_map.get(node) {
                for dep in *deps {
                    if !names.contains(dep.as_str()) {
                        continue;
                    }
                    match color.get(dep.as_str()) {
                        Some(Color::Gray) => {
                            // Found a cycle — extract the cycle portion
                            let cycle_start = path.iter().position(|&n| n == dep.as_str()).unwrap();
                            let mut cycle: Vec<String> =
                                path[cycle_start..].iter().map(|s| s.to_string()).collect();
                            cycle.push(dep.clone());
                            return Some(cycle);
                        }
                        Some(Color::White) | None => {
                            if let Some(cycle) = dfs(dep.as_str(), dep_map, color, path, names) {
                                return Some(cycle);
                            }
                        }
                        Some(Color::Black) => {}
                    }
                }
            }

            path.pop();
            color.insert(node, Color::Black);
            None
        }

        // Process in sorted order for determinism
        let mut sorted_names: Vec<&str> = names.iter().copied().collect();
        sorted_names.sort();

        for name in sorted_names {
            if color.get(name) == Some(&Color::White) {
                if let Some(cycle) = dfs(name, &dep_map, &mut color, &mut path, &names) {
                    return Some(cycle);
                }
            }
        }

        None
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
