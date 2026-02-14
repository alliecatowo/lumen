//! Package manager operations for Lumen projects.
//!
//! Provides `pkg init` (scaffold a new package) and `pkg build`
//! (resolve path-based dependencies and compile).

use crate::config::{DependencySpec, LumenConfig};
use crate::lockfile::{LockFile, LockedPackage};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

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
#[allow(dead_code)]
pub struct ResolvedDep {
    pub name: String,
    pub path: PathBuf,
    pub config: LumenConfig,
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
pub fn cmd_pkg_init(name: Option<String>) {
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

    let pkg_name = name.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-package".to_string())
    });

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
pub fn cmd_pkg_build() {
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
pub fn cmd_pkg_check() {
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
    let mut resolved = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();

    let root_name = config
        .package
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "(root)".to_string());
    stack.insert(root_name.clone());

    let mut entries: Vec<_> = config.dependencies.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (name, spec) in entries {
        resolve_dep(
            name,
            spec,
            project_dir,
            &mut resolved,
            &mut visited,
            &mut stack,
        )?;
    }

    Ok(resolved)
}

fn resolve_dep(
    name: &str,
    spec: &DependencySpec,
    parent_dir: &Path,
    resolved: &mut Vec<ResolvedDep>,
    visited: &mut HashSet<String>,
    stack: &mut HashSet<String>,
) -> Result<(), String> {
    if visited.contains(name) {
        return Ok(());
    }
    if stack.contains(name) {
        return Err(format!("circular dependency detected: '{}'", name));
    }
    stack.insert(name.to_string());

    let dep_path = match spec {
        DependencySpec::Path { path } => {
            let p = parent_dir.join(path);
            canonicalize_or_clean(&p)
        }
        DependencySpec::Version(version) => {
            return Err(format!(
                "dependency '{}': registry dependency '{}' is not available yet; use a path dependency",
                name, version
            ));
        }
        DependencySpec::VersionDetailed { version, registry } => {
            let registry_hint = registry
                .as_deref()
                .map(|r| format!(" from '{}'", r))
                .unwrap_or_default();
            return Err(format!(
                "dependency '{}': registry dependency '{}'{} is not available yet; use a path dependency",
                name, version, registry_hint
            ));
        }
    };

    if !dep_path.exists() {
        return Err(format!(
            "dependency '{}': path '{}' does not exist",
            name,
            dep_path.display()
        ));
    }

    // Check for lumen.toml or source files
    let dep_config_path = dep_path.join("lumen.toml");
    let dep_config = if dep_config_path.exists() {
        LumenConfig::load_from(&dep_config_path)?
    } else {
        // No lumen.toml — check for source files
        let has_sources = has_lumen_sources(&dep_path);
        if !has_sources {
            return Err(format!(
                "dependency '{}': no lumen.toml or .lm/.lm.md files found in '{}'",
                name,
                dep_path.display()
            ));
        }
        LumenConfig::default()
    };

    // Resolve transitive dependencies
    let mut entries: Vec<_> = dep_config.dependencies.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (sub_name, sub_spec) in entries {
        resolve_dep(sub_name, sub_spec, &dep_path, resolved, visited, stack)?;
    }

    visited.insert(name.to_string());
    stack.remove(name);
    resolved.push(ResolvedDep {
        name: name.to_string(),
        path: dep_path,
        config: dep_config,
    });

    Ok(())
}

/// Compile all `.lm` and `.lm.md` files found in a package directory.
/// Returns the number of files compiled, or the first error.
fn compile_package_sources(pkg_dir: &Path) -> Result<usize, String> {
    let sources = find_lumen_sources(pkg_dir);
    if sources.is_empty() {
        return Err(format!(
            "no .lm/.lm.md files found in '{}'",
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

    let compile_result = if is_markdown_source(source_path) {
        lumen_compiler::compile_with_imports(&content, &resolve_import)
    } else {
        lumen_compiler::compile_raw_with_imports(&content, &resolve_import)
    };

    compile_result.map_err(|e| format!("{}: {}", source_path.display(), e))?;
    Ok(())
}

fn resolve_module_from_roots(module_path: &str, roots: &[PathBuf]) -> Option<String> {
    let fs_path = module_path.replace('.', "/");
    for root in roots {
        let candidates = [
            root.join(format!("{}.lm", fs_path)),
            root.join(format!("{}.lm.md", fs_path)),
            root.join(fs_path.clone()).join("mod.lm"),
            root.join(fs_path.clone()).join("mod.lm.md"),
            root.join(fs_path.clone()).join("main.lm"),
            root.join(fs_path.clone()).join("main.lm.md"),
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

fn is_markdown_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".lm.md"))
        .unwrap_or(false)
}

fn is_lumen_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.ends_with(".lm") || name.ends_with(".lm.md"))
        .unwrap_or(false)
}

/// Find all `.lm` and `.lm.md` files in a directory (searches `src/` subdirectory first,
/// then top level).
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
    let deps = resolve_dependencies(config, project_dir)?;
    let mut lock = LockFile::default();

    for dep in &deps {
        let version = dep
            .config
            .package
            .as_ref()
            .and_then(|p| p.version.clone())
            .unwrap_or_else(|| "0.1.0".to_string());
        let mut locked_pkg =
            LockedPackage::from_path(dep.name.clone(), lock_source_path(project_dir, &dep.path));
        locked_pkg.version = version;
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
pub fn cmd_pkg_add(package: &str, path_opt: Option<&str>) {
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

    let path = path_opt.unwrap_or_else(|| {
        eprintln!(
            "{} --path is required for now (registry support coming soon)",
            red("error:")
        );
        std::process::exit(1);
    });

    // Add to dependencies map
    config.dependencies.insert(
        package.to_string(),
        DependencySpec::Path {
            path: path.to_string(),
        },
    );

    // Serialize back to TOML
    let toml_content = toml::to_string_pretty(&config).unwrap_or_else(|e| {
        eprintln!("{} serializing config: {}", red("error:"), e);
        std::process::exit(1);
    });

    std::fs::write(&config_path, &toml_content).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });

    println!(
        "{} dependency {} {{ path = \"{}\" }}",
        status_label("Added"),
        bold(package),
        path
    );
}

