//! Dependency resolver for Lumen packages.
//!
//! A deterministic backtracking solver over registry, path and git sources:
//!
//! - Packages are decided breadth-first in name order; each takes the most
//!   preferred candidate (locked, then previous solution, then highest) that
//!   satisfies every requirement collected so far.
//! - A dead end undoes the latest decision and tries the next candidate, so
//!   diamond dependencies and conflicting ranges are handled correctly.
//! - Registry candidates carry their artifacts and dependencies (read from the
//!   version metadata); path and git candidates are read from their manifests,
//!   including their own `[dependencies]`.
//! - Prerelease and yanked handling follow [`ResolutionPolicy`].
//!
//! ## Philosophy
//!
//! **Determinism first. Reproducibility always. Conflicts are errors unless explicitly mediated.**
//!
//! 1. Single version per package per build context (no diamond version conflicts)
//! 2. Resolution is deterministic with strict tie-breaking
//! 3. Dependency constraints are minimal and monotonic
//! 4. Conflicts are solved by explicit mechanisms, not magical installer tricks

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// Final result of the resolution process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    /// The resolved dependency graph.
    pub packages: Vec<ResolvedPackage>,
    /// The auditable proof of the resolution.
    pub proof: ResolutionProof,
}

use crate::config::{DependencySpec, FeatureDef};
use crate::semver::{Constraint, Version};
use crate::wares::{RegistryClient, RegistryPackageIndex, RegistryVersionMetadata};

// =============================================================================
// Core Types - Public API
// =============================================================================

/// Unique identifier for a package (namespace/name format).
pub type PackageId = String;

/// Type alias for version constraints used in dependency declarations.
pub type VersionConstraint = Constraint;

/// A feature flag name.
pub type FeatureName = String;

/// Dependency kind for resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DependencyKind {
    /// Normal runtime dependency.
    #[default]
    Normal,
    /// Development dependency (tests, benchmarks).
    Dev,
    /// Build dependency (build scripts, codegen).
    Build,
}

/// Request for dependency resolution.
#[derive(Debug, Clone, Default)]
pub struct ResolutionRequest {
    /// Root dependencies with version constraints.
    pub root_deps: HashMap<PackageId, DependencySpec>,
    /// Dev dependencies (only resolved for root package).
    pub dev_deps: HashMap<PackageId, DependencySpec>,
    /// Build dependencies (resolved before building).
    pub build_deps: HashMap<PackageId, DependencySpec>,
    /// Registry URL to use for resolution.
    pub registry_url: String,
    /// Features to enable for root package.
    pub features: Vec<FeatureName>,
    /// Whether to include dev dependencies.
    pub include_dev: bool,
    /// Whether to include build dependencies.
    pub include_build: bool,
    /// Whether to include yanked versions in resolution.
    pub include_yanked: bool,
}

/// A resolved package with its exact version and dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPackage {
    /// Package name.
    pub name: PackageId,
    /// Resolved version.
    pub version: String,
    /// Dependencies (name, spec).
    pub deps: Vec<(PackageId, DependencySpec)>,
    /// Source of the package.
    pub source: ResolvedSource,
    /// Enabled features for this package.
    pub enabled_features: Vec<FeatureName>,
    /// Dependency kind (normal, dev, build).
    pub kind: DependencyKind,
}

impl fmt::Display for ResolvedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

/// Source of a resolved package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolvedSource {
    /// Registry package.
    Registry {
        url: String,
        cid: Option<String>,
        artifacts: Vec<crate::lockfile::LockedArtifact>,
    },
    /// Path dependency.
    Path { path: String },
    /// Git dependency.
    Git { url: String, rev: String },
}

impl ResolvedSource {
    /// Check if this is a path dependency.
    pub fn is_path(&self) -> bool {
        matches!(self, ResolvedSource::Path { .. })
    }

    /// Check if this is a registry dependency.
    pub fn is_registry(&self) -> bool {
        matches!(self, ResolvedSource::Registry { .. })
    }

    /// Check if this is a git dependency.
    pub fn is_git(&self) -> bool {
        matches!(self, ResolvedSource::Git { .. })
    }
}

// =============================================================================
// Resolution Policy
// =============================================================================

/// Policy for resolution behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionPolicy {
    /// Resolution mode: single-version or allow forks.
    pub mode: ResolutionMode,
    /// Prefer locked versions when they still satisfy constraints.
    pub prefer_locked: bool,
    /// Prefer highest compatible versions.
    pub prefer_highest: bool,
    /// Minimize changes from existing lock.
    pub minimize_changes: bool,
    /// Explicit fork rules for allowing multiple versions.
    pub fork_rules: Vec<ForkRule>,
    /// Include prerelease versions in resolution.
    pub include_prerelease: bool,
    /// Include yanked versions in resolution (default: false).
    pub include_yanked: bool,
}

impl Default for ResolutionPolicy {
    fn default() -> Self {
        Self {
            mode: ResolutionMode::SingleVersion,
            prefer_locked: true,
            prefer_highest: true,
            minimize_changes: true,
            fork_rules: Vec::new(),
            include_prerelease: false,
            include_yanked: false,
        }
    }
}

/// Resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionMode {
    /// Only one version per package allowed (strict).
    SingleVersion,
    /// Allow multiple versions via explicit fork rules.
    AllowForks,
}

/// Rule for allowing a package fork.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkRule {
    /// Package to fork.
    pub package: PackageId,
    /// Alias for the forked version.
    pub alias: PackageId,
    /// Reason for the fork (for documentation).
    pub reason: String,
}

// =============================================================================
// Errors and Conflicts
// =============================================================================

/// Resolution error types.
#[derive(Debug, Clone)]
pub enum ResolutionError {
    /// No solution exists for the given constraints.
    NoSolution { conflicts: Vec<Conflict> },
    /// Circular dependency detected.
    CircularDependency { chain: Vec<PackageId> },
    /// Version not found in registry.
    VersionNotFound {
        package: PackageId,
        constraint: String,
    },
    /// Registry error.
    RegistryError { message: String },
    /// Internal solver error.
    InternalError { message: String },
    /// Feature resolution error.
    FeatureError {
        package: PackageId,
        feature: FeatureName,
        reason: String,
    },
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSolution { conflicts } => {
                writeln!(f, "No solution found:")?;
                for conflict in conflicts {
                    writeln!(f, "  - {}: {}", conflict.package, conflict.describe())?;
                }
                Ok(())
            }
            Self::CircularDependency { chain } => {
                write!(f, "Circular dependency: {}", chain.join(" -> "))
            }
            Self::VersionNotFound {
                package,
                constraint,
            } => {
                write!(
                    f,
                    "No version found for '{}' satisfying '{}'",
                    package, constraint
                )
            }
            Self::RegistryError { message } => write!(f, "Registry error: {}", message),
            Self::InternalError { message } => write!(f, "Internal resolver error: {}", message),
            Self::FeatureError {
                package,
                feature,
                reason,
            } => {
                write!(
                    f,
                    "Feature error for '{}': feature '{}' {}",
                    package, feature, reason
                )
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

use crate::lockfile::{LockFile, ResolutionDecision, ResolutionProof};

/// Information about a resolution conflict.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The conflicting package.
    pub package: PackageId,
    /// All requirements on this package.
    pub required_by: Vec<(PackageId, String)>,
    /// Suggestions for resolving the conflict.
    pub suggestions: Vec<ConflictSuggestion>,
}

impl Conflict {
    fn describe(&self) -> String {
        let reqs: Vec<String> = self
            .required_by
            .iter()
            .map(|(pkg, range)| format!("{} requires {}", pkg, range))
            .collect();
        format!("incompatible requirements: {}", reqs.join("; "))
    }
}

/// Suggestion for resolving a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSuggestion {
    /// Update a package to a different version.
    Update {
        package: PackageId,
        to: String,
        why: String,
    },
    /// Fork a package to allow multiple versions.
    Fork {
        package: PackageId,
        alias: PackageId,
        why: String,
    },
    /// Remove a dependency.
    Remove { package: PackageId, why: String },
}

