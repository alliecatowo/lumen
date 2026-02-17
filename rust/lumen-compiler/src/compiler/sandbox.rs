//! Recursive sandbox at import — capability restriction for imported modules.
//!
//! When importing a package, the importer can restrict what the imported code
//! can access (e.g., deny network access, allow read-only filesystem). Policies
//! compose hierarchically: a child sandbox can never exceed the permissions of
//! its parent.
//!
//! ## Example Lumen Syntax
//!
//! ```text
//! import untrusted_pkg: * granting { none }
//! import data_lib: process granting { fs: read_only }
//! import analytics: track granting { network: [api.example.com] }
//! ```
//!
//! ## Design
//!
//! - [`SandboxPolicy`] describes what capabilities a module is granted.
//! - [`ImportSandbox`] links a module to its policy and optional parent,
//!   forming a chain. The **effective policy** is the intersection of the
//!   entire chain (most restrictive wins at every level).

use std::collections::BTreeSet;
use std::fmt;

// ── Capability definitions ──────────────────────────────────────────

/// A category of system capability that a module may require.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Network access (HTTP, TCP, etc.).
    Network,
    /// Filesystem access (read / write files).
    FileSystem,
    /// Process spawning / management.
    Process,
    /// Environment variable access.
    Environment,
    /// Cryptographic operations.
    Crypto,
    /// Timer / sleep / scheduling operations.
    Timer,
    /// Meta-capability representing all capabilities.
    All,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Capability::Network => write!(f, "network"),
            Capability::FileSystem => write!(f, "filesystem"),
            Capability::Process => write!(f, "process"),
            Capability::Environment => write!(f, "environment"),
            Capability::Crypto => write!(f, "crypto"),
            Capability::Timer => write!(f, "timer"),
            Capability::All => write!(f, "all"),
        }
    }
}

/// Granularity of access for a single capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityLevel {
    /// Capability is completely denied.
    None,
    /// Read-only access (e.g., filesystem reads but no writes).
    ReadOnly,
    /// Full (unrestricted) access for this capability.
    Full,
    /// Access is restricted to a specific set of targets (e.g., domain allow-list).
    Restricted(Vec<String>),
}

impl fmt::Display for CapabilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilityLevel::None => write!(f, "none"),
            CapabilityLevel::ReadOnly => write!(f, "read_only"),
            CapabilityLevel::Full => write!(f, "full"),
            CapabilityLevel::Restricted(targets) => {
                write!(f, "restricted({})", targets.join(", "))
            }
        }
    }
}

/// A single capability grant: one capability at a specific level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrant {
    /// The capability being granted.
    pub capability: Capability,
    /// The level of access granted.
    pub level: CapabilityLevel,
}

// ── SandboxPolicy ───────────────────────────────────────────────────

/// Describes what capabilities a sandboxed module is allowed to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// Individual capability grants.
    pub grants: Vec<CapabilityGrant>,
    /// When `true`, all capabilities are denied regardless of grants.
    pub deny_all: bool,
    /// When `true`, the module may use unsafe operations.
    pub allow_unsafe: bool,
}

impl SandboxPolicy {
    /// Create a policy that denies every capability.
    pub fn none() -> Self {
        SandboxPolicy {
            grants: Vec::new(),
            deny_all: true,
            allow_unsafe: false,
        }
    }

    /// Create a policy that allows every capability at full level.
    pub fn full() -> Self {
        SandboxPolicy {
            grants: vec![CapabilityGrant {
                capability: Capability::All,
                level: CapabilityLevel::Full,
            }],
            deny_all: false,
            allow_unsafe: true,
        }
    }

    /// Return a builder for constructing a policy incrementally.
    pub fn builder() -> SandboxPolicyBuilder {
        SandboxPolicyBuilder {
            grants: Vec::new(),
            denials: Vec::new(),
            allow_unsafe: false,
        }
    }

