//! Wave 22 — T159: Recursive sandbox at import
//!
//! Comprehensive tests for the sandbox module: capability definitions,
//! policy construction, policy checking, import sandbox hierarchy,
//! policy merging, and error formatting.

use lumen_compiler::compiler::sandbox::*;

// ══════════════════════════════════════════════════════════════════════
// 1. SandboxPolicy::none / full
// ══════════════════════════════════════════════════════════════════════

#[test]
fn sandbox_policy_none_denies_all_capabilities() {
    let policy = SandboxPolicy::none();
    assert!(policy.deny_all);
    assert!(!policy.allow_unsafe);
    assert!(policy.grants.is_empty());
}

#[test]
fn sandbox_policy_full_allows_all_capabilities() {
    let policy = SandboxPolicy::full();
    assert!(!policy.deny_all);
    assert!(policy.allow_unsafe);
    assert_eq!(policy.grants.len(), 1);
    assert_eq!(policy.grants[0].capability, Capability::All);
    assert_eq!(policy.grants[0].level, CapabilityLevel::Full);
}

// ══════════════════════════════════════════════════════════════════════
// 2. SandboxPolicyBuilder
// ══════════════════════════════════════════════════════════════════════

#[test]
fn builder_grants_specific_capability() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::Network, CapabilityLevel::Full)
        .build();

    assert!(!policy.deny_all);
    assert_eq!(policy.grants.len(), 1);
    assert_eq!(policy.grants[0].capability, Capability::Network);
    assert_eq!(policy.grants[0].level, CapabilityLevel::Full);
}

#[test]
fn builder_grants_multiple_capabilities() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::Network, CapabilityLevel::Full)
        .grant(Capability::FileSystem, CapabilityLevel::ReadOnly)
        .grant(
            Capability::Timer,
            CapabilityLevel::Restricted(vec!["100ms".into()]),
        )
        .build();

    assert_eq!(policy.grants.len(), 3);
}

#[test]
fn builder_deny_removes_grant() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::Network, CapabilityLevel::Full)
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .deny(Capability::Network)
        .build();

    assert_eq!(policy.grants.len(), 1);
    assert_eq!(policy.grants[0].capability, Capability::FileSystem);
}

#[test]
fn builder_allow_unsafe_flag() {
    let policy = SandboxPolicy::builder().allow_unsafe(true).build();
    assert!(policy.allow_unsafe);
}

// ══════════════════════════════════════════════════════════════════════
// 3. check_capability
// ══════════════════════════════════════════════════════════════════════

#[test]
fn check_capability_none_policy_denies() {
    let policy = SandboxPolicy::none();
    let result = check_capability(&policy, &Capability::Network);
    match result {
        CapabilityCheck::Denied { capability, .. } => assert_eq!(capability, "network"),
        other => panic!("expected Denied, got {:?}", other),
    }
}

#[test]
fn check_capability_full_policy_allows() {
    let policy = SandboxPolicy::full();
    let result = check_capability(&policy, &Capability::Network);
    assert_eq!(result, CapabilityCheck::Allowed);
}

#[test]
fn check_capability_full_policy_allows_filesystem() {
    let policy = SandboxPolicy::full();
    assert_eq!(
        check_capability(&policy, &Capability::FileSystem),
        CapabilityCheck::Allowed
    );
}

#[test]
fn check_capability_readonly_level() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::ReadOnly)
        .build();

    match check_capability(&policy, &Capability::FileSystem) {
        CapabilityCheck::Restricted {
            capability,
            constraints,
        } => {
            assert_eq!(capability, "filesystem");
            assert_eq!(constraints, vec!["read_only".to_string()]);
        }
        other => panic!("expected Restricted, got {:?}", other),
    }
}

#[test]
fn check_capability_restricted_level() {
    let policy = SandboxPolicy::builder()
        .grant(
            Capability::Network,
            CapabilityLevel::Restricted(vec!["api.example.com".into()]),
        )
        .build();

    match check_capability(&policy, &Capability::Network) {
        CapabilityCheck::Restricted {
            capability,
            constraints,
        } => {
            assert_eq!(capability, "network");
            assert_eq!(constraints, vec!["api.example.com".to_string()]);
        }
        other => panic!("expected Restricted, got {:?}", other),
    }
}

#[test]
fn check_capability_explicit_none_level() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::Network, CapabilityLevel::None)
        .build();

    match check_capability(&policy, &Capability::Network) {
        CapabilityCheck::Denied { capability, .. } => assert_eq!(capability, "network"),
        other => panic!("expected Denied, got {:?}", other),
    }
}

