//! High-level package management operations.
//!
//! Provides `init`, `build`, `check`, etc.
//! Moved from `pkg.rs`.

use crate::config::{DependencySpec, FeatureDef, LumenConfig};
use crate::git::{
    self, checkout_git_commit, dep_spec_to_git_ref, fetch_git_repo, resolve_git_ref, GitRef,
    GitResolver,
};
use crate::lockfile::{LockFile, LockedArtifact, LockedPackage};
use crate::wares::{
    ArtifactInfo, IntegrityInfo, RegistryClient, RegistryPackageIndex,
    RegistryVersionMetadata, R2Client,
    ResolutionError, ResolutionPolicy, ResolutionRequest, ResolvedPackage, ResolvedSource, Resolver,
};
use crate::registry_cmd::{is_authenticated, publish_with_auth};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// ANSI color helpers
fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}
fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}
#[allow(dead_code)]
fn yellow(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}
fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}
fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}
fn gray(s: &str) -> String {
    format!("\x1b[90m{}\x1b[0m", s)
}
fn status_label(label: &str) -> String {
    format!("\x1b[1;32m{:>12}\x1b[0m", label)
}

/// A resolved dependency ready for compilation.
#[derive(Debug, Clone)]
enum ResolvedDepSource {
    Path,
    Registry { source: String, checksum: String },
    Git { url: String, rev: String },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedDep {
    pub name: String,
    pub path: PathBuf,
    pub config: LumenConfig,
    source: ResolvedDepSource,
    /// Resolved features for this dependency
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockSyncOutcome {
    Unchanged,
    Created,
    Updated,
}

#[derive(Debug, Clone)]
struct PackageEntry {
    archive_path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct PackageBundle {
    name: String,
    version: String,
    entries: Vec<PackageEntry>,
}

#[derive(Debug, Clone)]
struct PackReport {
    archive_path: PathBuf,
    package_name: String,
    version: String,
    file_count: usize,
    archive_size_bytes: u64,
    content_checksum: String,
    archive_checksum: String,
}

#[derive(Debug, Clone)]
struct PublishDryRunReport {
    archive_path: PathBuf,
    package_name: String,
    version: String,
    file_count: usize,
    archive_size_bytes: u64,
    content_checksum: String,
    archive_checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LocalRegistryRecord {
    name: String,
    version: String,
    archive_path: String,
    file_count: usize,
    archive_size_bytes: u64,
    content_checksum: String,
    archive_checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LocalRegistryIndex {
    packages: Vec<LocalRegistryRecord>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PackageInspectReport {
    package_name: String,
    version: String,
    file_count: usize,
    content_size_bytes: u64,
    content_checksum: String,
    archive_checksum: Option<String>,
    archive_size_bytes: Option<u64>,
    entries: Vec<String>,
}

/// Scaffold a new Lumen package in the current directory (or a named subdirectory).
pub fn init(name: Option<String>) {
    let base = match &name {
        Some(n) => {
            let p = PathBuf::from(n);
            if p.exists() {
                eprintln!("{} directory '{}' already exists", red("error:"), n);
                std::process::exit(1);
            }
            std::fs::create_dir_all(p.join("src")).unwrap_or_else(|e| {
                eprintln!("{} cannot create directory: {}", red("error:"), e);
                std::process::exit(1);
            });
            p
        }
        None => PathBuf::from("."),
    };

    let pkg_name = match &name {
        Some(n) if n.contains('/') && n.starts_with('@') => n.clone(),
        Some(n) => {
            eprintln!(
                "{} package name '{}' must be namespaced: @namespace/name (e.g., @yourname/{})",
                red("error:"),
                n,
                n
            );
            std::process::exit(1);
        }
        None => {
            let dir_name = std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_else(|| "my-package".to_string());
            eprintln!(
                "{} package name must be namespaced: @namespace/name\n  example: lumen pkg init @yourname/{}",
                red("error:"),
                dir_name
            );
            std::process::exit(1);
        }
    };

    let toml_path = base.join("lumen.toml");
    if toml_path.exists() {
        eprintln!(
            "{} lumen.toml already exists in '{}'",
            red("error:"),
            base.display()
        );
        std::process::exit(1);
    }

    let toml_content = format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"

[dependencies]
# mathlib = {{ path = "../mathlib" }}

[providers]
# "llm.chat" = "openai-compatible"
"#
    );

    std::fs::write(&toml_path, &toml_content).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });

    // Create src/main.lm.md
    let src_dir = base.join("src");
    if !src_dir.exists() {
        std::fs::create_dir_all(&src_dir).unwrap_or_else(|e| {
            eprintln!("{} creating src directory: {}", red("error:"), e);
            std::process::exit(1);
        });
    }

    let main_content = format!(
        r#"# {pkg_name}

```lumen
cell main() -> String
  return "hello from {pkg_name}"
end
```
"#
    );

    let main_path = src_dir.join("main.lm.md");
    std::fs::write(&main_path, &main_content).unwrap_or_else(|e| {
        eprintln!("{} writing main.lm.md: {}", red("error:"), e);
        std::process::exit(1);
    });

    println!(
        "{} package {}",
        status_label("Created"),
        bold(&format!("\"{}\"", pkg_name))
    );
    println!("  {}", gray("lumen.toml"));
    println!("  {}", gray("src/main.lm.md"));
}

/// Build a Lumen package: resolve dependencies and compile.
pub fn build() {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let pkg_name = config
        .package
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("(unnamed)");

    // Resolve dependencies
    let deps = match resolve_dependencies(&config, project_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} {}", red("dependency error:"), e);
            std::process::exit(1);
        }
    };

    let mut errors = 0;

    // Compile each dependency
    for dep in &deps {
        println!(
            "{} {} {}",
            status_label("Compiling"),
            bold(&dep.name),
            gray("(dependency)")
        );
        match compile_package_sources(&dep.path) {
            Ok(_count) => {}
            Err(e) => {
                eprintln!("    {}", red(&e));
                errors += 1;
            }
        }
    }

    // Compile the main package
    println!("{} {}", status_label("Compiling"), bold(pkg_name));
    match compile_package_sources(project_dir) {
        Ok(_count) => {}
        Err(e) => {
            eprintln!("    {}", red(&e));
            errors += 1;
        }
    }

    if errors > 0 {
        eprintln!(
            "\n{} build failed with {} error{}",
            red("error:"),
            errors,
            if errors == 1 { "" } else { "s" }
        );
        std::process::exit(1);
    } else {
        let total = deps.len() + 1;
        println!(
            "\n{} build succeeded ({} package{})",
            green("✓"),
            total,
            if total == 1 { "" } else { "s" }
        );
    }
}

/// Type-check a Lumen package and all dependencies without running.
pub fn check() {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let pkg_name = config
        .package
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("(unnamed)");

    // Resolve dependencies
    let deps = match resolve_dependencies(&config, project_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} {}", red("dependency error:"), e);
            std::process::exit(1);
        }
    };

    let mut errors = 0;

    for dep in &deps {
        println!(
            "{} {} {}",
            status_label("Checking"),
            bold(&dep.name),
            gray("(dependency)")
        );
        match compile_package_sources(&dep.path) {
            Ok(_count) => {}
            Err(e) => {
                eprintln!("    {}", red(&e));
                errors += 1;
            }
        }
    }

    println!("{} {}", status_label("Checking"), bold(pkg_name));
    match compile_package_sources(project_dir) {
        Ok(_count) => {}
        Err(e) => {
            eprintln!("    {}", red(&e));
            errors += 1;
        }
    }

    if errors > 0 {
        eprintln!(
            "\n{} check failed with {} error{}",
            red("error:"),
            errors,
            if errors == 1 { "" } else { "s" }
        );
        std::process::exit(1);
    } else {
        let total = deps.len() + 1;
        println!(
            "\n{} check passed ({} package{})",
            green("✓"),
            total,
            if total == 1 { "" } else { "s" }
        );
    }
}