// =============================================================================
// Feature Resolution
// =============================================================================

/// Resolved features for a package.
#[derive(Debug, Clone, Default)]
pub struct FeatureResolution {
    /// Features enabled for this package.
    pub enabled: HashSet<FeatureName>,
    /// Features that are required but not available.
    pub missing: Vec<FeatureName>,
    /// Optional dependencies activated by features.
    pub activated_deps: Vec<(PackageId, DependencySpec)>,
}

/// Resolves feature flags for a package given the registry metadata.
pub fn resolve_features(
    package: &PackageId,
    requested: &[FeatureName],
    metadata: &RegistryVersionMetadata,
    available_features: &HashMap<FeatureName, FeatureDef>,
) -> Result<FeatureResolution, ResolutionError> {
    let mut resolution = FeatureResolution::default();
    let mut to_process: VecDeque<FeatureName> = requested.iter().cloned().collect();
    let mut processed = HashSet::new();

    // Process default features if no features explicitly requested
    if requested.is_empty() {
        if let Some(default_def) = available_features.get("default") {
            let default_features = match default_def {
                FeatureDef::Simple(features) => features.clone(),
                FeatureDef::Detailed { enables, .. } => enables.clone(),
            };
            for f in default_features {
                if !processed.contains(&f) {
                    to_process.push_back(f);
                }
            }
        }
    }

    // Resolve all features recursively
    while let Some(feature) = to_process.pop_front() {
        if !processed.insert(feature.clone()) {
            continue;
        }

        // Check if feature exists
        if let Some(def) = available_features.get(&feature) {
            resolution.enabled.insert(feature.clone());

            // Get features this one enables
            let enables = match def {
                FeatureDef::Simple(features) => features.clone(),
                FeatureDef::Detailed { enables, .. } => enables.clone(),
            };

            for f in enables {
                // Check if it's another feature or an optional dependency
                if available_features.contains_key(&f) {
                    if !processed.contains(&f) {
                        to_process.push_back(f);
                    }
                } else {
                    // Might be an optional dependency - will be handled separately
                    resolution
                        .activated_deps
                        .push((f.clone(), DependencySpec::Version("*".to_string())));
                }
            }
        } else {
            // Check if it's an optional dependency in the metadata
            let opt_dep = metadata.optional_deps.get(&feature);
            if opt_dep.is_some() {
                // It's an optional dependency that was requested as a feature
                resolution.enabled.insert(feature.clone());
            } else {
                resolution.missing.push(feature.clone());
            }
        }
    }

    // Check for missing features
    if !resolution.missing.is_empty() {
        return Err(ResolutionError::FeatureError {
            package: package.clone(),
            feature: resolution.missing[0].clone(),
            reason: "is not defined".to_string(),
        });
    }

    // Add optional dependencies activated by features
    for feature in &resolution.enabled {
        if let Some(deps) = metadata.optional_deps.get(feature) {
            for dep in deps {
                if let Some((name, spec)) = parse_dep_spec(dep) {
                    resolution.activated_deps.push((name, spec));
                }
            }
        }
    }

    Ok(resolution)
}

fn parse_dep_spec(dep: &str) -> Option<(PackageId, DependencySpec)> {
    // Parse "@scope/name@version" or "@scope/name"
    // The first '@' is the namespace prefix, so use rfind to find the version separator
    if dep.starts_with('@') {
        // Namespaced: @scope/name or @scope/name@version
        // Find the version '@' — it's any '@' after the initial scope
        if let Some(slash_idx) = dep.find('/') {
            let after_slash = &dep[slash_idx + 1..];
            if let Some(ver_offset) = after_slash.find('@') {
                let ver_idx = slash_idx + 1 + ver_offset;
                let name = dep[..ver_idx].to_string();
                let version = dep[ver_idx + 1..].to_string();
                Some((name, DependencySpec::Version(version)))
            } else {
                Some((dep.to_string(), DependencySpec::Version("*".to_string())))
            }
        } else {
            // Invalid: @ but no slash — not a valid namespaced name
            None
        }
    } else {
        // Non-namespaced name — still parse but will fail validation elsewhere
        if let Some(idx) = dep.find('@') {
            let name = dep[..idx].to_string();
            let version = dep[idx + 1..].to_string();
            Some((name, DependencySpec::Version(version)))
        } else {
            Some((dep.to_string(), DependencySpec::Version("*".to_string())))
        }
    }
}

// =============================================================================
// Backtracking Solver Types
// =============================================================================

/// One requirement placed on a package by a dependent.
#[derive(Debug, Clone)]
struct Requirement {
    /// The package that declared the dependency (or `(root)`).
    from: PackageId,
    spec: DependencySpec,
}

/// A concrete, fully-described choice for a package.
#[derive(Debug, Clone)]
struct Candidate {
    version: Version,
    version_str: String,
    source: ResolvedSource,
    /// The candidate's own normal dependencies (path deps made absolute).
    deps: Vec<(PackageId, DependencySpec)>,
    /// Registry metadata (for feature resolution), when the source is a registry.
    metadata: Option<RegistryVersionMetadata>,
}

/// A candidate that may still need its metadata fetched.
#[derive(Debug, Clone)]
enum Choice {
    /// Path/git: everything is already known.
    Ready(Box<Candidate>),
    /// Registry: only the version is known; metadata is fetched when tried.
    Registry(Version, String),
}

/// Immutable-by-clone search state; small enough that cloning per decision is fine.
#[derive(Debug, Clone, Default)]
struct SolveState {
    selected: BTreeMap<PackageId, Candidate>,
    /// Order in which packages were decided (for the proof).
    order: Vec<PackageId>,
    reqs: HashMap<PackageId, Vec<Requirement>>,
}

/// Mutable bookkeeping shared by the whole search.
#[derive(Default)]
struct SolveCtx {
    /// Remaining candidate evaluations before giving up.
    budget: usize,
    /// Number of dead ends hit (reported in the proof).
    backtracks: usize,
    /// Path/git candidates keyed by their spec, so manifests and clones are read once.
    sources: HashMap<String, Candidate>,
}

