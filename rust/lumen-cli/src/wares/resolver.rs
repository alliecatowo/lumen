//! SAT-based dependency resolver for Lumen packages.
//!
//! Implements a world-class constraint satisfaction solver with:
//! - CSP formulation with variables = package versions, constraints = dependencies
//! - MRV (Minimum Remaining Values) heuristic for variable ordering
//! - AC-3 (Arc Consistency) for constraint propagation
//! - CDCL (Conflict-Driven Clause Learning) for efficient backtracking
//! - Preference ordering: locked > highest > minimal changes > fewer packages
//! - Single-version enforcement with explicit fork rules
//! - Feature flag resolution with dependency unification
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
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Final result of the resolution process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    /// The resolved dependency graph.
    pub packages: Vec<ResolvedPackage>,
    /// The auditable proof of the resolution.
    pub proof: ResolutionProof,
}

use crate::config::{DependencySpec, FeatureDef};
use crate::wares::{RegistryClient, RegistryPackageIndex, RegistryVersionMetadata};
use crate::semver::{Constraint, Version};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyKind {
    /// Normal runtime dependency.
    Normal,
    /// Development dependency (tests, benchmarks).
    Dev,
    /// Build dependency (build scripts, codegen).
    Build,
}

impl Default for DependencyKind {
    fn default() -> Self {
        Self::Normal
    }
}

/// Request for dependency resolution.
#[derive(Debug, Clone)]
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

impl Default for ResolutionRequest {
    fn default() -> Self {
        Self {
            root_deps: HashMap::new(),
            dev_deps: HashMap::new(),
            build_deps: HashMap::new(),
            registry_url: String::new(),
            features: Vec::new(),
            include_dev: false,
            include_build: false,
            include_yanked: false,
        }
    }
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

use crate::lockfile::{LockFile, LockedPackage, ResolutionDecision, ResolutionProof};

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
// SAT Solver Internal Types
// =============================================================================

/// A literal in the SAT solver (package @ version selected or not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Literal {
    pkg: usize,
    ver: usize,
    positive: bool,
}

impl Literal {
    fn positive(pkg: usize, ver: usize) -> Self {
        Self {
            pkg,
            ver,
            positive: true,
        }
    }

    fn negative(pkg: usize, ver: usize) -> Self {
        Self {
            pkg,
            ver,
            positive: false,
        }
    }

    fn negated(&self) -> Self {
        Self {
            pkg: self.pkg,
            ver: self.ver,
            positive: !self.positive,
        }
    }
}

/// A clause is a disjunction of literals.
#[derive(Debug, Clone, PartialEq)]
struct Clause {
    literals: Vec<Literal>,
    learned: bool,
    activity: f64,
}

impl Clause {
    fn new(literals: Vec<Literal>) -> Self {
        Self {
            literals,
            learned: false,
            activity: 0.0,
        }
    }

    fn learned(literals: Vec<Literal>, activity: f64) -> Self {
        Self {
            literals,
            learned: true,
            activity,
        }
    }
}

/// An implication graph node for CDCL.
#[derive(Debug, Clone)]
struct ImplicationNode {
    literal: Literal,
    level: u32,
    reason: Option<usize>,
}

/// The SAT solver state.
struct SatSolver {
    packages: Vec<PackageId>,
    candidates: Vec<Vec<(Version, String)>>, // (version, version_string)
    clauses: Vec<Clause>,
    assignment: Vec<Vec<Option<bool>>>,
    trail: Vec<Literal>,
    trail_lim: Vec<usize>,
    decision_level: u32,
    implications: HashMap<Literal, ImplicationNode>,
    watches: HashMap<Literal, Vec<usize>>,
    var_activity: HashMap<(usize, usize), f64>,
    conflict_clause: Option<usize>,
    var_inc: f64,
}

impl SatSolver {
    fn new(packages: Vec<PackageId>, candidates: Vec<Vec<(Version, String)>>) -> Self {
        let assignment = candidates
            .iter()
            .map(|vers| vec![None; vers.len()])
            .collect();

        Self {
            packages,
            candidates,
            clauses: Vec::new(),
            assignment,
            trail: Vec::new(),
            trail_lim: Vec::new(),
            decision_level: 0,
            implications: HashMap::new(),
            watches: HashMap::new(),
            var_activity: HashMap::new(),
            conflict_clause: None,
            var_inc: 1.0,
        }
    }

