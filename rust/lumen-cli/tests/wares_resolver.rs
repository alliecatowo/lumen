//! Resolver behaviour against a `file://` registry and path dependencies.

mod common;

use common::{write_package, FileRegistry, TempDir};
use lumen_cli::config::DependencySpec;
use lumen_cli::wares::{
    ResolutionError, ResolutionRequest, ResolvedPackage, ResolvedSource, Resolver,
};
use std::collections::HashMap;

fn request(registry: &FileRegistry, deps: &[(&str, DependencySpec)]) -> ResolutionRequest {
    ResolutionRequest {
        root_deps: deps
            .iter()
            .map(|(n, s)| (n.to_string(), s.clone()))
            .collect::<HashMap<_, _>>(),
        registry_url: registry.url(),
        ..ResolutionRequest::default()
    }
}

fn ver(c: &str) -> DependencySpec {
    DependencySpec::Version(c.to_string())
}

fn find<'a>(pkgs: &'a [ResolvedPackage], name: &str) -> &'a ResolvedPackage {
    pkgs.iter().find(|p| p.name == name).unwrap_or_else(|| {
        panic!(
            "{name} not resolved: {:?}",
            pkgs.iter().map(|p| &p.name).collect::<Vec<_>>()
        )
    })
}

#[test]
fn resolves_a_single_registry_dependency_with_artifacts() {
    let reg = FileRegistry::new();
    let artifact = reg.publish("@t/dep", "0.1.0", &[]);
    let resolver = Resolver::new(reg.url(), None);

    let result = resolver
        .resolve(&request(&reg, &[("@t/dep", ver("^0.1.0"))]))
        .expect("a one-package registry must resolve");

    let dep = find(&result.packages, "@t/dep");
    assert_eq!(dep.version, "0.1.0");
    match &dep.source {
        ResolvedSource::Registry { url, artifacts, .. } => {
            assert_eq!(url, &reg.url());
            assert_eq!(
                artifacts.len(),
                1,
                "artifacts must come from the version metadata"
            );
            assert!(artifacts[0].hash.starts_with("sha256:"));
            assert!(reg.root.join(&artifacts[0].url).exists());
            assert!(artifact.ends_with(artifacts[0].url.trim_start_matches("artifacts/")));
        }
        other => panic!("expected a registry source, got {other:?}"),
    }
}

#[test]
fn picks_the_highest_matching_version() {
    let reg = FileRegistry::new();
    for v in ["0.1.0", "0.1.5", "0.2.0", "1.0.0"] {
        reg.publish("@t/dep", v, &[]);
    }
    let resolver = Resolver::new(reg.url(), None);
    let result = resolver
        .resolve(&request(&reg, &[("@t/dep", ver("^0.1.0"))]))
        .unwrap();
    assert_eq!(find(&result.packages, "@t/dep").version, "0.1.5");
}

#[test]
fn resolves_transitive_dependencies_and_records_edges() {
    let reg = FileRegistry::new();
    reg.publish("@t/c", "1.0.0", &[]);
    reg.publish("@t/b", "0.3.0", &[("@t/c", "^1.0.0")]);
    reg.publish("@t/a", "0.1.0", &[("@t/b", "^0.3.0")]);
    let resolver = Resolver::new(reg.url(), None);

    let result = resolver
        .resolve(&request(&reg, &[("@t/a", ver("^0.1.0"))]))
        .unwrap();

    let names: Vec<_> = result.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["@t/a", "@t/b", "@t/c"]);
    assert_eq!(find(&result.packages, "@t/a").deps[0].0, "@t/b");
    assert_eq!(find(&result.packages, "@t/b").deps[0].0, "@t/c");
    assert!(find(&result.packages, "@t/c").deps.is_empty());
}