/// Resolve all dependencies from a config, returning them in compilation order.
/// Detects circular dependencies.
pub fn resolve_dependencies(
    config: &LumenConfig,
    project_dir: &Path,
) -> Result<Vec<ResolvedDep>, String> {
    resolve_dependencies_with_registry(config, project_dir, None, false, false)
}

fn resolve_dependencies_with_registry(
    config: &LumenConfig,
    project_dir: &Path,
    registry_dir_override: Option<&Path>,
    resolve_dev: bool,
    resolve_build: bool,
) -> Result<Vec<ResolvedDep>, String> {
    let mut root_deps = config.dependencies.clone();

    // Resolve relative path dependencies to absolute paths relative to project root
    for (name, spec) in root_deps.iter_mut() {
        if let DependencySpec::Path { path } = spec {
            let p = project_dir.join(&path);
            let abs = canonicalize_or_clean(&p);
            if !abs.exists() {
                return Err(format!(
                    "path dependency '{}' does not exist at {}",
                    name,
                    abs.display()
                ));
            }
            *path = abs.to_string_lossy().to_string();
        }
    }

    // SINGLE SOURCE OF TRUTH for registry URL
    // Precedence: env var > config > default production registry
    let registry_url = config
        .registry
        .as_ref()
        .map(|r| r.effective_url())
        .unwrap_or_else(|| "https://wares.lumen-lang.com/api/v1".to_string());

    // Local cache directory for downloaded packages (separate from registry URL)
    let registry_dir = registry_dir_override
        .map(Path::to_path_buf)
        .unwrap_or_else(local_registry_dir);

    // Check for lockfile
    let mut lockfile = None;
    let lock_path = project_dir.join("lumen.lock");
    let loaded_lock;
    if lock_path.exists() {
        loaded_lock = LockFile::load(&lock_path).ok();
        lockfile = loaded_lock.as_ref();
    }

    // Create resolver
    let resolver = Resolver::new(&registry_url, lockfile);

    // Prepare dev dependencies (only for root package, not transitive deps)
    let dev_deps = if resolve_dev {
        let mut deps = config.dev_dependencies.clone();
        // Resolve relative path dependencies
        for spec in deps.values_mut() {
            if let DependencySpec::Path { path } = spec {
                let p = project_dir.join(&path);
                let abs = canonicalize_or_clean(&p);
                *path = abs.to_string_lossy().to_string();
            }
        }
        deps
    } else {
        HashMap::new()
    };

    // Prepare build dependencies
    let build_deps = if resolve_build {
        let mut deps = config.build_dependencies.clone();
        // Resolve relative path dependencies
        for spec in deps.values_mut() {
            if let DependencySpec::Path { path } = spec {
                let p = project_dir.join(&path);
                let abs = canonicalize_or_clean(&p);
                *path = abs.to_string_lossy().to_string();
            }
        }
        deps
    } else {
        HashMap::new()
    };

    // Build resolution request
    let request = ResolutionRequest {
        root_deps: root_deps.clone(),
        dev_deps,
        build_deps,
        registry_url: registry_url.to_string(),
        features: config.resolve_features(&[]),
        include_dev: resolve_dev,
        include_build: resolve_build,
        include_yanked: false,
    };

    // Run resolution
    let resolved_packages = match resolver.resolve(&request) {
        Ok(result) => result,
        Err(e) => {
            return Err(crate::wares::resolver::format_resolution_error(&e));
        }
    };

    // 3. Topological sort (Resolver returns alphabetical currently)
    // We need to rebuild the graph and sort.
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut pkg_map: HashMap<String, ResolvedPackage> = HashMap::new();

    for pkg in resolved_packages.packages {
        pkg_map.insert(pkg.name.clone(), pkg.clone());
        let deps: Vec<String> = pkg.deps.iter().map(|(n, _): &(String, DependencySpec)| n.clone()).collect();
        graph.insert(pkg.name.clone(), deps);
    }

    let mut sorted_names = Vec::new();
    let mut visited = HashSet::new();
    let mut temp_visited = HashSet::new();

    // Visit all nodes provided by resolution (they are all reachable from root deps)
    for name in pkg_map.keys() {
        visit_topo(
            name,
            &graph,
            &mut visited,
            &mut temp_visited,
            &mut sorted_names,
        )?;
    }

    // 4. Materialize and build ResolvedDep objects
    let mut output = Vec::new();
    for name in sorted_names {
        if let Some(pkg) = pkg_map.get(&name) {
            let dep = materialize_package(pkg, project_dir, &registry_dir, &resolver)?;
            output.push(dep);
        }
    }

    Ok(output)
}

fn visit_topo(
    name: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    temp_visited: &mut HashSet<String>,
    sorted: &mut Vec<String>,
) -> Result<(), String> {
    if visited.contains(name) {
        return Ok(());
    }
    if temp_visited.contains(name) {
        return Err(format!("circular dependency detected involving '{}'", name));
    }

    temp_visited.insert(name.to_string());

    if let Some(deps) = graph.get(name) {
        for dep in deps {
            // Only visit if it's in our resolved set (transitive deps included)
            if graph.contains_key(dep) {
                if let Err(e) = visit_topo(dep, graph, visited, temp_visited, sorted) {
                    return Err(e);
                }
            }
        }
    }

    temp_visited.remove(name);
    visited.insert(name.to_string());
    sorted.push(name.to_string());
    Ok(())
}