/// Placeholder requester name for the root manifest's own dependencies.
const ROOT: &str = "(root)";
/// Upper bound on candidate evaluations; guards against pathological graphs.
const MAX_STEPS: usize = 100_000;

fn spec_constraint(spec: &DependencySpec) -> Result<Option<Constraint>, ResolutionError> {
    let text = match spec {
        DependencySpec::Version(v) => v,
        DependencySpec::VersionDetailed { version, .. } => version,
        _ => return Ok(None),
    };
    Constraint::parse(text)
        .map(Some)
        .map_err(|e| ResolutionError::RegistryError {
            message: format!("Invalid version constraint '{}': {}", text, e),
        })
}

fn spec_features(spec: &DependencySpec) -> Option<&Vec<FeatureName>> {
    match spec {
        DependencySpec::VersionDetailed { features, .. }
        | DependencySpec::Git { features, .. }
        | DependencySpec::Workspace { features, .. } => features.as_ref(),
        _ => None,
    }
}

fn spec_display(spec: &DependencySpec) -> String {
    match spec {
        DependencySpec::Version(v) => v.clone(),
        DependencySpec::VersionDetailed { version, .. } => version.clone(),
        DependencySpec::Path { path } => format!("path:{}", path),
        DependencySpec::Git { git, .. } => format!("git:{}", git),
        DependencySpec::Workspace { .. } => "workspace".to_string(),
    }
}

fn make_absolute(base: &std::path::Path, path: &str) -> String {
    let p = std::path::Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    let canonical = std::fs::canonicalize(&joined).unwrap_or(joined);
    // Windows canonicalization yields `\\?\C:\...`; keep paths in their plain form.
    #[cfg(windows)]
    let canonical = canonical
        .to_str()
        .and_then(|s| s.strip_prefix(r"\\?\"))
        .map(std::path::PathBuf::from)
        .unwrap_or(canonical);
    canonical.to_string_lossy().to_string()
}

/// Whether two path spellings (relative, `\\?\` verbatim, symlinked) name the same directory.
fn same_path(a: &str, b: &str) -> bool {
    let cwd = std::env::current_dir().unwrap_or_default();
    make_absolute(&cwd, a) == make_absolute(&cwd, b)
}

/// Whether two pinned (path/git/workspace) requirements point at the same source.
fn same_source(a: &DependencySpec, b: &DependencySpec) -> bool {
    match (a, b) {
        (DependencySpec::Path { path: x }, DependencySpec::Path { path: y }) => same_path(x, y),
        _ => a == b,
    }
}

/// Read the normal dependencies of a path/git package, making nested path
/// dependencies absolute relative to that package's directory.
fn manifest_deps(
    dir: &std::path::Path,
    config: &crate::config::LumenConfig,
) -> Vec<(PackageId, DependencySpec)> {
    let mut deps: Vec<(PackageId, DependencySpec)> = config
        .dependencies
        .iter()
        .map(|(name, spec)| {
            let spec = match spec {
                DependencySpec::Path { path } => DependencySpec::Path {
                    path: make_absolute(dir, path),
                },
                other => other.clone(),
            };
            (name.clone(), spec)
        })
        .collect();
    deps.sort_by(|a, b| a.0.cmp(&b.0));
    deps
}

fn conflict_for(package: &str, reqs: &[Requirement]) -> Conflict {
    let mut conflict = Conflict {
        package: package.to_string(),
        required_by: reqs
            .iter()
            .map(|r| (r.from.clone(), spec_display(&r.spec)))
            .collect(),
        suggestions: Vec::new(),
    };
    conflict.suggestions = generate_suggestions(&conflict);
    conflict
}

/// Why a search branch stopped.
enum SearchError {
    /// A genuine conflict; the caller may backtrack and try another candidate.
    Conflict(Conflict),
    /// An error that no other candidate can fix (registry/IO/config problem).
    Fatal(ResolutionError),
}

fn generate_suggestions(conflict: &Conflict) -> Vec<ConflictSuggestion> {
    let mut suggestions = Vec::new();

    let constraints: Vec<_> = conflict
        .required_by
        .iter()
        .map(|(_, c)| c.as_str())
        .collect();

    if let Some(common) = find_common_version(&constraints) {
        suggestions.push(ConflictSuggestion::Update {
            package: conflict.package.clone(),
            to: common,
            why: "This version satisfies all constraints".to_string(),
        });
    }

    // Suggest relaxing the most restrictive constraint.
    if let Some((most_restrictive_pkg, most_restrictive)) =
        conflict.required_by.iter().min_by_key(|(_, c)| {
            if c.starts_with('=') {
                0
            } else if c.starts_with('^') {
                1
            } else if c.starts_with('~') {
                2
            } else {
                3
            }
        })
    {
        suggestions.push(ConflictSuggestion::Update {
            package: most_restrictive_pkg.clone(),
            to: "broader version range".to_string(),
            why: format!("The constraint '{}' is too restrictive", most_restrictive),
        });
    }

    if conflict.required_by.len() == 2 {
        suggestions.push(ConflictSuggestion::Fork {
            package: conflict.package.clone(),
            alias: conflict.package.to_string(),
            why: "Allow different versions for different parts of the dependency tree".to_string(),
        });
    }

    suggestions
}

// =============================================================================
// Registry Cache
// =============================================================================

/// Cache for registry indices to avoid repeated fetches.
#[derive(Debug, Clone, Default)]
pub struct RegistryCache {
    /// Cached package indices: package_name -> (index, timestamp)
    indices: HashMap<PackageId, (RegistryPackageIndex, std::time::Instant)>,
    /// Cached version metadata: (package_name, version) -> (metadata, timestamp)
    metadata: HashMap<(PackageId, String), (RegistryVersionMetadata, std::time::Instant)>,
    /// Cache TTL in seconds (default: 5 minutes)
    ttl_secs: u64,
}

impl RegistryCache {
    /// Create a new registry cache with default TTL (5 minutes).
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            metadata: HashMap::new(),
            ttl_secs: 300, // 5 minutes
        }
    }

    /// Create a new registry cache with custom TTL.
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            indices: HashMap::new(),
            metadata: HashMap::new(),
            ttl_secs,
        }
    }

    /// Get a cached package index if not expired.
    pub fn get_index(&self, package: &str) -> Option<&RegistryPackageIndex> {
        self.indices.get(package).and_then(|(index, time)| {
            if time.elapsed().as_secs() < self.ttl_secs {
                Some(index)
            } else {
                None
            }
        })
    }

    /// Cache a package index.
    pub fn put_index(&mut self, package: PackageId, index: RegistryPackageIndex) {
        self.indices
            .insert(package, (index, std::time::Instant::now()));
    }

    /// Get cached version metadata if not expired.
    pub fn get_metadata(&self, package: &str, version: &str) -> Option<&RegistryVersionMetadata> {
        self.metadata
            .get(&(package.to_string(), version.to_string()))
            .and_then(|(meta, time)| {
                if time.elapsed().as_secs() < self.ttl_secs {
                    Some(meta)
                } else {
                    None
                }
            })
    }

    /// Cache version metadata.
    pub fn put_metadata(
        &mut self,
        package: PackageId,
        version: String,
        metadata: RegistryVersionMetadata,
    ) {
        self.metadata
            .insert((package, version), (metadata, std::time::Instant::now()));
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.indices.clear();
        self.metadata.clear();
    }

    /// Get cache stats.
    pub fn stats(&self) -> (usize, usize) {
        (self.indices.len(), self.metadata.len())
    }
}

