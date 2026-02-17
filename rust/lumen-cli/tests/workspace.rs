//! Integration tests for the workspace resolver (T190).

use lumen_cli::workspace::{
    ResolverBuildStep, ResolverDependencySource, ResolverDependencySpec, ResolverError,
    ResolverWorkspaceConfig, ResolverWorkspaceMember, WorkspaceResolver,
};
use std::collections::HashMap;
use std::path::PathBuf;

// =============================================================================
// Helper: build a config quickly
// =============================================================================

fn member(name: &str, path: &str, version: &str, deps: &[&str]) -> ResolverWorkspaceMember {
    ResolverWorkspaceMember {
        name: name.to_string(),
        path: PathBuf::from(path),
        version: version.to_string(),
        dependencies: deps.iter().map(|s| s.to_string()).collect(),
    }
}

fn config_with(members: Vec<ResolverWorkspaceMember>) -> ResolverWorkspaceConfig {
    ResolverWorkspaceConfig {
        root_dir: PathBuf::from("/workspace"),
        members,
        shared_dependencies: HashMap::new(),
        default_member: None,
    }
}

// =============================================================================
// Construction and validation
// =============================================================================

#[test]
fn ws_resolver_empty_workspace() {
    let cfg = config_with(vec![]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.resolve_order().is_empty());
}

#[test]
fn ws_resolver_single_member() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert_eq!(resolver.resolve_order(), &["core"]);
}

#[test]
fn ws_resolver_two_independent_members() {
    let cfg = config_with(vec![
        member("alpha", "crates/alpha", "1.0.0", &[]),
        member("beta", "crates/beta", "1.0.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let order = resolver.resolve_order();
    assert_eq!(order.len(), 2);
    // Both present, sorted alphabetically since they're independent
    assert!(order.contains(&"alpha".to_string()));
    assert!(order.contains(&"beta".to_string()));
}

#[test]
fn ws_resolver_linear_chain() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["lib"]),
        member("lib", "crates/lib", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert_eq!(resolver.resolve_order(), &["core", "lib", "app"]);
}

#[test]
fn ws_resolver_diamond_dependency() {
    // app -> lib-a, lib-b; lib-a -> core; lib-b -> core
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["lib-a", "lib-b"]),
        member("lib-a", "crates/lib-a", "0.1.0", &["core"]),
        member("lib-b", "crates/lib-b", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let order = resolver.resolve_order();
    assert_eq!(order[0], "core");
    assert_eq!(order[3], "app");
    // lib-a and lib-b are in between; both before app, both after core
    let pos_a = order.iter().position(|n| n == "lib-a").unwrap();
    let pos_b = order.iter().position(|n| n == "lib-b").unwrap();
    assert!(pos_a > 0 && pos_a < 3);
    assert!(pos_b > 0 && pos_b < 3);
}

#[test]
fn ws_resolver_duplicate_member_error() {
    let cfg = config_with(vec![
        member("core", "crates/core", "0.1.0", &[]),
        member("core", "crates/core2", "0.2.0", &[]),
    ]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::DuplicateMember(name)) => assert_eq!(name, "core"),
        other => panic!("expected DuplicateMember, got {:?}", other),
    }
}

#[test]
fn ws_resolver_missing_dep_error() {
    let cfg = config_with(vec![member("app", "crates/app", "0.1.0", &["nonexistent"])]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::MemberNotFound(msg)) => {
            assert!(msg.contains("nonexistent"));
        }
        other => panic!("expected MemberNotFound, got {:?}", other),
    }
}

#[test]
fn ws_resolver_self_cycle_error() {
    let cfg = config_with(vec![member("a", "crates/a", "0.1.0", &["a"])]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::CyclicDependency(cycle)) => {
            assert!(cycle.contains(&"a".to_string()));
        }
        other => panic!("expected CyclicDependency, got {:?}", other),
    }
}

#[test]
fn ws_resolver_two_node_cycle_error() {
    let cfg = config_with(vec![
        member("a", "crates/a", "0.1.0", &["b"]),
        member("b", "crates/b", "0.1.0", &["a"]),
    ]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::CyclicDependency(cycle)) => {
            assert!(cycle.len() >= 2);
        }
        other => panic!("expected CyclicDependency, got {:?}", other),
    }
}

#[test]
fn ws_resolver_three_node_cycle_error() {
    let cfg = config_with(vec![
        member("a", "crates/a", "0.1.0", &["b"]),
        member("b", "crates/b", "0.1.0", &["c"]),
        member("c", "crates/c", "0.1.0", &["a"]),
    ]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::CyclicDependency(cycle)) => {
            assert!(cycle.len() >= 3);
        }
        other => panic!("expected CyclicDependency, got {:?}", other),
    }
}