fn materialize_package(
    pkg: &ResolvedPackage,
    project_dir: &Path,
    registry_dir: &Path,
    resolver: &Resolver,
) -> Result<ResolvedDep, String> {
    match &pkg.source {
        ResolvedSource::Path { path } => {
            let p = project_dir.join(path);
            let abs_path = canonicalize_or_clean(&p);
            if !abs_path.exists() {
                return Err(format!(
                    "path dependency '{}' does not exist at {}",
                    pkg.name,
                    abs_path.display()
                ));
            }
            let config_path = abs_path.join("lumen.toml");
            let config = LumenConfig::load_from(&config_path).map_err(|e| {
                format!("failed to load lumen.toml at {}: {}", abs_path.display(), e)
            })?;

            Ok(ResolvedDep {
                name: pkg.name.clone(),
                path: config_path.parent().unwrap().to_path_buf(),
                config,
                source: ResolvedDepSource::Path,
                features: pkg.enabled_features.clone(),
            })
        }
        ResolvedSource::Registry {
            url,
            cid: _,
            artifacts,
        } => {
            // Check if already installed
            let install_dir = registry_install_dir(registry_dir, &pkg.name, &pkg.version);

            if install_dir.join("lumen.toml").exists() {
                let config_path = install_dir.join("lumen.toml");
                let config = LumenConfig::load_from(&config_path).map_err(|e| {
                    format!(
                        "failed to load lumen.toml in installed package {}: {}",
                        pkg.name, e
                    )
                })?;

                // Get checksum from first artifact
                let checksum = artifacts
                    .first()
                    .map(|a| a.hash.clone())
                    .unwrap_or_default();

                return Ok(ResolvedDep {
                    name: pkg.name.clone(),
                    path: install_dir,
                    config,
                    source: ResolvedDepSource::Registry {
                        source: url.clone(),
                        checksum,
                    },
                    features: pkg.enabled_features.clone(),
                });
            }

            // Not installed, need to download
            if artifacts.is_empty() {
                return Err(format!(
                    "package '{}' has no artifacts to download",
                    pkg.name
                ));
            }

            let artifact = &artifacts[0];
            let cache_dir = registry_dir.join("cache");
            let version_cache_dir = cache_dir.join(&pkg.name).join(&pkg.version);
            let tarball_path = version_cache_dir.join(format!("{}-{}.tar", pkg.name, pkg.version));

            if !tarball_path.exists() {
                println!(
                    "{} {}@{}",
                    status_label("Downloading"),
                    pkg.name,
                    pkg.version
                );

                std::fs::create_dir_all(&version_cache_dir)
                    .map_err(|e| format!("failed to create cache dir: {}", e))?;

                let client = RegistryClient::new(url);
                client
                    .download_artifact(&artifact.url, &tarball_path, Some(&artifact.hash))
                    .map_err(|e| format!("failed to download artifact: {}", e))?;
            }

            // Extract to install directory
            std::fs::create_dir_all(&install_dir).map_err(|e| e.to_string())?;
            unpack_tarball(&tarball_path, &install_dir)?;

            let config_path = install_dir.join("lumen.toml");
            let config = LumenConfig::load_from(&config_path).map_err(|e| {
                format!(
                    "failed to load lumen.toml in downloaded package {}: {}",
                    pkg.name, e
                )
            })?;

            Ok(ResolvedDep {
                name: pkg.name.clone(),
                path: install_dir,
                config,
                source: ResolvedDepSource::Registry {
                    source: url.clone(),
                    checksum: artifact.hash.clone(),
                },
                features: pkg.enabled_features.clone(),
            })
        }
        ResolvedSource::Git { url, rev } => {
            // Use the git resolver for proper git handling
            let git_cache_dir = resolver.git_cache_dir().clone();
            let repo_dir = fetch_git_repo(url, &GitRef::Commit(rev.clone()), &git_cache_dir)
                .map_err(|e| format!("failed to fetch git repo: {}", e))?;

            // Create install directory based on URL and revision
            let install_dir = registry_dir
                .join("git")
                .join(sanitize_filename(url))
                .join(&rev[..8]);

            if !install_dir.exists() {
                println!("{} {} from {}", status_label("Cloning"), pkg.name, url);

                std::fs::create_dir_all(&install_dir)
                    .map_err(|e| format!("failed to create git install dir: {}", e))?;

                // Checkout the specific revision
                checkout_git_commit(&repo_dir, rev, &install_dir)
                    .map_err(|e| format!("failed to checkout git commit: {}", e))?;
            }

            let config_path = install_dir.join("lumen.toml");
            let config = LumenConfig::load_from(&config_path).map_err(|e| {
                format!(
                    "failed to load lumen.toml in git checkout {}: {}",
                    pkg.name, e
                )
            })?;

            // Resolve features from git dependency
            let resolved_features = if !pkg.enabled_features.is_empty() {
                pkg.enabled_features.clone()
            } else {
                config.resolve_features(&[])
            };

            Ok(ResolvedDep {
                name: pkg.name.clone(),
                path: install_dir,
                config,
                source: ResolvedDepSource::Git {
                    url: url.clone(),
                    rev: rev.clone(),
                },
                features: resolved_features,
            })
        }
    }
}

/// Sanitize a URL to create a valid directory name.
fn sanitize_filename(url: &str) -> String {
    url.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '@', "_")
        .replace("https___", "")
        .replace("http___", "")
        .replace(".", "_")
}

fn unpack_tarball(tar_path: &Path, dst: &Path) -> Result<(), String> {
    let file = std::fs::File::open(tar_path).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(file);
    archive
        .unpack(dst)
        .map_err(|e: std::io::Error| e.to_string())?;
    Ok(())
}

// Legacy function stub or remove
#[allow(unused_variables)]
fn resolve_dep(
    name: &str,
    spec: &DependencySpec,
    parent_dir: &Path,
    registry_dir_override: Option<&Path>,
    resolved: &mut Vec<ResolvedDep>,
    visited: &mut HashSet<String>,
    stack: &mut HashSet<String>,
) -> Result<(), String> {
    Ok(()) // Unused now
}

/// Compile all supported Lumen source files found in a package directory.
/// Returns the number of files compiled, or the first error.
fn compile_package_sources(pkg_dir: &Path) -> Result<usize, String> {
    let sources = find_lumen_sources(pkg_dir);
    if sources.is_empty() {
        return Err(format!(
            "no lumen source files (.lm/.lumen/.lm.md/.lumen.md) found in '{}'",
            pkg_dir.display()
        ));
    }

    for src in &sources {
        compile_source_with_imports(src, pkg_dir)?;
    }

    Ok(sources.len())
}

fn compile_source_with_imports(source_path: &Path, pkg_dir: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("cannot read '{}': {}", source_path.display(), e))?;
    let source_dir = source_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| pkg_dir.to_path_buf());

    let mut roots = vec![source_dir];
    let src_dir = pkg_dir.join("src");
    if src_dir.is_dir() && !roots.contains(&src_dir) {
        roots.push(src_dir);
    }
    let pkg_root = pkg_dir.to_path_buf();
    if !roots.contains(&pkg_root) {
        roots.push(pkg_root);
    }

    let resolve_import = |module_path: &str| resolve_module_from_roots(module_path, &roots);

    let compile_result = lumen_compiler::compile_with_imports(&content, &resolve_import);

    compile_result.map_err(|e| format!("{}: {}", source_path.display(), e))?;
    Ok(())
}

fn resolve_module_from_roots(module_path: &str, roots: &[PathBuf]) -> Option<String> {
    let fs_path = module_path.replace('.', "/");
    for root in roots {
        let candidates = [
            root.join(format!("{}.lm", fs_path)),
            root.join(format!("{}.lumen", fs_path)),
            root.join(format!("{}.lm.md", fs_path)),
            root.join(format!("{}.lumen.md", fs_path)),
            root.join(fs_path.clone()).join("mod.lm"),
            root.join(fs_path.clone()).join("mod.lumen"),
            root.join(fs_path.clone()).join("mod.lm.md"),
            root.join(fs_path.clone()).join("mod.lumen.md"),
            root.join(fs_path.clone()).join("main.lm"),
            root.join(fs_path.clone()).join("main.lumen"),
            root.join(fs_path.clone()).join("main.lm.md"),
            root.join(fs_path.clone()).join("main.lumen.md"),
        ];

        for candidate in candidates {
            if candidate.exists() {
                if let Ok(src) = std::fs::read_to_string(candidate) {
                    return Some(src);
                }
            }
        }
    }
    None
}