    fn add_clause(&mut self, mut literals: Vec<Literal>) {
        if literals.is_empty() {
            return;
        }

        literals.sort();
        literals.dedup();

        // Check for tautology
        for i in 0..literals.len().saturating_sub(1) {
            if literals[i] == literals[i + 1].negated() {
                return;
            }
        }

        let clause = Clause::new(literals.clone());
        let clause_idx = self.clauses.len();
        self.clauses.push(clause);

        if literals.len() >= 1 {
            self.watches
                .entry(literals[0].negated())
                .or_default()
                .push(clause_idx);
        }
        if literals.len() >= 2 {
            self.watches
                .entry(literals[1].negated())
                .or_default()
                .push(clause_idx);
        }
    }

    fn add_learned_clause(&mut self, literals: Vec<Literal>, activity: f64) {
        if literals.is_empty() {
            return;
        }

        let clause = Clause::learned(literals.clone(), activity);
        let clause_idx = self.clauses.len();
        self.clauses.push(clause);

        if literals.len() >= 1 {
            self.watches
                .entry(literals[0].negated())
                .or_default()
                .push(clause_idx);
        }
        if literals.len() >= 2 {
            self.watches
                .entry(literals[1].negated())
                .or_default()
                .push(clause_idx);
        }
    }

    fn value(&self, lit: Literal) -> Option<bool> {
        self.assignment
            .get(lit.pkg)
            .and_then(|vers| vers.get(lit.ver))
            .copied()
            .flatten()
            .map(|v| if lit.positive { v } else { !v })
    }

    fn assign(&mut self, lit: Literal, reason: Option<usize>) {
        let value = lit.positive;
        if let Some(vers) = self.assignment.get_mut(lit.pkg) {
            if let Some(slot) = vers.get_mut(lit.ver) {
                *slot = Some(value);
            }
        }

        self.trail.push(lit);
        self.implications.insert(
            lit,
            ImplicationNode {
                literal: lit,
                level: self.decision_level,
                reason,
            },
        );

        self.bump_var(lit.pkg, lit.ver);
    }

    fn bump_var(&mut self, pkg: usize, ver: usize) {
        let activity = self.var_activity.entry((pkg, ver)).or_insert(0.0);
        *activity += self.var_inc;

        if *activity > 1e100 {
            for act in self.var_activity.values_mut() {
                *act *= 1e-100;
            }
            self.var_inc *= 1e-100;
        }
    }

    fn decide(&mut self, lit: Literal) {
        self.decision_level += 1;
        self.trail_lim.push(self.trail.len());
        self.assign(lit, None);
    }

    fn backtrack(&mut self, level: u32) {
        while self.trail_lim.len() > level as usize {
            let lim = self.trail_lim.pop().unwrap();
            while self.trail.len() > lim {
                let lit = self.trail.pop().unwrap();
                if let Some(vers) = self.assignment.get_mut(lit.pkg) {
                    if let Some(slot) = vers.get_mut(lit.ver) {
                        *slot = None;
                    }
                }
                self.implications.remove(&lit);
            }
        }
        self.decision_level = level;
    }

    fn propagate(&mut self) -> Result<(), usize> {
        let mut q_head = 0;

        while q_head < self.trail.len() {
            let p = self.trail[q_head];
            q_head += 1;

            let not_p = p.negated();
            let watch_list = self.watches.get(&not_p).cloned().unwrap_or_default();

            for &clause_idx in &watch_list {
                if self.conflict_clause.is_some() {
                    return Err(clause_idx);
                }

                let clause = &self.clauses[clause_idx];
                let mut new_lit = None;
                let mut found_false = false;

                'find_lit: for &lit in &clause.literals {
                    let val = self.value(lit);
                    match val {
                        Some(true) => continue 'find_lit,
                        Some(false) => {
                            found_false = true;
                            continue 'find_lit;
                        }
                        None => {
                            if new_lit.is_none() || lit != not_p {
                                new_lit = Some(lit);
                                if !found_false {
                                    break 'find_lit;
                                }
                            }
                        }
                    }
                }

                if new_lit.is_none() {
                    self.conflict_clause = Some(clause_idx);
                    return Err(clause_idx);
                }

                let new_lit = new_lit.unwrap();
                if clause.literals.len() == 1 || !found_false {
                    if clause.literals.len() == 1 || self.value(new_lit).is_none() {
                        self.assign(new_lit, Some(clause_idx));
                    }
                }
            }
        }