// =============================================================================
// Main Resolver
// =============================================================================

/// The SAT-based dependency resolver.
pub struct Resolver {
    registry: RegistryClient,
    locked: HashMap<PackageId, String>,
    policy: ResolutionPolicy,
    /// Previous solution for minimal change resolution
    previous_solution: Option<HashMap<PackageId, String>>,
    /// Git resolver for handling git dependencies
    git_cache_dir: PathBuf,
    /// Registry cache for indices and metadata
    cache: Arc<Mutex<RegistryCache>>,
    /// Cache directory for persisting registry data
    cache_dir: Option<PathBuf>,
}

impl Resolver {
    /// Create a new resolver with the given registry URL and optional lockfile.
    pub fn new(registry_url: impl Into<String>, lockfile: Option<&LockFile>) -> Self {
        let mut locked = HashMap::new();
        if let Some(lock) = lockfile {
            for pkg in &lock.packages {
                locked.insert(pkg.name.clone(), pkg.version.clone());
            }
        }

        let git_cache_dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("lumen")
            .join("git");

        let cache_dir = dirs::cache_dir().map(|d| d.join("lumen").join("registry-cache"));

        let cache = Arc::new(Mutex::new(RegistryCache::new()));

        // Try to load cache from disk
        if let Some(ref dir) = cache_dir {
            let _ = std::fs::create_dir_all(dir);
        }

        Self {
            registry: RegistryClient::new(registry_url),
            locked,
            policy: ResolutionPolicy::default(),
            previous_solution: None,
            git_cache_dir,
            cache,
            cache_dir,
        }
    }

    /// Create a resolver with custom policy.
    pub fn with_policy(
        registry_url: impl Into<String>,
        lockfile: Option<&LockFile>,
        policy: ResolutionPolicy,
    ) -> Self {
        let mut resolver = Self::new(registry_url, lockfile);
        resolver.policy = policy;
        resolver
    }

    /// Create a resolver for updating from an existing solution.
    pub fn for_update(
        registry_url: impl Into<String>,
        previous_lockfile: &LockFile,
        policy: ResolutionPolicy,
    ) -> Self {
        let mut previous_solution = HashMap::new();
        for pkg in &previous_lockfile.packages {
            previous_solution.insert(pkg.name.clone(), pkg.version.clone());
        }

        let mut resolver = Self::new(registry_url, Some(previous_lockfile));
        resolver.policy = policy;
        resolver.previous_solution = Some(previous_solution);
        resolver
    }