    /// Look up the grant level for a specific capability.
    ///
    /// Returns `None` (the option) when no explicit grant exists.
    fn grant_level_for(&self, cap: &Capability) -> Option<&CapabilityLevel> {
        // Exact match first, then fall back to `All`.
        self.grants
            .iter()
            .find(|g| &g.capability == cap)
            .or_else(|| self.grants.iter().find(|g| g.capability == Capability::All))
            .map(|g| &g.level)
    }
}

// ── SandboxPolicyBuilder ────────────────────────────────────────────

/// Incremental builder for [`SandboxPolicy`].
#[derive(Debug)]
pub struct SandboxPolicyBuilder {
    grants: Vec<CapabilityGrant>,
    denials: Vec<Capability>,
    allow_unsafe: bool,
}

impl SandboxPolicyBuilder {
    /// Grant a capability at the given level.
    pub fn grant(mut self, capability: Capability, level: CapabilityLevel) -> Self {
        self.grants.push(CapabilityGrant { capability, level });
        self
    }

    /// Explicitly deny a capability.
    pub fn deny(mut self, capability: Capability) -> Self {
        self.denials.push(capability);
        self
    }

    /// Set whether unsafe operations are permitted.
    pub fn allow_unsafe(mut self, allow: bool) -> Self {
        self.allow_unsafe = allow;
        self
    }

    /// Consume the builder and produce a [`SandboxPolicy`].
    pub fn build(self) -> SandboxPolicy {
        // Filter out any grants that were explicitly denied.
        let denial_set: BTreeSet<_> = self.denials.iter().collect();
        let grants: Vec<CapabilityGrant> = self
            .grants
            .into_iter()
            .filter(|g| !denial_set.contains(&g.capability))
            .collect();

        SandboxPolicy {
            grants,
            deny_all: false,
            allow_unsafe: self.allow_unsafe,
        }
    }
}

// ── Capability checking ─────────────────────────────────────────────

/// Result of checking a single capability against a policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityCheck {
    /// The capability is allowed.
    Allowed,
    /// The capability is denied.
    Denied {
        /// Human-readable capability name.
        capability: String,
        /// Reason for denial.
        reason: String,
    },
    /// The capability is allowed but restricted to certain targets.
    Restricted {
        /// Human-readable capability name.
        capability: String,
        /// The constraints that apply.
        constraints: Vec<String>,
    },
}

/// Check whether `policy` allows the given `required` capability.
pub fn check_capability(policy: &SandboxPolicy, required: &Capability) -> CapabilityCheck {
    if policy.deny_all {
        return CapabilityCheck::Denied {
            capability: required.to_string(),
            reason: "all capabilities are denied by policy".into(),
        };
    }

    match policy.grant_level_for(required) {
        Some(CapabilityLevel::Full) => CapabilityCheck::Allowed,
        Some(CapabilityLevel::ReadOnly) => CapabilityCheck::Restricted {
            capability: required.to_string(),
            constraints: vec!["read_only".into()],
        },
        Some(CapabilityLevel::Restricted(targets)) => CapabilityCheck::Restricted {
            capability: required.to_string(),
            constraints: targets.clone(),
        },
        Some(CapabilityLevel::None) => CapabilityCheck::Denied {
            capability: required.to_string(),
            reason: format!("capability '{}' is explicitly denied", required),
        },
        None => CapabilityCheck::Denied {
            capability: required.to_string(),
            reason: format!("no grant for capability '{}'", required),
        },
    }
}

// ── Tool access checking ────────────────────────────────────────────

/// A violation produced when a tool requires a capability the policy denies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityViolation {
    /// Name of the tool that was checked.
    pub tool_name: String,
    /// The capability the tool requires.
    pub required_capability: Capability,
    /// The level the policy grants for that capability.
    pub policy_level: CapabilityLevel,
    /// Human-readable explanation.
    pub message: String,
}

