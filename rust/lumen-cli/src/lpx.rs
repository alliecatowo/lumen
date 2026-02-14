//! lpx — Lumen Package Executor
//! Run a Lumen file or package cell directly.

mod module_resolver;

use clap::Parser;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "lpx",
    about = "Lumen Package Executor — run any .lm/.lumen/.lm.md/.lumen.md file",
    version
)]
struct Args {
    /// File or package to run
    file: PathBuf,
    /// Cell to execute
    #[arg(long, default_value = "main")]
    cell: String,
    /// Emit trace to the given directory
    #[arg(long)]
    trace_dir: Option<PathBuf>,
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("lumen.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn compile_source_file(
    path: &Path,
    source: &str,
) -> Result<lumen_compiler::compiler::lir::LirModule, lumen_compiler::CompileError> {
    let source_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut resolver = module_resolver::ModuleResolver::new(source_dir.clone());

    if let Some(project_root) = find_project_root(&source_dir) {
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && src_dir != source_dir {
            resolver.add_root(src_dir);
        }
        if project_root != source_dir {
            resolver.add_root(project_root);
        }
    }

    let resolver = RefCell::new(resolver);
    let resolve_import = |module_path: &str| resolver.borrow_mut().resolve(module_path);

    lumen_compiler::compile_with_imports(source, &resolve_import)
}

fn main() {
    let args = Args::parse();

    // Read and compile the source file
    let source = std::fs::read_to_string(&args.file).unwrap_or_else(|e| {
        eprintln!(
            "\x1b[31merror:\x1b[0m cannot read file '{}': {}",
            args.file.display(),
            e
        );
        std::process::exit(1);
    });

    let filename = args.file.display().to_string();

    println!("\x1b[1;32m{:>12}\x1b[0m {}", "Compiling", filename);
    let module = match compile_source_file(&args.file, &source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("\x1b[31merror:\x1b[0m compilation failed");
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    };

    // Set up VM with provider registry
    let registry = lumen_runtime::tools::ProviderRegistry::new();

    // Set up tracing if requested
    let mut trace_store = args
        .trace_dir
        .map(|dir| lumen_runtime::trace::store::TraceStore::new(&dir));

    if let Some(ref mut ts) = trace_store {
        ts.start_run(&module.doc_hash);
        ts.cell_start(&args.cell);
    }

    println!("\x1b[1;32m{:>12}\x1b[0m {}", "Running", args.cell);
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);

    match vm.execute(&args.cell, vec![]) {
        Ok(result) => {
            if let Some(ref mut ts) = trace_store {
                ts.cell_end(&args.cell);
                ts.end_run();
                println!("\x1b[90mtrace:\x1b[0m {}", ts.run_id());
            }
            println!("{}", result);
        }
        Err(e) => {
            if let Some(ref mut ts) = trace_store {
                ts.error(Some(&args.cell), &format!("{}", e));
                ts.end_run();
            }
            eprintln!("\x1b[31mruntime error:\x1b[0m {}", e);
            std::process::exit(1);
        }
    }
}