#[test]
fn check_capability_ungranteed_capability_denied() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .build();

    match check_capability(&policy, &Capability::Network) {
        CapabilityCheck::Denied { capability, .. } => assert_eq!(capability, "network"),
        other => panic!("expected Denied, got {:?}", other),
    }
}

// ══════════════════════════════════════════════════════════════════════
// 4. check_tool_access
// ══════════════════════════════════════════════════════════════════════

#[test]
fn check_tool_access_denied_network() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .build();

    let violations = check_tool_access(&policy, "http_get", &[Capability::Network]);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].tool_name, "http_get");
    assert_eq!(violations[0].required_capability, Capability::Network);
}

#[test]
fn check_tool_access_matching_policy() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::Network, CapabilityLevel::Full)
        .build();

    let violations = check_tool_access(&policy, "http_get", &[Capability::Network]);
    assert!(violations.is_empty());
}

#[test]
fn check_tool_access_multiple_requirements() {
    let policy = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .build();

    let violations = check_tool_access(
        &policy,
        "deploy_tool",
        &[
            Capability::Network,
            Capability::Process,
            Capability::FileSystem,
        ],
    );
    // Network and Process are denied; FileSystem is allowed.
    assert_eq!(violations.len(), 2);
    let names: Vec<_> = violations.iter().map(|v| &v.required_capability).collect();
    assert!(names.contains(&&Capability::Network));
    assert!(names.contains(&&Capability::Process));
}

#[test]
fn check_tool_access_none_policy_denies_all() {
    let policy = SandboxPolicy::none();
    let violations = check_tool_access(
        &policy,
        "any_tool",
        &[Capability::Network, Capability::FileSystem],
    );
    assert_eq!(violations.len(), 2);
}

// ══════════════════════════════════════════════════════════════════════
// 5. ImportSandbox
// ══════════════════════════════════════════════════════════════════════

#[test]
fn import_sandbox_new_and_can_access() {
    let sandbox = ImportSandbox::new(
        "data_lib",
        SandboxPolicy::builder()
            .grant(Capability::FileSystem, CapabilityLevel::Full)
            .build(),
    );

    assert!(sandbox.can_access(&Capability::FileSystem));
    assert!(!sandbox.can_access(&Capability::Network));
}

#[test]
fn import_sandbox_with_parent_restricts_child() {
    let parent = ImportSandbox::new(
        "app",
        SandboxPolicy::builder()
            .grant(Capability::FileSystem, CapabilityLevel::ReadOnly)
            .grant(Capability::Network, CapabilityLevel::Full)
            .build(),
    );

    // Child grants full FS, but parent only allows ReadOnly — effective is ReadOnly.
    let child = ImportSandbox::with_parent(
        "data_lib",
        SandboxPolicy::builder()
            .grant(Capability::FileSystem, CapabilityLevel::Full)
            .grant(Capability::Network, CapabilityLevel::Full)
            .build(),
        parent,
    );

    // ReadOnly counts as "can_access" (restricted, not denied).
    assert!(child.can_access(&Capability::FileSystem));
    assert!(child.can_access(&Capability::Network));

    // Verify effective_policy shows ReadOnly for FS.
    let eff = child.effective_policy();
    let fs_grant = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::FileSystem);
    assert_eq!(fs_grant.unwrap().level, CapabilityLevel::ReadOnly);
}

#[test]
fn import_sandbox_effective_policy_intersection() {
    let parent = ImportSandbox::new(
        "outer",
        SandboxPolicy::builder()
            .grant(Capability::Network, CapabilityLevel::Full)
            .grant(Capability::FileSystem, CapabilityLevel::Full)
            .build(),
    );

    let child = ImportSandbox::with_parent(
        "inner",
        SandboxPolicy::builder()
            .grant(Capability::Network, CapabilityLevel::ReadOnly)
            .build(),
        parent,
    );

    let eff = child.effective_policy();
    // Network: Full ∩ ReadOnly = ReadOnly
    let net = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::Network);
    assert_eq!(net.unwrap().level, CapabilityLevel::ReadOnly);

    // FileSystem: Full ∩ (not granted) = None → not in grants
    let fs = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::FileSystem);
    assert!(fs.is_none());
    assert!(!child.can_access(&Capability::FileSystem));
}

// ══════════════════════════════════════════════════════════════════════
// 6. intersect_policies
// ══════════════════════════════════════════════════════════════════════

#[test]
fn intersect_none_wins_over_full() {
    let parent = SandboxPolicy::none();
    let child = SandboxPolicy::full();
    let result = intersect_policies(&parent, &child);
    assert!(result.deny_all);
}

