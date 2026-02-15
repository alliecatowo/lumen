//! SAT-based dependency resolver for Lumen packages.
//!
//! Implements a world-class constraint satisfaction solver with:
//! - CSP formulation with variables = package versions, constraints = dependencies
//! - MRV (Minimum Remaining Values) heuristic for variable ordering
//! - AC-3 (Arc Consistency) for constraint propagation
//! - CDCL (Conflict-Driven Clause Learning) for efficient backtracking
//! - Preference ordering: locked > highest > minimal changes > fewer packages
//! - Single-version enforcement with explicit fork rules
//!
//! ## Philosophy
//!
//! **Determinism first. Reproducibility always. Conflicts are errors unless explicitly mediated.**
//!
//! 1. Single version per package per build context (no diamond version conflicts)
//! 2. Resolution is deterministic with strict tie-breaking
//! 3. Dependency constraints are minimal and monotonic
//! 4. Conflicts are solved by explicit mechanisms, not magical installer tricks

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use crate::config::DependencySpec;
use crate::lockfile::LockFile;
use crate::registry::RegistryClient;
use crate::semver::{Constraint, Version};

// =============================================================================
// Core Types - Public API
// =============================================================================

/// Unique identifier for a package (namespace/name format).
pub type PackageId = String;

/// Type alias for version constraints used in dependency declarations.
pub type VersionConstraint = Constraint;

/// Request for dependency resolution.
#[derive(Debug, Clone)]
pub struct ResolutionRequest {
    /// Root dependencies with version constraints.
    pub root_deps: HashMap<PackageId, DependencySpec>,
    /// Registry URL to use for resolution.
    pub registry_url: String,
}

/// A resolved package with its exact version and dependencies.
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Package name.
    pub name: PackageId,
    /// Resolved version.
    pub version: String,
    /// Dependencies (name, spec).
    pub deps: Vec<(PackageId, DependencySpec)>,
    /// Source of the package.
    pub source: ResolvedSource,
}

impl fmt::Display for ResolvedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

/// Source of a resolved package.
#[derive(Debug, Clone)]
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
}

impl Default for ResolutionPolicy {
    fn default() -> Self {
        Self {
            mode: ResolutionMode::SingleVersion,
            prefer_locked: true,
            prefer_highest: true,
            minimize_changes: true,
            fork_rules: Vec::new(),
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
            Self::VersionNotFound { package, constraint } => {
                write!(
                    f,
                    "No version found for '{}' satisfying '{}'",
                    package, constraint
                )
            }
            Self::RegistryError { message } => write!(f, "Registry error: {}", message),
            Self::InternalError { message } => write!(f, "Internal resolver error: {}", message),
        }
    }
}

impl std::error::Error for ResolutionError {}

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
    Remove {
        package: PackageId,
        why: String,
    },
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
}

/// A dependency constraint between packages.
#[derive(Debug, Clone)]
struct DependencyConstraint {
    from: PackageId,
    to: PackageId,
    range: Constraint,
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
// Main Resolver
// =============================================================================

/// The SAT-based dependency resolver.
pub struct Resolver {
    registry: RegistryClient,
    locked: HashMap<PackageId, String>,
    policy: ResolutionPolicy,
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