    /// Set a custom cache directory.
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = Some(dir);
        self
    }

    /// Get cache statistics.
    pub fn cache_stats(&self) -> (usize, usize) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).stats()
    }

    /// Clear the in-memory cache.
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Run the resolution algorithm.
    ///
    /// A deterministic backtracking search: packages are decided breadth-first
    /// in name order, each taking the most preferred candidate (locked, then
    /// previous, then highest) that satisfies every requirement seen so far.
    /// A dead end undoes the latest decision and tries the next candidate.
    pub fn resolve(
        &self,
        request: &ResolutionRequest,
    ) -> Result<ResolutionResult, ResolutionError> {
        let mut state = SolveState::default();
        let mut pending: VecDeque<PackageId> = VecDeque::new();

        let add_root = |deps: &HashMap<PackageId, DependencySpec>,
                        state: &mut SolveState,
                        pending: &mut VecDeque<PackageId>| {
            let mut names: Vec<&PackageId> = deps.keys().collect();
            names.sort();
            for name in names {
                state
                    .reqs
                    .entry(name.clone())
                    .or_default()
                    .push(Requirement {
                        from: ROOT.to_string(),
                        spec: deps[name].clone(),
                    });
                if !pending.contains(name) {
                    pending.push_back(name.clone());
                }
            }
        };
        add_root(&request.root_deps, &mut state, &mut pending);
        if request.include_dev {
            add_root(&request.dev_deps, &mut state, &mut pending);
        }
        if request.include_build {
            add_root(&request.build_deps, &mut state, &mut pending);
        }

        let mut ctx = SolveCtx {
            budget: MAX_STEPS,
            ..SolveCtx::default()
        };
        let solved = self
            .search(state, pending, request, &mut ctx)
            .map_err(|e| match e {
                SearchError::Fatal(err) => err,
                SearchError::Conflict(conflict) => ResolutionError::NoSolution {
                    conflicts: vec![conflict],
                },
            })?;

        Ok(self.build_result(solved, request, ctx.backtracks))
    }

    /// Resolve dependencies with feature flags enabled.
    pub fn resolve_with_features(
        &self,
        request: &ResolutionRequest,
        features: &[FeatureName],
    ) -> Result<ResolutionResult, ResolutionError> {
        let mut modified_request = request.clone();
        modified_request.features = features.to_vec();
        self.resolve(&modified_request)
    }

    /// Resolve from a lockfile, respecting exact versions where possible.
    /// When dependencies or constraints have changed, will re-resolve.
    pub fn resolve_from_lock(
        &self,
        request: &ResolutionRequest,
        lockfile: &LockFile,
    ) -> Result<ResolutionResult, ResolutionError> {
        let mut needs_re_resolve = false;

        for (name, spec) in &request.root_deps {
            match spec {
                DependencySpec::Version(constraint)
                | DependencySpec::VersionDetailed {
                    version: constraint,
                    ..
                } => {
                    if let Some(locked_pkg) = lockfile.get_package(name) {
                        if let Ok(constraint) = Constraint::parse(constraint) {
                            if let Ok(version) = Version::from_str(&locked_pkg.version) {
                                if !constraint.matches(&version) {
                                    needs_re_resolve = true;
                                    break;
                                }
                            }
                        }
                    } else {
                        needs_re_resolve = true;
                        break;
                    }
                }
                _ => {
                    needs_re_resolve = true;
                    break;
                }
            }
        }

        if !needs_re_resolve {
            let mut packages = Vec::new();
            for locked in &lockfile.packages {
                let source = if locked.is_path_dependency() {
                    ResolvedSource::Path {
                        path: locked.get_path().unwrap_or(".").to_string(),
                    }
                } else if locked.is_git_dependency() {
                    if let Some((url, rev)) = locked.parse_git_source() {
                        ResolvedSource::Git { url, rev }
                    } else {
                        continue;
                    }
                } else {
                    let artifacts = locked.artifacts.clone();
                    ResolvedSource::Registry {
                        url: locked.get_registry_url().unwrap_or("").to_string(),
                        cid: locked.get_cid().map(|s| s.to_string()),
                        artifacts,
                    }
                };

                let kind = locked
                    .kind
                    .as_deref()
                    .map(|k| match k {
                        "dev" => DependencyKind::Dev,
                        "build" => DependencyKind::Build,
                        _ => DependencyKind::Normal,
                    })
                    .unwrap_or(DependencyKind::Normal);

                packages.push(ResolvedPackage {
                    name: locked.name.clone(),
                    version: locked.version.clone(),
                    deps: Vec::new(),
                    source,
                    enabled_features: locked.features.clone(),
                    kind,
                });
            }

            return Ok(ResolutionResult {
                packages,
                proof: ResolutionProof {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        .to_string(),
                    resolver_type: "SAT-v1".to_string(),
                    explanation: "Resolved using existing lockfile.".to_string(),
                    decisions: vec![],
                    conflicts_solved: 0,
                },
            });
        }

        self.resolve(request)
    }

    /// Update dependencies with minimal changes from existing solution.
    pub fn update(
        &self,
        request: &ResolutionRequest,
        previous_lockfile: &LockFile,
        packages_to_update: Option<&[PackageId]>,
    ) -> Result<ResolutionResult, ResolutionError> {
        let mut policy = self.policy.clone();

        if let Some(to_update) = packages_to_update {
            // Keep every package that is not being updated pinned to its locked version.
            let mut modified_resolver =
                Self::new(self.registry.base_url(), Some(previous_lockfile));
            modified_resolver
                .locked
                .retain(|name, _| !to_update.contains(name));
            modified_resolver.policy = policy;
            modified_resolver.previous_solution = self.previous_solution.clone();
            modified_resolver.git_cache_dir = self.git_cache_dir.clone();
            return modified_resolver.resolve(request);
        }

        // Update everything: take the highest compatible versions, ignoring the lock.
        policy.prefer_locked = false;
        policy.minimize_changes = false;
        policy.prefer_highest = true;

        let mut resolver = Self::for_update(self.registry.base_url(), previous_lockfile, policy);
        resolver.git_cache_dir = self.git_cache_dir.clone();
        resolver.resolve(request)
    }

    /// Fetch version metadata with caching.
    fn fetch_version_metadata_with_cache(
        &self,
        pkg_name: &str,
        version: &str,
    ) -> Result<RegistryVersionMetadata, String> {
        // Check cache first
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(metadata) = cache.get_metadata(pkg_name, version) {
                return Ok(metadata.clone());
            }
        }

        // Fetch from registry
        let metadata = self.registry.fetch_version_metadata(pkg_name, version)?;

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.put_metadata(pkg_name.to_string(), version.to_string(), metadata.clone());
        }

        Ok(metadata)
    }

    /// Candidates for `pkg`, most preferred first, honouring every requirement.
    fn candidates(
        &self,
        pkg: &str,
        reqs: &[Requirement],
        request: &ResolutionRequest,
        ctx: &mut SolveCtx,
    ) -> Result<Vec<Choice>, SearchError> {
        // Path/git/workspace requirements pin the source; they must all agree.
        let pinned: Vec<&DependencySpec> = reqs
            .iter()
            .map(|r| &r.spec)
            .filter(|s| {
                matches!(
                    s,
                    DependencySpec::Path { .. }
                        | DependencySpec::Git { .. }
                        | DependencySpec::Workspace { .. }
                )
            })
            .collect();

        if let Some(first) = pinned.first() {
            if pinned.iter().any(|s| !same_source(s, first)) {
                return Err(SearchError::Conflict(conflict_for(pkg, reqs)));
            }
            let candidate = self.source_candidate(pkg, first, ctx)?;
            for r in reqs {
                if !Self::candidate_satisfies(&candidate, &r.spec, self.include_prerelease(request))
                {
                    return Err(SearchError::Conflict(conflict_for(pkg, reqs)));
                }
            }
            return Ok(vec![Choice::Ready(Box::new(candidate))]);
        }

        let mut constraints = Vec::new();
        for r in reqs {
            if let Some(c) = spec_constraint(&r.spec).map_err(SearchError::Fatal)? {
                constraints.push(c);
            }
        }

        let index = self.package_index(pkg).map_err(SearchError::Fatal)?;
        let include_pre = self.include_prerelease(request);
        let include_yanked = self.policy.include_yanked || request.include_yanked;
        let locked = self.locked.get(pkg);

        let mut compatible: Vec<(Version, String)> = Vec::new();
        for v_str in &index.versions {
            // A yanked version stays usable only when the lockfile pins it.
            if index.yanked.contains_key(v_str) && !include_yanked && locked != Some(v_str) {
                continue;
            }
            let Ok(v) = Version::from_str(v_str) else {
                continue;
            };
            if constraints.iter().all(|c| c.matches_pre(&v, include_pre)) {
                compatible.push((v, v_str.clone()));
            }
        }

        let rank = |s: &str| -> u8 {
            if self.policy.prefer_locked && locked.map(|l| l == s).unwrap_or(false) {
                2
            } else if self.policy.minimize_changes
                && self
                    .previous_solution
                    .as_ref()
                    .and_then(|sol| sol.get(pkg))
                    .map(|p| p == s)
                    .unwrap_or(false)
            {
                1
            } else {
                0
            }
        };
        compatible.sort_by(|a, b| {
            rank(&b.1).cmp(&rank(&a.1)).then_with(|| {
                if self.policy.prefer_highest {
                    b.0.cmp(&a.0)
                } else {
                    a.0.cmp(&b.0)
                }
            })
        });

        Ok(compatible
            .into_iter()
            .map(|(v, s)| Choice::Registry(v, s))
            .collect())
    }

    fn include_prerelease(&self, _request: &ResolutionRequest) -> bool {
        self.policy.include_prerelease
    }

    /// Does an already-built candidate satisfy a new requirement?
    fn candidate_satisfies(
        candidate: &Candidate,
        spec: &DependencySpec,
        include_pre: bool,
    ) -> bool {
        match spec {
            DependencySpec::Version(_) | DependencySpec::VersionDetailed { .. } => {
                match spec_constraint(spec) {
                    Ok(Some(c)) => {
                        // Path/git packages carry whatever version their manifest declares.
                        c.matches_pre(
                            &candidate.version,
                            include_pre || !candidate.source.is_registry(),
                        )
                    }
                    _ => false,
                }
            }
            DependencySpec::Path { path } => {
                matches!(&candidate.source, ResolvedSource::Path { path: p } if same_path(p, path))
            }
            DependencySpec::Git { git, .. } => {
                matches!(&candidate.source, ResolvedSource::Git { url, .. } if url == git)
            }
            DependencySpec::Workspace { .. } => false,
        }
    }

    /// Build a candidate for a path or git requirement by reading its manifest.
    fn source_candidate(
        &self,
        pkg: &str,
        spec: &DependencySpec,
        ctx: &mut SolveCtx,
    ) -> Result<Candidate, SearchError> {
        let key = format!("{}|{:?}", pkg, spec);
        if let Some(c) = ctx.sources.get(&key) {
            return Ok(c.clone());
        }

        let (dir, source) = match spec {
            DependencySpec::Path { path } => {
                let abs = make_absolute(&std::env::current_dir().unwrap_or_default(), path);
                let dir = PathBuf::from(&abs);
                if !dir.exists() {
                    return Err(SearchError::Fatal(ResolutionError::RegistryError {
                        message: format!(
                            "path dependency '{}' does not exist at {}",
                            pkg,
                            dir.display()
                        ),
                    }));
                }
                (dir, ResolvedSource::Path { path: abs })
            }
            DependencySpec::Git { .. } => {
                let (url, git_ref) = crate::git::dep_spec_to_git_ref(spec).ok_or_else(|| {
                    SearchError::Fatal(ResolutionError::InternalError {
                        message: format!("invalid git dependency for '{}'", pkg),
                    })
                })?;
                let git_err = |e: crate::git::GitError| {
                    SearchError::Fatal(ResolutionError::RegistryError {
                        message: format!("git dependency '{}': {}", pkg, e),
                    })
                };
                let repo = crate::git::fetch_git_repo(&url, &git_ref, &self.git_cache_dir)
                    .map_err(git_err)?;
                let rev = crate::git::resolve_git_ref(&repo, &git_ref).map_err(git_err)?;
                let dir = self
                    .git_cache_dir
                    .join("resolved")
                    .join(
                        url.chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                            .collect::<String>(),
                    )
                    .join(&rev);
                if !dir.join("lumen.toml").exists() {
                    crate::git::checkout_git_commit(&repo, &rev, &dir).map_err(git_err)?;
                }
                (dir, ResolvedSource::Git { url, rev })
            }
            _ => {
                return Err(SearchError::Fatal(ResolutionError::RegistryError {
                    message: format!(
                        "dependency '{}' uses `workspace = true`, which must be resolved by the workspace before installing",
                        pkg
                    ),
                }))
            }
        };

        let manifest = dir.join("lumen.toml");
        let config = crate::config::LumenConfig::load_from(&manifest).map_err(|e| {
            SearchError::Fatal(ResolutionError::RegistryError {
                message: format!("failed to load lumen.toml for '{}': {}", pkg, e),
            })
        })?;
        let version_str = config
            .package
            .as_ref()
            .and_then(|p| p.version.clone())
            .unwrap_or_else(|| "0.1.0".to_string());
        let version = Version::from_str(&version_str).unwrap_or_else(|_| Version::new(0, 1, 0));
        let candidate = Candidate {
            version,
            version_str,
            source,
            deps: manifest_deps(&dir, &config),
            metadata: None,
        };
        ctx.sources.insert(key, candidate.clone());
        Ok(candidate)
    }

    /// Build a registry candidate: fetch metadata, dependencies and artifacts.
    fn registry_candidate(
        &self,
        pkg: &str,
        version: Version,
        version_str: String,
    ) -> Result<Candidate, ResolutionError> {
        let metadata = self
            .fetch_version_metadata_with_cache(pkg, &version_str)
            .map_err(|e| ResolutionError::RegistryError {
                message: format!(
                    "Failed to fetch metadata for {}@{}: {}",
                    pkg, version_str, e
                ),
            })?;

        let mut deps: Vec<(PackageId, DependencySpec)> = metadata
            .deps
            .iter()
            .map(|(name, constraint)| (name.clone(), DependencySpec::Version(constraint.clone())))
            .collect();
        deps.sort_by(|a, b| a.0.cmp(&b.0));

        let base = self.registry.base_url().to_string();
        let artifacts = metadata
            .artifacts
            .iter()
            .map(|a| crate::lockfile::LockedArtifact {
                kind: a.kind.clone(),
                url: a
                    .url
                    .clone()
                    .unwrap_or_else(|| crate::wares::client::artifact_path_for_hash(&a.hash)),
                hash: a.hash.clone(),
                size: a.size,
                arch: a.arch.clone(),
                platform: a.os.clone(),
            })
            .collect();

        Ok(Candidate {
            version,
            version_str,
            source: ResolvedSource::Registry {
                url: base,
                cid: None,
                artifacts,
            },
            deps,
            metadata: Some(metadata),
        })
    }

    fn package_index(&self, pkg: &str) -> Result<RegistryPackageIndex, ResolutionError> {
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(idx) = cache.get_index(pkg) {
                return Ok(idx.clone());
            }
        }
        let idx =
            self.registry
                .fetch_package_index(pkg)
                .map_err(|e| ResolutionError::RegistryError {
                    message: format!("Failed to fetch package index for '{}': {}", pkg, e),
                })?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.put_index(pkg.to_string(), idx.clone());
        Ok(idx)
    }

    /// Depth-first search with backtracking over per-package candidate lists.
    fn search(
        &self,
        state: SolveState,
        mut pending: VecDeque<PackageId>,
        request: &ResolutionRequest,
        ctx: &mut SolveCtx,
    ) -> Result<SolveState, SearchError> {
        // Next package that still needs a decision.
        let pkg = loop {
            match pending.pop_front() {
                None => return Ok(state),
                Some(p) if state.selected.contains_key(&p) => continue,
                Some(p) => break p,
            }
        };

        let reqs = state.reqs.get(&pkg).cloned().unwrap_or_default();
        let choices = self.candidates(&pkg, &reqs, request, ctx)?;
        if choices.is_empty() {
            ctx.backtracks += 1;
            // A single requirement with nothing matching is "not found", not a conflict.
            if let [only] = reqs.as_slice() {
                if only.from == ROOT {
                    return Err(SearchError::Fatal(ResolutionError::VersionNotFound {
                        package: pkg,
                        constraint: spec_display(&only.spec),
                    }));
                }
            }
            return Err(SearchError::Conflict(conflict_for(&pkg, &reqs)));
        }

        let mut last_conflict: Option<Conflict> = None;
        let mut last_registry_error: Option<ResolutionError> = None;
        for choice in choices {
            if ctx.budget == 0 {
                return Err(SearchError::Fatal(ResolutionError::InternalError {
                    message: format!(
                        "dependency resolution gave up after {} steps; the constraints are too complex",
                        MAX_STEPS
                    ),
                }));
            }
            ctx.budget -= 1;

            let candidate = match choice {
                Choice::Ready(c) => *c,
                Choice::Registry(v, s) => match self.registry_candidate(&pkg, v, s) {
                    Ok(c) => c,
                    Err(e) => {
                        // This version is unusable (metadata missing/corrupt); try another.
                        last_registry_error = Some(e);
                        continue;
                    }
                },
            };

            let mut next = state.clone();
            let mut next_pending = pending.clone();
            let mut ok = true;
            for (dep_name, dep_spec) in &candidate.deps {
                next.reqs
                    .entry(dep_name.clone())
                    .or_default()
                    .push(Requirement {
                        from: pkg.clone(),
                        spec: dep_spec.clone(),
                    });
                if let Some(chosen) = next.selected.get(dep_name) {
                    if !Self::candidate_satisfies(
                        chosen,
                        dep_spec,
                        self.include_prerelease(request),
                    ) {
                        let all = next.reqs.get(dep_name).cloned().unwrap_or_default();
                        last_conflict = Some(conflict_for(dep_name, &all));
                        ok = false;
                        break;
                    }
                } else if !next_pending.contains(dep_name) {
                    next_pending.push_back(dep_name.clone());
                }
            }
            if !ok {
                ctx.backtracks += 1;
                continue;
            }

            next.order.push(pkg.clone());
            next.selected.insert(pkg.clone(), candidate);

            match self.search(next, next_pending, request, ctx) {
                Ok(done) => return Ok(done),
                Err(SearchError::Fatal(e)) => return Err(SearchError::Fatal(e)),
                Err(SearchError::Conflict(c)) => {
                    ctx.backtracks += 1;
                    last_conflict = Some(c);
                }
            }
        }

        if let Some(c) = last_conflict {
            return Err(SearchError::Conflict(c));
        }
        if let Some(e) = last_registry_error {
            return Err(SearchError::Fatal(e));
        }
        Err(SearchError::Conflict(conflict_for(&pkg, &reqs)))
    }

    /// Turn a solved state into the public result (features, kinds, proof).
    fn build_result(
        &self,
        solved: SolveState,
        request: &ResolutionRequest,
        backtracks: usize,
    ) -> ResolutionResult {
        // Features requested for each package by whoever depends on it.
        let mut wanted: HashMap<PackageId, Vec<FeatureName>> = HashMap::new();
        for (name, reqs) in &solved.reqs {
            for r in reqs {
                if let Some(feats) = spec_features(&r.spec) {
                    let entry = wanted.entry(name.clone()).or_default();
                    for f in feats {
                        if !entry.contains(f) {
                            entry.push(f.clone());
                        }
                    }
                }
            }
        }

        let mut packages = Vec::new();
        for (name, cand) in &solved.selected {
            let enabled_features = match wanted.get(name) {
                Some(features) => match &cand.metadata {
                    Some(metadata) => {
                        let available = HashMap::new();
                        resolve_features(name, features, metadata, &available)
                            .map(|r| {
                                let mut v: Vec<_> = r.enabled.into_iter().collect();
                                v.sort();
                                v
                            })
                            .unwrap_or_default()
                    }
                    None => features.clone(),
                },
                None => Vec::new(),
            };

            let kind = if request.dev_deps.contains_key(name) {
                DependencyKind::Dev
            } else if request.build_deps.contains_key(name) {
                DependencyKind::Build
            } else {
                DependencyKind::Normal
            };

            packages.push(ResolvedPackage {
                name: name.clone(),
                version: cand.version_str.clone(),
                deps: cand.deps.clone(),
                source: cand.source.clone(),
                enabled_features,
                kind,
            });
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));

        let decisions: Vec<ResolutionDecision> = solved
            .order
            .iter()
            .enumerate()
            .filter_map(|(level, name)| {
                let cand = solved.selected.get(name)?;
                let n = solved.reqs.get(name).map(|r| r.len()).unwrap_or(0);
                Some(ResolutionDecision {
                    package: name.clone(),
                    version: cand.version_str.clone(),
                    reason: format!(
                        "Most preferred version satisfying {} requirement{}",
                        n,
                        if n == 1 { "" } else { "s" }
                    ),
                    level: level as u32,
                })
            })
            .collect();

        let proof = ResolutionProof {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
                .to_string(),
            resolver_type: "backtracking-v1".to_string(),
            explanation: format!(
                "Deterministic backtracking resolution with {} decisions and {} backtracks.",
                decisions.len(),
                backtracks
            ),
            decisions,
            conflicts_solved: backtracks,
        };

        ResolutionResult { packages, proof }
    }

    /// Get the git cache directory.
    pub fn git_cache_dir(&self) -> &std::path::PathBuf {
        &self.git_cache_dir
    }
}