/// Remove a dependency from lumen.toml
#[allow(dead_code)]
pub fn cmd_pkg_remove(package: &str) {
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
pub fn cmd_pkg_list() {
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
            DependencySpec::VersionDetailed { version, registry } => {
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
        }
    }
}

/// Install dependencies from lumen.toml and generate lumen.lock.
#[allow(dead_code)]
pub fn cmd_pkg_install() {
    cmd_pkg_install_with_lock(false);
}

/// Install dependencies from lumen.toml and generate lumen.lock.
/// When `frozen` is true, fail instead of writing if the lockfile would change.
pub fn cmd_pkg_install_with_lock(frozen: bool) {
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
pub fn cmd_pkg_update() {
    cmd_pkg_update_with_lock(false);
}

/// Update dependencies to latest compatible versions.
/// When `frozen` is true, fail instead of writing if the lockfile would change.
pub fn cmd_pkg_update_with_lock(frozen: bool) {
    println!(
        "{} note: update is equivalent to install for now (registry support coming soon)",
        gray("")
    );
    cmd_pkg_install_with_lock(frozen);
}

/// Search for packages in the registry
pub fn cmd_pkg_search(_query: &str) {
    println!("{} Package registry not yet available.", gray(""));
    println!();
    println!("Registry support is planned for a future release.");
    println!("For now, use path dependencies:");
    println!(
        "  {} = {{ path = \"../package-name\" }}",
        gray("package-name")
    );
}

/// Show package metadata and deterministic checksums for a local package or archive.
#[allow(dead_code)]
pub fn cmd_pkg_info(target: Option<&str>) {
    let report = match target {
        Some(path_str) => {
            let path = Path::new(path_str);
            if path.is_dir() {
                match read_local_package_inspect(path) {
                    Ok(report) => report,
                    Err(e) => {
                        eprintln!("{} {}", red("error:"), e);
                        std::process::exit(1);
                    }
                }
            } else if path.is_file() {
                match read_archive_package_inspect(path) {
                    Ok(report) => report,
                    Err(e) => {
                        eprintln!("{} {}", red("error:"), e);
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!(
                    "{} target '{}' is neither a directory nor a file",
                    red("error:"),
                    path.display()
                );
                std::process::exit(1);
            }
        }
        None => {
            let (config_path, config) = match LumenConfig::load_with_path() {
                Some(pair) => pair,
                None => {
                    eprintln!(
                        "{} no lumen.toml found (run `lumen pkg init` first or pass an archive path)",
                        red("error:")
                    );
                    std::process::exit(1);
                }
            };
            let project_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
            match inspect_from_bundle(project_dir, build_package_bundle(project_dir, &config)) {
                Ok(report) => report,
                Err(e) => {
                    eprintln!("{} {}", red("error:"), e);
                    std::process::exit(1);
                }
            }
        }
    };

    println!(
        "{} {}@{}",
        status_label("Package"),
        bold(&report.package_name),
        gray(&report.version)
    );
    println!(
        "{} {} file{} ({})",
        green("✓"),
        report.file_count,
        if report.file_count == 1 { "" } else { "s" },
        format_byte_size(report.content_size_bytes)
    );
    println!(
        "{} {}",
        status_label("Content"),
        gray(&report.content_checksum)
    );
    if let Some(archive_checksum) = report.archive_checksum.as_ref() {
        println!("{} {}", status_label("Archive"), gray(archive_checksum));
    }
    if let Some(archive_size_bytes) = report.archive_size_bytes {
        println!(
            "{} {}",
            status_label("Archive size"),
            gray(&format_byte_size(archive_size_bytes))
        );
    }
    println!("{}:", status_label("Entries"));
    for entry in &report.entries {
        println!("  {}", gray(entry));
    }
}

/// Create a deterministic package tarball for the current package.
pub fn cmd_pkg_pack() {
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
    let output_path = match default_pack_output_path(project_dir, &config) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    let output_display = output_path.display().to_string();
    println!("{} {}", status_label("Packing"), gray(&output_display));

    match pack_current_package(project_dir, &config, &output_path) {
        Ok(report) => {
            println!(
                "{} {}@{}",
                status_label("Validated"),
                bold(&report.package_name),
                gray(&report.version)
            );
            println!(
                "{} {} file{} ({})",
                green("✓"),
                report.file_count,
                if report.file_count == 1 { "" } else { "s" },
                format_byte_size(report.archive_size_bytes)
            );
            println!(
                "{} {}",
                status_label("Content"),
                gray(&report.content_checksum)
            );
            println!(
                "{} {}",
                status_label("Archive"),
                gray(&report.archive_checksum)
            );
            let archive_display = report.archive_path.display().to_string();
            println!("{} {}", status_label("Created"), gray(&archive_display));
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// Validate package metadata/contents and run publish pipeline locally.
pub fn cmd_pkg_publish(dry_run: bool) {
    if !dry_run {
        eprintln!(
            "{} registry upload is not implemented yet; run `lpm publish --dry-run`",
            red("error:")
        );
        std::process::exit(1);
    }

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
    println!("{} package metadata + contents", status_label("Validating"));

    match run_publish_dry_run(project_dir, &config) {
        Ok(report) => {
            println!(
                "{} {}@{}",
                status_label("Prepared"),
                bold(&report.package_name),
                gray(&report.version)
            );
            println!(
                "{} {} file{} ({})",
                green("✓"),
                report.file_count,
                if report.file_count == 1 { "" } else { "s" },
                format_byte_size(report.archive_size_bytes)
            );
            println!("{}:", status_label("Checklist"));
            println!("  {} package metadata", green("✓"));
            println!("  {} source discovery", green("✓"));
            println!("  {} deterministic entry order", green("✓"));
            println!("  {} content checksum", green("✓"));
            println!("  {} archive checksum", green("✓"));
            println!(
                "{} {}",
                status_label("Artifact"),
                gray(&report.archive_path.display().to_string())
            );
            println!(
                "{} {}",
                status_label("Content"),
                gray(&report.content_checksum)
            );
            println!(
                "{} {}",
                status_label("Archive"),
                gray(&report.archive_checksum)
            );
            println!(
                "{} dry-run only (registry upload TODO)",
                status_label("Skipped")
            );
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

fn default_pack_output_path(project_dir: &Path, config: &LumenConfig) -> Result<PathBuf, String> {
    let (name, version) = validate_package_metadata(config)?;
    Ok(project_dir
        .join("dist")
        .join(format!("{}-{}.tar", name, version)))
}

fn pack_current_package(
    project_dir: &Path,
    config: &LumenConfig,
    output_path: &Path,
) -> Result<PackReport, String> {
    let bundle = build_package_bundle(project_dir, config)?;
    validate_stable_entry_order(&bundle.entries)?;
    let content_checksum = package_content_checksum(&bundle.entries)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create '{}': {}", parent.display(), e))?;
    }
    let archive_size_bytes = write_deterministic_tar(&bundle.entries, output_path)?;
    let archive_checksum = file_sha256_checksum(output_path)?;
    Ok(PackReport {
        archive_path: output_path.to_path_buf(),
        package_name: bundle.name,
        version: bundle.version,
        file_count: bundle.entries.len(),
        archive_size_bytes,
        content_checksum,
        archive_checksum,
    })
}

fn run_publish_dry_run(
    project_dir: &Path,
    config: &LumenConfig,
) -> Result<PublishDryRunReport, String> {
    let temp_path = publish_dry_run_archive_path();
    let packed = pack_current_package(project_dir, config, &temp_path)?;
    Ok(PublishDryRunReport {
        archive_path: packed.archive_path,
        package_name: packed.package_name,
        version: packed.version,
        file_count: packed.file_count,
        archive_size_bytes: packed.archive_size_bytes,
        content_checksum: packed.content_checksum,
        archive_checksum: packed.archive_checksum,
    })
}

fn publish_dry_run_archive_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "lumen-publish-dry-run-{}-{}.tar",
        std::process::id(),
        stamp
    ))
}

fn build_package_bundle(project_dir: &Path, config: &LumenConfig) -> Result<PackageBundle, String> {
    let (name, version) = validate_package_metadata(config)?;
    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    add_package_entry(
        &mut entries,
        &mut seen,
        read_package_entry(project_dir, Path::new("lumen.toml"))?,
    );

    let sources = find_lumen_sources(project_dir);
    if sources.is_empty() {
        return Err("package must contain at least one .lm or .lm.md source file".to_string());
    }
    for source in sources {
        let rel = source.strip_prefix(project_dir).map_err(|_| {
            format!(
                "cannot package source outside project directory: '{}'",
                source.display()
            )
        })?;
        add_package_entry(
            &mut entries,
            &mut seen,
            read_package_entry(project_dir, rel)?,
        );
    }

    if let Some(readme_path) = resolve_readme_path(project_dir, config)? {
        add_package_entry(
            &mut entries,
            &mut seen,
            read_package_entry(project_dir, &readme_path)?,
        );
    }

    let license_path = Path::new("LICENSE");
    if project_dir.join(license_path).is_file() {
        add_package_entry(
            &mut entries,
            &mut seen,
            read_package_entry(project_dir, license_path)?,
        );
    }

    entries.sort_by(|a, b| a.archive_path.cmp(&b.archive_path));

    Ok(PackageBundle {
        name,
        version,
        entries,
    })
}

#[allow(dead_code)]
fn inspect_from_bundle(
    project_dir: &Path,
    bundle: Result<PackageBundle, String>,
) -> Result<PackageInspectReport, String> {
    let bundle = bundle?;
    validate_stable_entry_order(&bundle.entries)?;
    let content_checksum = package_content_checksum(&bundle.entries)?;
    let archive_path = project_dir.join("dist").join(format!(
        "{}-{}.tar",
        bundle.name.as_str(),
        bundle.version.as_str()
    ));

    let archive_size_bytes = std::fs::metadata(&archive_path).ok().map(|m| m.len());
    let archive_checksum = if archive_path.is_file() {
        Some(file_sha256_checksum(&archive_path)?)
    } else {
        None
    };

    Ok(PackageInspectReport {
        package_name: bundle.name,
        version: bundle.version,
        file_count: bundle.entries.len(),
        content_size_bytes: bundle.entries.iter().map(|e| e.bytes.len() as u64).sum(),
        content_checksum,
        archive_checksum,
        archive_size_bytes,
        entries: bundle.entries.into_iter().map(|e| e.archive_path).collect(),
    })
}

#[allow(dead_code)]
fn read_local_package_inspect(project_dir: &Path) -> Result<PackageInspectReport, String> {
    let config_path = project_dir.join("lumen.toml");
    if !config_path.is_file() {
        return Err(format!("missing file '{}'", config_path.display()));
    }
    let config = LumenConfig::load_from(&config_path)?;
    inspect_from_bundle(project_dir, build_package_bundle(project_dir, &config))
}

#[allow(dead_code)]
fn read_archive_package_inspect(path: &Path) -> Result<PackageInspectReport, String> {
    let entries = read_tar_entries(path)?;
    validate_stable_entry_order(&entries)?;
    let content_checksum = package_content_checksum(&entries)?;
    let content_size_bytes = entries.iter().map(|e| e.bytes.len() as u64).sum();

    let manifest_entry = entries
        .iter()
        .find(|entry| entry.archive_path == "lumen.toml")
        .ok_or_else(|| "archive is missing lumen.toml".to_string())?;
    let manifest_str = std::str::from_utf8(&manifest_entry.bytes)
        .map_err(|_| "archive lumen.toml is not valid UTF-8".to_string())?;
    let config: LumenConfig = toml::from_str(manifest_str)
        .map_err(|e| format!("invalid lumen.toml in archive: {}", e))?;
    let (package_name, version) = validate_package_metadata(&config)?;

    Ok(PackageInspectReport {
        package_name,
        version,
        file_count: entries.len(),
        content_size_bytes,
        content_checksum,
        archive_checksum: Some(file_sha256_checksum(path)?),
        archive_size_bytes: Some(
            std::fs::metadata(path)
                .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?
                .len(),
        ),
        entries: entries
            .into_iter()
            .map(|entry| entry.archive_path)
            .collect(),
    })
}

fn add_package_entry(
    entries: &mut Vec<PackageEntry>,
    seen: &mut HashSet<String>,
    entry: PackageEntry,
) {
    if seen.insert(entry.archive_path.clone()) {
        entries.push(entry);
    }
}

fn validate_package_metadata(config: &LumenConfig) -> Result<(String, String), String> {
    let package = config
        .package
        .as_ref()
        .ok_or_else(|| "lumen.toml is missing required [package] metadata".to_string())?;

    let name = package.name.trim();
    if !is_valid_package_name(name) {
        return Err(format!(
            "invalid package name '{}': use lowercase letters, numbers, and '-' only",
            package.name
        ));
    }

    let version = package
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "package.version is required for packing/publishing".to_string())?;
    if !is_valid_semver(version) {
        return Err(format!(
            "invalid package.version '{}': expected semantic version (e.g. 1.2.3)",
            version
        ));
    }

    if let Some(keywords) = package.keywords.as_ref() {
        if keywords.len() > 5 {
            return Err("package.keywords must contain at most 5 entries".to_string());
        }
    }

    Ok((name.to_string(), version.to_string()))
}

fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') {
        return false;
    }

    let mut prev_dash = false;
    for ch in name.chars() {
        match ch {
            'a'..='z' | '0'..='9' => prev_dash = false,
            '-' => {
                if prev_dash {
                    return false;
                }
                prev_dash = true;
            }
            _ => return false,
        }
    }
    true
}

fn is_valid_semver(version: &str) -> bool {
    let (core_and_pre, build) = match version.split_once('+') {
        Some((left, right)) => (left, Some(right)),
        None => (version, None),
    };
    if let Some(build_meta) = build {
        if build_meta.is_empty()
            || !build_meta
                .split('.')
                .all(|part| is_valid_semver_identifier(part, false))
        {
            return false;
        }
    }

    let (core, pre) = match core_and_pre.split_once('-') {
        Some((left, right)) => (left, Some(right)),
        None => (core_and_pre, None),
    };

    let mut nums = core.split('.');
    let major = nums.next().unwrap_or_default();
    let minor = nums.next().unwrap_or_default();
    let patch = nums.next().unwrap_or_default();
    if nums.next().is_some() {
        return false;
    }
    if !is_valid_numeric_semver_part(major)
        || !is_valid_numeric_semver_part(minor)
        || !is_valid_numeric_semver_part(patch)
    {
        return false;
    }

    if let Some(pre_release) = pre {
        if pre_release.is_empty()
            || !pre_release
                .split('.')
                .all(|part| is_valid_semver_identifier(part, true))
        {
            return false;
        }
    }
    true
}

fn is_valid_numeric_semver_part(part: &str) -> bool {
    !part.is_empty()
        && part.chars().all(|c| c.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'))
}

fn is_valid_semver_identifier(part: &str, enforce_numeric_no_leading_zero: bool) -> bool {
    if part.is_empty() {
        return false;
    }
    if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    if enforce_numeric_no_leading_zero && part.chars().all(|c| c.is_ascii_digit()) {
        return is_valid_numeric_semver_part(part);
    }
    true
}

fn resolve_readme_path(
    project_dir: &Path,
    config: &LumenConfig,
) -> Result<Option<PathBuf>, String> {
    let configured_readme = config
        .package
        .as_ref()
        .and_then(|pkg| pkg.readme.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let readme_rel = if let Some(path_str) = configured_readme {
        Some(normalize_manifest_relative_path(path_str)?)
    } else {
        let default = PathBuf::from("README.md");
        if project_dir.join(&default).is_file() {
            Some(default)
        } else {
            None
        }
    };

    if let Some(rel) = readme_rel {
        let full = project_dir.join(&rel);
        if !full.is_file() {
            return Err(format!(
                "configured readme '{}' does not exist",
                rel.display()
            ));
        }
        return Ok(Some(rel));
    }

    Ok(None)
}

fn normalize_manifest_relative_path(path_str: &str) -> Result<PathBuf, String> {
    let path = Path::new(path_str);
    if path.is_absolute() {
        return Err(format!(
            "path '{}' must be relative to the package root",
            path_str
        ));
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => clean.push(part),
            _ => {
                return Err(format!(
                    "path '{}' must stay within the package root",
                    path_str
                ));
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err(format!("path '{}' cannot be empty", path_str));
    }
    Ok(clean)
}

fn read_package_entry(project_dir: &Path, relative_path: &Path) -> Result<PackageEntry, String> {
    let archive_path = path_to_archive_path(relative_path)?;
    let full_path = project_dir.join(relative_path);
    if !full_path.is_file() {
        return Err(format!("missing file '{}'", relative_path.display()));
    }
    let bytes = std::fs::read(&full_path)
        .map_err(|e| format!("cannot read '{}': {}", full_path.display(), e))?;

    Ok(PackageEntry {
        archive_path,
        bytes,
    })
}

fn path_to_archive_path(path: &Path) -> Result<String, String> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => clean.push(part),
            _ => return Err(format!("invalid package path '{}'", path.display())),
        }
    }

    if clean.as_os_str().is_empty() {
        return Err("empty package path".to_string());
    }
    Ok(clean.to_string_lossy().replace('\\', "/"))
}

fn validate_stable_entry_order(entries: &[PackageEntry]) -> Result<(), String> {
    let mut prev: Option<&str> = None;
    for entry in entries {
        let curr = entry.archive_path.as_str();
        if let Some(last) = prev {
            if curr <= last {
                return Err(format!(
                    "non-deterministic package entry order detected: '{}' appears after '{}'",
                    curr, last
                ));
            }
        }
        prev = Some(curr);
    }
    Ok(())
}

fn package_content_checksum(entries: &[PackageEntry]) -> Result<String, String> {
    validate_stable_entry_order(entries)?;

    let mut hasher = Sha256::new();
    for entry in entries {
        let path = entry.archive_path.as_bytes();
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path);
        hasher.update((entry.bytes.len() as u64).to_le_bytes());
        hasher.update(&entry.bytes);
    }

    Ok(format!("sha256:{}", hex_encode(&hasher.finalize())))
}

fn file_sha256_checksum(path: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{}", hex_encode(&hasher.finalize())))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

#[allow(dead_code)]
fn read_tar_entries(path: &Path) -> Result<Vec<PackageEntry>, String> {
    let data =
        std::fs::read(path).map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let mut entries = Vec::new();
    let mut offset = 0usize;

    while offset + 512 <= data.len() {
        let header = &data[offset..offset + 512];
        if header.iter().all(|b| *b == 0) {
            break;
        }

        let name = read_tar_text_field(&header[0..100])?;
        let prefix = read_tar_text_field(&header[345..500])?;
        let archive_path = if prefix.is_empty() {
            name
        } else if name.is_empty() {
            prefix
        } else {
            format!("{}/{}", prefix, name)
        };

        if archive_path.is_empty() {
            return Err(format!("invalid empty tar path in '{}'", path.display()));
        }

        let size = parse_tar_octal_field(&header[124..136], "size")? as usize;
        let body_start = offset + 512;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| format!("tar entry size overflow in '{}'", path.display()))?;
        if body_end > data.len() {
            return Err(format!(
                "corrupt tar archive '{}': entry '{}' exceeds archive length",
                path.display(),
                archive_path
            ));
        }

        entries.push(PackageEntry {
            archive_path,
            bytes: data[body_start..body_end].to_vec(),
        });

        offset = body_end;
        let rem = offset % 512;
        if rem != 0 {
            offset += 512 - rem;
        }
    }

    if entries.is_empty() {
        return Err(format!("archive '{}' has no file entries", path.display()));
    }
    Ok(entries)
}

#[allow(dead_code)]
fn read_tar_text_field(field: &[u8]) -> Result<String, String> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    let text = std::str::from_utf8(&field[..end])
        .map_err(|_| "tar header contains invalid UTF-8".to_string())?
        .trim();
    Ok(text.to_string())
}