#[test]
fn ws_resolver_invalid_path_error() {
    let cfg = config_with(vec![member("bad", "", "0.1.0", &[])]);
    match WorkspaceResolver::new(cfg) {
        Err(ResolverError::InvalidPath(p)) => {
            assert_eq!(p, PathBuf::from(""));
        }
        other => panic!("expected InvalidPath, got {:?}", other),
    }
}

// =============================================================================
// resolve_member / member_dependencies / reverse_dependencies
// =============================================================================

#[test]
fn ws_resolver_resolve_member_found() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let m = resolver.resolve_member("core").unwrap();
    assert_eq!(m.name, "core");
    assert_eq!(m.version, "0.1.0");
}

#[test]
fn ws_resolver_resolve_member_not_found() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.resolve_member("missing").is_none());
}

#[test]
fn ws_resolver_member_dependencies() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["core", "utils"]),
        member("core", "crates/core", "0.1.0", &[]),
        member("utils", "crates/utils", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let deps = resolver.member_dependencies("app");
    let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"core"));
    assert!(names.contains(&"utils"));
    assert_eq!(deps.len(), 2);
}

#[test]
fn ws_resolver_member_dependencies_empty() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.member_dependencies("core").is_empty());
}

#[test]
fn ws_resolver_member_dependencies_missing_member() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.member_dependencies("nonexistent").is_empty());
}

#[test]
fn ws_resolver_reverse_dependencies() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["core"]),
        member("lib", "crates/lib", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let rdeps = resolver.reverse_dependencies("core");
    let names: Vec<&str> = rdeps.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"app"));
    assert!(names.contains(&"lib"));
    assert_eq!(rdeps.len(), 2);
}

#[test]
fn ws_resolver_reverse_dependencies_none() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.reverse_dependencies("app").is_empty());
}

// =============================================================================
// Build plan
// =============================================================================

#[test]
fn ws_resolver_build_plan_empty() {
    let cfg = config_with(vec![]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.build_plan().is_empty());
}

#[test]
fn ws_resolver_build_plan_single() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let plan = resolver.build_plan();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].member, "core");
    assert_eq!(plan[0].parallel_group, 0);
}

#[test]
fn ws_resolver_build_plan_parallel_roots() {
    let cfg = config_with(vec![
        member("a", "crates/a", "0.1.0", &[]),
        member("b", "crates/b", "0.1.0", &[]),
        member("c", "crates/c", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let plan = resolver.build_plan();
    assert_eq!(plan.len(), 3);
    // All independent -> all in group 0
    for step in &plan {
        assert_eq!(step.parallel_group, 0);
    }
}

#[test]
fn ws_resolver_build_plan_linear_groups() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["lib"]),
        member("lib", "crates/lib", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let plan = resolver.build_plan();

    let find_group = |name: &str| {
        plan.iter()
            .find(|s| s.member == name)
            .unwrap()
            .parallel_group
    };
    assert_eq!(find_group("core"), 0);
    assert_eq!(find_group("lib"), 1);
    assert_eq!(find_group("app"), 2);
}

#[test]
fn ws_resolver_build_plan_diamond_groups() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["left", "right"]),
        member("left", "crates/left", "0.1.0", &["core"]),
        member("right", "crates/right", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let plan = resolver.build_plan();

    let find_group = |name: &str| {
        plan.iter()
            .find(|s| s.member == name)
            .unwrap()
            .parallel_group
    };
    assert_eq!(find_group("core"), 0);
    assert_eq!(find_group("left"), 1);
    assert_eq!(find_group("right"), 1);
    assert_eq!(find_group("app"), 2);
}

// =============================================================================
// Affected members
// =============================================================================

#[test]
fn ws_resolver_affected_members_none() {
    let cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let affected = resolver.affected_members(&[PathBuf::from("unrelated/file.rs")]);
    assert!(affected.is_empty());
}

#[test]
fn ws_resolver_affected_members_direct() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let affected = resolver.affected_members(&[PathBuf::from("crates/core/src/lib.rs")]);
    // core is directly affected, app depends on core so is transitively affected
    assert!(affected.contains(&"core".to_string()));
    assert!(affected.contains(&"app".to_string()));
}

#[test]
fn ws_resolver_affected_members_leaf_only() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let affected = resolver.affected_members(&[PathBuf::from("crates/app/src/main.rs")]);
    // Only app is affected; core is not
    assert!(affected.contains(&"app".to_string()));
    assert!(!affected.contains(&"core".to_string()));
}

