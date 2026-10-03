//! End-to-end: `wares install` / `build` / `pack` against path dependencies
//! and a `file://` registry, exercising resolution, download, checksum
//! verification, unpacking and the lockfile.

mod common;

use common::{write_package, FileRegistry, TempDir};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Project {
    _tmp: TempDir,
    dir: PathBuf,
    cache: PathBuf,
}

impl Project {
    fn new(label: &str, deps: &[(&str, &str)]) -> Self {
        let tmp = TempDir::new(label);
        let dir = tmp.path().join("app");
        write_package(&dir, "@t/app", "0.1.0", deps);
        let cache = tmp.path().join("regcache");
        Project {
            dir,
            cache,
            _tmp: tmp,
        }
    }

    fn wares(&self, registry: Option<&FileRegistry>, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_wares"));
        cmd.args(args)
            .current_dir(&self.dir)
            .env("LUMEN_REGISTRY_DIR", &self.cache)
            .env_remove("WARES_REGISTRY");
        match registry {
            Some(r) => cmd.env("LUMEN_REGISTRY", r.url()),
            None => cmd.env_remove("LUMEN_REGISTRY"),
        };
        cmd.output().expect("failed to run wares")
    }

    fn lock(&self) -> String {
        std::fs::read_to_string(self.dir.join("lumen.lock")).expect("lumen.lock missing")
    }

    fn installed(&self, name: &str, version: &str) -> PathBuf {
        self.cache.join("installed").join(name).join(version)
    }
}

fn text(o: &Output) -> String {
    format!(
        "status={:?}\nstdout:\n{}\nstderr:\n{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn installs_a_path_dependency() {
    let proj = Project::new("pathdep", &[("@t/util", "{ path = \"../util\" }")]);
    write_package(
        &proj.dir.parent().unwrap().join("util"),
        "@t/util",
        "0.2.0",
        &[],
    );

    let out = proj.wares(None, &["install"]);
    assert!(out.status.success(), "{}", text(&out));

    let lock = proj.lock();
    assert!(lock.contains("@t/util"), "{lock}");
    assert!(lock.contains("0.2.0"), "{lock}");
    assert!(lock.contains("../util"), "{lock}");
}

#[test]
fn installs_transitive_path_dependencies() {
    let proj = Project::new("pathchain", &[("@t/a", "{ path = \"../a\" }")]);
    let root = proj.dir.parent().unwrap();
    write_package(
        &root.join("a"),
        "@t/a",
        "0.1.0",
        &[("@t/b", "{ path = \"../b\" }")],
    );
    write_package(&root.join("b"), "@t/b", "0.3.0", &[]);

    let out = proj.wares(None, &["install"]);
    assert!(out.status.success(), "{}", text(&out));
    let lock = proj.lock();
    assert!(lock.contains("@t/a") && lock.contains("@t/b"), "{lock}");
}

#[test]
fn installs_registry_dependencies_with_dependencies() {
    let reg = FileRegistry::new();
    reg.publish("@t/leaf", "1.0.0", &[]);
    reg.publish("@t/leaf", "1.1.0", &[]);
    reg.publish("@t/mid", "0.2.0", &[("@t/leaf", "^1.0.0")]);
    let proj = Project::new("regdeps", &[("@t/mid", "\"^0.2.0\"")]);

    let out = proj.wares(Some(&reg), &["install"]);
    assert!(out.status.success(), "{}", text(&out));

    // Both packages were downloaded, verified and unpacked.
    assert!(
        proj.installed("@t/mid", "0.2.0")
            .join("lumen.toml")
            .exists(),
        "{}",
        text(&out)
    );
    assert!(
        proj.installed("@t/leaf", "1.1.0")
            .join("lumen.toml")
            .exists(),
        "{}",
        text(&out)
    );
    assert!(proj
        .installed("@t/leaf", "1.1.0")
        .join("src/main.lm.md")
        .exists());

    let lock = proj.lock();
    assert!(
        lock.contains("@t/mid") && lock.contains("@t/leaf"),
        "{lock}"
    );
    assert!(lock.contains("1.1.0"), "{lock}");
    assert!(lock.contains("sha256:"), "{lock}");
}

#[test]
fn mixes_path_and_registry_dependencies_and_builds() {
    let reg = FileRegistry::new();
    reg.publish("@t/remote", "0.5.0", &[]);
    let proj = Project::new(
        "mixed",
        &[
            ("@t/local", "{ path = \"../local\" }"),
            ("@t/remote", "\"^0.5.0\""),
        ],
    );
    write_package(
        &proj.dir.parent().unwrap().join("local"),
        "@t/local",
        "0.1.0",
        &[("@t/remote", "\"^0.5.0\"")],
    );

    let out = proj.wares(Some(&reg), &["build"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("build succeeded"),
        "{}",
        text(&out)
    );
}

#[test]
fn install_is_repeatable_and_frozen_passes_afterwards() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "0.1.0", &[]);
    let proj = Project::new("frozen", &[("@t/dep", "\"^0.1.0\"")]);

    assert!(proj.wares(Some(&reg), &["install"]).status.success());
    let first = proj.lock();

    let again = proj.wares(Some(&reg), &["install"]);
    assert!(again.status.success(), "{}", text(&again));
    assert_eq!(first, proj.lock());

    let frozen = proj.wares(Some(&reg), &["install", "--frozen"]);
    assert!(frozen.status.success(), "{}", text(&frozen));
}

