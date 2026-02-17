//! Semantic Versioning 2.0.0 parsing and comparison for Lumen's package manager.
//!
//! This module implements full semver 2.0.0 specification including:
//! - Version parsing with pre-release and build metadata
//! - Version constraints (exact, caret, tilde, range, wildcard, compound)
//! - Pre-release handling per semver spec
//! - Constraint satisfaction checking
//! - Constraint compatibility checking for conflict detection

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// Type alias for constraint used in dependency declarations.
/// This is the primary type for version constraints in package dependencies.
pub type DependencyConstraint = Constraint;

/// Type alias for constraint used in resolved versions.
pub type VersionConstraint = Constraint;

/// A semantic version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    /// Major version number (X in X.Y.Z)
    pub major: u64,
    /// Minor version number (Y in X.Y.Z)
    pub minor: u64,
    /// Patch version number (Z in X.Y.Z)
    pub patch: u64,
    /// Pre-release identifiers (e.g., ["alpha", "1"] in 1.0.0-alpha.1)
    pub pre: Vec<PrereleaseIdentifier>,
    /// Build metadata (e.g., ["build", "123"] in 1.0.0+build.123)
    pub build: Vec<String>,
}

/// A pre-release identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrereleaseIdentifier {
    /// A numeric identifier (e.g., "1" in "alpha.1")
    Numeric(u64),
    /// An alphanumeric identifier (e.g., "alpha", "beta", "rc")
    Alpha(String),
}

impl PrereleaseIdentifier {
    /// Parse a pre-release identifier from a string.
    fn parse(s: &str) -> Result<Self, SemverError> {
        if s.is_empty() {
            return Err(SemverError::InvalidIdentifier(s.to_string()));
        }

        // Check for leading zeros in numeric identifiers
        if s.chars().all(|c| c.is_ascii_digit()) {
            if s.len() > 1 && s.starts_with('0') {
                return Err(SemverError::LeadingZero(s.to_string()));
            }
            s.parse::<u64>()
                .map(PrereleaseIdentifier::Numeric)
                .map_err(|_| SemverError::InvalidIdentifier(s.to_string()))
        } else {
            // Alphanumeric identifiers must contain only ASCII alphanumerics and hyphens
            if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(SemverError::InvalidIdentifier(s.to_string()));
            }
            Ok(PrereleaseIdentifier::Alpha(s.to_string()))
        }
    }
}

