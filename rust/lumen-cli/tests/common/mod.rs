//! Shared helpers for the package-manager integration tests: a throwaway
//! directory, package fixtures, and a `file://` registry builder that writes
//! the same layout `RegistryClient` reads.
#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A unique temp directory removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "lumen-wares-test-{}-{}-{}",
            label,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Canonicalize so paths compare equal to what the tool reports (macOS /var symlink).
        TempDir(std::fs::canonicalize(&dir).unwrap())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write a package directory: `lumen.toml` plus a trivial source file.
/// `deps` are rendered verbatim as `key = value` lines (so tests can use
/// either `"^0.1.0"` or `{ path = "../x" }`).
pub fn write_package(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut toml =
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n");
    for (dep, spec) in deps {
        toml.push_str(&format!("\"{dep}\" = {spec}\n"));
    }
    std::fs::write(dir.join("lumen.toml"), toml).unwrap();
    std::fs::write(
        dir.join("src").join("main.lm.md"),
        format!("# {name}\n\n```lumen\ncell main() -> String\n  return \"hello from {name}\"\nend\n```\n"),
    )
    .unwrap();
}

/// Build a `.tgz` of a package directory the way `wares pack` lays it out.
pub fn tgz_of(dir: &Path) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    tar.append_dir_all(".", dir).unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// A `file://` package registry on disk.
pub struct FileRegistry {
    pub root: PathBuf,
    scratch: TempDir,
}

impl FileRegistry {
    pub fn new() -> Self {
        let scratch = TempDir::new("registry");
        let root = scratch.path().join("registry");
        std::fs::create_dir_all(&root).unwrap();
        FileRegistry { root, scratch }
    }

    pub fn url(&self) -> String {
        format!("file://{}", self.root.display())
    }

    fn pkg_dir(&self, name: &str) -> PathBuf {
        self.root.join("packages").join(name)
    }

    /// Publish `name@version` with the given `(dep, constraint)` pairs. Returns the artifact path.
    pub fn publish(&self, name: &str, version: &str, deps: &[(&str, &str)]) -> PathBuf {
        let src = self.scratch.path().join("src").join(name).join(version);
        let toml_deps: Vec<(&str, String)> =
            deps.iter().map(|(d, c)| (*d, format!("\"{c}\""))).collect();
        let refs: Vec<(&str, &str)> = toml_deps.iter().map(|(d, s)| (*d, s.as_str())).collect();
        write_package(&src, name, version, &refs);

        let bytes = tgz_of(&src);
        let hex = sha256_hex(&bytes);
        let rel = format!("artifacts/sha256/{}/{}", &hex[..2], &hex[2..]);
        let artifact = self.root.join(&rel);
        std::fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        std::fs::write(&artifact, &bytes).unwrap();

        let dir = self.pkg_dir(name);
        std::fs::create_dir_all(&dir).unwrap();
        let deps_map: BTreeMap<&str, &str> = deps.iter().copied().collect();
        let meta = serde_json::json!({
            "name": name,
            "version": version,
            "deps": deps_map,
            "artifacts": [{ "kind": "tgz", "url": rel, "hash": format!("sha256:{hex}") }],
        });
        std::fs::write(dir.join(format!("{version}.json")), meta.to_string()).unwrap();

        let index_path = dir.join("index.json");
        let mut versions: Vec<String> = std::fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| serde_json::from_value(v["versions"].clone()).ok())
            .unwrap_or_default();
        if !versions.iter().any(|v| v == version) {
            versions.push(version.to_string());
        }
        self.write_index(name, &versions, &[]);
        artifact
    }

    /// Mark versions as yanked in the package index.
    pub fn yank(&self, name: &str, version: &str) {
        let index_path = self.pkg_dir(name).join("index.json");
        let mut idx: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&index_path).unwrap()).unwrap();
        idx["yanked"] = serde_json::json!({ version: "yanked in test" });
        std::fs::write(index_path, idx.to_string()).unwrap();
    }

    fn write_index(&self, name: &str, versions: &[String], yanked: &[&str]) {
        let yanked_map: BTreeMap<&str, &str> = yanked.iter().map(|v| (*v, "yanked")).collect();
        let idx = serde_json::json!({
            "name": name,
            "versions": versions,
            "latest": versions.last(),
            "yanked": yanked_map,
        });
        std::fs::write(self.pkg_dir(name).join("index.json"), idx.to_string()).unwrap();
    }
}