/// Find a version that satisfies all constraints (best effort).
fn find_common_version(constraints: &[&str]) -> Option<String> {
    // This is a simplified heuristic - in practice would use the semver module
    // to find actual common versions from the registry

    // Look for common major version
    let mut majors: Vec<u64> = Vec::new();
    for c in constraints {
        if let Some(major_str) = c
            .trim_start_matches('^')
            .trim_start_matches('~')
            .split('.')
            .next()
        {
            if let Ok(major) = major_str.parse::<u64>() {
                majors.push(major);
            }
        }
    }

    if majors.iter().all(|&m| m == majors[0]) && !majors.is_empty() {
        // All same major version - suggest latest of that major
        return Some(format!("^{}.0.0", majors[0]));
    }

    None
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_creation() {
        let resolver = Resolver::new("https://example.com/registry", None);
        assert!(resolver.locked.is_empty());
    }

    #[test]
    fn test_policy_default() {
        let policy = ResolutionPolicy::default();
        assert_eq!(policy.mode, ResolutionMode::SingleVersion);
        assert!(policy.prefer_locked);
        assert!(policy.prefer_highest);
        assert!(policy.minimize_changes);
        assert!(!policy.include_prerelease);
        assert!(policy.fork_rules.is_empty());
    }

    #[test]
    fn test_resolution_request() {
        let mut deps = HashMap::new();
        deps.insert(
            "test".to_string(),
            DependencySpec::Version("^1.0.0".to_string()),
        );

        let request = ResolutionRequest {
            root_deps: deps,
            registry_url: "https://example.com/registry".to_string(),
            features: vec!["default".to_string()],
            include_dev: false,
            include_build: false,
            include_yanked: false,
            dev_deps: HashMap::new(),
            build_deps: HashMap::new(),
        };

        assert_eq!(request.root_deps.len(), 1);
        assert_eq!(request.features.len(), 1);
    }

    #[test]
    fn test_conflict_describe() {
        let conflict = Conflict {
            package: "test-pkg".to_string(),
            required_by: vec![
                ("pkg-a".to_string(), "^1.0.0".to_string()),
                ("pkg-b".to_string(), "^2.0.0".to_string()),
            ],
            suggestions: vec![],
        };

        let desc = conflict.describe();
        assert!(desc.contains("pkg-a"));
        assert!(desc.contains("pkg-b"));
        assert!(desc.contains("^1.0.0"));
        assert!(desc.contains("^2.0.0"));
    }

    #[test]
    fn test_resolved_source_helpers() {
        let path = ResolvedSource::Path {
            path: "../foo".to_string(),
        };
        assert!(path.is_path());
        assert!(!path.is_registry());
        assert!(!path.is_git());

        let reg = ResolvedSource::Registry {
            url: "https://example.com".to_string(),
            cid: None,
            artifacts: vec![],
        };
        assert!(!reg.is_path());
        assert!(reg.is_registry());
        assert!(!reg.is_git());

        let git = ResolvedSource::Git {
            url: "https://github.com/foo/bar".to_string(),
            rev: "abc123".to_string(),
        };
        assert!(!git.is_path());
        assert!(!git.is_registry());
        assert!(git.is_git());
    }
}