impl fmt::Display for PrereleaseIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrereleaseIdentifier::Numeric(n) => write!(f, "{}", n),
            PrereleaseIdentifier::Alpha(s) => write!(f, "{}", s),
        }
    }
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        // Per semver spec: numeric identifiers have lower precedence than alphanumeric
        match (self, other) {
            (PrereleaseIdentifier::Numeric(a), PrereleaseIdentifier::Numeric(b)) => a.cmp(b),
            (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::Alpha(_)) => Ordering::Less,
            (PrereleaseIdentifier::Alpha(_), PrereleaseIdentifier::Numeric(_)) => Ordering::Greater,
            (PrereleaseIdentifier::Alpha(a), PrereleaseIdentifier::Alpha(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    /// Create a new version with the given major, minor, and patch numbers.
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    /// Create a version with pre-release identifiers.
    pub fn with_pre(mut self, pre: Vec<PrereleaseIdentifier>) -> Self {
        self.pre = pre;
        self
    }

    /// Create a version with build metadata.
    pub fn with_build(mut self, build: Vec<String>) -> Self {
        self.build = build;
        self
    }

    /// Check if this is a pre-release version.
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// Check if this version satisfies the given constraint.
    pub fn satisfies(&self, constraint: &Constraint) -> bool {
        constraint.matches(self)
    }

    /// Get the base version without pre-release or build metadata.
    pub fn base(&self) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
            pre: Vec::new(),
            build: Vec::new(),
        }
    }

    /// Compare two versions with optional pre-release inclusion.
    /// When `include_prerelease` is false, pre-release versions are treated as
    /// less than their release counterparts (standard semver behavior).
    pub fn cmp_with_prerelease(&self, other: &Self, include_prerelease: bool) -> Ordering {
        if include_prerelease {
            return self.cmp(other);
        }

        // Compare base versions only
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Ignore pre-release differences when include_prerelease is false
        Ordering::Equal
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-")?;
            for (i, id) in self.pre.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                write!(f, "{}", id)?;
            }
        }
        if !self.build.is_empty() {
            write!(f, "+")?;
            for (i, b) in self.build.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                write!(f, "{}", b)?;
            }
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = SemverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Split off build metadata
        let (version_str, build) = if let Some(plus_pos) = s.find('+') {
            let build_str = &s[plus_pos + 1..];
            let build: Vec<String> = build_str.split('.').map(|s| s.to_string()).collect();
            // Validate build metadata identifiers
            for b in &build {
                if b.is_empty() || !b.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    return Err(SemverError::InvalidBuildMetadata(b.clone()));
                }
            }
            (&s[..plus_pos], build)
        } else {
            (s, Vec::new())
        };

        // Split off pre-release
        let (core_str, pre) = if let Some(dash_pos) = version_str.find('-') {
            let pre_str = &version_str[dash_pos + 1..];
            let pre: Result<Vec<_>, _> = pre_str
                .split('.')
                .map(PrereleaseIdentifier::parse)
                .collect();
            let pre = pre?;
            (&version_str[..dash_pos], pre)
        } else {
            (version_str, Vec::new())
        };

        // Parse core version (major.minor.patch)
        let parts: Vec<&str> = core_str.split('.').collect();
        if parts.len() != 3 {
            return Err(SemverError::InvalidVersion(format!(
                "Expected major.minor.patch, got {}",
                core_str
            )));
        }

        // Check for leading zeros
        for (i, part) in parts.iter().enumerate() {
            if part.len() > 1 && part.starts_with('0') {
                return Err(SemverError::LeadingZero(format!(
                    "{}.{}",
                    ["major", "minor", "patch"][i],
                    part
                )));
            }
        }

        let major = parts[0]
            .parse::<u64>()
            .map_err(|_| SemverError::InvalidVersion(format!("Invalid major: {}", parts[0])))?;
        let minor = parts[1]
            .parse::<u64>()
            .map_err(|_| SemverError::InvalidVersion(format!("Invalid minor: {}", parts[1])))?;
        let patch = parts[2]
            .parse::<u64>()
            .map_err(|_| SemverError::InvalidVersion(format!("Invalid patch: {}", parts[2])))?;

        Ok(Version {
            major,
            minor,
            patch,
            pre,
            build,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare major.minor.patch first
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Build metadata does not affect precedence
        // Pre-release versions have lower precedence than normal versions
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater, // self is release, other is pre-release
            (false, true) => Ordering::Less,    // self is pre-release, other is release
            (false, false) => {
                // Both have pre-release, compare identifiers
                for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                    match a.cmp(b) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                // A larger set of pre-release identifiers has higher precedence
                self.pre.len().cmp(&other.pre.len())
            }
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A version constraint for dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Exact version match: `=1.2.3`
    Exact(Version),
    /// Caret constraint: `^1.2.3` (compatible with 1.x.x where x >= 2.3)
    Caret(Version),
    /// Tilde constraint: `~1.2.3` (compatible with 1.2.x where x >= 3)
    Tilde(Version),
    /// Greater than: `>1.2.3` or `>=1.2.3`
    GreaterThan(Version, bool),
    /// Less than: `<1.2.3` or `<=1.2.3`
    LessThan(Version, bool),
    /// Range: `>=1.2.3 <2.0.0`
    Range {
        min: Version,
        max: Version,
        min_inclusive: bool,
        max_inclusive: bool,
    },
    /// Wildcard: `1.2.*`, `1.x`, `*`
    Wildcard {
        major: Option<u64>,
        minor: Option<u64>,
    },
    /// Logical OR of constraints: `^1.2.3 || ^2.0.0`
    Or(Vec<Constraint>),
    /// Logical AND of constraints: `>=1.0.0 <2.0.0`
    And(Vec<Constraint>),
    /// Matches any version
    Any,
    /// Matches no version
    None,
}

impl Constraint {
    /// Check if a version satisfies this constraint.
    pub fn matches(&self, version: &Version) -> bool {
        self.matches_pre(version, false)
    }

    /// Check if a version satisfies this constraint with explicit prerelease handling.
    ///
    /// # Arguments
    ///
    /// * `version` - The version to check
    /// * `include_prerelease` - If true, pre-release versions are considered for matching
    ///   even when the constraint doesn't specify a pre-release
    ///
    /// # Semver Prerelease Rules
    ///
    /// By default (include_prerelease=false):
    /// - Pre-release versions are excluded from ranges unless the constraint version
    ///   is also a pre-release
    /// - Wildcard `*` includes pre-releases
    /// - Exact constraints include pre-releases if they match exactly
    ///
    /// When include_prerelease=true:
    /// - Pre-release versions are considered for all constraint types
    pub fn matches_pre(&self, version: &Version, include_prerelease: bool) -> bool {
        match self {
            Constraint::Exact(v) => version == v,

            Constraint::Caret(v) => {
                // ^1.2.3 := >=1.2.3 <2.0.0
                // ^0.2.3 := >=0.2.3 <0.3.0
                // ^0.0.3 := >=0.0.3 <0.0.4
                // ^0.0.0 := >=0.0.0 <0.0.1

                // Pre-release handling per semver spec:
                // Pre-release versions are excluded from ranges unless the constraint
                // version is also a pre-release (and include_prerelease is false)
                if version.is_prerelease() && !v.is_prerelease() && !include_prerelease {
                    return false;
                }

                if v.major > 0 {
                    version.major == v.major && version >= v
                } else if v.minor > 0 {
                    version.major == 0 && version.minor == v.minor && version.patch >= v.patch
                } else {
                    version.major == 0 && version.minor == 0 && version.patch == v.patch
                }
            }

            Constraint::Tilde(v) => {
                // ~1.2.3 := >=1.2.3 <1.3.0
                // Pre-release handling similar to caret
                if version.is_prerelease() && !v.is_prerelease() && !include_prerelease {
                    return version.base() == v.base() && version >= v;
                }

                version.major == v.major && version.minor == v.minor && version.patch >= v.patch
            }

            Constraint::GreaterThan(v, inclusive) => {
                if *inclusive {
                    version >= v
                } else {
                    version > v
                }
            }

            Constraint::LessThan(v, inclusive) => {
                if *inclusive {
                    version <= v
                } else {
                    version < v
                }
            }

            Constraint::Range {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => {
                // Pre-release handling for ranges
                if version.is_prerelease()
                    && !min.is_prerelease()
                    && !max.is_prerelease()
                    && !include_prerelease
                {
                    return false;
                }

                let min_ok = if *min_inclusive {
                    version >= min
                } else {
                    version > min
                };
                let max_ok = if *max_inclusive {
                    version <= max
                } else {
                    version < max
                };
                min_ok && max_ok
            }

            Constraint::Wildcard { major, minor } => {
                match (major, minor) {
                    (None, None) => true, // * - includes pre-releases
                    (Some(maj), None) => {
                        // 1.x or 1.*
                        if version.is_prerelease() && !include_prerelease {
                            return false;
                        }
                        version.major == *maj
                    }
                    (Some(maj), Some(min)) => {
                        // 1.2.*
                        if version.is_prerelease() && !include_prerelease {
                            return false;
                        }
                        version.major == *maj && version.minor == *min
                    }
                    (None, Some(_)) => unreachable!(), // Invalid state
                }
            }

            Constraint::Or(constraints) => constraints
                .iter()
                .any(|c| c.matches_pre(version, include_prerelease)),

            Constraint::And(constraints) => constraints
                .iter()
                .all(|c| c.matches_pre(version, include_prerelease)),

            Constraint::Any => true,

            Constraint::None => false,
        }
    }

    /// Check if two constraints can both be satisfied by some version.
    /// This is useful for detecting conflicts early in resolution.
    ///
    /// Returns true if there exists at least one version that satisfies both constraints.
    pub fn is_compatible(&self, other: &Constraint) -> bool {
        // Handle special cases
        match (self, other) {
            (Constraint::Any, _) | (_, Constraint::Any) => return true,
            (Constraint::None, _) | (_, Constraint::None) => return false,
            _ => {}
        }

        // For OR constraints, at least one branch must be compatible
        if let Constraint::Or(branches) = self {
            return branches.iter().any(|b| b.is_compatible(other));
        }
        if let Constraint::Or(branches) = other {
            return branches.iter().any(|b| b.is_compatible(self));
        }

        // For AND constraints, all branches must be compatible
        if let Constraint::And(branches) = self {
            return branches.iter().all(|b| b.is_compatible(other));
        }
        if let Constraint::And(branches) = other {
            return branches.iter().all(|b| b.is_compatible(self));
        }

        // Compare version bounds
        let self_bounds = self.version_bounds();
        let other_bounds = other.version_bounds();

        // Check if bounds overlap
        self_bounds.overlaps(&other_bounds)
    }

    /// Get the version bounds represented by this constraint.
    fn version_bounds(&self) -> VersionBounds {
        match self {
            Constraint::Exact(v) => VersionBounds {
                min: Some(v.clone()),
                max: Some(v.clone()),
                min_inclusive: true,
                max_inclusive: true,
            },
            Constraint::Caret(v) => {
                if v.major > 0 {
                    VersionBounds {
                        min: Some(v.clone()),
                        max: Some(Version::new(v.major + 1, 0, 0)),
                        min_inclusive: true,
                        max_inclusive: false,
                    }
                } else if v.minor > 0 {
                    VersionBounds {
                        min: Some(v.clone()),
                        max: Some(Version::new(0, v.minor + 1, 0)),
                        min_inclusive: true,
                        max_inclusive: false,
                    }
                } else {
                    VersionBounds {
                        min: Some(v.clone()),
                        max: Some(Version::new(0, 0, v.patch + 1)),
                        min_inclusive: true,
                        max_inclusive: false,
                    }
                }
            }
            Constraint::Tilde(v) => VersionBounds {
                min: Some(v.clone()),
                max: Some(Version::new(v.major, v.minor + 1, 0)),
                min_inclusive: true,
                max_inclusive: false,
            },
            Constraint::GreaterThan(v, inclusive) => VersionBounds {
                min: Some(v.clone()),
                max: None,
                min_inclusive: *inclusive,
                max_inclusive: false,
            },
            Constraint::LessThan(v, inclusive) => VersionBounds {
                min: None,
                max: Some(v.clone()),
                min_inclusive: false,
                max_inclusive: *inclusive,
            },
            Constraint::Range {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => VersionBounds {
                min: Some(min.clone()),
                max: Some(max.clone()),
                min_inclusive: *min_inclusive,
                max_inclusive: *max_inclusive,
            },
            Constraint::Wildcard { major, minor } => match (major, minor) {
                (None, None) => VersionBounds::unbounded(),
                (Some(maj), None) => VersionBounds {
                    min: Some(Version::new(*maj, 0, 0)),
                    max: Some(Version::new(maj + 1, 0, 0)),
                    min_inclusive: true,
                    max_inclusive: false,
                },
                (Some(maj), Some(min)) => VersionBounds {
                    min: Some(Version::new(*maj, *min, 0)),
                    max: Some(Version::new(*maj, min + 1, 0)),
                    min_inclusive: true,
                    max_inclusive: false,
                },
                (None, Some(_)) => VersionBounds::unbounded(),
            },
            Constraint::Or(_) | Constraint::And(_) => {
                // Should have been handled above
                VersionBounds::unbounded()
            }
            Constraint::Any => VersionBounds::unbounded(),
            Constraint::None => VersionBounds::empty(),
        }
    }

    /// Parse a constraint from a string.
    pub fn parse(s: &str) -> Result<Self, SemverError> {
        let s = s.trim();

        if s.is_empty() {
            return Err(SemverError::InvalidConstraint(
                "Empty constraint".to_string(),
            ));
        }

        // Handle OR constraints (||)
        if s.contains("||") {
            let parts: Vec<&str> = s.split("||").collect();
            let constraints: Result<Vec<_>, _> =
                parts.iter().map(|p| Constraint::parse(p.trim())).collect();
            return Ok(Constraint::Or(constraints?));
        }

        // Handle space-separated AND constraints (but not inside a single operator)
        // Check if it's a range like ">=1.0.0 <2.0.0"
        let tokens = tokenize_constraint(s);
        if tokens.len() > 1 {
            // Multiple constraints ANDed together
            let constraints: Result<Vec<_>, _> =
                tokens.iter().map(|t| parse_single_constraint(t)).collect();
            let constraints = constraints?;

            // Optimize ranges into a single Range constraint
            if constraints.len() == 2 {
                let range_opt = match (&constraints[0], &constraints[1]) {
                    (Constraint::GreaterThan(min, min_inc), Constraint::LessThan(max, max_inc)) => {
                        Some((min.clone(), *min_inc, max.clone(), *max_inc))
                    }
                    (Constraint::LessThan(max, max_inc), Constraint::GreaterThan(min, min_inc)) => {
                        Some((min.clone(), *min_inc, max.clone(), *max_inc))
                    }
                    _ => None,
                };

                if let Some((min, min_inclusive, max, max_inclusive)) = range_opt {
                    return Ok(Constraint::Range {
                        min,
                        max,
                        min_inclusive,
                        max_inclusive,
                    });
                }
            }

            return Ok(Constraint::And(constraints));
        }

        parse_single_constraint(s)
    }

    /// Create an exact version constraint.
    pub fn exact(version: Version) -> Self {
        Constraint::Exact(version)
    }

    /// Create a caret constraint.
    pub fn caret(version: Version) -> Self {
        Constraint::Caret(version)
    }

    /// Create a tilde constraint.
    pub fn tilde(version: Version) -> Self {
        Constraint::Tilde(version)
    }

    /// Create a range constraint.
    pub fn range(min: Version, max: Version, min_inclusive: bool, max_inclusive: bool) -> Self {
        Constraint::Range {
            min,
            max,
            min_inclusive,
            max_inclusive,
        }
    }

    /// Create a wildcard constraint.
    pub fn wildcard(major: Option<u64>, minor: Option<u64>) -> Self {
        Constraint::Wildcard { major, minor }
    }

    /// Create an "any" constraint.
    pub fn any() -> Self {
        Constraint::Any
    }

    /// Create a "none" constraint.
    pub fn none() -> Self {
        Constraint::None
    }

    /// Filter a list of versions, returning only those that satisfy this constraint.
    pub fn filter<'a, I>(&self, versions: I) -> Vec<Version>
    where
        I: IntoIterator<Item = &'a Version>,
    {
        versions
            .into_iter()
            .filter(|v| self.matches(v))
            .cloned()
            .collect()
    }

    /// Find the best (highest) version that satisfies this constraint.
    pub fn find_best<'a, I>(&self, versions: I) -> Option<Version>
    where
        I: IntoIterator<Item = &'a Version>,
    {
        versions
            .into_iter()
            .filter(|v| self.matches(v))
            .max()
            .cloned()
    }

    /// Find the best (highest) version that satisfies this constraint with prerelease handling.
    pub fn find_best_with_prerelease<'a, I>(
        &self,
        versions: I,
        include_prerelease: bool,
    ) -> Option<Version>
    where
        I: IntoIterator<Item = &'a Version>,
    {
        versions
            .into_iter()
            .filter(|v| self.matches_pre(v, include_prerelease))
            .max()
            .cloned()
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Exact(v) => write!(f, "={}", v),
            Constraint::Caret(v) => write!(f, "^{}", v),
            Constraint::Tilde(v) => write!(f, "~{}", v),
            Constraint::GreaterThan(v, true) => write!(f, ">={}", v),
            Constraint::GreaterThan(v, false) => write!(f, ">{}", v),
            Constraint::LessThan(v, true) => write!(f, "<={}", v),
            Constraint::LessThan(v, false) => write!(f, "<{}", v),
            Constraint::Range {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => {
                let min_op = if *min_inclusive { ">=" } else { ">" };
                let max_op = if *max_inclusive { "<=" } else { "<" };
                write!(f, "{}{} {}{}", min_op, min, max_op, max)
            }
            Constraint::Wildcard { major, minor } => match (major, minor) {
                (None, None) => write!(f, "*"),
                (Some(m), None) => write!(f, "{}.*", m),
                (Some(m), Some(mi)) => write!(f, "{}.{}.*", m, mi),
                (None, Some(_)) => write!(f, "*"),
            },
            Constraint::Or(constraints) => {
                let parts: Vec<String> = constraints.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", parts.join(" || "))
            }
            Constraint::And(constraints) => {
                let parts: Vec<String> = constraints.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", parts.join(" "))
            }
            Constraint::Any => write!(f, "*"),
            Constraint::None => write!(f, "none"),
        }
    }
}

/// Version bounds for constraint compatibility checking.
#[derive(Debug, Clone)]
struct VersionBounds {
    min: Option<Version>,
    max: Option<Version>,
    min_inclusive: bool,
    max_inclusive: bool,
}

impl VersionBounds {
    fn unbounded() -> Self {
        Self {
            min: None,
            max: None,
            min_inclusive: false,
            max_inclusive: false,
        }
    }

    fn empty() -> Self {
        Self {
            min: Some(Version::new(0, 0, 0)),
            max: Some(Version::new(0, 0, 0)),
            min_inclusive: false,
            max_inclusive: false,
        }
    }

    fn overlaps(&self, other: &VersionBounds) -> bool {
        // Check if there's any version that satisfies both bounds

        // Determine the effective minimum
        let (min, min_inclusive) = match (&self.min, &other.min) {
            (None, None) => (None, true),
            (Some(v), None) => (Some(v.clone()), self.min_inclusive),
            (None, Some(v)) => (Some(v.clone()), other.min_inclusive),
            (Some(a), Some(b)) => match a.cmp(b) {
                Ordering::Greater => (Some(a.clone()), self.min_inclusive),
                Ordering::Less => (Some(b.clone()), other.min_inclusive),
                Ordering::Equal => (Some(a.clone()), self.min_inclusive && other.min_inclusive),
            },
        };

        // Determine the effective maximum
        let (max, max_inclusive) = match (&self.max, &other.max) {
            (None, None) => (None, true),
            (Some(v), None) => (Some(v.clone()), self.max_inclusive),
            (None, Some(v)) => (Some(v.clone()), other.max_inclusive),
            (Some(a), Some(b)) => match a.cmp(b) {
                Ordering::Less => (Some(a.clone()), self.max_inclusive),
                Ordering::Greater => (Some(b.clone()), other.max_inclusive),
                Ordering::Equal => (Some(a.clone()), self.max_inclusive && other.max_inclusive),
            },
        };

        // Check if bounds are compatible
        match (&min, &max) {
            (Some(min_v), Some(max_v)) => match min_v.cmp(max_v) {
                Ordering::Less => true,
                Ordering::Equal => min_inclusive && max_inclusive,
                Ordering::Greater => false,
            },
            _ => true, // At least one bound is unbounded, so they overlap
        }
    }
}

/// Tokenize a constraint string into individual constraint tokens.
fn tokenize_constraint(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == ' ' {
            if !current.is_empty() {
                // Check if the next token starts with an operator
                let remaining: String = chars.clone().collect();
                if remaining.starts_with('<')
                    || remaining.starts_with('>')
                    || remaining.starts_with('=')
                    || remaining.starts_with('~')
                    || remaining.starts_with('^')
                {
                    tokens.push(current.clone());
                    current.clear();
                } else {
                    current.push(c);
                }
            }
        } else if c == '<' || c == '>' || c == '=' || c == '~' || c == '^' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(c);
            // Check for >=, <=
            if let Some(&next) = chars.peek() {
                if next == '=' && (c == '<' || c == '>' || c == '=') {
                    current.push(chars.next().unwrap());
                }
            }
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    // Filter out empty tokens and join operators with their versions
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].trim();
        if token.is_empty() {
            i += 1;
            continue;
        }

        // If this is an operator and the next token is a version, combine them
        if (token == ">=" || token == ">" || token == "<=" || token == "<" || token == "=")
            && i + 1 < tokens.len()
        {
            let combined = format!("{}{}", token, tokens[i + 1].trim());
            result.push(combined);
            i += 2;
        } else {
            result.push(token.to_string());
            i += 1;
        }
    }

    result
}