#[allow(dead_code)]
fn parse_tar_octal_field(field: &[u8], label: &str) -> Result<u64, String> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    let txt = std::str::from_utf8(&field[..end])
        .map_err(|_| format!("invalid tar {} field encoding", label))?
        .trim()
        .trim_end_matches(' ');
    if txt.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(txt, 8).map_err(|_| format!("invalid tar {} field value", label))
}

fn write_deterministic_tar(entries: &[PackageEntry], output_path: &Path) -> Result<u64, String> {
    const BLOCK: usize = 512;
    validate_stable_entry_order(entries)?;
    let mut file = std::fs::File::create(output_path)
        .map_err(|e| format!("cannot create '{}': {}", output_path.display(), e))?;
    let mut total_written = 0u64;

    for entry in entries {
        write_tar_header(
            &mut file,
            &entry.archive_path,
            entry.bytes.len() as u64,
            0o644,
        )?;
        file.write_all(&entry.bytes)
            .map_err(|e| format!("cannot write '{}': {}", output_path.display(), e))?;
        total_written += BLOCK as u64 + entry.bytes.len() as u64;

        let pad = (BLOCK - (entry.bytes.len() % BLOCK)) % BLOCK;
        if pad > 0 {
            let zeros = [0u8; BLOCK];
            file.write_all(&zeros[..pad])
                .map_err(|e| format!("cannot write '{}': {}", output_path.display(), e))?;
            total_written += pad as u64;
        }
    }

    let trailer = [0u8; BLOCK * 2];
    file.write_all(&trailer)
        .map_err(|e| format!("cannot finalize '{}': {}", output_path.display(), e))?;
    total_written += trailer.len() as u64;

    Ok(total_written)
}