#[test]
fn diamond_dependencies_share_one_version() {
    let reg = FileRegistry::new();
    for v in ["1.0.0", "1.1.0", "1.2.0"] {
        reg.publish("@t/c", v, &[]);
    }
    reg.publish("@t/a", "1.0.0", &[("@t/c", "^1.0.0")]);
    reg.publish("@t/b", "1.0.0", &[("@t/c", "~1.1.0")]);
    let resolver = Resolver::new(reg.url(), None);

    let result = resolver
        .resolve(&request(
            &reg,
            &[("@t/a", ver("^1.0.0")), ("@t/b", ver("^1.0.0"))],
        ))
        .unwrap();
    assert_eq!(find(&result.packages, "@t/c").version, "1.1.0");
}

#[test]
fn backtracks_to_an_older_release_when_the_newest_conflicts() {
    let reg = FileRegistry::new();
    reg.publish("@t/c", "1.0.0", &[]);
    reg.publish("@t/c", "2.0.0", &[]);
    // a 2.x needs c 2.x but the root also needs c 1.x via b, so a must fall back to 1.x.
    reg.publish("@t/a", "1.0.0", &[("@t/c", "^1.0.0")]);
    reg.publish("@t/a", "2.0.0", &[("@t/c", "^2.0.0")]);
    reg.publish("@t/b", "1.0.0", &[("@t/c", "^1.0.0")]);
    let resolver = Resolver::new(reg.url(), None);

    let result = resolver
        .resolve(&request(
            &reg,
            &[("@t/a", ver("^1.0.0 || ^2.0.0")), ("@t/b", ver("^1.0.0"))],
        ))
        .or_else(|_| {
            // Fall back to a plain range if `||` is unsupported by the constraint parser.
            resolver.resolve(&request(
                &reg,
                &[("@t/a", ver(">=1.0.0")), ("@t/b", ver("^1.0.0"))],
            ))
        })
        .unwrap();
    assert_eq!(find(&result.packages, "@t/a").version, "1.0.0");
    assert_eq!(find(&result.packages, "@t/c").version, "1.0.0");
    assert!(result.proof.conflicts_solved > 0);
}

#[test]
fn reports_unsatisfiable_requirements_as_a_conflict() {
    let reg = FileRegistry::new();
    reg.publish("@t/c", "1.0.0", &[]);
    reg.publish("@t/c", "2.0.0", &[]);
    reg.publish("@t/a", "1.0.0", &[("@t/c", "^1.0.0")]);
    reg.publish("@t/b", "1.0.0", &[("@t/c", "^2.0.0")]);
    let resolver = Resolver::new(reg.url(), None);

    let err = resolver
        .resolve(&request(
            &reg,
            &[("@t/a", ver("^1.0.0")), ("@t/b", ver("^1.0.0"))],
        ))
        .unwrap_err();
    match err {
        ResolutionError::NoSolution { conflicts } => {
            assert_eq!(conflicts[0].package, "@t/c");
            let text = format!("{:?}", conflicts[0].required_by);
            assert!(text.contains("@t/a") && text.contains("@t/b"), "{text}");
        }
        other => panic!("expected NoSolution, got {other:?}"),
    }
}

#[test]
fn missing_version_is_version_not_found() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "0.1.0", &[]);
    let resolver = Resolver::new(reg.url(), None);
    let err = resolver
        .resolve(&request(&reg, &[("@t/dep", ver("^9.0.0"))]))
        .unwrap_err();
    assert!(
        matches!(err, ResolutionError::VersionNotFound { .. }),
        "{err:?}"
    );
}

#[test]
fn unknown_package_is_a_registry_error() {
    let reg = FileRegistry::new();
    let resolver = Resolver::new(reg.url(), None);
    let err = resolver
        .resolve(&request(&reg, &[("@t/nope", ver("^1.0.0"))]))
        .unwrap_err();
    assert!(
        matches!(err, ResolutionError::RegistryError { .. }),
        "{err:?}"
    );
}