/// Check whether `policy` allows a tool that requires the given capabilities.
///
/// Returns one [`CapabilityViolation`] for each required capability that is
/// denied or insufficiently granted.
pub fn check_tool_access(
    policy: &SandboxPolicy,
    tool_name: &str,
    tool_capabilities: &[Capability],
) -> Vec<CapabilityViolation> {
    let mut violations = Vec::new();
    for cap in tool_capabilities {
        match check_capability(policy, cap) {
            CapabilityCheck::Allowed => {}
            CapabilityCheck::Restricted { .. } => {
                // Restricted is not a violation per se — the tool may still
                // operate within the restricted scope.
            }
            CapabilityCheck::Denied { reason, .. } => {
                let level = policy
                    .grant_level_for(cap)
                    .cloned()
                    .unwrap_or(CapabilityLevel::None);
                violations.push(CapabilityViolation {
                    tool_name: tool_name.to_string(),
                    required_capability: cap.clone(),
                    policy_level: level,
                    message: format!(
                        "tool '{}' requires {} but policy denies it: {}",
                        tool_name, cap, reason
                    ),
                });
            }
        }
    }
    violations
}

// ── Policy merging ──────────────────────────────────────────────────

/// Merge two capability levels such that the most restrictive one wins.
///
/// Ordering (most to least restrictive): `None` > `ReadOnly` > `Restricted` > `Full`.
/// When both are `Restricted`, the result is the intersection of their target
/// lists.
pub fn merge_capability_levels(
    parent: &CapabilityLevel,
    child: &CapabilityLevel,
) -> CapabilityLevel {
    match (parent, child) {
        // None always wins.
        (CapabilityLevel::None, _) | (_, CapabilityLevel::None) => CapabilityLevel::None,
        // ReadOnly beats Full and Restricted.
        (CapabilityLevel::ReadOnly, CapabilityLevel::Full)
        | (CapabilityLevel::Full, CapabilityLevel::ReadOnly) => CapabilityLevel::ReadOnly,
        (CapabilityLevel::ReadOnly, CapabilityLevel::ReadOnly) => CapabilityLevel::ReadOnly,
        // ReadOnly beats Restricted (ReadOnly is more restrictive since it
        // limits the *kind* of operation rather than just *targets*).
        (CapabilityLevel::ReadOnly, CapabilityLevel::Restricted(_))
        | (CapabilityLevel::Restricted(_), CapabilityLevel::ReadOnly) => CapabilityLevel::ReadOnly,
        // Two Restricted — intersect target lists.
        (CapabilityLevel::Restricted(a), CapabilityLevel::Restricted(b)) => {
            let set_a: BTreeSet<_> = a.iter().collect();
            let intersected: Vec<String> =
                b.iter().filter(|t| set_a.contains(t)).cloned().collect();
            if intersected.is_empty() {
                CapabilityLevel::None
            } else {
                CapabilityLevel::Restricted(intersected)
            }
        }
        // Restricted beats Full.
        (CapabilityLevel::Restricted(r), CapabilityLevel::Full) => {
            CapabilityLevel::Restricted(r.clone())
        }
        (CapabilityLevel::Full, CapabilityLevel::Restricted(r)) => {
            CapabilityLevel::Restricted(r.clone())
        }
        // Both Full.
        (CapabilityLevel::Full, CapabilityLevel::Full) => CapabilityLevel::Full,
    }
}

/// Intersect two policies so that the most restrictive grant wins for each
/// capability.
///
/// If either policy has `deny_all` set, the result denies all. `allow_unsafe`
/// is only true if both policies allow it.
pub fn intersect_policies(parent: &SandboxPolicy, child: &SandboxPolicy) -> SandboxPolicy {
    if parent.deny_all || child.deny_all {
        return SandboxPolicy::none();
    }

    // Collect all capabilities mentioned in either policy.
    let mut all_caps: BTreeSet<Capability> = BTreeSet::new();
    for g in &parent.grants {
        all_caps.insert(g.capability.clone());
    }
    for g in &child.grants {
        all_caps.insert(g.capability.clone());
    }

    let mut grants = Vec::new();
    for cap in &all_caps {
        let p_level = parent.grant_level_for(cap);
        let c_level = child.grant_level_for(cap);

        let merged = match (p_level, c_level) {
            (Some(p), Some(c)) => merge_capability_levels(p, c),
            // If one side has no grant, treat it as None (deny) — the
            // missing grant means the policy does not permit it.
            (Some(p), None) => {
                // Child doesn't grant it — child is more restrictive.
                // However, if the child has an `All` grant, it was already
                // resolved by `grant_level_for`. So reaching here means the
                // child truly omits it.
                merge_capability_levels(p, &CapabilityLevel::None)
            }
            (None, Some(c)) => merge_capability_levels(&CapabilityLevel::None, c),
            (None, None) => CapabilityLevel::None,
        };

        // Skip `None` grants to keep the list tidy.
        if merged != CapabilityLevel::None {
            grants.push(CapabilityGrant {
                capability: cap.clone(),
                level: merged,
            });
        }
    }

    SandboxPolicy {
        grants,
        deny_all: false,
        allow_unsafe: parent.allow_unsafe && child.allow_unsafe,
    }
}

