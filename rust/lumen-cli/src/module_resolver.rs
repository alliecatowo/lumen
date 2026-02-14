//! Module resolution for Lumen imports.

use std::collections::HashMap;
use std::path::PathBuf;

/// Resolves import paths to source files.
///
/// Handles `.lm`, `.lumen`, `.lm.md`, and `.lumen.md` sources.
/// For module paths like "utils.math", checks:
/// - utils/math.lm
/// - utils/math.lumen
/// - utils/math.lm.md
/// - utils/math.lumen.md
pub struct ModuleResolver {
    /// Search roots for resolving relative imports.
    search_roots: Vec<PathBuf>,
    /// Cache of resolved module paths to source content
    cache: HashMap<String, String>,
}

impl ModuleResolver {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            search_roots: vec![base_dir],
            cache: HashMap::new(),
        }
    }

    pub fn add_root(&mut self, root: PathBuf) {
        if !self.search_roots.contains(&root) {
            self.search_roots.push(root);
        }
    }

    /// Resolve a module path to its source content.
    ///
    /// Module path format: "utils.math" resolves to a supported Lumen source path.
    pub fn resolve(&mut self, module_path: &str) -> Option<String> {
        // Check cache first
        if let Some(cached) = self.cache.get(module_path) {
            return Some(cached.clone());
        }

        // Convert module.path.notation to filesystem path
        let fs_path = module_path.replace('.', "/");

        for root in &self.search_roots {
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

            for path in &candidates {
                if path.exists() {
                    if let Ok(source) = std::fs::read_to_string(path) {
                        self.cache.insert(module_path.to_string(), source.clone());
                        return Some(source);
                    }
                }
            }
        }

        None
    }
}