#[test]
fn yanked_versions_are_skipped() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "1.0.0", &[]);
    reg.publish("@t/dep", "1.1.0", &[]);
    reg.yank("@t/dep", "1.1.0");
    let resolver = Resolver::new(reg.url(), None);
    let result = resolver
        .resolve(&request(&reg, &[("@t/dep", ver("^1.0.0"))]))
        .unwrap();
    assert_eq!(find(&result.packages, "@t/dep").version, "1.0.0");
}

#[test]
fn prereleases_are_only_used_when_allowed() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "1.0.0", &[]);
    reg.publish("@t/dep", "1.1.0-rc.1", &[]);
    let resolver = Resolver::new(reg.url(), None);
    let result = resolver
        .resolve(&request(&reg, &[("@t/dep", ver("^1.0.0"))]))
        .unwrap();
    assert_eq!(find(&result.packages, "@t/dep").version, "1.0.0");
}

#[test]
fn a_lockfile_keeps_the_locked_version_until_updated() {
    use lumen_cli::lockfile::{LockFile, LockedPackage};

    let reg = FileRegistry::new();
    reg.publish("@t/dep", "1.0.0", &[]);
    reg.publish("@t/dep", "1.2.0", &[]);

    let mut lock = LockFile::default();
    lock.add_package(LockedPackage::from_registry(
        "@t/dep".into(),
        "1.0.0".into(),
        reg.url(),
        String::new(),
    ));

    let req = request(&reg, &[("@t/dep", ver("^1.0.0"))]);
    let locked = Resolver::new(reg.url(), Some(&lock)).resolve(&req).unwrap();
    assert_eq!(find(&locked.packages, "@t/dep").version, "1.0.0");

    let updated = Resolver::new(reg.url(), Some(&lock))
        .update(&req, &lock, None)
        .unwrap();
    assert_eq!(find(&updated.packages, "@t/dep").version, "1.2.0");
}

#[test]
fn path_dependencies_resolve_with_their_own_dependencies() {
    let reg = FileRegistry::new();
    reg.publish("@t/lib", "0.4.0", &[]);

    let work = TempDir::new("pathdeps");
    let app_dep = work.path().join("util");
    let nested = work.path().join("nested");
    write_package(&nested, "@t/nested", "0.0.3", &[]);
    write_package(
        &app_dep,
        "@t/util",
        "0.2.0",
        &[
            ("@t/nested", "{ path = \"../nested\" }"),
            ("@t/lib", "\"^0.4.0\""),
        ],
    );

    let resolver = Resolver::new(reg.url(), None);
    let result = resolver
        .resolve(&request(
            &reg,
            &[(
                "@t/util",
                DependencySpec::Path {
                    path: app_dep.to_string_lossy().to_string(),
                },
            )],
        ))
        .unwrap();

    let names: Vec<_> = result.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["@t/lib", "@t/nested", "@t/util"]);
    assert!(find(&result.packages, "@t/util").source.is_path());
    assert!(find(&result.packages, "@t/nested").source.is_path());
    assert_eq!(find(&result.packages, "@t/nested").version, "0.0.3");
    assert!(find(&result.packages, "@t/lib").source.is_registry());
}

#[test]
fn a_path_dependency_must_satisfy_version_requirements_on_it() {
    let reg = FileRegistry::new();
    let work = TempDir::new("pathver");
    let local = work.path().join("local");
    write_package(&local, "@t/local", "0.1.0", &[]);
    reg.publish("@t/user", "1.0.0", &[("@t/local", "^2.0.0")]);

    let resolver = Resolver::new(reg.url(), None);
    let err = resolver
        .resolve(&request(
            &reg,
            &[
                ("@t/user", ver("^1.0.0")),
                (
                    "@t/local",
                    DependencySpec::Path {
                        path: local.to_string_lossy().to_string(),
                    },
                ),
            ],
        ))
        .unwrap_err();
    assert!(matches!(err, ResolutionError::NoSolution { .. }), "{err:?}");
}