#[test]
fn a_new_registry_release_does_not_change_a_locked_install() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "0.1.0", &[]);
    let proj = Project::new("locked", &[("@t/dep", "\"^0.1.0\"")]);
    assert!(proj.wares(Some(&reg), &["install"]).status.success());

    reg.publish("@t/dep", "0.1.9", &[]);
    let out = proj.wares(Some(&reg), &["install", "--frozen"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(proj.lock().contains("0.1.0"));

    let update = proj.wares(Some(&reg), &["update"]);
    assert!(update.status.success(), "{}", text(&update));
    assert!(proj.lock().contains("0.1.9"), "{}", proj.lock());
}

#[test]
fn unsatisfiable_dependencies_fail_with_a_readable_conflict() {
    let reg = FileRegistry::new();
    reg.publish("@t/c", "1.0.0", &[]);
    reg.publish("@t/c", "2.0.0", &[]);
    reg.publish("@t/a", "1.0.0", &[("@t/c", "^1.0.0")]);
    reg.publish("@t/b", "1.0.0", &[("@t/c", "^2.0.0")]);
    let proj = Project::new(
        "conflict",
        &[("@t/a", "\"^1.0.0\""), ("@t/b", "\"^1.0.0\"")],
    );

    let out = proj.wares(Some(&reg), &["install"]);
    assert!(!out.status.success(), "{}", text(&out));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("@t/c"), "{err}");
    assert!(!err.contains("Too many conflicts"), "{err}");
}

#[test]
fn a_tampered_artifact_is_rejected_and_never_installed() {
    let reg = FileRegistry::new();
    let artifact = reg.publish("@t/dep", "0.1.0", &[]);
    std::fs::write(&artifact, b"not the package you published").unwrap();
    let proj = Project::new("tamper", &[("@t/dep", "\"^0.1.0\"")]);

    for attempt in 0..2 {
        let out = proj.wares(Some(&reg), &["install"]);
        assert!(!out.status.success(), "attempt {attempt}: {}", text(&out));
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("checksum mismatch"),
            "attempt {attempt}: {}",
            text(&out)
        );
        assert!(!proj.installed("@t/dep", "0.1.0").exists());
        // No half-downloaded file may be left behind for the next run to trust.
        let cached = proj.cache.join("cache");
        assert!(
            !contains_file(&cached),
            "partial download left on disk at {}",
            cached.display()
        );
    }
}