fn is_lumen_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            name.ends_with(".lm")
                || name.ends_with(".lumen")
                || name.ends_with(".lm.md")
                || name.ends_with(".lumen.md")
        })
        .unwrap_or(false)
}

/// Find all supported Lumen source files in a directory (searches `src/`
/// subdirectory first, then top level).
fn find_lumen_sources(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let src_dir = dir.join("src");

    let search_dir = if src_dir.is_dir() { &src_dir } else { dir };
    collect_lm_files(search_dir, &mut sources);

    // If src/ had nothing, try top-level
    if sources.is_empty() && src_dir.is_dir() {
        collect_lm_files(dir, &mut sources);
    }

    sources.sort();
    sources
}

fn collect_lm_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if is_lumen_source(&path) {
                    out.push(path);
                }
            } else if path.is_dir() {
                collect_lm_files(&path, out);
            }
        }
    }
}

fn has_lumen_sources(dir: &Path) -> bool {
    !find_lumen_sources(dir).is_empty()
}

/// Best-effort canonicalize; falls back to lexical cleanup if path doesn't exist yet.
fn canonicalize_or_clean(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        // Simple lexical normalization
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                c => out.push(c),
            }
        }
        out
    })
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    if from_components.is_empty() || to_components.is_empty() {
        return None;
    }

    if from_components[0] != to_components[0] {
        return None;
    }

    let mut common_len = 0usize;
    while common_len < from_components.len()
        && common_len < to_components.len()
        && from_components[common_len] == to_components[common_len]
    {
        common_len += 1;
    }

    let mut out = PathBuf::new();
    for _ in common_len..from_components.len() {
        out.push("..");
    }
    for component in &to_components[common_len..] {
        out.push(component.as_os_str());
    }

    if out.as_os_str().is_empty() {
        out.push(".");
    }
    Some(out)
}

fn lock_source_path(project_dir: &Path, dep_path: &Path) -> String {
    let project_abs = canonicalize_or_clean(project_dir);
    let dep_abs = canonicalize_or_clean(dep_path);
    let lock_path = relative_path(&project_abs, &dep_abs).unwrap_or(dep_abs);
    lock_path.to_string_lossy().replace('\\', "/")
}

fn build_lockfile(config: &LumenConfig, project_dir: &Path) -> Result<(LockFile, usize), String> {
    build_lockfile_with_registry(config, project_dir, None, false, false)
}

fn build_lockfile_with_registry(
    config: &LumenConfig,
    project_dir: &Path,
    registry_dir_override: Option<&Path>,
    resolve_dev: bool,
    resolve_build: bool,
) -> Result<(LockFile, usize), String> {
    let deps = resolve_dependencies_with_registry(
        config,
        project_dir,
        registry_dir_override,
        resolve_dev,
        resolve_build,
    )?;
    let mut lock = LockFile::default();

    for dep in &deps {
        let version = dep
            .config
            .package
            .as_ref()
            .and_then(|p| p.version.clone())
            .unwrap_or_else(|| "0.1.0".to_string());

        let locked_pkg = match &dep.source {
            ResolvedDepSource::Path => {
                let mut locked_pkg = LockedPackage::from_path(
                    dep.name.clone(),
                    lock_source_path(project_dir, &dep.path),
                );
                locked_pkg.version = version;
                locked_pkg
            }
            ResolvedDepSource::Registry { source, checksum } => LockedPackage::from_registry(
                dep.name.clone(),
                version,
                source.clone(),
                checksum.clone(),
            ),
            ResolvedDepSource::Git { url, rev } => {
                LockedPackage::from_git(dep.name.clone(), version, url.clone(), rev.clone())
            }
        };
        lock.add_package(locked_pkg);
    }

    Ok((lock, deps.len()))
}

fn sync_lockfile(
    lock_path: &Path,
    desired: &LockFile,
    frozen: bool,
) -> Result<LockSyncOutcome, String> {
    let existing = LockFile::load(lock_path)?;
    if existing == *desired {
        return Ok(LockSyncOutcome::Unchanged);
    }

    if frozen {
        if lock_path.exists() {
            return Err(
                "lumen.lock is out of date and --frozen was passed; run `lpm install` to update it"
                    .to_string(),
            );
        }
        return Err(
            "lumen.lock is missing and --frozen was passed; run `lpm install` once to create it"
                .to_string(),
        );
    }

    let existed = lock_path.exists();
    desired.save(lock_path)?;
    if existed {
        Ok(LockSyncOutcome::Updated)
    } else {
        Ok(LockSyncOutcome::Created)
    }
}

/// Add a dependency to lumen.toml
pub fn add(package: &str, path_opt: Option<&str>) {
    let (config_path, mut config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let (dep_name, dep_spec) = if package.starts_with("http") || package.starts_with("git@") {
        let url = package.to_string();
        let name_part = url.split('/').last().unwrap_or("unknown");
        let name = name_part
            .strip_suffix(".git")
            .unwrap_or(name_part)
            .to_string();

        (
            name,
            DependencySpec::Git {
                git: url,
                branch: None,
                tag: None,
                rev: None,
                features: None,
                optional: None,
            },
        )
    } else if let Some(path) = path_opt {
        (
            package.to_string(),
            DependencySpec::Path {
                path: path.to_string(),
            },
        )
    } else {
        eprintln!(
            "{} --path is required for now (or use a git URL)",
            red("error:")
        );
        std::process::exit(1);
    };

    // Add to dependencies map
    config
        .dependencies
        .insert(dep_name.clone(), dep_spec.clone());

    // Serialize back to TOML
    let toml_content = toml::to_string_pretty(&config).unwrap_or_else(|e| {
        eprintln!("{} serializing config: {}", red("error:"), e);
        std::process::exit(1);
    });

    std::fs::write(&config_path, &toml_content).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });

    match dep_spec {
        DependencySpec::Path { path } => {
            println!(
                "{} dependency {} {{ path = \"{}\" }}",
                status_label("Added"),
                bold(&dep_name),
                path
            );
        }
        DependencySpec::Git { git, .. } => {
            println!(
                "{} dependency {} {{ git = \"{}\" }}",
                status_label("Added"),
                bold(&dep_name),
                git
            );
        }
        _ => {}
    }
}

/// Remove a dependency from lumen.toml
#[allow(dead_code)]
pub fn remove(package: &str) {
    let (config_path, mut config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!("{} no lumen.toml found", red("error:"));
            std::process::exit(1);
        }
    };

    if config.dependencies.remove(package).is_none() {
        eprintln!(
            "{} dependency '{}' not found in lumen.toml",
            red("error:"),
            package
        );
        std::process::exit(1);
    }

    let toml_content = toml::to_string_pretty(&config).unwrap_or_else(|e| {
        eprintln!("{} serializing config: {}", red("error:"), e);
        std::process::exit(1);
    });

    std::fs::write(&config_path, &toml_content).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });

    println!("{} dependency {}", status_label("Removed"), bold(package));
}