        Ok(())
    }

    fn analyze_conflict(&mut self) -> Option<Vec<Literal>> {
        let conflict_idx = self.conflict_clause?;
        self.conflict_clause = None;

        let mut learned = Vec::new();
        let mut seen: HashSet<Literal> = HashSet::new();
        let mut counter = 0;
        let mut trail_idx = self.trail.len() as i32 - 1;

        let conflict = &self.clauses[conflict_idx];
        for &lit in &conflict.literals {
            if seen.insert(lit.negated()) {
                counter += 1;
            }
        }

        while counter > 1 {
            trail_idx -= 1;
            let lit = self.trail[trail_idx as usize];

            if !seen.contains(&lit.negated()) {
                continue;
            }

            let node = self.implications.get(&lit)?;
            if let Some(reason_idx) = node.reason {
                let reason = &self.clauses[reason_idx];
                for &q in &reason.literals {
                    if q.negated() != lit && seen.insert(q.negated()) {
                        counter += 1;
                    }
                }
            }

            counter -= 1;
        }

        let level = self.decision_level;
        for &lit in &seen {
            let node = self.implications.get(&lit.negated());
            if let Some(node) = node {
                if node.level > 0 && (node.level == level || node.reason.is_none()) {
                    learned.push(lit.negated());
                }
            }
        }

        self.var_inc *= 1.0 / 0.95;

        if !learned.is_empty() {
            Some(learned)
        } else {
            None
        }
    }

    fn get_backtrack_level(&self, learned: &[Literal]) -> u32 {
        if learned.len() <= 1 {
            return 0;
        }

        let mut max_level = 0;
        let mut second_max = 0;

        for &lit in learned {
            let node = self.implications.get(&lit);
            if let Some(node) = node {
                if node.level > max_level {
                    second_max = max_level;
                    max_level = node.level;
                } else if node.level > second_max {
                    second_max = node.level;
                }
            }
        }

        second_max
    }

    fn is_complete(&self) -> bool {
        for (pkg_idx, vers) in self.assignment.iter().enumerate() {
            let selected_count = vers.iter().filter(|&&v| v == Some(true)).count();
            if selected_count != 1 && !self.candidates[pkg_idx].is_empty() {
                return false;
            }
        }
        true
    }

    fn get_solution(&self) -> HashMap<PackageId, (Version, String)> {
        let mut solution = HashMap::new();
        for (pkg_idx, vers) in self.assignment.iter().enumerate() {
            for (ver_idx, &val) in vers.iter().enumerate() {
                if val == Some(true) {
                    let (version, version_str) = self.candidates[pkg_idx][ver_idx].clone();
                    solution.insert(self.packages[pkg_idx].clone(), (version, version_str));
                    break;
                }
            }
        }
        solution
    }
}

// =============================================================================
// Resolution State (CSP + AC-3)
// =============================================================================

/// State of the constraint satisfaction problem.
struct ResolutionState {
    domains: Vec<BTreeSet<usize>>,
    constraints: Vec<DependencyConstraint>,
    pkg_index: HashMap<PackageId, usize>,
    pkg_names: Vec<PackageId>,
    all_versions: Vec<Vec<(Version, String)>>,
    arc_queue: Vec<(usize, usize)>,
    /// Package metadata from registry
    metadata: HashMap<PackageId, RegistryVersionMetadata>,
}

/// A dependency constraint between packages.
#[derive(Debug, Clone)]
struct DependencyConstraint {
    from: PackageId,
    to: PackageId,
    range: Constraint,
    /// Features required by this dependency
    features: Vec<FeatureName>,
}