/// Parse a single constraint (not compound).
fn parse_single_constraint(s: &str) -> Result<Constraint, SemverError> {
    let s = s.trim();

    if s == "*" || s == "x" || s == "X" {
        return Ok(Constraint::Wildcard {
            major: None,
            minor: None,
        });
    }

    // Handle wildcard patterns like 1.x, 1.*, 1.2.x, 1.2.*
    if let Some(wildcard) = parse_wildcard(s) {
        return Ok(wildcard);
    }

    // Handle caret constraint
    if let Some(rest) = s.strip_prefix('^') {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::Caret(version));
    }

    // Handle tilde constraint
    if let Some(rest) = s.strip_prefix('~') {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::Tilde(version));
    }

    // Handle exact version with =
    if let Some(rest) = s.strip_prefix('=') {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::Exact(version));
    }

    // Handle >= and <=
    if let Some(rest) = s.strip_prefix(">=") {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::GreaterThan(version, true));
    }
    if let Some(rest) = s.strip_prefix("<=") {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::LessThan(version, true));
    }

    // Handle > and <
    if let Some(rest) = s.strip_prefix('>') {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::GreaterThan(version, false));
    }
    if let Some(rest) = s.strip_prefix('<') {
        let version = Version::from_str(rest.trim())?;
        return Ok(Constraint::LessThan(version, false));
    }

    // Plain version is treated as exact match
    let version = Version::from_str(s)?;
    Ok(Constraint::Exact(version))
}