// =============================================================================
// Enhanced Conflict Reporting
// =============================================================================

impl Conflict {
    /// Create a detailed human-readable report for this conflict.
    pub fn to_report(&self) -> ConflictReport {
        ConflictReport {
            package: self.package.clone(),
            requirements: self.required_by.clone(),
            suggestions: self.suggestions.iter().map(|s| s.to_actionable()).collect(),
            severity: self.severity(),
        }
    }

    /// Determine the severity of this conflict.
    fn severity(&self) -> ConflictSeverity {
        if self.required_by.len() > 5 {
            ConflictSeverity::Critical
        } else if self.required_by.len() > 2 {
            ConflictSeverity::High
        } else {
            ConflictSeverity::Medium
        }
    }
}

/// Severity of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// A detailed conflict report.
#[derive(Debug, Clone)]
pub struct ConflictReport {
    pub package: PackageId,
    pub requirements: Vec<(PackageId, String)>,
    pub suggestions: Vec<ActionableSuggestion>,
    pub severity: ConflictSeverity,
}

impl fmt::Display for ConflictReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity_str = match self.severity {
            ConflictSeverity::Low => "LOW",
            ConflictSeverity::Medium => "MEDIUM",
            ConflictSeverity::High => "HIGH",
            ConflictSeverity::Critical => "CRITICAL",
        };

        writeln!(f, "\n━━━ Dependency Conflict [{severity_str}] ━━━")?;
        writeln!(f, "Package: {}", self.package)?;
        writeln!(f, "\nRequired by:")?;
        for (pkg, constraint) in &self.requirements {
            writeln!(f, "  • {pkg} requires {constraint}")?;
        }

        if !self.suggestions.is_empty() {
            writeln!(f, "\n💡 Suggestions to resolve:")?;
            for (i, suggestion) in self.suggestions.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, suggestion)?;
            }
        }

        Ok(())
    }
}