#[test]
fn a_corrupted_cached_tarball_is_discarded_and_redownloaded() {
    let reg = FileRegistry::new();
    reg.publish("@t/dep", "0.1.0", &[]);
    let proj = Project::new("cache", &[("@t/dep", "\"^0.1.0\"")]);
    assert!(proj.wares(Some(&reg), &["install"]).status.success());

    // Corrupt the cache and remove the install so the next run must re-fetch it.
    let cached = find_file(&proj.cache.join("cache")).expect("cached tarball");
    std::fs::write(&cached, b"corrupted").unwrap();
    std::fs::remove_dir_all(proj.installed("@t/dep", "0.1.0")).unwrap();

    let out = proj.wares(Some(&reg), &["install"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(proj
        .installed("@t/dep", "0.1.0")
        .join("lumen.toml")
        .exists());
}

#[test]
fn the_default_registry_is_not_the_website() {
    // `search` talks to the registry; with LUMEN_REGISTRY unset the default is used,
    // so just assert the compiled-in default targets the worker API.
    assert!(lumen_cli::config::DEFAULT_REGISTRY_URL.ends_with("/api/v1"));
    assert!(!lumen_cli::config::DEFAULT_REGISTRY_URL.contains("lumen-lang.com"));
}

#[test]
fn pack_works_for_scoped_package_names() {
    let tmp = TempDir::new("pack");
    let dir = tmp.path().join("pkg");
    write_package(&dir, "@t/packme", "0.3.0", &[]);

    let out = Command::new(env!("CARGO_BIN_EXE_wares"))
        .arg("pack")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let tgz = dir.join("dist").join("t-packme-0.3.0.tgz");
    assert!(tgz.exists(), "{}", text(&out));

    // The packed archive is a valid gzip tar containing the manifest and sources.
    let decoder = flate2::read::GzDecoder::new(std::fs::File::open(&tgz).unwrap());
    let mut archive = tar::Archive::new(decoder);
    let names: Vec<String> = archive
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(names.iter().any(|n| n.ends_with("lumen.toml")), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("main.lm.md")), "{names:?}");
}

#[test]
fn packed_packages_can_be_served_and_installed() {
    // pack -> publish into a file registry -> install: the full round trip.
    let tmp = TempDir::new("roundtrip");
    let pkg = tmp.path().join("pkg");
    write_package(&pkg, "@t/round", "1.2.3", &[]);
    let out = Command::new(env!("CARGO_BIN_EXE_wares"))
        .arg("pack")
        .current_dir(&pkg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let bytes = std::fs::read(pkg.join("dist/t-round-1.2.3.tgz")).unwrap();

    let reg = FileRegistry::new();
    let artifact = reg.publish("@t/round", "1.2.3", &[]);
    std::fs::write(&artifact, &bytes).unwrap();
    // Re-point the metadata hash at the packed bytes.
    let hex = common::sha256_hex(&bytes);
    let meta_path = reg.root.join("packages/@t/round/1.2.3.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["artifacts"][0]["hash"] = serde_json::json!(format!("sha256:{hex}"));
    std::fs::write(&meta_path, meta.to_string()).unwrap();

    let proj = Project::new("roundtrip-app", &[("@t/round", "\"^1.2.0\"")]);
    let out = proj.wares(Some(&reg), &["install"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(proj
        .installed("@t/round", "1.2.3")
        .join("lumen.toml")
        .exists());
}

fn contains_file(dir: &Path) -> bool {
    find_file(dir).is_some()
}

fn find_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p) {
                return Some(f);
            }
        } else {
            return Some(p);
        }
    }
    None
}

#[test]
fn init_creates_a_flat_directory_with_a_lint_clean_template() {
    let tmp = TempDir::new("init");
    let out = Command::new(env!("CARGO_BIN_EXE_wares"))
        .args(["init", "@t/fresh"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));

    // `./fresh`, not a nested `./@t/fresh` tree.
    let pkg = tmp.path().join("fresh");
    assert!(pkg.join("lumen.toml").exists(), "{}", text(&out));
    assert!(!tmp.path().join("@t").exists());
    let manifest = std::fs::read_to_string(pkg.join("lumen.toml")).unwrap();
    assert!(manifest.contains("name = \"@t/fresh\""), "{manifest}");

    // The generated sources pass the strict linter and type-check.
    let lint = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args(["lint", "--strict"])
        .arg(pkg.join("src/main.lm.md"))
        .output()
        .unwrap();
    let lint_text = text(&lint);
    assert!(lint.status.success(), "{lint_text}");
    assert!(
        !lint_text.contains("warning(s)") && !lint_text.contains("error["),
        "{lint_text}"
    );
    let check = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .arg("check")
        .arg(pkg.join("src/main.lm.md"))
        .output()
        .unwrap();
    assert!(check.status.success(), "{}", text(&check));
}

#[test]
fn init_rejects_bad_names_without_creating_anything() {
    let tmp = TempDir::new("initbad");
    for bad in ["plain", "@t/Upper", "@t/a/b", "../x", "@t/"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wares"))
            .args(["init", bad])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(!out.status.success(), "{bad}: {}", text(&out));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("lumen pkg"), "{stderr}");
    }
    assert_eq!(
        std::fs::read_dir(tmp.path()).unwrap().count(),
        0,
        "init left files behind"
    );
}

fn publish_in(dir: &Path, home: &Path, registry: &FileRegistry, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wares"))
        .arg("publish")
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("LUMEN_REGISTRY", registry.url())
        .env_remove("LUMEN_AUTH_TOKEN")
        .env_remove("WARES_REGISTRY")
        .output()
        .unwrap()
}

#[test]
fn publish_dry_run_builds_the_archive_without_credentials() {
    let tmp = TempDir::new("pubdry");
    let reg = FileRegistry::new();
    reg.publish("@t/lib", "1.0.0", &[]);
    let pkg = tmp.path().join("pkg");
    write_package(&pkg, "@t/pub", "0.1.0", &[("@t/lib", "\"^1.0.0\"")]);

    let out = publish_in(&pkg, tmp.path(), &reg, &["--dry-run"]);
    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Dry run"), "{stdout}");
    assert!(stdout.contains("sha256:"), "{stdout}");
}

#[test]
fn publish_refuses_path_dependencies() {
    let tmp = TempDir::new("pubpath");
    let reg = FileRegistry::new();
    let pkg = tmp.path().join("pkg");
    write_package(&pkg, "@t/pub", "0.1.0", &[("@t/local", "{ path = \"../local\" }")]);

    let out = publish_in(&pkg, tmp.path(), &reg, &["--dry-run"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be published"), "{}", text(&out));
}

#[test]
fn publish_without_credentials_fails_cleanly() {
    let tmp = TempDir::new("pubauth");
    let reg = FileRegistry::new();
    let pkg = tmp.path().join("pkg");
    write_package(&pkg, "@t/pub", "0.1.0", &[]);

    let out = publish_in(&pkg, tmp.path(), &reg, &[]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("wares login"),
        "{}",
        text(&out)
    );
}