#[test]
fn intersect_readonly_wins_over_full() {
    let parent = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::ReadOnly)
        .build();
    let child = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .build();

    let result = intersect_policies(&parent, &child);
    assert_eq!(result.grants.len(), 1);
    assert_eq!(result.grants[0].level, CapabilityLevel::ReadOnly);
}

#[test]
fn intersect_restricted_domains() {
    let parent = SandboxPolicy::builder()
        .grant(
            Capability::Network,
            CapabilityLevel::Restricted(vec!["a.com".into(), "b.com".into(), "c.com".into()]),
        )
        .build();
    let child = SandboxPolicy::builder()
        .grant(
            Capability::Network,
            CapabilityLevel::Restricted(vec!["b.com".into(), "c.com".into(), "d.com".into()]),
        )
        .build();

    let result = intersect_policies(&parent, &child);
    assert_eq!(result.grants.len(), 1);
    match &result.grants[0].level {
        CapabilityLevel::Restricted(domains) => {
            assert!(domains.contains(&"b.com".to_string()));
            assert!(domains.contains(&"c.com".to_string()));
            assert!(!domains.contains(&"a.com".to_string()));
            assert!(!domains.contains(&"d.com".to_string()));
        }
        other => panic!("expected Restricted, got {:?}", other),
    }
}

#[test]
fn intersect_restricted_disjoint_becomes_none() {
    let parent = SandboxPolicy::builder()
        .grant(
            Capability::Network,
            CapabilityLevel::Restricted(vec!["a.com".into()]),
        )
        .build();
    let child = SandboxPolicy::builder()
        .grant(
            Capability::Network,
            CapabilityLevel::Restricted(vec!["b.com".into()]),
        )
        .build();

    let result = intersect_policies(&parent, &child);
    // Disjoint restricted → None, so grant is omitted.
    assert!(result.grants.is_empty());
}

#[test]
fn intersect_allow_unsafe_requires_both() {
    let p1 = SandboxPolicy::full(); // allow_unsafe = true
    let p2 = SandboxPolicy::builder()
        .grant(Capability::All, CapabilityLevel::Full)
        .allow_unsafe(false)
        .build();

    let result = intersect_policies(&p1, &p2);
    assert!(!result.allow_unsafe);
}

// ══════════════════════════════════════════════════════════════════════
// 7. merge_capability_levels — all combinations
// ══════════════════════════════════════════════════════════════════════

#[test]
fn merge_none_with_anything_is_none() {
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::None, &CapabilityLevel::Full),
        CapabilityLevel::None
    );
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::Full, &CapabilityLevel::None),
        CapabilityLevel::None
    );
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::None, &CapabilityLevel::ReadOnly),
        CapabilityLevel::None
    );
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::None, &CapabilityLevel::None),
        CapabilityLevel::None
    );
    assert_eq!(
        merge_capability_levels(
            &CapabilityLevel::None,
            &CapabilityLevel::Restricted(vec!["x".into()])
        ),
        CapabilityLevel::None
    );
}

#[test]
fn merge_readonly_with_full_is_readonly() {
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::ReadOnly, &CapabilityLevel::Full),
        CapabilityLevel::ReadOnly
    );
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::Full, &CapabilityLevel::ReadOnly),
        CapabilityLevel::ReadOnly
    );
}

#[test]
fn merge_readonly_with_readonly_is_readonly() {
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::ReadOnly, &CapabilityLevel::ReadOnly),
        CapabilityLevel::ReadOnly
    );
}

#[test]
fn merge_readonly_with_restricted_is_readonly() {
    assert_eq!(
        merge_capability_levels(
            &CapabilityLevel::ReadOnly,
            &CapabilityLevel::Restricted(vec!["x".into()])
        ),
        CapabilityLevel::ReadOnly
    );
    assert_eq!(
        merge_capability_levels(
            &CapabilityLevel::Restricted(vec!["x".into()]),
            &CapabilityLevel::ReadOnly
        ),
        CapabilityLevel::ReadOnly
    );
}

#[test]
fn merge_restricted_with_full_is_restricted() {
    let r = CapabilityLevel::Restricted(vec!["a.com".into()]);
    assert_eq!(
        merge_capability_levels(&r, &CapabilityLevel::Full),
        CapabilityLevel::Restricted(vec!["a.com".into()])
    );
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::Full, &r),
        CapabilityLevel::Restricted(vec!["a.com".into()])
    );
}

#[test]
fn merge_full_with_full_is_full() {
    assert_eq!(
        merge_capability_levels(&CapabilityLevel::Full, &CapabilityLevel::Full),
        CapabilityLevel::Full
    );
}