fn write_tar_header<W: Write>(
    writer: &mut W,
    path: &str,
    size: u64,
    mode: u64,
) -> Result<(), String> {
    let (name, prefix) = split_ustar_path(path)?;
    let mut header = [0u8; 512];

    write_header_bytes(&mut header[0..100], name.as_bytes(), "tar name")?;
    write_octal(&mut header[100..108], mode)?;
    write_octal(&mut header[108..116], 0)?; // uid
    write_octal(&mut header[116..124], 0)?; // gid
    write_octal(&mut header[124..136], size)?;
    write_octal(&mut header[136..148], 0)?; // mtime
    header[148..156].fill(b' ');
    header[156] = b'0'; // file type
    write_header_bytes(&mut header[257..263], b"ustar\0", "tar magic")?;
    write_header_bytes(&mut header[263..265], b"00", "tar version")?;
    if let Some(prefix) = prefix {
        write_header_bytes(&mut header[345..500], prefix.as_bytes(), "tar prefix")?;
    }

    let checksum: u64 = header.iter().map(|&b| u64::from(b)).sum();
    write_tar_checksum(&mut header[148..156], checksum)?;

    writer
        .write_all(&header)
        .map_err(|e| format!("cannot write tar header: {}", e))
}

fn write_header_bytes(dst: &mut [u8], src: &[u8], label: &str) -> Result<(), String> {
    if src.len() > dst.len() {
        return Err(format!("{} too long for tar header", label));
    }
    dst[..src.len()].copy_from_slice(src);
    Ok(())
}