#[test]
fn ws_resolver_affected_members_transitive() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["lib"]),
        member("lib", "crates/lib", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let affected = resolver.affected_members(&[PathBuf::from("crates/core/src/types.rs")]);
    assert_eq!(affected, vec!["core", "lib", "app"]);
}

#[test]
fn ws_resolver_affected_members_preserves_topo_order() {
    let cfg = config_with(vec![
        member("app", "crates/app", "0.1.0", &["lib"]),
        member("lib", "crates/lib", "0.1.0", &["core"]),
        member("core", "crates/core", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let affected = resolver.affected_members(&[PathBuf::from("crates/core/x.rs")]);
    // Must be in topological order
    let pos_core = affected.iter().position(|n| n == "core").unwrap();
    let pos_lib = affected.iter().position(|n| n == "lib").unwrap();
    let pos_app = affected.iter().position(|n| n == "app").unwrap();
    assert!(pos_core < pos_lib);
    assert!(pos_lib < pos_app);
}

// =============================================================================
// Validation
// =============================================================================

#[test]
fn ws_resolver_validate_clean() {
    let cfg = config_with(vec![
        member("a", "crates/a", "0.1.0", &[]),
        member("b", "crates/b", "0.1.0", &["a"]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.validate().is_empty());
}

#[test]
fn ws_resolver_validate_default_member_exists() {
    let mut cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    cfg.default_member = Some("core".to_string());
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.validate().is_empty());
}

#[test]
fn ws_resolver_validate_default_member_missing() {
    let mut cfg = config_with(vec![member("core", "crates/core", "0.1.0", &[])]);
    cfg.default_member = Some("missing".to_string());
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let errors = resolver.validate();
    assert!(errors
        .iter()
        .any(|e| matches!(e, ResolverError::MemberNotFound(s) if s.contains("missing"))));
}

#[test]
fn ws_resolver_detect_cycles_none() {
    let cfg = config_with(vec![
        member("a", "crates/a", "0.1.0", &["b"]),
        member("b", "crates/b", "0.1.0", &[]),
    ]);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert!(resolver.detect_cycles().is_none());
}

// =============================================================================
// TOML parsing
// =============================================================================

#[test]
fn ws_resolver_from_toml_basic() {
    let toml = r#"
[workspace]
root = "/my/workspace"
default_member = "core"

[[workspace.members]]
name = "core"
path = "crates/core"
version = "0.1.0"
dependencies = []

[[workspace.members]]
name = "app"
path = "crates/app"
version = "0.2.0"
dependencies = ["core"]
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    assert_eq!(cfg.root_dir, PathBuf::from("/my/workspace"));
    assert_eq!(cfg.members.len(), 2);
    assert_eq!(cfg.default_member, Some("core".to_string()));
    assert_eq!(cfg.members[0].name, "core");
    assert_eq!(cfg.members[1].name, "app");
    assert_eq!(cfg.members[1].dependencies, vec!["core".to_string()]);
}

#[test]
fn ws_resolver_from_toml_shared_deps_registry() {
    let toml = r#"
[workspace]
root = "."

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"

[workspace.shared_dependencies.serde]
version = "1.0"
source = { registry = "https://crates.io" }
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let dep = cfg.shared_dependencies.get("serde").unwrap();
    assert_eq!(dep.version, "1.0");
    assert_eq!(
        dep.source,
        ResolverDependencySource::Registry("https://crates.io".to_string())
    );
}

#[test]
fn ws_resolver_from_toml_shared_deps_path() {
    let toml = r#"
[workspace]
root = "."

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"

[workspace.shared_dependencies.local-lib]
version = "0.1.0"
source = { path = "../local-lib" }
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let dep = cfg.shared_dependencies.get("local-lib").unwrap();
    assert_eq!(
        dep.source,
        ResolverDependencySource::Path(PathBuf::from("../local-lib"))
    );
}

#[test]
fn ws_resolver_from_toml_shared_deps_git() {
    let toml = r#"
[workspace]
root = "."

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"

[workspace.shared_dependencies.remote-lib]
version = "0.5.0"
source = { git = "https://github.com/org/repo.git", rev = "abc123" }
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let dep = cfg.shared_dependencies.get("remote-lib").unwrap();
    assert_eq!(
        dep.source,
        ResolverDependencySource::Git {
            url: "https://github.com/org/repo.git".to_string(),
            rev: Some("abc123".to_string()),
        }
    );
}

#[test]
fn ws_resolver_from_toml_no_workspace_section() {
    let toml = r#"
[package]
name = "something"
"#;
    match WorkspaceResolver::from_toml(toml) {
        Err(ResolverError::ParseError(msg)) => {
            assert!(msg.contains("workspace"));
        }
        other => panic!("expected ParseError, got {:?}", other),
    }
}

#[test]
fn ws_resolver_from_toml_invalid_toml() {
    let toml = "this is not valid toml {{{";
    match WorkspaceResolver::from_toml(toml) {
        Err(ResolverError::ParseError(_)) => {}
        other => panic!("expected ParseError, got {:?}", other),
    }
}

#[test]
fn ws_resolver_from_toml_member_missing_name() {
    let toml = r#"
[workspace]

[[workspace.members]]
path = "a"
version = "1.0.0"
"#;
    match WorkspaceResolver::from_toml(toml) {
        Err(ResolverError::ParseError(msg)) => {
            assert!(msg.contains("name"));
        }
        other => panic!("expected ParseError, got {:?}", other),
    }
}

#[test]
fn ws_resolver_from_toml_member_missing_version() {
    let toml = r#"
[workspace]

[[workspace.members]]
name = "a"
path = "a"
"#;
    match WorkspaceResolver::from_toml(toml) {
        Err(ResolverError::ParseError(msg)) => {
            assert!(msg.contains("version"));
        }
        other => panic!("expected ParseError, got {:?}", other),
    }
}

#[test]
fn ws_resolver_from_toml_member_missing_path() {
    let toml = r#"
[workspace]

[[workspace.members]]
name = "a"
version = "1.0.0"
"#;
    match WorkspaceResolver::from_toml(toml) {
        Err(ResolverError::ParseError(msg)) => {
            assert!(msg.contains("path"));
        }
        other => panic!("expected ParseError, got {:?}", other),
    }
}

#[test]
fn ws_resolver_from_toml_empty_members() {
    let toml = r#"
[workspace]
root = "/ws"
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    assert!(cfg.members.is_empty());
}

#[test]
fn ws_resolver_from_toml_no_root_defaults_to_dot() {
    let toml = r#"
[workspace]

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    assert_eq!(cfg.root_dir, PathBuf::from("."));
}

// =============================================================================
// End-to-end: TOML -> resolver -> build plan
// =============================================================================

#[test]
fn ws_resolver_toml_roundtrip() {
    let toml = r#"
[workspace]
root = "/project"
default_member = "app"

[[workspace.members]]
name = "core"
path = "crates/core"
version = "0.1.0"
dependencies = []

[[workspace.members]]
name = "utils"
path = "crates/utils"
version = "0.1.0"
dependencies = ["core"]

[[workspace.members]]
name = "app"
path = "crates/app"
version = "0.1.0"
dependencies = ["core", "utils"]
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    assert_eq!(resolver.resolve_order(), &["core", "utils", "app"]);

    let plan = resolver.build_plan();
    let find_group = |name: &str| {
        plan.iter()
            .find(|s| s.member == name)
            .unwrap()
            .parallel_group
    };
    assert_eq!(find_group("core"), 0);
    assert_eq!(find_group("utils"), 1);
    assert_eq!(find_group("app"), 2);
}

// =============================================================================
// Error Display
// =============================================================================

#[test]
fn ws_resolver_error_display_member_not_found() {
    let err = ResolverError::MemberNotFound("xyz".to_string());
    assert!(err.to_string().contains("xyz"));
}

#[test]
fn ws_resolver_error_display_cyclic() {
    let err = ResolverError::CyclicDependency(vec!["a".into(), "b".into(), "a".into()]);
    let msg = err.to_string();
    assert!(msg.contains("a -> b -> a"));
}

#[test]
fn ws_resolver_error_display_duplicate() {
    let err = ResolverError::DuplicateMember("dup".to_string());
    assert!(err.to_string().contains("dup"));
}

#[test]
fn ws_resolver_error_display_invalid_path() {
    let err = ResolverError::InvalidPath(PathBuf::from("/bad/path"));
    assert!(err.to_string().contains("/bad/path"));
}

#[test]
fn ws_resolver_error_display_parse() {
    let err = ResolverError::ParseError("bad toml".to_string());
    assert!(err.to_string().contains("bad toml"));
}

#[test]
fn ws_resolver_error_display_version_conflict() {
    let err = ResolverError::VersionConflict {
        dependency: "serde".to_string(),
        versions: vec!["1.0".to_string(), "2.0".to_string()],
    };
    let msg = err.to_string();
    assert!(msg.contains("serde"));
    assert!(msg.contains("1.0"));
    assert!(msg.contains("2.0"));
}

// =============================================================================
// Config accessor
// =============================================================================

#[test]
fn ws_resolver_config_accessor() {
    let mut cfg = config_with(vec![member("a", "crates/a", "1.0.0", &[])]);
    cfg.default_member = Some("a".to_string());
    cfg.shared_dependencies.insert(
        "serde".to_string(),
        ResolverDependencySpec {
            version: "1.0".to_string(),
            source: ResolverDependencySource::Registry("default".to_string()),
        },
    );
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let c = resolver.config();
    assert_eq!(c.default_member, Some("a".to_string()));
    assert!(c.shared_dependencies.contains_key("serde"));
    assert_eq!(c.root_dir, PathBuf::from("/workspace"));
}

// =============================================================================
// Build step equality
// =============================================================================

#[test]
fn ws_resolver_build_step_eq() {
    let a = ResolverBuildStep {
        member: "x".to_string(),
        parallel_group: 0,
    };
    let b = ResolverBuildStep {
        member: "x".to_string(),
        parallel_group: 0,
    };
    assert_eq!(a, b);
}

// =============================================================================
// Large workspace
// =============================================================================

#[test]
fn ws_resolver_large_workspace() {
    // 20 members in a linear chain
    let mut members = Vec::new();
    for i in 0..20 {
        let deps: Vec<&str> = if i > 0 {
            vec![Box::leak(format!("m{}", i - 1).into_boxed_str()) as &str]
        } else {
            vec![]
        };
        members.push(member(
            Box::leak(format!("m{}", i).into_boxed_str()),
            Box::leak(format!("crates/m{}", i).into_boxed_str()),
            "0.1.0",
            &deps,
        ));
    }
    let cfg = config_with(members);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let order = resolver.resolve_order();
    assert_eq!(order.len(), 20);
    // m0 must be first, m19 must be last
    assert_eq!(order[0], "m0");
    assert_eq!(order[19], "m19");
}

#[test]
fn ws_resolver_wide_fan_out() {
    // One root, 10 leaves all depending on it
    let mut members = vec![member("root", "crates/root", "0.1.0", &[])];
    for i in 0..10 {
        members.push(member(
            Box::leak(format!("leaf{}", i).into_boxed_str()),
            Box::leak(format!("crates/leaf{}", i).into_boxed_str()),
            "0.1.0",
            &["root"],
        ));
    }
    let cfg = config_with(members);
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    let order = resolver.resolve_order();
    assert_eq!(order[0], "root");
    assert_eq!(order.len(), 11);

    let plan = resolver.build_plan();
    let root_group = plan
        .iter()
        .find(|s| s.member == "root")
        .unwrap()
        .parallel_group;
    assert_eq!(root_group, 0);
    // All leaves should be in group 1
    for step in &plan {
        if step.member != "root" {
            assert_eq!(step.parallel_group, 1);
        }
    }
}

// =============================================================================
// Shared dependencies
// =============================================================================

#[test]
fn ws_resolver_from_toml_git_no_rev() {
    let toml = r#"
[workspace]
root = "."

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"

[workspace.shared_dependencies.remote]
version = "1.0.0"
source = { git = "https://github.com/org/repo.git" }
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let dep = cfg.shared_dependencies.get("remote").unwrap();
    assert_eq!(
        dep.source,
        ResolverDependencySource::Git {
            url: "https://github.com/org/repo.git".to_string(),
            rev: None,
        }
    );
}

#[test]
fn ws_resolver_from_toml_default_source() {
    let toml = r#"
[workspace]
root = "."

[[workspace.members]]
name = "a"
path = "a"
version = "1.0.0"

[workspace.shared_dependencies.bare]
version = "2.0.0"
"#;
    let cfg = WorkspaceResolver::from_toml(toml).unwrap();
    let dep = cfg.shared_dependencies.get("bare").unwrap();
    assert_eq!(
        dep.source,
        ResolverDependencySource::Registry("default".to_string())
    );
}

// =============================================================================
// Affected members with root-relative paths
// =============================================================================

#[test]
fn ws_resolver_affected_members_root_relative() {
    let cfg = ResolverWorkspaceConfig {
        root_dir: PathBuf::from("/workspace"),
        members: vec![
            member("core", "crates/core", "0.1.0", &[]),
            member("app", "crates/app", "0.1.0", &["core"]),
        ],
        shared_dependencies: HashMap::new(),
        default_member: None,
    };
    let resolver = WorkspaceResolver::new(cfg).unwrap();
    // Using absolute path matching root_dir + member.path
    let affected = resolver.affected_members(&[PathBuf::from("/workspace/crates/core/src/lib.rs")]);
    assert!(affected.contains(&"core".to_string()));
    assert!(affected.contains(&"app".to_string()));
}