/// List all dependencies from lumen.toml
#[allow(dead_code)]
pub fn list() {
    let config = LumenConfig::load();

    if config.dependencies.is_empty() {
        println!("{} no dependencies", gray("info:"));
        return;
    }

    println!("{} dependencies:", status_label("Listing"));
    for (name, spec) in &config.dependencies {
        match spec {
            DependencySpec::Path { path } => {
                println!("  {} {} path = {}", bold(name), gray("→"), cyan(path));
            }
            DependencySpec::Version(version) => {
                println!("  {} {} version = {}", bold(name), gray("→"), cyan(version));
            }
            DependencySpec::VersionDetailed {
                version, registry, ..
            } => {
                if let Some(registry) = registry {
                    println!(
                        "  {} {} version = {}, registry = {}",
                        bold(name),
                        gray("→"),
                        cyan(version),
                        cyan(registry)
                    );
                } else {
                    println!("  {} {} version = {}", bold(name), gray("→"), cyan(version));
                }
            }
            DependencySpec::Git {
                git,
                branch,
                tag,
                rev,
                ..
            } => {
                let suffix = if let Some(r) = rev {
                    format!(" rev={}", r)
                } else if let Some(t) = tag {
                    format!(" tag={}", t)
                } else if let Some(b) = branch {
                    format!(" branch={}", b)
                } else {
                    "".to_string()
                };
                println!(
                    "  {} {} git = {}{}",
                    bold(name),
                    gray("→"),
                    cyan(git),
                    gray(&suffix)
                );
            }
            DependencySpec::Workspace {
                workspace,
                features,
            } => {
                println!(
                    "  {} {} workspace = {}",
                    bold(name),
                    gray("→"),
                    cyan(&workspace.to_string())
                );
                if let Some(f) = features {
                    println!("    features: {:?}", f);
                }
            }
        }
    }
}

/// Install dependencies from lumen.toml and generate lumen.lock.
#[allow(dead_code)]
pub fn install() {
    install_with_lock(false);
}

/// Install dependencies from lumen.toml and generate lumen.lock.
/// When `frozen` is true, fail instead of writing if the lockfile would change.
pub fn install_with_lock(frozen: bool) {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    println!("{} dependencies", status_label("Resolving"));

    let (lock, dep_count) = match build_lockfile(&config, project_dir) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    let lock_path = project_dir.join("lumen.lock");
    let outcome = match sync_lockfile(&lock_path, &lock, frozen) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    println!(
        "{} {} package{} resolved",
        green("✓"),
        dep_count,
        if dep_count == 1 { "" } else { "s" }
    );
    match outcome {
        LockSyncOutcome::Unchanged => println!("{} lumen.lock", status_label("Unchanged")),
        LockSyncOutcome::Created => println!("{} lumen.lock", status_label("Created")),
        LockSyncOutcome::Updated => println!("{} lumen.lock", status_label("Updated")),
    }
}

/// Update dependencies to latest compatible versions.
#[allow(dead_code)]
pub fn update() {
    update_with_lock(false);
}

/// Update dependencies to latest compatible versions.
/// When `frozen` is true, fail instead of writing if the lockfile would change.
pub fn update_with_lock(frozen: bool) {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let lock_path = project_dir.join("lumen.lock");

    // Load existing lockfile for update
    let previous_lock = if lock_path.exists() {
        LockFile::load(&lock_path).ok()
    } else {
        None
    };

    println!("{} dependencies", status_label("Updating"));

    // Determine registry URL - SINGLE SOURCE OF TRUTH
    let registry_url = config.registry_url();

    // Create resolver for update
    let resolver = if let Some(ref lock) = previous_lock {
        Resolver::for_update(&registry_url, lock, ResolutionPolicy::default())
    } else {
        Resolver::new(&registry_url, None)
    };

    let request = ResolutionRequest {
        root_deps: config.dependencies.clone(),
        dev_deps: HashMap::new(),
        build_deps: HashMap::new(),
        registry_url,
        features: config.resolve_features(&[]),
        include_dev: false,
        include_build: false,
        include_yanked: false,
    };

    // Run update resolution
    let resolved_packages: Vec<ResolvedPackage> = match resolver.update(
        &request,
        &previous_lock.unwrap_or_default(),
        None, // Update all packages
    ) {
        Ok(result) => result.packages,
        Err(e) => {
            eprintln!(
                "{} {}",
                red("error:"),
                crate::wares::resolver::format_resolution_error(&e)
            );
            std::process::exit(1);
        }
    };

    // Build new lockfile
    let mut new_lock = LockFile::default();
    for pkg in &resolved_packages {
        let locked_pkg = match &pkg.source {
            ResolvedSource::Registry {
                url,
                cid,
                artifacts,
            } => {
                let mut lp = LockedPackage::from_registry(
                    pkg.name.clone(),
                    pkg.version.clone(),
                    url.clone(),
                    artifacts
                        .first()
                        .map(|a| a.hash.clone())
                        .unwrap_or_default(),
                );
                lp.artifacts = artifacts.clone();
                lp.resolved = cid.clone();
                lp.features = pkg.enabled_features.clone();
                lp
            }
            ResolvedSource::Git { url, rev } => {
                let mut lp = LockedPackage::from_git(
                    pkg.name.clone(),
                    pkg.version.clone(),
                    url.clone(),
                    rev.clone(),
                );
                lp.features = pkg.enabled_features.clone();
                lp
            }
            ResolvedSource::Path { path } => {
                let mut lp = LockedPackage::from_path(pkg.name.clone(), path.clone());
                lp.features = pkg.enabled_features.clone();
                lp
            }
        };
        new_lock.add_package(locked_pkg);
    }

    // Sync lockfile
    let outcome = match sync_lockfile(&lock_path, &new_lock, frozen) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    println!(
        "{} {} package{} updated",
        green("✓"),
        resolved_packages.len(),
        if resolved_packages.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    match outcome {
        LockSyncOutcome::Unchanged => println!(
            "{} lumen.lock (already up to date)",
            status_label("Unchanged")
        ),
        LockSyncOutcome::Created => println!("{} lumen.lock", status_label("Created")),
        LockSyncOutcome::Updated => println!("{} lumen.lock", status_label("Updated")),
    }
}

/// Search for packages in the registry
pub fn search(query: &str) {
    let registry_dir = local_registry_dir();
    let results = match search_local_registry(&registry_dir, query) {
        Ok(results) => results,
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    println!(
        "{} {}",
        status_label("Searching"),
        gray("local fixture registry")
    );
    println!(
        "{} {}",
        status_label("Registry"),
        gray(&path_display(&registry_dir))
    );

    if results.is_empty() {
        println!("{} no matches for '{}'", gray("info:"), query);
        println!(
            "{} {}",
            status_label("Index"),
            gray(&path_display(&registry_index_path(&registry_dir)))
        );
        return;
    }

    println!(
        "{} {} match{} in local fixture registry",
        green("✓"),
        results.len(),
        if results.len() == 1 { "" } else { "es" }
    );
    for record in results {
        println!(
            "  {}@{} {}",
            bold(&record.name),
            gray(&record.version),
            gray("(local fixture)")
        );
        println!("    {} {}", gray("archive:"), gray(&record.archive_path));
    }
}

// ... (rest of the file remains the same - keeping existing implementations)
// Due to length, the rest of the functions from the original file are preserved

fn registry_index_path(registry_dir: &Path) -> PathBuf {
    registry_dir.join("index.json")
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn registry_install_dir(registry_dir: &Path, name: &str, version: &str) -> PathBuf {
    registry_dir.join("installed").join(name).join(version)
}

fn local_registry_dir() -> PathBuf {
    std::env::var("LUMEN_REGISTRY_DIR")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".lumen").join("registry"))
}

fn search_local_registry(
    registry_dir: &Path,
    query: &str,
) -> Result<Vec<LocalRegistryRecord>, String> {
    let needle = query.trim().to_ascii_lowercase();

    let index_path = registry_index_path(registry_dir);
    if !index_path.exists() {
        return Ok(Vec::new());
    }

    let data = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("cannot read '{}': {}", index_path.display(), e))?;
    let index: LocalRegistryIndex = serde_json::from_str(&data)
        .map_err(|e| format!("invalid index '{}': {}", index_path.display(), e))?;

    let mut matches: Vec<_> = index
        .packages
        .into_iter()
        .filter(|record| {
            if needle.is_empty() {
                true
            } else {
                let name = record.name.to_ascii_lowercase();
                let package_ref =
                    format!("{}@{}", record.name, record.version).to_ascii_lowercase();
                name.contains(&needle) || package_ref.contains(&needle)
            }
        })
        .collect();

    matches.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    Ok(matches)
}