fn split_ustar_path(path: &str) -> Result<(String, Option<String>), String> {
    if path.is_empty() {
        return Err("empty tar path".to_string());
    }
    let bytes = path.as_bytes();
    if bytes.len() <= 100 {
        return Ok((path.to_string(), None));
    }

    let mut split_at = None;
    for (idx, b) in bytes.iter().enumerate() {
        if *b == b'/' && idx <= 155 {
            let prefix_len = idx;
            let name_len = bytes.len() - idx - 1;
            if prefix_len <= 155 && name_len <= 100 {
                split_at = Some(idx);
            }
        }
    }

    match split_at {
        Some(idx) => Ok((path[idx + 1..].to_string(), Some(path[..idx].to_string()))),
        None => Err(format!("path '{}' is too long for ustar archive", path)),
    }
}

fn write_octal(dst: &mut [u8], value: u64) -> Result<(), String> {
    if dst.len() < 2 {
        return Err("tar numeric field is too short".to_string());
    }
    let width = dst.len() - 1;
    let encoded = format!("{:o}", value);
    if encoded.len() > width {
        return Err("value does not fit tar numeric field".to_string());
    }
    dst[..width].fill(b'0');
    let start = width - encoded.len();
    dst[start..width].copy_from_slice(encoded.as_bytes());
    dst[width] = 0;
    Ok(())
}