/// Parse a wildcard pattern like 1.x, 1.*, 1.2.x, 1.2.*
fn parse_wildcard(s: &str) -> Option<Constraint> {
    let s = s.trim();

    // Handle 1.2.x, 1.2.*, 1.2.X
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 3 {
        let major = parts[0].parse::<u64>().ok()?;
        let minor = parts[1].parse::<u64>().ok()?;
        if parts[2] == "*" || parts[2] == "x" || parts[2] == "X" {
            return Some(Constraint::Wildcard {
                major: Some(major),
                minor: Some(minor),
            });
        }
    }

    // Handle 1.x, 1.*, 1.X
    if parts.len() == 2 {
        let major = parts[0].parse::<u64>().ok()?;
        if parts[1] == "*" || parts[1] == "x" || parts[1] == "X" {
            return Some(Constraint::Wildcard {
                major: Some(major),
                minor: None,
            });
        }
    }

    None
}

/// Errors that can occur during semver parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemverError {
    /// Invalid version string
    InvalidVersion(String),
    /// Leading zero in version number
    LeadingZero(String),
    /// Invalid pre-release identifier
    InvalidIdentifier(String),
    /// Invalid build metadata
    InvalidBuildMetadata(String),
    /// Invalid constraint
    InvalidConstraint(String),
}

impl fmt::Display for SemverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemverError::InvalidVersion(s) => write!(f, "Invalid version: {}", s),
            SemverError::LeadingZero(s) => write!(f, "Leading zero not allowed: {}", s),
            SemverError::InvalidIdentifier(s) => {
                write!(f, "Invalid pre-release identifier: {}", s)
            }
            SemverError::InvalidBuildMetadata(s) => write!(f, "Invalid build metadata: {}", s),
            SemverError::InvalidConstraint(s) => write!(f, "Invalid constraint: {}", s),
        }
    }
}