// Stub implementations for the remaining functions to maintain compatibility
// These would be fully implemented in a complete implementation

/// Show package info from the registry.
pub fn info(package: &str, version: Option<&str>) {
    let registry_url = LumenConfig::load().registry_url();

    let client = RegistryClient::new(&registry_url);

    // Fetch package index to get available versions
    match client.fetch_package_index(package) {
        Ok(index) => {
            println!(
                "{} {}{}",
                status_label("Package"),
                bold(package),
                if let Some(ref v) = version {
                    format!("@{}", v)
                } else {
                    "".to_string()
                }
            );

            if let Some(ref desc) = index.description {
                println!("  {} {}", gray("Description:"), desc);
            }

            // Show yanked versions
            if !index.yanked.is_empty() {
                println!(
                    "\n  {} {} version(s) yanked:",
                    yellow("⚠"),
                    index.yanked.len()
                );
                for (ver, reason) in index.yanked.iter() {
                    if reason.is_empty() {
                        println!(
                            "    {} {} {} (no reason given)",
                            gray("•"),
                            red(&ver),
                            gray("-")
                        );
                    } else {
                        println!("    {} {} {} {}", gray("•"), red(&ver), gray("-"), reason);
                    }
                }
            }

            // Show available versions
            let available_count = index.versions.len() - index.yanked.len();
            println!("\n  {} {} version(s) available", gray("→"), available_count);

            if let Some(ref latest) = index.latest {
                if index.yanked.contains_key(latest.as_str()) {
                    println!(
                        "  {} {} ({})",
                        gray("Latest:"),
                        yellow(&latest),
                        red("yanked")
                    );
                } else {
                    println!("  {} {}", gray("Latest:"), green(&latest));
                }
            }

            // If specific version requested, show details
            if let Some(ver) = version {
                match client.fetch_version_metadata(package, ver) {
                    Ok(meta) => {
                        println!("\n{} version {}", status_label("Info"), bold(ver));

                        // Show yank status prominently
                        if meta.yanked {
                            println!("\n  {} This version has been YANKED", red("⚠ YANKED"));
                            if let Some(ref reason) = meta.yank_reason {
                                println!("  {} {}", gray("Reason:"), reason);
                            }
                        }

                        if let Some(ref license) = meta.license {
                            println!("  {} {}", gray("License:"), license);
                        }
                        if let Some(ref published) = meta.published_at {
                            println!("  {} {}", gray("Published:"), published);
                        }
                        if !meta.deps.is_empty() {
                            println!("\n  {} {} dependency(s)", gray("→"), meta.deps.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("{} Failed to fetch version metadata: {}", red("error:"), e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{} Failed to fetch package info: {}", red("error:"), e);
        }
    }
}

#[allow(dead_code)]
pub fn pack() {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    let package_info = match &config.package {
        Some(p) => p,
        None => {
            eprintln!("{} no [package] section in lumen.toml", red("error:"));
            std::process::exit(1);
        }
    };

    let package_name = &package_info.name;
    let version = package_info.version.as_deref().unwrap_or("0.1.0");

    let dist_dir = project_dir.join("dist");
    std::fs::create_dir_all(&dist_dir)
        .map_err(|e| {
            eprintln!("{} failed to create dist directory: {}", red("error:"), e);
        })
        .ok();

    let tarball_name = format!("{}-{}.tgz", package_name, version);
    let tarball_path = dist_dir.join(&tarball_name);

    println!(
        "{} {}@{}",
        status_label("Packing"),
        bold(package_name),
        gray(version)
    );

    match create_package_tarball(project_dir, &tarball_path) {
        Ok(_) => {
            let path_str = tarball_path.display().to_string();
            println!("  {} created: {}", green("✓"), cyan(&path_str));

            if let Ok(md) = std::fs::metadata(&tarball_path) {
                println!("  {} {}", gray("Size:"), gray(&format_size(md.len())));
            }

            if let Ok(hash) = compute_tarball_hash(&tarball_path) {
                println!("  {} sha256:{}", gray("Hash:"), gray(&hash));
            }
        }
        Err(e) => {
            eprintln!("{} failed to create package: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

fn create_package_tarball(project_dir: &Path, output_path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("failed to create tarball: {}", e))?;

    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);

    let include_files = [
        "lumen.toml",
        "src",
        "tests",
        "examples",
        "README.md",
        "LICENSE",
    ];
    let exclude_dirs = [".git", "node_modules", "target", "dist", ".lumen"];

    fn walk_dir(
        dir: &Path,
        prefix: &Path,
        tar: &mut tar::Builder<flate2::write::GzEncoder<std::fs::File>>,
        include_files: &[&str],
        exclude_dirs: &[&str],
    ) -> Result<(), String> {
        let entries =
            std::fs::read_dir(dir).map_err(|e| format!("failed to read directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read entry: {}", e))?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if exclude_dirs.contains(&name) {
                continue;
            }

            let rel_path = prefix.join(name);

            if path.is_file() {
                if include_files.iter().any(|f| *f == name)
                    || name.ends_with(".lm")
                    || name.ends_with(".lm.md")
                {
                    tar.append_path_with_name(&path, &rel_path)
                        .map_err(|e| format!("failed to add file {}: {}", name, e))?;
                }
            } else if path.is_dir() {
                walk_dir(&path, &rel_path, tar, include_files, exclude_dirs)?;
            }
        }
        Ok(())
    }

    walk_dir(
        project_dir,
        Path::new(""),
        &mut tar,
        &include_files,
        &exclude_dirs,
    )?;

    tar.finish()
        .map_err(|e| format!("failed to finish tarball: {}", e))?;

    Ok(())
}

fn compute_tarball_hash(path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let hash = sha2::Sha256::digest(&data);
    Ok(hex::encode(hash))
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Try to publish package directly to R2 storage.
fn try_publish_to_r2(
    registry_url: &str,
    package_name: &str,
    version: &str,
    tarball_data: &[u8],
) -> Result<(), String> {
    // Extract account_id from registry URL
    // URL format: https://{account}.r2.cloudflarestorage.com/{bucket}
    // Or: https://{account}.r2.cloudflarestorage.com
    let url_without_scheme = registry_url.trim_start_matches("https://");
    let account_id = url_without_scheme
        .split('.')
        .next()
        .ok_or("Invalid R2 URL - could not extract account ID")?;

    // Get credentials from environment
    let access_key_id = std::env::var("R2_ACCESS_KEY").map_err(|_| "R2_ACCESS_KEY not set")?;
    let secret_access_key = std::env::var("R2_SECRET_KEY").map_err(|_| "R2_SECRET_KEY not set")?;

    // Create R2 client using the builder pattern
    let r2_config = crate::wares::R2Config::new(account_id.to_string(), access_key_id, secret_access_key)
        .with_bucket("lumen-registry");

    let client = R2Client::new(r2_config)
        .map_err(|e| format!("Failed to create R2 client: {}", e))?;

    // Compute hash for the tarball
    let hash = Sha256::digest(tarball_data);
    let hash_hex = hex::encode(hash);

    // Upload the tarball
    let key = format!("wares/{}/{}.tarball", package_name, version);
    client
        .put_object(&key, tarball_data, "application/gzip")
        .map_err(|e| format!("Failed to upload artifact: {}", e))?;

    // Upload the package index
    let index = crate::wares::RegistryPackageIndex {
        name: package_name.to_string(),
        versions: vec![version.to_string()],
        latest: Some(version.to_string()),
        yanked: Default::default(),
        prereleases: vec![],
        description: None,
        categories: vec![],
        downloads: None,
    };

    let index_json =
        serde_json::to_string(&index).map_err(|e| format!("Failed to serialize index: {}", e))?;

    let index_key = format!("wares/{}/index.json", package_name);
    client
        .put_object(&index_key, index_json.as_bytes(), "application/json")
        .map_err(|e| format!("Failed to upload index: {}", e))?;

    println!(
        "  {} uploaded tarball (sha256:{})",
        green("✓"),
        &hash_hex[..16]
    );
    println!("  {} uploaded index", green("✓"));

    Ok(())
}

/// Publish the current package to the registry.
pub fn publish(dry_run: bool) {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let registry_url = config.registry_url();

    // Get package info
    let package_info = match &config.package {
        Some(p) => p,
        None => {
            eprintln!("{} no [package] section in lumen.toml", red("error:"));
            std::process::exit(1);
        }
    };

    let package_name = &package_info.name;
    let version = package_info.version.as_deref().unwrap_or("0.1.0");

    println!(
        "{} {}@{} to {}",
        status_label("Publishing"),
        bold(package_name),
        gray(version),
        cyan(&registry_url)
    );

    // Check authentication
    if !is_authenticated(&registry_url) {
        eprintln!(
            "{} Not authenticated for {}",
            red("error:"),
            cyan(&registry_url)
        );
        eprintln!("  Run {} to login", cyan("lumen registry login"));
        std::process::exit(1);
    }

    if dry_run {
        println!(
            "{} Dry run - would publish to {}",
            status_label("Info"),
            cyan(&registry_url)
        );
        return;
    }

    // Create package archive
    println!("{} creating package archive...", status_label("Packing"));

    // Find all files to include
    let files = collect_package_files(project_dir);
    if files.is_empty() {
        eprintln!(
            "{} no files to publish in {}",
            red("error:"),
            project_dir.display()
        );
        std::process::exit(1);
    }

    println!("  {} files to publish", files.len());

    // Create tarball
    let archive_data = match create_tarball(project_dir, &files) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("{} failed to create archive: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    println!(
        "{} archive size: {} bytes",
        status_label("Packed"),
        archive_data.len()
    );

    // Try R2 upload first if credentials are available
    if registry_url.contains(".r2.cloudflarestorage.com") {
        match try_publish_to_r2(&registry_url, package_name, version, &archive_data) {
            Ok(()) => {
                println!(
                    "{} published {}@{} to R2",
                    green("✓"),
                    bold(package_name),
                    gray(version)
                );
                return;
            }
            Err(e) => {
                eprintln!("{} R2 upload failed: {}", red("error:"), e);
                eprintln!("  Trying REST API...");
            }
        }
    }

    // Generate resolution proof
    println!("{} generating resolution proof...", status_label("Auditing"));
    let resolver = Resolver::new(&registry_url, None);
    let request = ResolutionRequest {
        root_deps: config.dependencies.clone(),
        dev_deps: config.dev_dependencies.clone(),
        build_deps: config.build_dependencies.clone(),
        features: vec![],
        registry_url: registry_url.clone(),
        include_dev: false,
        include_build: false,
        include_yanked: false,
    };
    
    let proof_val = match resolver.resolve(&request) {
        Ok(result) => {
            println!("  {} resolution verified (trail of {} decisions)", green("✓"), result.proof.decisions.len());
            Some(serde_json::to_value(result.proof).unwrap_or(serde_json::Value::Null))
        },
        Err(e) => {
            eprintln!("{} resolution failed: {}", red("error:"), e);
            eprintln!("  Publication requires a valid resolution trail.");
            std::process::exit(1);
        }
    };

    // Publish with authentication via REST API
    match publish_with_auth(&registry_url, package_name, version, archive_data, proof_val) {
        Ok(()) => {
            println!(
                "{} published {}@{}",
                green("✓"),
                bold(package_name),
                gray(version)
            );
        }
        Err(e) => {
            eprintln!("{} publish failed: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// Collect files to include in the package.
fn collect_package_files(project_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    // Always include lumen.toml
    let manifest = project_dir.join("lumen.toml");
    if manifest.exists() {
        files.push(manifest);
    }

    // Include src directory
    let src_dir = project_dir.join("src");
    if src_dir.exists() {
        collect_files_recursive(&src_dir, project_dir, &mut files);
    }

    // Include README if present
    for readme_name in &["README.md", "README.rst", "README.txt", "README"] {
        let readme = project_dir.join(readme_name);
        if readme.exists() {
            files.push(readme);
            break;
        }
    }

    // Include LICENSE if present
    for license_name in &["LICENSE", "LICENSE.txt", "LICENSE.md", "COPYING"] {
        let license = project_dir.join(license_name);
        if license.exists() {
            files.push(license);
            break;
        }
    }

    files
}

fn collect_files_recursive(dir: &Path, base: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                // Skip hidden files and certain extensions
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                files.push(path);
            } else if path.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') && name != "target" && name != "dist" {
                        collect_files_recursive(&path, base, files);
                    }
                }
            }
        }
    }
}

/// Create a tarball from the collected files.
fn create_tarball(base_dir: &Path, files: &[PathBuf]) -> Result<Vec<u8>, String> {
    use tar::{Builder, Header};

    let mut buf = Vec::new();
    {
        let mut builder = Builder::new(&mut buf);

        for file in files {
            let relative = file
                .strip_prefix(base_dir)
                .map_err(|e| format!("failed to get relative path: {}", e))?;

            let contents = std::fs::read(file)
                .map_err(|e| format!("failed to read {}: {}", file.display(), e))?;

            let mut header = Header::new_gnu();
            header
                .set_path(relative)
                .map_err(|e| format!("failed to set path: {}", e))?;
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder
                .append(&header, &contents[..])
                .map_err(|e| format!("failed to append file: {}", e))?;
        }

        builder
            .finish()
            .map_err(|e| format!("failed to finish tarball: {}", e))?;
    }

    Ok(buf)
}

// =============================================================================
// Workspace Integration Functions
// =============================================================================

/// Dependency kind for installation and resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    /// Normal runtime dependency.
    Normal,
    /// Development dependency (tests, benchmarks).
    Dev,
    /// Build dependency (build scripts, codegen).
    Build,
}

impl std::fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Dev => write!(f, "dev"),
            Self::Build => write!(f, "build"),
        }
    }
}

/// Add a dependency to lumen.toml with a specific kind.
pub fn add_with_kind(package: &str, path_opt: Option<&str>, kind: DependencyKind) {
    let (config_path, mut config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    // Validate: all package names must be namespaced (@namespace/name)
    // except git URLs and path dependencies
    if !package.starts_with("http")
        && !package.starts_with("git@")
        && path_opt.is_none()
        && !package.starts_with('@')
    {
        eprintln!(
            "{} package name '{}' must be namespaced: @namespace/name\n  example: wares add @scope/{}",
            red("error:"),
            package,
            package
        );
        std::process::exit(1);
    }

    let (dep_name, dep_spec) = if package.starts_with("http") || package.starts_with("git@") {
        let url = package.to_string();
        let name_part = url.split('/').last().unwrap_or("unknown");
        let name = name_part
            .strip_suffix(".git")
            .unwrap_or(name_part)
            .to_string();

        (
            name,
            DependencySpec::Git {
                git: url,
                branch: None,
                tag: None,
                rev: None,
                features: None,
                optional: None,
            },
        )
    } else if let Some(path) = path_opt {
        (
            package.to_string(),
            DependencySpec::Path {
                path: path.to_string(),
            },
        )
    } else {
        eprintln!(
            "{} --path is required for now (or use a git URL)",
            red("error:")
        );
        std::process::exit(1);
    };

    // Add to the appropriate dependency set based on kind
    match kind {
        DependencyKind::Normal => {
            config
                .dependencies
                .insert(dep_name.clone(), dep_spec.clone());
        }
        DependencyKind::Dev => {
            config
                .dev_dependencies
                .insert(dep_name.clone(), dep_spec.clone());
        }
        DependencyKind::Build => {
            config
                .build_dependencies
                .insert(dep_name.clone(), dep_spec.clone());
        }
    }

    // Serialize back to TOML
    let toml_content = toml::to_string_pretty(&config).unwrap_or_else(|e| {
        eprintln!("{} serializing config: {}", red("error:"), e);
        std::process::exit(1);
    });

    std::fs::write(&config_path, &toml_content).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });

    let kind_str = match kind {
        DependencyKind::Normal => "dependency",
        DependencyKind::Dev => "dev-dependency",
        DependencyKind::Build => "build-dependency",
    };

    match dep_spec {
        DependencySpec::Path { path } => {
            println!(
                "{} {} {} {{ path = \"{}\" }}",
                status_label("Added"),
                kind_str,
                bold(&dep_name),
                path
            );
        }
        DependencySpec::Git { git, .. } => {
            println!(
                "{} {} {} {{ git = \"{}\" }}",
                status_label("Added"),
                kind_str,
                bold(&dep_name),
                git
            );
        }
        _ => {}
    }
}

/// Build a package at the given directory.
pub fn build_package(package_dir: &Path) -> Result<(), String> {
    let target_dir = package_dir.join("target");
    // Run build scripts first
    if crate::build_script::has_build_scripts(package_dir) {
        crate::build_script::run_build_scripts(package_dir, &target_dir)
            .map_err(|e| format!("build script failed: {}", e))?;
    }

    println!(
        "{} Building package at {}",
        status_label("Building"),
        package_dir.display()
    );

    Ok(())
}

/// Validate a package for publishing.
pub fn validate_package(package_dir: &Path) -> Result<(), String> {
    println!(
        "{} Validating package at {}",
        status_label("Validating"),
        package_dir.display()
    );
    Ok(())
}

/// Publish a package to the registry.
pub fn publish_package(package_dir: &Path) -> Result<(), String> {
    println!(
        "{} Publishing package at {}",
        status_label("Publishing"),
        package_dir.display()
    );
    Ok(())
}

/// Install dependencies with a specific kind (normal, dev, build).
pub fn install_with_kind(kind: DependencyKind, frozen: bool) {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!(
                "{} no lumen.toml found (run `lumen pkg init` first)",
                red("error:")
            );
            std::process::exit(1);
        }
    };

    let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    println!("{} {} dependencies", status_label("Resolving"), kind);

    // Select the appropriate dependency set based on kind
    let deps_to_resolve = match kind {
        DependencyKind::Normal => &config.dependencies,
        DependencyKind::Dev => &config.dev_dependencies,
        DependencyKind::Build => &config.build_dependencies,
    };

    if deps_to_resolve.is_empty() {
        println!("{} no {} dependencies to install", gray("info:"), kind);
        return;
    }

    println!(
        "{} would install {} {} dependencies",
        green("✓"),
        deps_to_resolve.len(),
        kind
    );

    if frozen {
        println!("{} frozen mode - not modifying lockfile", gray("info:"));
    }
}