fn write_tar_checksum(dst: &mut [u8], checksum: u64) -> Result<(), String> {
    if dst.len() != 8 {
        return Err("tar checksum field must be 8 bytes".to_string());
    }
    let encoded = format!("{:o}", checksum);
    if encoded.len() > 6 {
        return Err("tar checksum value too large".to_string());
    }
    dst[..6].fill(b'0');
    let start = 6 - encoded.len();
    dst[start..6].copy_from_slice(encoded.as_bytes());
    dst[6] = 0;
    dst[7] = b' ';
    Ok(())
}

fn format_byte_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let kib = bytes as f64 / 1024.0;
    format!("{:.1} KiB", kib)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
                description: None,
                authors: None,
                license: None,
                repository: None,
                keywords: None,
                readme: None,
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
    fn circular_dependency_detected() {
        // Build two temp dirs that reference each other
        let tmp = std::env::temp_dir().join("lumen_circ_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let pkg_a = tmp.join("a");
        let pkg_b = tmp.join("b");
        std::fs::create_dir_all(&pkg_a).unwrap();
        std::fs::create_dir_all(&pkg_b).unwrap();

        // a depends on b
        std::fs::write(
            pkg_a.join("lumen.toml"),
            "[package]\nname = \"a\"\n\n[dependencies]\nb = { path = \"../b\" }\n",
        )
        .unwrap();
        // b depends on a
        std::fs::write(
            pkg_b.join("lumen.toml"),
            "[package]\nname = \"b\"\n\n[dependencies]\na = { path = \"../a\" }\n",
        )
        .unwrap();
        // Give them source files so they pass the source check
        let src_a = pkg_a.join("src");
        let src_b = pkg_b.join("src");
        std::fs::create_dir_all(&src_a).unwrap();
        std::fs::create_dir_all(&src_b).unwrap();
        std::fs::write(
            src_a.join("main.lm.md"),
            "# A\n```lumen\ncell a() -> Int\n  return 1\nend\n```\n",
        )
        .unwrap();
        std::fs::write(
            src_b.join("main.lm.md"),
            "# B\n```lumen\ncell b() -> Int\n  return 2\nend\n```\n",
        )
        .unwrap();

        let cfg_a = LumenConfig::load_from(&pkg_a.join("lumen.toml")).unwrap();
        let result = resolve_dependencies(&cfg_a, &pkg_a);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("circular dependency"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_valid_path_dep() {
        let tmp = std::env::temp_dir().join("lumen_valid_dep_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let lib_dir = tmp.join("mylib");
        let lib_src = lib_dir.join("src");
        std::fs::create_dir_all(&lib_src).unwrap();
        std::fs::write(lib_dir.join("lumen.toml"), "[package]\nname = \"mylib\"\n").unwrap();
        std::fs::write(
            lib_src.join("main.lm.md"),
            "# Lib\n```lumen\ncell helper() -> Int\n  return 42\nend\n```\n",
        )
        .unwrap();

        let app_dir = tmp.join("app");
        std::fs::create_dir_all(&app_dir).unwrap();

        let cfg = make_config(vec![("mylib", "../mylib")]);
        let deps = resolve_dependencies(&cfg, &app_dir).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "mylib");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn package_info_parsing() {
        let toml_str = r#"
[package]
name = "test"
version = "1.0.0"
description = "A test package"
authors = ["Dev"]

[dependencies]
foo = { path = "./foo" }
"#;
        let cfg: LumenConfig = toml::from_str(toml_str).unwrap();
        let pkg = cfg.package.unwrap();
        assert_eq!(pkg.name, "test");
        assert_eq!(pkg.version.unwrap(), "1.0.0");
        assert_eq!(pkg.description.unwrap(), "A test package");
        assert_eq!(pkg.authors.unwrap(), vec!["Dev"]);
        assert_eq!(cfg.dependencies.len(), 1);
    }

    #[test]
    fn find_lumen_sources_in_src() {
        let tmp = std::env::temp_dir().join("lumen_find_src_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.lm.md"), "# test\n").unwrap();
        std::fs::write(src.join("lib.lm.md"), "# lib\n").unwrap();
        std::fs::write(src.join("readme.md"), "# not lumen\n").unwrap();

        let sources = find_lumen_sources(&tmp);
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|p| is_lumen_source(p)));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_lumen_sources_supports_raw_files() {
        let tmp = std::env::temp_dir().join("lumen_find_raw_src_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.lm"), "cell main() -> Int\n  return 1\nend\n").unwrap();
        std::fs::write(
            src.join("models.lm.md"),
            "# m\n```lumen\ncell x() -> Int\n  return 1\nend\n```\n",
        )
        .unwrap();

        let sources = find_lumen_sources(&tmp);
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|p| is_lumen_source(p)));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn unique_tmp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), stamp))
    }

    #[test]
    fn lock_source_path_is_project_relative() {
        let tmp = unique_tmp_dir("lumen_lock_source_relative");
        let app_dir = tmp.join("app");
        let dep_dir = tmp.join("dep");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&dep_dir).unwrap();

        let source = lock_source_path(&app_dir, &dep_dir);
        assert_eq!(source, "../dep");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_lockfile_uses_relative_path_sources() {
        let tmp = unique_tmp_dir("lumen_build_lockfile_relative");
        let app_dir = tmp.join("app");
        let lib_dir = tmp.join("mylib");
        std::fs::create_dir_all(app_dir.join("src")).unwrap();
        std::fs::create_dir_all(lib_dir.join("src")).unwrap();

        std::fs::write(
            app_dir.join("lumen.toml"),
            "[package]\nname = \"app\"\n\n[dependencies]\nmylib = { path = \"../mylib\" }\n",
        )
        .unwrap();
        std::fs::write(
            lib_dir.join("lumen.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.3.0\"\n",
        )
        .unwrap();
        std::fs::write(
            lib_dir.join("src/main.lm.md"),
            "# lib\n```lumen\ncell helper() -> Int\n  return 1\nend\n```\n",
        )
        .unwrap();

        let cfg = LumenConfig::load_from(&app_dir.join("lumen.toml")).unwrap();
        let (lock, _) = build_lockfile(&cfg, &app_dir).unwrap();
        let mylib = lock.get_package("mylib").unwrap();
        assert_eq!(mylib.version, "0.3.0");
        assert_eq!(mylib.get_path(), Some("../mylib"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sync_lockfile_frozen_fails_when_missing() {
        let tmp = unique_tmp_dir("lumen_sync_frozen_missing");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock_path = tmp.join("lumen.lock");
        let mut desired = LockFile::default();
        desired.add_package(LockedPackage::from_path(
            "dep".to_string(),
            "../dep".to_string(),
        ));

        let err = sync_lockfile(&lock_path, &desired, true).unwrap_err();
        assert!(err.contains("missing"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sync_lockfile_frozen_fails_when_outdated() {
        let tmp = unique_tmp_dir("lumen_sync_frozen_outdated");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock_path = tmp.join("lumen.lock");

        let mut existing = LockFile::default();
        let mut dep = LockedPackage::from_path("dep".to_string(), "../dep".to_string());
        dep.version = "0.1.0".to_string();
        existing.add_package(dep);
        existing.save(&lock_path).unwrap();

        let mut desired = LockFile::default();
        let mut dep = LockedPackage::from_path("dep".to_string(), "../dep".to_string());
        dep.version = "0.2.0".to_string();
        desired.add_package(dep);

        let err = sync_lockfile(&lock_path, &desired, true).unwrap_err();
        assert!(err.contains("out of date"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sync_lockfile_frozen_succeeds_when_matching() {
        let tmp = unique_tmp_dir("lumen_sync_frozen_match");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock_path = tmp.join("lumen.lock");
        let mut desired = LockFile::default();
        let mut dep = LockedPackage::from_path("dep".to_string(), "../dep".to_string());
        dep.version = "0.2.0".to_string();
        desired.add_package(dep);
        desired.save(&lock_path).unwrap();

        let outcome = sync_lockfile(&lock_path, &desired, true).unwrap();
        assert_eq!(outcome, LockSyncOutcome::Unchanged);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sync_lockfile_updates_when_not_frozen() {
        let tmp = unique_tmp_dir("lumen_sync_not_frozen_update");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock_path = tmp.join("lumen.lock");

        let mut existing = LockFile::default();
        let mut dep = LockedPackage::from_path("dep".to_string(), "../dep".to_string());
        dep.version = "0.1.0".to_string();
        existing.add_package(dep);
        existing.save(&lock_path).unwrap();

        let mut desired = LockFile::default();
        let mut dep = LockedPackage::from_path("dep".to_string(), "../dep".to_string());
        dep.version = "0.2.0".to_string();
        desired.add_package(dep);

        let outcome = sync_lockfile(&lock_path, &desired, false).unwrap();
        assert_eq!(outcome, LockSyncOutcome::Updated);
        let loaded = LockFile::load(&lock_path).unwrap();
        assert_eq!(loaded.get_package("dep").unwrap().version, "0.2.0");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn write_packable_fixture(tmp: &Path) -> (PathBuf, LumenConfig) {
        let pkg_dir = tmp.join("fixture");
        std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
        std::fs::write(
            pkg_dir.join("lumen.toml"),
            "[package]\nname = \"demo-pkg\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            pkg_dir.join("src/main.lm.md"),
            "# main\n```lumen\ncell main() -> Int\n  return 1\nend\n```\n",
        )
        .unwrap();
        std::fs::write(
            pkg_dir.join("src/lib.lm.md"),
            "# lib\n```lumen\ncell helper() -> Int\n  return 2\nend\n```\n",
        )
        .unwrap();
        std::fs::write(pkg_dir.join("README.md"), "# Demo\n").unwrap();
        std::fs::write(pkg_dir.join("LICENSE"), "MIT\n").unwrap();

        let cfg = LumenConfig::load_from(&pkg_dir.join("lumen.toml")).unwrap();
        (pkg_dir, cfg)
    }

    fn parse_tar_entry_names(path: &Path) -> Vec<String> {
        let data = std::fs::read(path).unwrap();
        let mut names = Vec::new();
        let mut offset = 0usize;

        while offset + 512 <= data.len() {
            let header = &data[offset..offset + 512];
            if header.iter().all(|b| *b == 0) {
                break;
            }

            let name = read_tar_text(&header[0..100]);
            let prefix = read_tar_text(&header[345..500]);
            if prefix.is_empty() {
                names.push(name);
            } else {
                names.push(format!("{}/{}", prefix, name));
            }

            let size = parse_tar_octal(&header[124..136]) as usize;
            offset += 512 + size;
            let rem = offset % 512;
            if rem != 0 {
                offset += 512 - rem;
            }
        }

        names
    }

    fn read_tar_text(field: &[u8]) -> String {
        let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
        String::from_utf8_lossy(&field[..end]).trim().to_string()
    }

    fn parse_tar_octal(field: &[u8]) -> u64 {
        let raw: Vec<u8> = field
            .iter()
            .copied()
            .take_while(|b| *b != 0 && *b != b' ')
            .collect();
        let txt = String::from_utf8_lossy(&raw).trim().to_string();
        if txt.is_empty() {
            0
        } else {
            u64::from_str_radix(&txt, 8).unwrap()
        }
    }

    #[test]
    fn pack_pipeline_is_deterministic() {
        let tmp = unique_tmp_dir("lumen_pack_deterministic");
        std::fs::create_dir_all(&tmp).unwrap();
        let (pkg_dir, cfg) = write_packable_fixture(&tmp);

        let first_tar = pkg_dir.join("dist/first.tar");
        let second_tar = pkg_dir.join("dist/second.tar");
        let first_report = pack_current_package(&pkg_dir, &cfg, &first_tar).unwrap();
        let second_report = pack_current_package(&pkg_dir, &cfg, &second_tar).unwrap();

        let first_bytes = std::fs::read(&first_tar).unwrap();
        let second_bytes = std::fs::read(&second_tar).unwrap();
        assert_eq!(first_bytes, second_bytes);
        assert_eq!(first_report.file_count, 5);
        assert_eq!(second_report.file_count, 5);
        assert_eq!(
            first_report.content_checksum,
            second_report.content_checksum
        );
        assert_eq!(
            first_report.archive_checksum,
            second_report.archive_checksum
        );
        assert!(first_report.content_checksum.starts_with("sha256:"));
        assert!(first_report.archive_checksum.starts_with("sha256:"));

        let entry_names = parse_tar_entry_names(&first_tar);
        assert_eq!(
            entry_names,
            vec![
                "LICENSE".to_string(),
                "README.md".to_string(),
                "lumen.toml".to_string(),
                "src/lib.lm.md".to_string(),
                "src/main.lm.md".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn publish_dry_run_reports_pack_results() {
        let tmp = unique_tmp_dir("lumen_publish_dry_run");
        std::fs::create_dir_all(&tmp).unwrap();
        let (pkg_dir, cfg) = write_packable_fixture(&tmp);

        let report = run_publish_dry_run(&pkg_dir, &cfg).unwrap();
        assert_eq!(report.package_name, "demo-pkg");
        assert_eq!(report.version, "0.1.0");
        assert_eq!(report.file_count, 5);
        assert!(report.archive_size_bytes > 0);
        assert!(report.archive_path.is_file());
        assert!(report.content_checksum.starts_with("sha256:"));
        assert!(report.archive_checksum.starts_with("sha256:"));

        let _ = std::fs::remove_file(&report.archive_path);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn publish_dry_run_fails_without_version() {
        let tmp = unique_tmp_dir("lumen_publish_no_version");
        std::fs::create_dir_all(&tmp).unwrap();
        let (pkg_dir, mut cfg) = write_packable_fixture(&tmp);
        cfg.package.as_mut().unwrap().version = None;

        let err = run_publish_dry_run(&pkg_dir, &cfg).unwrap_err();
        assert!(err.contains("package.version"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn package_content_checksum_requires_sorted_entries() {
        let entries = vec![
            PackageEntry {
                archive_path: "z.lm".to_string(),
                bytes: vec![1, 2, 3],
            },
            PackageEntry {
                archive_path: "a.lm".to_string(),
                bytes: vec![4, 5, 6],
            },
        ];
        let err = package_content_checksum(&entries).unwrap_err();
        assert!(err.contains("non-deterministic package entry order"));
    }

    #[test]
    fn inspect_archive_reports_metadata_and_checksums() {
        let tmp = unique_tmp_dir("lumen_info_archive");
        std::fs::create_dir_all(&tmp).unwrap();
        let (pkg_dir, cfg) = write_packable_fixture(&tmp);

        let archive_path = pkg_dir.join("dist/info.tar");
        pack_current_package(&pkg_dir, &cfg, &archive_path).unwrap();
        let report = read_archive_package_inspect(&archive_path).unwrap();

        assert_eq!(report.package_name, "demo-pkg");
        assert_eq!(report.version, "0.1.0");
        assert_eq!(report.file_count, 5);
        assert_eq!(report.entries[0], "LICENSE");
        assert!(report.archive_checksum.unwrap().starts_with("sha256:"));
        assert!(report.content_checksum.starts_with("sha256:"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