/// An actionable suggestion for resolving a conflict.
#[derive(Debug, Clone)]
pub enum ActionableSuggestion {
    /// Use a specific common version.
    UseCommonVersion {
        package: PackageId,
        version: String,
        reason: String,
    },
    /// Relax a constraint.
    RelaxConstraint {
        package: PackageId,
        current: String,
        suggested: String,
        reason: String,
    },
    /// Fork the package.
    ForkPackage { package: PackageId, reason: String },
    /// Remove a dependency.
    RemoveDependency { package: PackageId, reason: String },
}

impl fmt::Display for ActionableSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UseCommonVersion {
                package,
                version,
                reason,
            } => {
                write!(f, "Use {package}@{version} ({reason})")
            }
            Self::RelaxConstraint {
                package,
                current,
                suggested,
                reason,
            } => {
                write!(f, "Relax {package} from {current} - {suggested} ({reason})")
            }
            Self::ForkPackage { package, reason } => {
                write!(
                    f,
                    "Fork {package} - add [fork-rules] to lumen.toml ({reason})"
                )
            }
            Self::RemoveDependency { package, reason } => {
                write!(f, "Remove {package} from dependencies ({reason})")
            }
        }
    }
}

impl ConflictSuggestion {
    /// Convert to actionable suggestion.
    fn to_actionable(&self) -> ActionableSuggestion {
        match self {
            ConflictSuggestion::Update { package, to, why } => {
                ActionableSuggestion::UseCommonVersion {
                    package: package.clone(),
                    version: to.clone(),
                    reason: why.clone(),
                }
            }
            ConflictSuggestion::Fork {
                package,
                alias: _,
                why,
            } => ActionableSuggestion::ForkPackage {
                package: package.clone(),
                reason: why.clone(),
            },
            ConflictSuggestion::Remove { package, why } => ActionableSuggestion::RemoveDependency {
                package: package.clone(),
                reason: why.clone(),
            },
        }
    }

    /// Format this suggestion as a human-readable string.
    pub fn format(&self) -> String {
        match self {
            Self::Update { package, to, why } => {
                format!("Update {package} to {to} ({why})")
            }
            Self::Fork {
                package,
                alias,
                why,
            } => {
                format!("Fork {package} as {alias} ({why})")
            }
            Self::Remove { package, why } => {
                format!("Remove {package} ({why})")
            }
        }
    }
}

/// Format a resolution error for display.
pub fn format_resolution_error(error: &ResolutionError) -> String {
    let mut output = String::new();

    match error {
        ResolutionError::NoSolution { conflicts } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║            Dependency Resolution Failed                   ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");

            if conflicts.is_empty() {
                output
                    .push_str("The dependency graph contains conflicts that cannot be resolved.\n");
                output
                    .push_str("No specific conflicts were identified. This may indicate a bug.\n");
            } else {
                output.push_str(&format!("Found {} conflict(s):\n", conflicts.len()));

                for conflict in conflicts {
                    let report = conflict.to_report();
                    output.push_str(&format!("{}", report));
                }
            }

            output.push_str("\n━━━ General Suggestions ━━━\n");
            output.push_str("1. Try running `lumen pkg update` to get latest versions\n");
            output.push_str("2. Check if any dependencies have been yanked from the registry\n");
            output.push_str("3. Review your lumen.toml for conflicting version constraints\n");
            output.push_str("4. Consider using a lockfile to pin specific versions\n");
        }
        ResolutionError::CircularDependency { chain } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║              Circular Dependency Detected                ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");

            output.push_str(&format!("Dependency chain: {}\n\n", chain.join(" -> ")));

            if let Some((first, rest)) = chain.split_first() {
                if rest.contains(first) {
                    output.push_str(&format!(
                        "Package '{first}' transitively depends on itself.\n"
                    ));
                    output.push_str("This is usually caused by:\n");
                    output.push_str("  • A package accidentally depending on itself\n");
                    output.push_str("  • Two packages depending on each other (use dev-dependencies for tests)\n");
                    output.push_str("  • A path dependency cycle in a workspace\n");
                }
            }
        }
        ResolutionError::VersionNotFound {
            package,
            constraint,
        } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║                Version Not Found                          ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");

            output.push_str(&format!("Package: {package}\n"));
            output.push_str(&format!("Constraint: {constraint}\n\n"));

            output.push_str("Possible causes:\n");
            output.push_str("  • The version doesn't exist in the registry\n");
            output.push_str("  • The package name is misspelled\n");
            output.push_str("  • The version was yanked due to security issues\n");
            output.push_str("  • The registry is unreachable\n\n");

            output.push_str("Try:\n");
            output.push_str(&format!("  lumen pkg search {package}\n"));
            output.push_str("  (to see available versions)\n");
        }
        ResolutionError::RegistryError { message } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║                  Registry Error                           ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");
            output.push_str(&format!("Error: {message}\n\n"));
            output.push_str("This could be caused by:\n");
            output.push_str("  • Network connectivity issues\n");
            output.push_str("  • Registry server problems\n");
            output.push_str("  • Invalid registry configuration\n");
        }
        ResolutionError::InternalError { message } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║                Internal Solver Error                      ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");
            output.push_str(&format!("Error: {message}\n\n"));
            output.push_str("This is a bug in the resolver. Please report it at:\n");
            output.push_str("https://github.com/lumen-lang/lumen/issues\n");
        }
        ResolutionError::FeatureError {
            package,
            feature,
            reason,
        } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║                Feature Resolution Error                   ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");
            output.push_str(&format!("Package: {package}\n"));
            output.push_str(&format!("Feature: {feature}\n"));
            output.push_str(&format!("Reason: {reason}\n"));
        }
    }

    output
}