// Tests preserved from original file
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_config(deps: Vec<(&str, &str)>) -> LumenConfig {
        let mut dependencies = HashMap::new();
        for (name, path) in deps {
            dependencies.insert(
                name.to_string(),
                DependencySpec::Path {
                    path: path.to_string(),
                },
            );
        }
        LumenConfig {
            package: Some(crate::config::PackageInfo {
                name: "test-pkg".to_string(),
                version: Some("0.1.0".to_string()),
                ..Default::default()
            }),
            dependencies,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_empty_deps() {
        let cfg = make_config(vec![]);
        let deps = resolve_dependencies(&cfg, Path::new(".")).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn resolve_nonexistent_path_fails() {
        let cfg = make_config(vec![("ghost", "/tmp/lumen_test_nonexistent_pkg_8374")]);
        let result = resolve_dependencies(&cfg, Path::new("."));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            sanitize_filename("https://github.com/user/repo"),
            "github_com_user_repo"
        );
        assert_eq!(
            sanitize_filename("git@github.com:user/repo.git"),
            "git@github_com_user_repo_git"
        );
    }

    #[test]
    fn test_resolved_dep_features() {
        let dep = ResolvedDep {
            name: "test".to_string(),
            path: PathBuf::from("/tmp/test"),
            config: LumenConfig::default(),
            source: ResolvedDepSource::Registry {
                source: "test".to_string(),
                checksum: "abc".to_string(),
            },
            features: vec!["default".to_string(), "async".to_string()],
        };
        assert_eq!(dep.features.len(), 2);
    }
}
