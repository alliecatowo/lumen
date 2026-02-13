//! Package manager operations for Lumen projects.
//!
//! Provides `pkg init` (scaffold a new package) and `pkg build`
//! (resolve path-based dependencies and compile).

use crate::config::{DependencySpec, LumenConfig};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A resolved dependency ready for compilation.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub path: PathBuf,
    pub config: LumenConfig,
}

/// Scaffold a new Lumen package in the current directory (or a named subdirectory).
pub fn cmd_pkg_init(name: Option<String>) {
    let base = match &name {
        Some(n) => {
            let p = PathBuf::from(n);
            if p.exists() {
                eprintln!("error: directory '{}' already exists", n);
                std::process::exit(1);
            }
            std::fs::create_dir_all(p.join("src")).unwrap_or_else(|e| {
                eprintln!("error: cannot create directory: {}", e);
                std::process::exit(1);
            });
            p
        }
        None => PathBuf::from("."),
    };

    let pkg_name = name
        .clone()
        .unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_else(|| "my-package".to_string())
        });

    let toml_path = base.join("lumen.toml");
    if toml_path.exists() {
        eprintln!("error: lumen.toml already exists in '{}'", base.display());
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
        eprintln!("error writing lumen.toml: {}", e);
        std::process::exit(1);
    });

    // Create src/main.lm.md
    let src_dir = base.join("src");
    if !src_dir.exists() {
        std::fs::create_dir_all(&src_dir).unwrap_or_else(|e| {
            eprintln!("error creating src directory: {}", e);
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
        eprintln!("error writing main.lm.md: {}", e);
        std::process::exit(1);
    });

    if name.is_some() {
        println!("created package '{}' in {}/", pkg_name, base.display());
    } else {
        println!("initialized package '{}' in current directory", pkg_name);
    }
    println!("  lumen.toml");
    println!("  src/main.lm.md");
}

/// Build a Lumen package: resolve dependencies and compile.
pub fn cmd_pkg_build() {
    let (config_path, config) = match LumenConfig::load_with_path() {
        Some(pair) => pair,
        None => {
            eprintln!("error: no lumen.toml found (run `lumen pkg init` first)");
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
            eprintln!("dependency error: {}", e);
            std::process::exit(1);
        }
    };

    let mut errors = 0;

    // Compile each dependency
    for dep in &deps {
        print!("  compiling dependency '{}' ... ", dep.name);
        match compile_package_sources(&dep.path) {
            Ok(count) => println!("ok ({} file{})", count, if count == 1 { "" } else { "s" }),
            Err(e) => {
                println!("FAILED");
                eprintln!("    {}", e);
                errors += 1;
            }
        }
    }

    // Compile the main package
    print!("  compiling '{}' ... ", pkg_name);
    match compile_package_sources(project_dir) {
        Ok(count) => println!("ok ({} file{})", count, if count == 1 { "" } else { "s" }),
        Err(e) => {
            println!("FAILED");
            eprintln!("    {}", e);
            errors += 1;
        }
    }

    if errors > 0 {
        eprintln!("\nbuild failed with {} error{}", errors, if errors == 1 { "" } else { "s" });
        std::process::exit(1);
    } else {
        let total = deps.len() + 1;
        println!(
            "\nbuild succeeded ({} package{})",
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

    for (name, spec) in &config.dependencies {
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
    };

    if !dep_path.exists() {
        return Err(format!(
            "dependency '{}': path '{}' does not exist",
            name,
            dep_path.display()
        ));
    }

    // Check for lumen.toml or .lm.md files
    let dep_config_path = dep_path.join("lumen.toml");
    let dep_config = if dep_config_path.exists() {
        LumenConfig::load_from(&dep_config_path)?
    } else {
        // No lumen.toml — check for .lm.md files
        let has_sources = has_lumen_sources(&dep_path);
        if !has_sources {
            return Err(format!(
                "dependency '{}': no lumen.toml or .lm.md files found in '{}'",
                name,
                dep_path.display()
            ));
        }
        LumenConfig::default()
    };

    // Resolve transitive dependencies
    for (sub_name, sub_spec) in &dep_config.dependencies {
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

/// Compile all `.lm.md` files found in a package directory.
/// Returns the number of files compiled, or the first error.
fn compile_package_sources(pkg_dir: &Path) -> Result<usize, String> {
    let sources = find_lumen_sources(pkg_dir);
    if sources.is_empty() {
        return Err(format!("no .lm.md files found in '{}'", pkg_dir.display()));
    }

    for src in &sources {
        let content = std::fs::read_to_string(src)
            .map_err(|e| format!("cannot read '{}': {}", src.display(), e))?;
        lumen_compiler::compile(&content)
            .map_err(|e| format!("{}: {}", src.display(), e))?;
    }

    Ok(sources.len())
}

/// Find all `.lm.md` files in a directory (searches `src/` subdirectory first,
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

    sources
}

fn collect_lm_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".lm.md") {
                        out.push(path);
                    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        std::fs::write(src_a.join("main.lm.md"), "# A\n```lumen\ncell a() -> Int\n  return 1\nend\n```\n").unwrap();
        std::fs::write(src_b.join("main.lm.md"), "# B\n```lumen\ncell b() -> Int\n  return 2\nend\n```\n").unwrap();

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
        std::fs::write(
            lib_dir.join("lumen.toml"),
            "[package]\nname = \"mylib\"\n",
        )
        .unwrap();
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
        assert!(sources.iter().all(|p| p.to_str().unwrap().ends_with(".lm.md")));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