// ── ImportSandbox ───────────────────────────────────────────────────

/// A sandbox scope for an imported module, optionally nested inside a parent
/// sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSandbox {
    /// The module path this sandbox applies to.
    pub module_path: String,
    /// The policy specified at the import site.
    pub policy: SandboxPolicy,
    /// Optional parent sandbox (the importer's own sandbox).
    pub parent_sandbox: Option<Box<ImportSandbox>>,
}

impl ImportSandbox {
    /// Create a new root sandbox for a module.
    pub fn new(module_path: &str, policy: SandboxPolicy) -> Self {
        ImportSandbox {
            module_path: module_path.to_string(),
            policy,
            parent_sandbox: None,
        }
    }

    /// Create a child sandbox nested under a parent.
    pub fn with_parent(module_path: &str, policy: SandboxPolicy, parent: ImportSandbox) -> Self {
        ImportSandbox {
            module_path: module_path.to_string(),
            policy,
            parent_sandbox: Some(Box::new(parent)),
        }
    }

    /// Compute the effective policy by intersecting this sandbox's policy with
    /// every ancestor's policy. The most restrictive grant wins at every level.
    pub fn effective_policy(&self) -> SandboxPolicy {
        match &self.parent_sandbox {
            None => self.policy.clone(),
            Some(parent) => {
                let parent_effective = parent.effective_policy();
                intersect_policies(&parent_effective, &self.policy)
            }
        }
    }

    /// Check whether the effective policy allows the given capability.
    pub fn can_access(&self, capability: &Capability) -> bool {
        let policy = self.effective_policy();
        matches!(
            check_capability(&policy, capability),
            CapabilityCheck::Allowed | CapabilityCheck::Restricted { .. }
        )
    }
}

// ── Error type ──────────────────────────────────────────────────────

/// Errors arising from sandbox policy enforcement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// A module attempted to use a capability that its policy denies.
    CapabilityDenied {
        /// The module that caused the violation.
        module: String,
        /// The denied capability.
        capability: String,
    },
    /// Two policies conflict in a way that cannot be resolved.
    PolicyConflict {
        /// Description of the conflict.
        message: String,
    },
    /// An invalid grant was specified in the import.
    InvalidGrant(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::CapabilityDenied { module, capability } => {
                write!(
                    f,
                    "capability '{}' denied for module '{}'",
                    capability, module
                )
            }
            SandboxError::PolicyConflict { message } => {
                write!(f, "sandbox policy conflict: {}", message)
            }
            SandboxError::InvalidGrant(msg) => {
                write!(f, "invalid sandbox grant: {}", msg)
            }
        }
    }
}

impl std::error::Error for SandboxError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_capability() {
        assert_eq!(Capability::Network.to_string(), "network");
        assert_eq!(Capability::All.to_string(), "all");
    }

    #[test]
    fn display_capability_level() {
        assert_eq!(CapabilityLevel::None.to_string(), "none");
        assert_eq!(CapabilityLevel::ReadOnly.to_string(), "read_only");
        assert_eq!(CapabilityLevel::Full.to_string(), "full");
        assert_eq!(
            CapabilityLevel::Restricted(vec!["a.com".into(), "b.com".into()]).to_string(),
            "restricted(a.com, b.com)"
        );
    }
}