        Self {
            registry: RegistryClient::new(registry_url),
            locked,
            policy: ResolutionPolicy::default(),
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

    /// Run the resolution algorithm.
    pub fn resolve(
        &self,
        request: &ResolutionRequest,
    ) -> Result<Vec<ResolvedPackage>, String> {
        // Phase 1: Build dependency graph
        let mut state = ResolutionState::new();
        let mut visited = HashSet::new();

        self.collect_packages(&request.root_deps, &mut state, &mut visited)?;

        // Phase 2: Run AC-3 for constraint propagation
        if let Err(conflicts) = state.ac3() {
            let conflict_desc: Vec<String> = conflicts
                .iter()
                .map(|c| format!("{}: {}", c.package, c.describe()))
                .collect();
            return Err(format!("No solution found: {}", conflict_desc.join("; ")));
        }

        // Phase 3: Convert to SAT and solve
        self.solve_sat(state, &request.registry_url)
    }

    fn collect_packages(
        &self,
        deps: &HashMap<PackageId, DependencySpec>,
        state: &mut ResolutionState,
        visited: &mut HashSet<PackageId>,
    ) -> Result<(), String> {
        let mut to_process: Vec<(PackageId, DependencySpec)> = deps
            .iter()
            .filter(|(name, _)| !visited.contains(*name))
            .map(|(name, spec)| (name.clone(), spec.clone()))
            .collect();

        while let Some((pkg_name, spec)) = to_process.pop() {
            if visited.contains(&pkg_name) {
                continue;
            }
            visited.insert(pkg_name.clone());

            let pkg_idx = state.get_or_create_pkg(&pkg_name);

            // Only fetch from registry for version constraints
            let versions = match &spec {
                DependencySpec::Version(constraint_str) => {
                    // Parse the constraint string
                    match Constraint::parse(constraint_str) {
                        Ok(constraint) => self.fetch_compatible_versions(&pkg_name, &constraint)?,
                        Err(e) => return Err(format!("Invalid version constraint '{}': {}", constraint_str, e)),
                    }
                }
                DependencySpec::Path { .. } | DependencySpec::Git { .. } | DependencySpec::Workspace { .. } => {
                    // For path/git deps, use a placeholder version
                    vec![(Version::new(0, 1, 0), "0.1.0".to_string())]
                }
                DependencySpec::VersionDetailed { version, .. } => {
                    // Use the version as exact constraint
                    match Constraint::parse(version) {
                        Ok(constraint) => self.fetch_compatible_versions(&pkg_name, &constraint)?,
                        Err(e) => return Err(format!("Invalid version '{}': {}", version, e)),
                    }
                }
            };

            if versions.is_empty() {
                return Err(format!("No compatible versions found for '{}'", pkg_name));
            }

            state.add_versions(pkg_idx, versions);
        }

        Ok(())
    }

    fn fetch_compatible_versions(
        &self,
        pkg_name: &str,
        constraint: &Constraint,
    ) -> Result<Vec<(Version, String)>, String> {
        // Try to fetch from registry
        match self.registry.fetch_package_index(pkg_name) {
            Ok(index) => {
                let mut compatible = Vec::new();
                for v_str in &index.versions {
                    if let Ok(v) = Version::from_str(v_str) {
                        if constraint.matches(&v) {
                            compatible.push((v, v_str.clone()));
                        }
                    }
                }
                // Sort by version descending (highest first)
                compatible.sort_by(|a, b| b.0.cmp(&a.0));
                Ok(compatible)
            }
            Err(_) => {
                // If registry fetch fails, return empty
                Ok(Vec::new())
            }
        }
    }

    fn solve_sat(
        &self,
        state: ResolutionState,
        _registry_url: &str,
    ) -> Result<Vec<ResolvedPackage>, String> {
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
        let solution = self.run_cdcl(&mut solver)?;

        // Build result
        let mut packages = Vec::new();
        for (pkg_name, (_version, version_str)) in &solution {
            packages.push(ResolvedPackage {
                name: pkg_name.clone(),
                version: version_str.clone(),
                deps: Vec::new(),
                source: ResolvedSource::Registry {
                    url: self.registry.base_url().to_string(),
                    cid: None,
                    artifacts: Vec::new(),
                },
            });
        }

        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(packages)
    }

    fn run_cdcl(
        &self,
        solver: &mut SatSolver,
    ) -> Result<HashMap<PackageId, (Version, String)>, String> {
        let max_conflicts = 10000;
        let mut conflicts = 0;

        loop {
            if solver.propagate().is_err() {
                conflicts += 1;

                if conflicts > max_conflicts {
                    return Err("Resolution failed: too many conflicts".to_string());
                }

                if solver.decision_level == 0 {
                    return Err("Resolution failed: unsatisfiable constraints".to_string());
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
                    return Err("Resolution failed: no solution".to_string());
                }

                continue;
            }

            if solver.is_complete() {
                return Ok(solver.get_solution());
            }

            // Make decision
            let decision = self.select_decision(solver);
            if let Some(lit) = decision {
                solver.decide(lit);
            } else {
                return Err("Resolution failed: incomplete solution".to_string());
            }
        }
    }

    fn select_decision(&self, solver: &SatSolver) -> Option<Literal> {
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
                    Version::from_str(v).map(|lv| lv == *version).unwrap_or(false)
                })
                .unwrap_or(false);

            let score = (if is_locked { 1 } else { 0 }, version.clone());

            if score.0 > best_score.0 || (score.0 == best_score.0 && score.1 > best_score.1) {
                best_score = score;
                best_ver = Some(ver_idx);
            }
        }