impl std::error::Error for SemverError {}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Version Parsing Tests ====================

    #[test]
    fn test_parse_simple_version() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_empty());
        assert!(v.build.is_empty());
    }

    #[test]
    fn test_parse_version_with_prerelease() {
        let v: Version = "1.2.3-alpha.1".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre.len(), 2);
        assert_eq!(v.pre[0], PrereleaseIdentifier::Alpha("alpha".to_string()));
        assert_eq!(v.pre[1], PrereleaseIdentifier::Numeric(1));
    }

    #[test]
    fn test_parse_version_with_build() {
        let v: Version = "1.2.3+build.123".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.build, vec!["build", "123"]);
    }

    #[test]
    fn test_parse_version_with_prerelease_and_build() {
        let v: Version = "1.2.3-alpha.1+build.123".parse().unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre.len(), 2);
        assert_eq!(v.build, vec!["build", "123"]);
    }

    #[test]
    fn test_parse_prerelease_identifiers() {
        // Numeric
        let v: Version = "1.0.0-1".parse().unwrap();
        assert_eq!(v.pre[0], PrereleaseIdentifier::Numeric(1));

        // Alpha
        let v: Version = "1.0.0-alpha".parse().unwrap();
        assert_eq!(v.pre[0], PrereleaseIdentifier::Alpha("alpha".to_string()));

        // Mixed
        let v: Version = "1.0.0-alpha.1.beta.2".parse().unwrap();
        assert_eq!(v.pre.len(), 4);
        assert_eq!(v.pre[0], PrereleaseIdentifier::Alpha("alpha".to_string()));
        assert_eq!(v.pre[1], PrereleaseIdentifier::Numeric(1));
        assert_eq!(v.pre[2], PrereleaseIdentifier::Alpha("beta".to_string()));
        assert_eq!(v.pre[3], PrereleaseIdentifier::Numeric(2));
    }

    #[test]
    fn test_parse_rejects_leading_zeros() {
        assert!(matches!(
            "01.0.0".parse::<Version>(),
            Err(SemverError::LeadingZero(_))
        ));
        assert!(matches!(
            "1.01.0".parse::<Version>(),
            Err(SemverError::LeadingZero(_))
        ));
        assert!(matches!(
            "1.0.01".parse::<Version>(),
            Err(SemverError::LeadingZero(_))
        ));
        assert!(matches!(
            "1.0.0-01".parse::<Version>(),
            Err(SemverError::LeadingZero(_))
        ));
    }

    #[test]
    fn test_parse_allows_zero() {
        let v: Version = "0.0.0".parse().unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_rejects_invalid_identifiers() {
        assert!(matches!(
            "1.0.0-".parse::<Version>(),
            Err(SemverError::InvalidIdentifier(_))
        ));
        assert!(matches!(
            "1.0.0-@".parse::<Version>(),
            Err(SemverError::InvalidIdentifier(_))
        ));
        assert!(matches!(
            "1.0.0-#".parse::<Version>(),
            Err(SemverError::InvalidIdentifier(_))
        ));
    }

    #[test]
    fn test_parse_large_versions() {
        let v: Version = "18446744073709551615.0.0".parse().unwrap();
        assert_eq!(v.major, u64::MAX);
    }

    // ==================== Version Comparison Tests ====================

    #[test]
    fn test_version_equality() {
        let v1: Version = "1.2.3".parse().unwrap();
        let v2: Version = "1.2.3".parse().unwrap();
        assert_eq!(v1, v2);

        let v3: Version = "1.2.4".parse().unwrap();
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_version_ordering() {
        let v1_0_0: Version = "1.0.0".parse().unwrap();
        let v1_0_1: Version = "1.0.1".parse().unwrap();
        let v1_1_0: Version = "1.1.0".parse().unwrap();
        let v2_0_0: Version = "2.0.0".parse().unwrap();

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_1_0);
        assert!(v1_1_0 < v2_0_0);
    }

    #[test]
    fn test_prerelease_ordering() {
        let v_alpha: Version = "1.0.0-alpha".parse().unwrap();
        let v_beta: Version = "1.0.0-beta".parse().unwrap();
        let v_release: Version = "1.0.0".parse().unwrap();

        assert!(v_alpha < v_beta);
        assert!(v_beta < v_release);
    }

    #[test]
    fn test_prerelease_numeric_ordering() {
        let v_1: Version = "1.0.0-alpha.1".parse().unwrap();
        let v_2: Version = "1.0.0-alpha.2".parse().unwrap();
        let v_10: Version = "1.0.0-alpha.10".parse().unwrap();

        assert!(v_1 < v_2);
        assert!(v_2 < v_10);
    }

    #[test]
    fn test_prerelease_numeric_vs_alpha() {
        // Numeric identifiers have lower precedence than alphanumeric
        let v_1: Version = "1.0.0-1".parse().unwrap();
        let v_alpha: Version = "1.0.0-alpha".parse().unwrap();

        assert!(v_1 < v_alpha);
    }

    #[test]
    fn test_prerelease_different_lengths() {
        let v_short: Version = "1.0.0-alpha".parse().unwrap();
        let v_long: Version = "1.0.0-alpha.1".parse().unwrap();

        assert!(v_short < v_long);
    }

    #[test]
    fn test_build_metadata_no_effect_on_ordering() {
        // Per semver spec: build metadata does not affect precedence
        let v1: Version = "1.0.0+build.1".parse().unwrap();
        let v2: Version = "1.0.0+build.2".parse().unwrap();
        let v3: Version = "1.0.0".parse().unwrap();

        // Ordering should be equal (neither is greater or less)
        assert_eq!(v1.cmp(&v2), Ordering::Equal);
        assert_eq!(v1.cmp(&v3), Ordering::Equal);

        // But equality considers all fields including build metadata
        assert_ne!(v1, v2); // Different build metadata means not equal
        assert_ne!(v1, v3); // Has build vs no build
    }

    #[test]
    fn test_version_display() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v.to_string(), "1.2.3");

        let v: Version = "1.2.3-alpha.1".parse().unwrap();
        assert_eq!(v.to_string(), "1.2.3-alpha.1");

        let v: Version = "1.2.3+build.123".parse().unwrap();
        assert_eq!(v.to_string(), "1.2.3+build.123");

        let v: Version = "1.2.3-alpha.1+build.123".parse().unwrap();
        assert_eq!(v.to_string(), "1.2.3-alpha.1+build.123");
    }

    // ==================== Constraint Parsing Tests ====================

    #[test]
    fn test_parse_exact_constraint() {
        let c = Constraint::parse("=1.2.3").unwrap();
        assert_eq!(c, Constraint::Exact("1.2.3".parse().unwrap()));

        // Plain version is also exact
        let c = Constraint::parse("1.2.3").unwrap();
        assert_eq!(c, Constraint::Exact("1.2.3".parse().unwrap()));
    }

    #[test]
    fn test_parse_caret_constraint() {
        let c = Constraint::parse("^1.2.3").unwrap();
        assert_eq!(c, Constraint::Caret("1.2.3".parse().unwrap()));
    }

    #[test]
    fn test_parse_tilde_constraint() {
        let c = Constraint::parse("~1.2.3").unwrap();
        assert_eq!(c, Constraint::Tilde("1.2.3".parse().unwrap()));
    }

    #[test]
    fn test_parse_comparison_constraints() {
        let c = Constraint::parse(">1.2.3").unwrap();
        assert_eq!(c, Constraint::GreaterThan("1.2.3".parse().unwrap(), false));

        let c = Constraint::parse(">=1.2.3").unwrap();
        assert_eq!(c, Constraint::GreaterThan("1.2.3".parse().unwrap(), true));

        let c = Constraint::parse("<1.2.3").unwrap();
        assert_eq!(c, Constraint::LessThan("1.2.3".parse().unwrap(), false));

        let c = Constraint::parse("<=1.2.3").unwrap();
        assert_eq!(c, Constraint::LessThan("1.2.3".parse().unwrap(), true));
    }

    #[test]
    fn test_parse_wildcard_constraints() {
        let c = Constraint::parse("*").unwrap();
        assert_eq!(
            c,
            Constraint::Wildcard {
                major: None,
                minor: None
            }
        );

        let c = Constraint::parse("1.x").unwrap();
        assert_eq!(
            c,
            Constraint::Wildcard {
                major: Some(1),
                minor: None
            }
        );

        let c = Constraint::parse("1.*").unwrap();
        assert_eq!(
            c,
            Constraint::Wildcard {
                major: Some(1),
                minor: None
            }
        );

        let c = Constraint::parse("1.2.x").unwrap();
        assert_eq!(
            c,
            Constraint::Wildcard {
                major: Some(1),
                minor: Some(2)
            }
        );

        let c = Constraint::parse("1.2.*").unwrap();
        assert_eq!(
            c,
            Constraint::Wildcard {
                major: Some(1),
                minor: Some(2)
            }
        );
    }

    #[test]
    fn test_parse_range_constraint() {
        let c = Constraint::parse(">=1.2.3 <2.0.0").unwrap();
        match c {
            Constraint::Range {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => {
                assert_eq!(min, "1.2.3".parse::<Version>().unwrap());
                assert_eq!(max, "2.0.0".parse::<Version>().unwrap());
                assert!(min_inclusive);
                assert!(!max_inclusive);
            }
            _ => panic!("Expected Range constraint"),
        }
    }

    #[test]
    fn test_parse_or_constraint() {
        let c = Constraint::parse("^1.2.3 || ^2.0.0").unwrap();
        match c {
            Constraint::Or(constraints) => {
                assert_eq!(constraints.len(), 2);
                assert_eq!(constraints[0], Constraint::Caret("1.2.3".parse().unwrap()));
                assert_eq!(constraints[1], Constraint::Caret("2.0.0".parse().unwrap()));
            }
            _ => panic!("Expected Or constraint"),
        }
    }

    // ==================== Constraint Matching Tests ====================

    #[test]
    fn test_exact_matches() {
        let c = Constraint::parse("=1.2.3").unwrap();

        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(!c.matches(&"1.2.4".parse().unwrap()));
        assert!(!c.matches(&"1.3.0".parse().unwrap()));
        assert!(!c.matches(&"2.0.0".parse().unwrap()));
    }

    #[test]
    fn test_caret_matches() {
        let c = Constraint::parse("^1.2.3").unwrap();

        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"1.2.4".parse().unwrap()));
        assert!(c.matches(&"1.3.0".parse().unwrap()));
        assert!(!c.matches(&"2.0.0".parse().unwrap()));
        assert!(!c.matches(&"1.2.2".parse().unwrap()));
        assert!(!c.matches(&"0.9.0".parse().unwrap()));
    }

    #[test]
    fn test_caret_zero_version() {
        // ^0.0.3 matches only 0.0.3
        let c = Constraint::parse("^0.0.3").unwrap();
        assert!(c.matches(&"0.0.3".parse().unwrap()));
        assert!(!c.matches(&"0.0.4".parse().unwrap()));
        assert!(!c.matches(&"0.1.0".parse().unwrap()));

        // ^0.2.3 matches 0.2.x where x >= 3
        let c = Constraint::parse("^0.2.3").unwrap();
        assert!(c.matches(&"0.2.3".parse().unwrap()));
        assert!(c.matches(&"0.2.4".parse().unwrap()));
        assert!(!c.matches(&"0.3.0".parse().unwrap()));
        assert!(!c.matches(&"0.2.2".parse().unwrap()));
    }

    #[test]
    fn test_tilde_matches() {
        let c = Constraint::parse("~1.2.3").unwrap();

        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"1.2.4".parse().unwrap()));
        assert!(c.matches(&"1.2.100".parse().unwrap()));
        assert!(!c.matches(&"1.3.0".parse().unwrap()));
        assert!(!c.matches(&"1.2.2".parse().unwrap()));
    }

    #[test]
    fn test_comparison_matches() {
        let gt = Constraint::parse(">1.2.3").unwrap();
        assert!(!gt.matches(&"1.2.3".parse().unwrap()));
        assert!(gt.matches(&"1.2.4".parse().unwrap()));

        let gte = Constraint::parse(">=1.2.3").unwrap();
        assert!(gte.matches(&"1.2.3".parse().unwrap()));
        assert!(gte.matches(&"1.2.4".parse().unwrap()));

        let lt = Constraint::parse("<1.2.3").unwrap();
        assert!(!lt.matches(&"1.2.3".parse().unwrap()));
        assert!(lt.matches(&"1.2.2".parse().unwrap()));

        let lte = Constraint::parse("<=1.2.3").unwrap();
        assert!(lte.matches(&"1.2.3".parse().unwrap()));
        assert!(lte.matches(&"1.2.2".parse().unwrap()));
    }

    #[test]
    fn test_range_matches() {
        let c = Constraint::parse(">=1.2.3 <2.0.0").unwrap();

        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"1.5.0".parse().unwrap()));
        assert!(c.matches(&"1.9.9".parse().unwrap()));
        assert!(!c.matches(&"1.2.2".parse().unwrap()));
        assert!(!c.matches(&"2.0.0".parse().unwrap()));
    }

    #[test]
    fn test_wildcard_matches() {
        let any = Constraint::parse("*").unwrap();
        assert!(any.matches(&"1.2.3".parse().unwrap()));
        assert!(any.matches(&"100.0.0".parse().unwrap()));

        let major = Constraint::parse("1.*").unwrap();
        assert!(major.matches(&"1.0.0".parse().unwrap()));
        assert!(major.matches(&"1.9.9".parse().unwrap()));
        assert!(!major.matches(&"2.0.0".parse().unwrap()));

        let minor = Constraint::parse("1.2.*").unwrap();
        assert!(minor.matches(&"1.2.0".parse().unwrap()));
        assert!(minor.matches(&"1.2.99".parse().unwrap()));
        assert!(!minor.matches(&"1.3.0".parse().unwrap()));
    }

    #[test]
    fn test_or_matches() {
        let c = Constraint::parse("^1.2.3 || ^2.0.0").unwrap();

        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"1.9.0".parse().unwrap()));
        assert!(c.matches(&"2.0.0".parse().unwrap()));
        assert!(c.matches(&"2.5.0".parse().unwrap()));
        assert!(!c.matches(&"3.0.0".parse().unwrap()));
        assert!(!c.matches(&"1.2.2".parse().unwrap()));
    }

    #[test]
    fn test_and_matches() {
        let c = Constraint::parse(">=1.0.0 <2.0.0").unwrap();

        assert!(c.matches(&"1.5.0".parse().unwrap()));
        assert!(!c.matches(&"0.9.0".parse().unwrap()));
        assert!(!c.matches(&"2.0.0".parse().unwrap()));
    }

    // ==================== Pre-release Handling Tests ====================

    #[test]
    fn test_prerelease_excluded_from_caret() {
        let c = Constraint::parse("^1.0.0").unwrap();

        // Pre-release versions are excluded from caret ranges by default
        assert!(!c.matches(&"1.0.1-alpha".parse().unwrap()));
        assert!(!c.matches(&"1.1.0-alpha".parse().unwrap()));

        // But release versions match
        assert!(c.matches(&"1.0.0".parse().unwrap()));
        assert!(c.matches(&"1.0.1".parse().unwrap()));
        assert!(c.matches(&"1.1.0".parse().unwrap()));
    }

    #[test]
    fn test_prerelease_matches_with_include_flag() {
        let c = Constraint::parse("^1.0.0").unwrap();

        // Without include_prerelease flag
        assert!(!c.matches_pre(&"1.0.1-alpha".parse().unwrap(), false));

        // With include_prerelease flag
        assert!(c.matches_pre(&"1.0.1-alpha".parse().unwrap(), true));
    }

    #[test]
    fn test_prerelease_excluded_from_tilde() {
        let c = Constraint::parse("~1.2.0").unwrap();

        assert!(!c.matches(&"1.2.1-alpha".parse().unwrap()));
        assert!(c.matches(&"1.2.1".parse().unwrap()));
    }

    #[test]
    fn test_prerelease_excluded_from_wildcard() {
        let c = Constraint::parse("1.*").unwrap();
        assert!(!c.matches(&"1.0.0-alpha".parse().unwrap()));

        let c = Constraint::parse("1.2.*").unwrap();
        assert!(!c.matches(&"1.2.0-alpha".parse().unwrap()));
    }

    #[test]
    fn test_prerelease_matches_prerelease_constraint() {
        // If the constraint is a pre-release, pre-release versions can match
        let c = Constraint::parse(">=1.0.0-alpha <2.0.0").unwrap();

        assert!(c.matches(&"1.0.0-alpha".parse().unwrap()));
        assert!(c.matches(&"1.0.0-beta".parse().unwrap()));
        assert!(c.matches(&"1.0.0".parse().unwrap()));
    }

    #[test]
    fn test_prerelease_ordering_complete() {
        let versions: Vec<Version> = vec![
            "1.0.0-alpha".parse().unwrap(),
            "1.0.0-alpha.1".parse().unwrap(),
            "1.0.0-alpha.beta".parse().unwrap(),
            "1.0.0-beta".parse().unwrap(),
            "1.0.0-beta.2".parse().unwrap(),
            "1.0.0-beta.11".parse().unwrap(),
            "1.0.0-rc.1".parse().unwrap(),
            "1.0.0".parse().unwrap(),
        ];

        // Verify ordering
        for i in 0..versions.len() - 1 {
            assert!(
                versions[i] < versions[i + 1],
                "{} should be < {}",
                versions[i],
                versions[i + 1]
            );
        }
    }

    // ==================== Constraint Compatibility Tests ====================

    #[test]
    fn test_compatible_exact() {
        let c1 = Constraint::parse("=1.2.3").unwrap();
        let c2 = Constraint::parse("=1.2.3").unwrap();
        assert!(c1.is_compatible(&c2));

        let c3 = Constraint::parse("=1.2.4").unwrap();
        assert!(!c1.is_compatible(&c3));
    }

    #[test]
    fn test_compatible_caret_overlap() {
        let c1 = Constraint::parse("^1.2.0").unwrap();
        let c2 = Constraint::parse("^1.3.0").unwrap();
        assert!(c1.is_compatible(&c2));

        let c3 = Constraint::parse("^2.0.0").unwrap();
        assert!(!c1.is_compatible(&c3));
    }

    #[test]
    fn test_compatible_range_overlap() {
        let c1 = Constraint::parse(">=1.0.0 <2.0.0").unwrap();
        let c2 = Constraint::parse(">=1.5.0 <1.8.0").unwrap();
        assert!(c1.is_compatible(&c2));

        let c3 = Constraint::parse(">=2.0.0 <3.0.0").unwrap();
        assert!(!c1.is_compatible(&c3));
    }

    #[test]
    fn test_compatible_wildcard() {
        let c1 = Constraint::parse("1.*").unwrap();
        let c2 = Constraint::parse("^1.2.0").unwrap();
        assert!(c1.is_compatible(&c2));

        let c3 = Constraint::parse("2.*").unwrap();
        assert!(!c1.is_compatible(&c3));
    }

    #[test]
    fn test_compatible_or() {
        let c1 = Constraint::parse("^1.0.0 || ^2.0.0").unwrap();
        let c2 = Constraint::parse("^1.5.0").unwrap();
        assert!(c1.is_compatible(&c2));

        let c3 = Constraint::parse("^3.0.0").unwrap();
        assert!(!c1.is_compatible(&c3));
    }

    #[test]
    fn test_compatible_and() {
        let c1 = Constraint::parse(">=1.0.0").unwrap();
        let c2 = Constraint::parse("<2.0.0").unwrap();
        let c3 = Constraint::And(vec![c1.clone(), c2.clone()]);

        let c4 = Constraint::parse("^1.5.0").unwrap();
        assert!(c3.is_compatible(&c4));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_zero_version() {
        let v: Version = "0.0.0".parse().unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_large_version_numbers() {
        let v: Version = "999999.999999.999999".parse().unwrap();
        assert_eq!(v.major, 999999);
        assert_eq!(v.minor, 999999);
        assert_eq!(v.patch, 999999);
    }

    #[test]
    fn test_complex_build_metadata() {
        let v: Version = "1.0.0+build.123.abc-xyz".parse().unwrap();
        assert_eq!(v.build, vec!["build", "123", "abc-xyz"]);
    }

    #[test]
    fn test_complex_prerelease() {
        let v: Version = "1.0.0-alpha-123.456.beta-xyz".parse().unwrap();
        assert_eq!(v.pre.len(), 3);
        assert_eq!(
            v.pre[0],
            PrereleaseIdentifier::Alpha("alpha-123".to_string())
        );
        assert_eq!(v.pre[1], PrereleaseIdentifier::Numeric(456));
        assert_eq!(
            v.pre[2],
            PrereleaseIdentifier::Alpha("beta-xyz".to_string())
        );
    }

    #[test]
    fn test_any_constraint() {
        let c = Constraint::any();
        assert!(c.matches(&"0.0.0".parse().unwrap()));
        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"999.999.999".parse().unwrap()));

        // Any is compatible with everything
        assert!(c.is_compatible(&Constraint::parse("^1.0.0").unwrap()));
    }

    #[test]
    fn test_none_constraint() {
        let c = Constraint::none();
        assert!(!c.matches(&"0.0.0".parse().unwrap()));
        assert!(!c.matches(&"1.2.3".parse().unwrap()));

        // None is compatible with nothing
        assert!(!c.is_compatible(&Constraint::parse("^1.0.0").unwrap()));
    }

    #[test]
    fn test_constraint_display() {
        let c = Constraint::Caret("1.2.3".parse().unwrap());
        assert_eq!(c.to_string(), "^1.2.3");

        let c = Constraint::Tilde("1.2.3".parse().unwrap());
        assert_eq!(c.to_string(), "~1.2.3");

        let c = Constraint::Exact("1.2.3".parse().unwrap());
        assert_eq!(c.to_string(), "=1.2.3");

        let c = Constraint::GreaterThan("1.2.3".parse().unwrap(), true);
        assert_eq!(c.to_string(), ">=1.2.3");

        let c = Constraint::Range {
            min: "1.0.0".parse().unwrap(),
            max: "2.0.0".parse().unwrap(),
            min_inclusive: true,
            max_inclusive: false,
        };
        assert_eq!(c.to_string(), ">=1.0.0 <2.0.0");

        let c = Constraint::Or(vec![
            Constraint::Caret("1.2.3".parse().unwrap()),
            Constraint::Caret("2.0.0".parse().unwrap()),
        ]);
        assert_eq!(c.to_string(), "^1.2.3 || ^2.0.0");
    }

    #[test]
    fn test_version_base() {
        let v: Version = "1.2.3-alpha.1+build.123".parse().unwrap();
        let base = v.base();
        assert_eq!(base.to_string(), "1.2.3");
        assert!(!base.is_prerelease());
    }

    #[test]
    fn test_is_prerelease() {
        let v: Version = "1.2.3".parse().unwrap();
        assert!(!v.is_prerelease());

        let v: Version = "1.2.3-alpha".parse().unwrap();
        assert!(v.is_prerelease());

        let v: Version = "1.2.3+build".parse().unwrap();
        assert!(!v.is_prerelease());
    }

    #[test]
    fn test_complex_or_constraint() {
        let c = Constraint::parse("^1.0.0 || ^2.0.0 || ^3.0.0").unwrap();
        assert!(c.matches(&"1.5.0".parse().unwrap()));
        assert!(c.matches(&"2.5.0".parse().unwrap()));
        assert!(c.matches(&"3.5.0".parse().unwrap()));
        assert!(!c.matches(&"4.0.0".parse().unwrap()));
    }

    #[test]
    fn test_whitespace_handling() {
        let c = Constraint::parse("  >=  1.2.3  <  2.0.0  ").unwrap();
        assert!(c.matches(&"1.5.0".parse().unwrap()));
    }

    #[test]
    fn test_rc_sorting() {
        let v1: Version = "1.0.0-rc.1".parse().unwrap();
        let v2: Version = "1.0.0-rc.2".parse().unwrap();
        let v3: Version = "1.0.0-rc.10".parse().unwrap();
        let v4: Version = "1.0.0".parse().unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
    }

    #[test]
    fn test_prerelease_identifier_edge_cases() {
        // Hyphen in identifier
        let v: Version = "1.0.0-alpha-beta".parse().unwrap();
        assert_eq!(
            v.pre[0],
            PrereleaseIdentifier::Alpha("alpha-beta".to_string())
        );

        // Numeric followed by alpha
        let v: Version = "1.0.0-0.alpha".parse().unwrap();
        assert_eq!(v.pre[0], PrereleaseIdentifier::Numeric(0));
        assert_eq!(v.pre[1], PrereleaseIdentifier::Alpha("alpha".to_string()));
    }

    #[test]
    fn test_version_new() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_empty());
        assert!(v.build.is_empty());
    }

    #[test]
    fn test_version_builder() {
        let v = Version::new(1, 2, 3)
            .with_pre(vec![
                PrereleaseIdentifier::Alpha("alpha".to_string()),
                PrereleaseIdentifier::Numeric(1),
            ])
            .with_build(vec!["build".to_string(), "123".to_string()]);

        assert_eq!(v.to_string(), "1.2.3-alpha.1+build.123");
    }

    #[test]
    fn test_constraint_constructors() {
        let v = Version::new(1, 2, 3);

        assert_eq!(Constraint::exact(v.clone()), Constraint::Exact(v.clone()));
        assert_eq!(Constraint::caret(v.clone()), Constraint::Caret(v.clone()));
        assert_eq!(Constraint::tilde(v.clone()), Constraint::Tilde(v.clone()));
        assert_eq!(
            Constraint::range(v.clone(), Version::new(2, 0, 0), true, false),
            Constraint::Range {
                min: v,
                max: Version::new(2, 0, 0),
                min_inclusive: true,
                max_inclusive: false
            }
        );
        assert_eq!(
            Constraint::wildcard(Some(1), None),
            Constraint::Wildcard {
                major: Some(1),
                minor: None
            }
        );
        assert_eq!(Constraint::any(), Constraint::Any);
        assert_eq!(Constraint::none(), Constraint::None);
    }

    #[test]
    fn test_satisfies_method() {
        let v: Version = "1.2.3".parse().unwrap();
        let c = Constraint::parse("^1.0.0").unwrap();
        assert!(v.satisfies(&c));
    }

    #[test]
    fn test_exact_prerelease_match() {
        let c = Constraint::parse("=1.0.0-alpha").unwrap();
        assert!(c.matches(&"1.0.0-alpha".parse().unwrap()));
        assert!(!c.matches(&"1.0.0-beta".parse().unwrap()));
        assert!(!c.matches(&"1.0.0".parse().unwrap()));
    }

    #[test]
    fn test_inclusive_range() {
        let c = Constraint::parse(">=1.0.0 <=2.0.0").unwrap();
        assert!(c.matches(&"1.0.0".parse().unwrap()));
        assert!(c.matches(&"1.5.0".parse().unwrap()));
        assert!(c.matches(&"2.0.0".parse().unwrap()));
        assert!(!c.matches(&"0.9.0".parse().unwrap()));
        assert!(!c.matches(&"2.0.1".parse().unwrap()));
    }

    #[test]
    fn test_multiple_and_constraints() {
        // Test that we can have more than 2 constraints ANDed
        // Note: != operator is not implemented, so we test with >= and <
        let c = Constraint::parse(">=1.0.0 <2.0.0 >=1.2.0").unwrap();
        match c {
            Constraint::And(constraints) => {
                assert_eq!(constraints.len(), 3);
            }
            _ => panic!("Expected And constraint"),
        }
    }

    #[test]
    fn test_prerelease_with_same_base_as_constraint() {
        // Per semver spec: pre-releases are excluded from caret ranges by default
        // They only match if the constraint itself is a pre-release
        let c = Constraint::parse("^1.2.3").unwrap();
        // 1.2.3-alpha should NOT match ^1.2.3 (pre-release vs release constraint)
        assert!(!c.matches(&"1.2.3-alpha".parse().unwrap()));
        // 1.2.4-alpha should also not match
        assert!(!c.matches(&"1.2.4-alpha".parse().unwrap()));
        // But release versions match
        assert!(c.matches(&"1.2.3".parse().unwrap()));
        assert!(c.matches(&"1.2.4".parse().unwrap()));

        // If the constraint IS a pre-release, then pre-releases can match
        let c_pre = Constraint::parse("^1.2.3-alpha").unwrap();
        assert!(c_pre.matches(&"1.2.3-alpha".parse().unwrap()));
        assert!(c_pre.matches(&"1.2.3-beta".parse().unwrap())); // beta > alpha
        assert!(c_pre.matches(&"1.2.4".parse().unwrap())); // release
    }

    #[test]
    fn test_wildcard_any_includes_prereleases() {
        // The "*" wildcard should include pre-releases
        let c = Constraint::parse("*").unwrap();
        assert!(c.matches(&"1.0.0-alpha".parse().unwrap()));
        assert!(c.matches(&"1.0.0".parse().unwrap()));
    }

    #[test]
    fn test_find_best_with_prerelease() {
        let versions: Vec<Version> = vec![
            "1.0.0-alpha".parse().unwrap(),
            "1.0.0-beta".parse().unwrap(),
            "1.0.0".parse().unwrap(),
            "1.1.0-alpha".parse().unwrap(),
            "1.1.0".parse().unwrap(),
        ];

        let c = Constraint::parse("^1.0.0").unwrap();

        // Without prerelease, should get 1.1.0
        assert_eq!(c.find_best(&versions), Some("1.1.0".parse().unwrap()));

        // Without prerelease, 1.1.0-alpha should not be considered
        assert_eq!(
            c.find_best_with_prerelease(&versions, false),
            Some("1.1.0".parse().unwrap())
        );

        // With prerelease, 1.1.0-alpha could be found if we filtered for it
        let c_alpha = Constraint::parse(">=1.1.0-alpha").unwrap();
        assert!(c_alpha.matches_pre(&"1.1.0-alpha".parse().unwrap(), true));
    }
}