#[test]
fn merge_restricted_intersects_targets() {
    let a = CapabilityLevel::Restricted(vec!["x.com".into(), "y.com".into()]);
    let b = CapabilityLevel::Restricted(vec!["y.com".into(), "z.com".into()]);
    match merge_capability_levels(&a, &b) {
        CapabilityLevel::Restricted(targets) => {
            assert_eq!(targets, vec!["y.com".to_string()]);
        }
        other => panic!("expected Restricted, got {:?}", other),
    }
}

// ══════════════════════════════════════════════════════════════════════
// 8. Nested sandboxes (3 levels deep)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn nested_sandbox_three_levels() {
    let root = ImportSandbox::new(
        "root",
        SandboxPolicy::builder()
            .grant(Capability::Network, CapabilityLevel::Full)
            .grant(Capability::FileSystem, CapabilityLevel::Full)
            .grant(Capability::Timer, CapabilityLevel::Full)
            .build(),
    );

    let mid = ImportSandbox::with_parent(
        "middleware",
        SandboxPolicy::builder()
            .grant(Capability::Network, CapabilityLevel::ReadOnly)
            .grant(Capability::FileSystem, CapabilityLevel::Full)
            .grant(Capability::Timer, CapabilityLevel::Full)
            .build(),
        root,
    );

    let leaf = ImportSandbox::with_parent(
        "untrusted",
        SandboxPolicy::builder()
            .grant(
                Capability::Network,
                CapabilityLevel::Restricted(vec!["safe.api".into()]),
            )
            .grant(Capability::FileSystem, CapabilityLevel::ReadOnly)
            .grant(Capability::Timer, CapabilityLevel::Full)
            .build(),
        mid,
    );

    let eff = leaf.effective_policy();

    // Network: Full → ReadOnly → Restricted → ReadOnly wins over Restricted.
    let net = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::Network);
    assert_eq!(net.unwrap().level, CapabilityLevel::ReadOnly);

    // FS: Full → Full → ReadOnly → ReadOnly
    let fs = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::FileSystem);
    assert_eq!(fs.unwrap().level, CapabilityLevel::ReadOnly);

    // Timer: Full → Full → Full → Full
    let timer = eff
        .grants
        .iter()
        .find(|g| g.capability == Capability::Timer);
    assert_eq!(timer.unwrap().level, CapabilityLevel::Full);

    // Process not granted at any level
    assert!(!leaf.can_access(&Capability::Process));
}

// ══════════════════════════════════════════════════════════════════════
// 9. SandboxError Display
// ══════════════════════════════════════════════════════════════════════

#[test]
fn sandbox_error_capability_denied_display() {
    let err = SandboxError::CapabilityDenied {
        module: "evil_pkg".into(),
        capability: "network".into(),
    };
    assert_eq!(
        err.to_string(),
        "capability 'network' denied for module 'evil_pkg'"
    );
}

#[test]
fn sandbox_error_policy_conflict_display() {
    let err = SandboxError::PolicyConflict {
        message: "contradictory grants".into(),
    };
    assert_eq!(
        err.to_string(),
        "sandbox policy conflict: contradictory grants"
    );
}

#[test]
fn sandbox_error_invalid_grant_display() {
    let err = SandboxError::InvalidGrant("unknown capability 'quantum'".into());
    assert_eq!(
        err.to_string(),
        "invalid sandbox grant: unknown capability 'quantum'"
    );
}

#[test]
fn sandbox_error_is_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(SandboxError::InvalidGrant("test".into()));
    assert!(err.to_string().contains("test"));
}

// ══════════════════════════════════════════════════════════════════════
// 10. Additional edge cases
// ══════════════════════════════════════════════════════════════════════

#[test]
fn full_policy_allows_all_individual_capabilities() {
    let policy = SandboxPolicy::full();
    for cap in &[
        Capability::Network,
        Capability::FileSystem,
        Capability::Process,
        Capability::Environment,
        Capability::Crypto,
        Capability::Timer,
    ] {
        assert_eq!(check_capability(&policy, cap), CapabilityCheck::Allowed);
    }
}

#[test]
fn builder_deny_nonexistent_is_harmless() {
    // Denying something that was never granted should not panic.
    let policy = SandboxPolicy::builder()
        .grant(Capability::FileSystem, CapabilityLevel::Full)
        .deny(Capability::Crypto)
        .build();
    assert_eq!(policy.grants.len(), 1);
}

#[test]
fn capability_violation_message_contains_tool_name() {
    let policy = SandboxPolicy::none();
    let violations = check_tool_access(&policy, "secret_tool", &[Capability::Crypto]);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("secret_tool"));
    assert!(violations[0].message.contains("crypto"));
}