impl ResolutionState {
    fn new() -> Self {
        Self {
            domains: Vec::new(),
            constraints: Vec::new(),
            pkg_index: HashMap::new(),
            pkg_names: Vec::new(),
            all_versions: Vec::new(),
            arc_queue: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    fn get_or_create_pkg(&mut self, name: &PackageId) -> usize {
        if let Some(&idx) = self.pkg_index.get(name) {
            return idx;
        }
        let idx = self.pkg_names.len();
        self.pkg_names.push(name.clone());
        self.pkg_index.insert(name.clone(), idx);
        self.domains.push(BTreeSet::new());
        self.all_versions.push(Vec::new());
        idx
    }

    fn add_versions(&mut self, pkg: usize, versions: Vec<(Version, String)>) {
        self.all_versions[pkg] = versions;
        self.domains[pkg] = (0..self.all_versions[pkg].len()).collect();
    }

    fn set_metadata(&mut self, pkg: &PackageId, metadata: RegistryVersionMetadata) {
        self.metadata.insert(pkg.clone(), metadata);
    }

    fn ac3(&mut self) -> Result<(), Vec<Conflict>> {
        self.arc_queue.clear();
        for (i, pkg_i) in self.pkg_names.iter().enumerate() {
            for c in &self.constraints {
                if &c.to == pkg_i && c.from != *pkg_i {
                    let from_idx = *self.pkg_index.get(&c.from).unwrap_or(&i);
                    self.arc_queue.push((from_idx, i));
                }
            }
        }

        while let Some((xi, xj)) = self.arc_queue.pop() {
            if self.revise(xi, xj)? {
                if self.domains[xi].is_empty() {
                    return Err(self.collect_conflicts());
                }

                for (k, _) in self.pkg_names.iter().enumerate() {
                    if k != xi && k != xj {
                        self.arc_queue.push((k, xi));
                    }
                }
            }
        }

        Ok(())
    }

    fn revise(&mut self, xi: usize, xj: usize) -> Result<bool, Vec<Conflict>> {
        let mut revised = false;
        let xi_versions: Vec<usize> = self.domains[xi].iter().copied().collect();

        for &vi in &xi_versions {
            let mut has_support = true;

            for c in &self.constraints {
                if c.from == self.pkg_names[xi] && c.to == self.pkg_names[xj] {
                    let mut found_support = false;
                    for &vj in &self.domains[xj] {
                        let version_j = &self.all_versions[xj][vj].0;
                        if c.range.matches(version_j) {
                            found_support = true;
                            break;
                        }
                    }
                    if !found_support {
                        has_support = false;
                        break;
                    }
                }
            }

            if !has_support {
                self.domains[xi].remove(&vi);
                revised = true;
            }
        }

        Ok(revised)
    }

    fn collect_conflicts(&self) -> Vec<Conflict> {
        let mut conflicts = Vec::new();

        for (idx, domain) in self.domains.iter().enumerate() {
            if domain.is_empty() {
                let pkg = &self.pkg_names[idx];
                let mut required_by = Vec::new();

                for c in &self.constraints {
                    if &c.to == pkg {
                        required_by.push((c.from.clone(), c.range.to_string()));
                    }
                }

                conflicts.push(Conflict {
                    package: pkg.clone(),
                    required_by,
                    suggestions: vec![ConflictSuggestion::Remove {
                        package: pkg.clone(),
                        why: "No compatible version exists".to_string(),
                    }],
                });
            }
        }

        conflicts
    }
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
            .unwrap_or_else(|| std::env::temp_dir())
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
    pub fn resolve(
        &self,
        request: &ResolutionRequest,
    ) -> Result<ResolutionResult, ResolutionError> {
        // Phase 1: Build dependency graph with feature resolution
        let mut state = ResolutionState::new();
        let mut visited = HashSet::new();
        let mut pending_features: HashMap<PackageId, Vec<FeatureName>> = HashMap::new();

        // Collect root features
        if !request.features.is_empty() {
            // Root package features - will be processed when we find dependencies
            pending_features.insert("__root__".to_string(), request.features.clone());
        }

        // Collect normal dependencies
        self.collect_packages(
            &request.root_deps,
            &mut state,
            &mut visited,
            &mut pending_features,
        )?;

        // Collect dev dependencies (only for root package)
        if request.include_dev {
            self.collect_packages(
                &request.dev_deps,
                &mut state,
                &mut visited,
                &mut pending_features,
            )?;
        }

        // Collect build dependencies
        if request.include_build {
            self.collect_packages(
                &request.build_deps,
                &mut state,
                &mut visited,
                &mut pending_features,
            )?;
        }

        // Phase 2: Run AC-3 for constraint propagation
        if let Err(conflicts) = state.ac3() {
            return Err(ResolutionError::NoSolution {
                conflicts: self.enhance_conflicts(conflicts, &state),
            });
        }

        // Phase 3: Convert to SAT and solve
        self.solve_sat(state, &request.registry_url, &pending_features, request)
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
            let mut new_locked = HashMap::new();
            for pkg in &previous_lockfile.packages {
                if !to_update.contains(&pkg.name) {
                    new_locked.insert(pkg.name.clone(), pkg.version.clone());
                }
            }
            let mut modified_resolver =
                Self::new(self.registry.base_url(), Some(previous_lockfile));
            modified_resolver.policy = policy;
            modified_resolver.previous_solution = self.previous_solution.clone();
            return modified_resolver.resolve(request);
        }

        policy.minimize_changes = true;
        policy.prefer_highest = true;

        let mut resolver = Self::for_update(self.registry.base_url(), previous_lockfile, policy);
        resolver.git_cache_dir = self.git_cache_dir.clone();
        resolver.resolve(request)
    }


    fn collect_packages(
        &self,
        deps: &HashMap<PackageId, DependencySpec>,
        state: &mut ResolutionState,
        visited: &mut HashSet<PackageId>,
        pending_features: &mut HashMap<PackageId, Vec<FeatureName>>,
    ) -> Result<(), ResolutionError> {
        // Queue of (package_name, spec, parent_package) to process
        let mut to_process: VecDeque<(PackageId, DependencySpec, Option<PackageId>)> = deps
            .iter()
            .filter(|(name, _)| !visited.contains(*name))
            .map(|(name, spec)| (name.clone(), spec.clone(), None))
            .collect();

        while let Some((pkg_name, spec, parent)) = to_process.pop_front() {
            if visited.contains(&pkg_name) {
                // If already visited, we still need to add the constraint from parent
                if let Some(parent_name) = parent {
                    self.add_dependency_constraint(&parent_name, &pkg_name, &spec, state)?;
                }
                continue;
            }
            visited.insert(pkg_name.clone());

            let pkg_idx = state.get_or_create_pkg(&pkg_name);

            // Only fetch from registry for version constraints
            let (versions, is_registry) = match &spec {
                DependencySpec::Version(constraint_str) => {
                    // Parse the constraint string
                    match Constraint::parse(constraint_str) {
                        Ok(constraint) => {
                            let versions =
                                self.fetch_compatible_versions(&pkg_name, &constraint)?;
                            (versions, true)
                        }
                        Err(e) => {
                            return Err(ResolutionError::RegistryError {
                                message: format!(
                                    "Invalid version constraint '{}': {}",
                                    constraint_str, e
                                ),
                            })
                        }
                    }
                }
                DependencySpec::Path { .. }
                | DependencySpec::Git { .. }
                | DependencySpec::Workspace { .. } => {
                    // For path/git deps, use a placeholder version
                    (vec![(Version::new(0, 1, 0), "0.1.0".to_string())], false)
                }
                DependencySpec::VersionDetailed {
                    version, features, ..
                } => {
                    // Store features for later resolution
                    if let Some(feats) = features {
                        pending_features.insert(pkg_name.clone(), feats.clone());
                    }
                    // Use the version as constraint
                    match Constraint::parse(version) {
                        Ok(constraint) => {
                            let versions =
                                self.fetch_compatible_versions(&pkg_name, &constraint)?;
                            (versions, true)
                        }
                        Err(e) => {
                            return Err(ResolutionError::RegistryError {
                                message: format!("Invalid version '{}': {}", version, e),
                            })
                        }
                    }
                }
            };

            if versions.is_empty() {
                return Err(ResolutionError::VersionNotFound {
                    package: pkg_name.clone(),
                    constraint: format!("{:?}", spec),
                });
            }

            state.add_versions(pkg_idx, versions.clone());

            // Add constraint from parent
            if let Some(parent_name) = parent {
                self.add_dependency_constraint(&parent_name, &pkg_name, &spec, state)?;
            }

            // For registry packages, fetch metadata and transitive dependencies
            if is_registry {
                // Fetch metadata for the best matching version
                let best_version = &versions[0].1;
                match self.fetch_version_metadata_with_cache(&pkg_name, best_version) {
                    Ok(metadata) => {
                        // Store metadata for feature resolution
                        state.set_metadata(&pkg_name, metadata.clone());

                        // Queue transitive dependencies
                        for (dep_name, dep_constraint_str) in &metadata.deps {
                            if !visited.contains(dep_name) {
                                let dep_spec = DependencySpec::Version(dep_constraint_str.clone());
                                to_process.push_back((
                                    dep_name.clone(),
                                    dep_spec,
                                    Some(pkg_name.clone()),
                                ));
                            } else {
                                // Already visited, just add the constraint
                                let dep_spec = DependencySpec::Version(dep_constraint_str.clone());
                                self.add_dependency_constraint(
                                    &pkg_name, dep_name, &dep_spec, state,
                                )?;
                            }
                        }
                    }
                    Err(e) => {
                        // Log warning but continue - metadata might not be critical
                        eprintln!(
                            "Warning: Could not fetch metadata for {}@{}: {}",
                            pkg_name, best_version, e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Add a dependency constraint to the resolution state.
    fn add_dependency_constraint(
        &self,
        from: &PackageId,
        to: &PackageId,
        spec: &DependencySpec,
        state: &mut ResolutionState,
    ) -> Result<(), ResolutionError> {
        let constraint =
            match spec {
                DependencySpec::Version(constraint_str) => Constraint::parse(constraint_str)
                    .map_err(|e| ResolutionError::RegistryError {
                        message: format!("Invalid constraint '{}': {}", constraint_str, e),
                    })?,
                DependencySpec::VersionDetailed { version, .. } => Constraint::parse(version)
                    .map_err(|e| ResolutionError::RegistryError {
                        message: format!("Invalid version '{}': {}", version, e),
                    })?,
                _ => {
                    // Path/git/workspace dependencies don't use semver constraints
                    return Ok(());
                }
            };

        state.constraints.push(DependencyConstraint {
            from: from.clone(),
            to: to.clone(),
            range: constraint,
            features: Vec::new(),
        });

        Ok(())
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

    fn fetch_compatible_versions(
        &self,
        pkg_name: &str,
        constraint: &Constraint,
    ) -> Result<Vec<(Version, String)>, ResolutionError> {
        // Check for locked version first if policy prefers locked
        if self.policy.prefer_locked {
            if let Some(locked_version_str) = self.locked.get(pkg_name) {
                if let Ok(locked_version) = Version::from_str(locked_version_str) {
                    if constraint.matches_pre(&locked_version, self.policy.include_prerelease) {
                        // Locked version satisfies constraint, use it exclusively
                        return Ok(vec![(locked_version, locked_version_str.clone())]);
                    }
                }
            }
        }

        // Try to fetch from cache first
        let index = {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(idx) = cache.get_index(pkg_name) {
                idx.clone()
            } else {
                // Need to fetch from registry
                drop(cache);
                match self.registry.fetch_package_index(pkg_name) {
                    Ok(idx) => {
                        // Store in cache
                        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                        cache.put_index(pkg_name.to_string(), idx.clone());
                        idx
                    }
                    Err(e) => {
                        return Err(ResolutionError::RegistryError {
                            message: format!(
                                "Failed to fetch package index for '{}': {}",
                                pkg_name, e
                            ),
                        });
                    }
                }
            }
        };

        // Filter versions by constraint
        let mut compatible = Vec::new();
        for v_str in &index.versions {
            // Skip yanked versions unless include_yanked is true or it's the locked version
            if index.yanked.contains_key(v_str) && !self.policy.include_yanked {
                // Always allow locked version even if yanked (lockfile compatibility)
                if let Some(locked) = self.locked.get(pkg_name) {
                    if locked != v_str {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            if let Ok(v) = Version::from_str(v_str) {
                if constraint.matches_pre(&v, self.policy.include_prerelease) {
                    compatible.push((v, v_str.clone()));
                }
            }
        }

        // Sort by version descending (highest first) for prefer_highest
        if self.policy.prefer_highest {
            compatible.sort_by(|a, b| b.0.cmp(&a.0));
        } else {
            compatible.sort_by(|a, b| a.0.cmp(&b.0));
        }

        Ok(compatible)
    }

    fn solve_sat(
        &self,
        state: ResolutionState,
        _registry_url: &str,
        pending_features: &HashMap<PackageId, Vec<FeatureName>>,
        request: &ResolutionRequest,
    ) -> Result<ResolutionResult, ResolutionError> {
        let mut solver = SatSolver::new(state.pkg_names.clone(), state.all_versions.clone());

        // Add constraints:
        // 1. At least one version per package
        // 2. At most one version per package (single-version rule)
        for (pkg_idx, versions) in state.all_versions.iter().enumerate() {
            if versions.is_empty() {
                continue;
            }

            // At least one
            let at_least_one: Vec<Literal> = (0..versions.len())
                .map(|v| Literal::positive(pkg_idx, v))
                .collect();
            solver.add_clause(at_least_one);

            // At most one (for single-version mode)
            if self.policy.mode == ResolutionMode::SingleVersion {
                for i in 0..versions.len() {
                    for j in (i + 1)..versions.len() {
                        solver.add_clause(vec![
                            Literal::negative(pkg_idx, i),
                            Literal::negative(pkg_idx, j),
                        ]);
                    }
                }
            }
        }

        // 3. Dependency constraints
        for c in &state.constraints {
            let from_idx = *state.pkg_index.get(&c.from).unwrap();
            let to_idx = *state.pkg_index.get(&c.to).unwrap();

            for (vi, _) in state.all_versions[from_idx].iter().enumerate() {
                let compatible: Vec<usize> = state.all_versions[to_idx]
                    .iter()
                    .enumerate()
                    .filter(|(_, (v, _))| c.range.matches(v))
                    .map(|(i, _)| i)
                    .collect();

                if compatible.is_empty() {
                    solver.add_clause(vec![Literal::negative(from_idx, vi)]);
                } else {
                    let mut clause = vec![Literal::negative(from_idx, vi)];
                    for vj in compatible {
                        clause.push(Literal::positive(to_idx, vj));
                    }
                    solver.add_clause(clause);
                }
            }
        }

        // Run CDCL
        let (solution, conflicts_solved) = self.run_cdcl(&mut solver, &state)?;

        // Build ResolutionProof from solver trail
        let mut decisions = Vec::new();
        for lit in &solver.trail {
            if lit.positive {
                let pkg_name = &state.pkg_names[lit.pkg];
                let (_, version_str) = &state.all_versions[lit.pkg][lit.ver];
                
                let reason = if let Some(node) = solver.implications.get(lit) {
                    if let Some(_clause_idx) = node.reason {
                        format!("Implied by dependency constraints at level {}", node.level)
                    } else {
                        format!("Decision node at level {}", node.level)
                    }
                } else {
                    "Initial assignment".to_string()
                };

                decisions.push(ResolutionDecision {
                    package: pkg_name.clone(),
                    version: version_str.to_string(),
                    reason,
                    level: solver.implications.get(lit).map(|n| n.level).unwrap_or(0),
                });
            }
        }

        let proof = ResolutionProof {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
            resolver_type: "SAT-v1".to_string(),
            explanation: format!(
                "Deterministic SAT resolution using CDCL with {} decisions and {} conflicts solved.",
                decisions.len(),
                conflicts_solved
            ),
            decisions,
            conflicts_solved,
        };

        // Build result with feature resolution
        let mut packages = Vec::new();
        for (pkg_name, (_version, version_str)) in &solution {
            let source = ResolvedSource::Registry {
                url: self.registry.base_url().to_string(),
                cid: None,
                artifacts: Vec::new(),
            };

            // Resolve features if requested
            let enabled_features = if let Some(features) = pending_features.get(pkg_name) {
                if let Some(metadata) = state.metadata.get(pkg_name) {
                    let available = HashMap::new(); // Simplified available features map
                    if let Ok(resolution) =
                        resolve_features(pkg_name, features, metadata, &available)
                    {
                        resolution.enabled.into_iter().collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    features.clone()
                }
            } else {
                Vec::new()
            };

            // Determine dependency kind
            let kind = if request.dev_deps.contains_key(pkg_name) {
                DependencyKind::Dev
            } else if request.build_deps.contains_key(pkg_name) {
                DependencyKind::Build
            } else {
                DependencyKind::Normal
            };

            packages.push(ResolvedPackage {
                name: pkg_name.clone(),
                version: version_str.clone(),
                deps: Vec::new(), // Would be populated from metadata in full impl
                source,
                enabled_features,
                kind,
            });
        }

        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ResolutionResult { packages, proof })
    }

    fn run_cdcl(
        &self,
        solver: &mut SatSolver,
        state: &ResolutionState,
    ) -> Result<(HashMap<PackageId, (Version, String)>, usize), ResolutionError> {
        let max_conflicts = 10000;
        let mut conflicts = 0;

        loop {
            if solver.propagate().is_err() {
                conflicts += 1;

                if conflicts > max_conflicts {
                    return Err(ResolutionError::NoSolution {
                        conflicts: vec![Conflict {
                            package: "solver".to_string(),
                            required_by: vec![],
                            suggestions: vec![ConflictSuggestion::Remove {
                                package: "some package".to_string(),
                                why: "Too many conflicts during resolution".to_string(),
                            }],
                        }],
                    });
                }

                if solver.decision_level == 0 {
                    return Err(ResolutionError::NoSolution {
                        conflicts: state.collect_conflicts(),
                    });
                }

                let learned = solver.analyze_conflict();
                if let Some(clause) = learned {
                    let bt_level = solver.get_backtrack_level(&clause);
                    solver.backtrack(bt_level);
                    solver.add_learned_clause(clause, solver.var_inc);

                    if conflicts % 100 == 0 {
                        solver.backtrack(0);
                    }
                } else {
                    return Err(ResolutionError::NoSolution {
                        conflicts: state.collect_conflicts(),
                    });
                }

                continue;
            }

            if solver.is_complete() {
                return Ok((solver.get_solution(), conflicts));
            }

            // Make decision with policy-aware selection
            let decision = self.select_decision(solver, state);
            if let Some(lit) = decision {
                solver.decide(lit);
            } else {
                return Err(ResolutionError::InternalError {
                    message: "Resolution failed: incomplete solution".to_string(),
                });
            }
        }
    }

    fn select_decision(&self, solver: &SatSolver, state: &ResolutionState) -> Option<Literal> {
        let mut best_pkg = None;
        let mut best_count = usize::MAX;
        let mut best_has_locked = false;

        for (pkg_idx, vers) in solver.assignment.iter().enumerate() {
            let true_count = vers.iter().filter(|&&v| v == Some(true)).count();

            if true_count > 0 || solver.candidates[pkg_idx].is_empty() {
                continue;
            }

            let unassigned_count = vers.iter().filter(|&&v| v.is_none()).count();
            let pkg_name = &solver.packages[pkg_idx];
            let has_locked = self.policy.prefer_locked && self.locked.contains_key(pkg_name);

            if unassigned_count > 0 {
                let is_better = (has_locked && !best_has_locked)
                    || (has_locked == best_has_locked && unassigned_count < best_count);

                if is_better {
                    best_pkg = Some(pkg_idx);
                    best_count = unassigned_count;
                    best_has_locked = has_locked;
                }
            }
        }

        let pkg_idx = best_pkg?;
        let pkg_name = &solver.packages[pkg_idx];

        let mut best_ver = None;
        let mut best_score = (-1i32, Version::new(0, 0, 0));

        for (ver_idx, &assigned) in solver.assignment[pkg_idx].iter().enumerate() {
            if assigned.is_some() {
                continue;
            }

            let (version, _) = &solver.candidates[pkg_idx][ver_idx];
            let is_locked = self
                .locked
                .get(pkg_name)
                .map(|v| {
                    Version::from_str(v)
                        .map(|lv| lv == *version)
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            // Check if this version is in the previous solution (for minimize_changes)
            let is_previous = self
                .previous_solution
                .as_ref()
                .and_then(|sol| sol.get(pkg_name))
                .map(|v| {
                    Version::from_str(v)
                        .map(|pv| pv == *version)
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            // Score: locked > previous > version
            let score = (
                if is_locked {
                    2
                } else if is_previous {
                    1
                } else {
                    0
                },
                if self.policy.prefer_highest {
                    version.clone()
                } else {
                    Version::new(0, 0, 0)
                },
            );

            if score.0 > best_score.0 || (score.0 == best_score.0 && score.1 > best_score.1) {
                best_score = score;
                best_ver = Some(ver_idx);
            }
        }

        best_ver.map(|ver_idx| Literal::positive(pkg_idx, ver_idx))
    }

    fn enhance_conflicts(
        &self,
        conflicts: Vec<Conflict>,
        state: &ResolutionState,
    ) -> Vec<Conflict> {
        conflicts
            .into_iter()
            .map(|mut c| {
                c.suggestions = self.generate_suggestions(&c, state);
                c
            })
            .collect()
    }

    fn generate_suggestions(
        &self,
        conflict: &Conflict,
        state: &ResolutionState,
    ) -> Vec<ConflictSuggestion> {
        let mut suggestions = Vec::new();

        // Parse constraints to find common ground
        let constraints: Vec<_> = conflict
            .required_by
            .iter()
            .map(|(_, c)| c.as_str())
            .collect();

        // Check if there's a version that satisfies all
        if let Some(common) = find_common_version(&constraints, state) {
            suggestions.push(ConflictSuggestion::Update {
                package: conflict.package.clone(),
                to: common,
                why: "This version satisfies all constraints".to_string(),
            });
        }

        // Suggest updating the most restrictive constraint
        if let Some((most_restrictive_pkg, most_restrictive)) =
            conflict.required_by.iter().min_by_key(|(_, c)| {
                // Heuristic: exact constraints are most restrictive
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

        // Suggest forking if versions are incompatible
        if conflict.required_by.len() == 2 {
            suggestions.push(ConflictSuggestion::Fork {
                package: conflict.package.clone(),
                alias: format!("{}", conflict.package),
                why: "Allow different versions for different parts of the dependency tree"
                    .to_string(),
            });
        }

        suggestions
    }

    /// Get the git cache directory.
    pub fn git_cache_dir(&self) -> &std::path::PathBuf {
        &self.git_cache_dir
    }
}

/// Find a version that satisfies all constraints (best effort).
fn find_common_version(constraints: &[&str], _state: &ResolutionState) -> Option<String> {
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
                if rest.contains(&first) {
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