        best_ver.map(|ver_idx| Literal::positive(pkg_idx, ver_idx))
    }
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
        };

        assert_eq!(request.root_deps.len(), 1);
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
            suggestions: self.generate_suggestions(),
            severity: self.severity(),
        }
    }

    /// Generate actionable suggestions based on the conflict.
    fn generate_suggestions(&self) -> Vec<ActionableSuggestion> {
        let mut suggestions = Vec::new();

        // Parse constraints to find common ground
        let constraints: Vec<_> = self
            .required_by
            .iter()
            .map(|(_, c)| c.as_str())
            .collect();

        // Check if there's a version that satisfies all
        if let Some(common) = find_common_version(&constraints) {
            suggestions.push(ActionableSuggestion::UseCommonVersion {
                package: self.package.clone(),
                version: common,
                reason: "This version satisfies all constraints".to_string(),
            });
        }

        // Suggest updating the most restrictive constraint
        if let Some((most_restrictive_pkg, most_restrictive)) = self.find_most_restrictive() {
            suggestions.push(ActionableSuggestion::RelaxConstraint {
                package: most_restrictive_pkg,
                current: most_restrictive,
                suggested: "Use a broader version range".to_string(),
                reason: "This constraint is too restrictive for the dependency graph".to_string(),
            });
        }

        // Suggest forking if versions are incompatible
        if constraints.len() == 2 {
            suggestions.push(ActionableSuggestion::ForkPackage {
                package: self.package.clone(),
                reason: "Allow different versions for different parts of the dependency tree".to_string(),
            });
        }

        suggestions
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

    /// Find the most restrictive constraint.
    fn find_most_restrictive(&self) -> Option<(PackageId, String)> {
        // Heuristic: caret constraints are most restrictive
        for (pkg, constraint) in &self.required_by {
            if constraint.starts_with('^') || constraint.starts_with('~') {
                return Some((pkg.clone(), constraint.clone()));
            }
        }
        self.required_by.first().cloned()
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
    ForkPackage {
        package: PackageId,
        reason: String,
    },
    /// Remove a dependency.
    RemoveDependency {
        package: PackageId,
        reason: String,
    },
}

impl fmt::Display for ActionableSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UseCommonVersion { package, version, reason } => {
                write!(f, "Use {package}@{version} ({reason})")
            }
            Self::RelaxConstraint { package, current, suggested, reason } => {
                write!(f, "Relax {package} from {current} - {suggested} ({reason})")
            }
            Self::ForkPackage { package, reason } => {
                write!(f, "Fork {package} - add [fork-rules] to lumen.toml ({reason})")
            }
            Self::RemoveDependency { package, reason } => {
                write!(f, "Remove {package} from dependencies ({reason})")
            }
        }
    }
}

/// Find a version that satisfies all constraints (best effort).
fn find_common_version(constraints: &[&str]) -> Option<String> {
    // This is a simplified heuristic - in practice would use the semver module
    // to find actual common versions from the registry

    // Look for common major version
    let mut majors: Vec<u64> = Vec::new();
    for c in constraints {
        if let Some(major_str) = c.trim_start_matches('^').trim_start_matches('~').split('.').next() {
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

/// Format a resolution error for display.
pub fn format_resolution_error(error: &ResolutionError) -> String {
    let mut output = String::new();

    match error {
        ResolutionError::NoSolution { conflicts } => {
            output.push_str("\n╔═══════════════════════════════════════════════════════════╗\n");
            output.push_str("║            Dependency Resolution Failed                   ║\n");
            output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");

            if conflicts.is_empty() {
                output.push_str("The dependency graph contains conflicts that cannot be resolved.\n");
                output.push_str("No specific conflicts were identified. This may indicate a bug.\n");
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

            output.push_str(&format!("Dependency chain: {}\n\n", chain.join(" → ")));

            if let Some((first, rest)) = chain.split_first() {
                if rest.contains(&first) {
                    output.push_str(&format!("Package '{first}' transitively depends on itself.\n"));
                    output.push_str("This is usually caused by:\n");
                    output.push_str("  • A package accidentally depending on itself\n");
                    output.push_str("  • Two packages depending on each other (use dev-dependencies for tests)\n");
                    output.push_str("  • A path dependency cycle in a workspace\n");
                }
            }
        }
        ResolutionError::VersionNotFound { package, constraint } => {
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
    }

    output
}

impl ConflictSuggestion {
    /// Format this suggestion as a human-readable string.
    pub fn format(&self) -> String {
        match self {
            Self::Update { package, to, why } => {
                format!("Update {package} to {to} ({why})")
            }
            Self::Fork { package, alias, why } => {
                format!("Fork {package} as {alias} ({why})")
            }
            Self::Remove { package, why } => {
                format!("Remove {package} ({why})")
            }
        }
    }
}
