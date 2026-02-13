//! Lumen CLI — command-line interface for the Lumen language.

mod config;
mod doc;
mod fmt;
mod pkg;
mod repl;

use clap::{Parser as ClapParser, Subcommand};
use std::path::{Path, PathBuf};

// ANSI color helpers
fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}
fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}
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

#[derive(ClapParser)]
#[command(name = "lumen", version, about = "The Lumen programming language")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Type-check a .lm.md source file
    Check {
        /// Path to the .lm.md file
        #[arg()]
        file: PathBuf,
    },
    /// Compile and run a .lm.md file
    Run {
        /// Path to the .lm.md file
        #[arg()]
        file: PathBuf,

        /// Entry cell name (default: main)
        #[arg(long, default_value = "main")]
        cell: String,

        /// Emit trace to the given directory
        #[arg(long)]
        trace_dir: Option<PathBuf>,
    },
    /// Compile a .lm.md file to LIR JSON
    Emit {
        /// Path to the .lm.md file
        #[arg()]
        file: PathBuf,

        /// Output path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Show trace for a run
    Trace {
        #[command(subcommand)]
        sub: TraceCommands,
    },
    /// Manage the tool result cache
    Cache {
        #[command(subcommand)]
        sub: CacheCommands,
    },
    /// Create a lumen.toml config file in the current directory
    Init,
    /// Start an interactive REPL
    Repl,
    /// Package manager commands
    Pkg {
        #[command(subcommand)]
        sub: PkgCommands,
    },
    /// Format Lumen source files
    Fmt {
        /// Files to format (or stdin)
        files: Vec<PathBuf>,
        /// Check mode: exit 1 if files would change
        #[arg(long)]
        check: bool,
    },
    /// Generate documentation from .lm.md files
    Doc {
        /// Input file or directory
        path: PathBuf,
        /// Output format (markdown or json)
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Output file (defaults to stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TraceCommands {
    /// Show trace events for a run
    Show {
        /// Run ID
        run_id: String,
        /// Trace directory
        #[arg(long, default_value = ".lumen/trace")]
        trace_dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum CacheCommands {
    /// Clear the tool result cache
    Clear {
        /// Cache directory
        #[arg(long, default_value = ".lumen/cache")]
        cache_dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum PkgCommands {
    /// Create a new Lumen package
    Init {
        /// Package name (creates a subdirectory; omit to init in current dir)
        name: Option<String>,
    },
    /// Compile the package and all dependencies
    Build,
    /// Type-check the package and all dependencies without running
    Check,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { file } => cmd_check(&file),
        Commands::Run {
            file,
            cell,
            trace_dir,
        } => cmd_run(&file, &cell, trace_dir),
        Commands::Emit { file, output } => cmd_emit(&file, output),
        Commands::Trace { sub } => match sub {
            TraceCommands::Show { run_id, trace_dir } => cmd_trace_show(&run_id, &trace_dir),
        },
        Commands::Cache { sub } => match sub {
            CacheCommands::Clear { cache_dir } => cmd_cache_clear(&cache_dir),
        },
        Commands::Init => cmd_init(),
        Commands::Repl => repl::run_repl(),
        Commands::Pkg { sub } => match sub {
            PkgCommands::Init { name } => pkg::cmd_pkg_init(name),
            PkgCommands::Build => pkg::cmd_pkg_build(),
            PkgCommands::Check => pkg::cmd_pkg_check(),
        },
        Commands::Fmt { files, check } => cmd_fmt(files, check),
        Commands::Doc {
            path,
            format,
            output,
        } => cmd_doc(&path, &format, output),
    }
}

fn cmd_doc(path: &PathBuf, format: &str, output: Option<PathBuf>) {
    match doc::cmd_doc(path, format, output.as_deref()) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

fn read_source(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("{} cannot read file '{}': {}", red("error:"), bold(&path.display().to_string()), e);
        std::process::exit(1);
    })
}

fn cmd_check(file: &PathBuf) {
    let source = read_source(file);
    let filename = file.display().to_string();
    match lumen_compiler::compile(&source) {
        Ok(_module) => {
            println!("{} {} {}", green("✓"), bold(&filename), gray("— no errors found"));
        }
        Err(e) => {
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    }
}

fn cmd_run(file: &PathBuf, cell: &str, trace_dir: Option<PathBuf>) {
    let source = read_source(file);
    let filename = file.display().to_string();

    println!("{} {}", status_label("Compiling"), filename);
    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} compilation failed", red("error:"));
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    };

    // Load project config and build provider registry
    let _config = config::LumenConfig::load();
    let registry = lumen_runtime::tools::ProviderRegistry::new();
    // TODO: populate registry from _config.providers once concrete providers exist

    // Optionally set up tracing
    let mut trace_store = trace_dir.map(|dir| lumen_runtime::trace::store::TraceStore::new(&dir));

    if let Some(ref mut ts) = trace_store {
        ts.start_run(&module.doc_hash);
        ts.cell_start(cell);
    }

    println!("{} {}", status_label("Running"), cell);
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);
    match vm.execute(cell, vec![]) {
        Ok(result) => {
            if let Some(ref mut ts) = trace_store {
                ts.cell_end(cell);
                ts.end_run();
                println!("{} {}", gray("trace:"), ts.run_id());
            }
            println!("{}", result);
        }
        Err(e) => {
            if let Some(ref mut ts) = trace_store {
                ts.error(Some(cell), &format!("{}", e));
                ts.end_run();
            }
            eprintln!("{} {}", red("runtime error:"), e);
            std::process::exit(1);
        }
    }
}

fn cmd_emit(file: &PathBuf, output: Option<PathBuf>) {
    let source = read_source(file);
    let filename = file.display().to_string();

    println!("{} {}", status_label("Compiling"), filename);
    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} compilation failed", red("error:"));
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    };

    let json = lumen_compiler::compiler::emit::emit_json(&module);

    if let Some(ref out_path) = output {
        println!("{} LIR to {}", status_label("Emitting"), out_path.display());
        std::fs::write(out_path, &json).unwrap_or_else(|e| {
            eprintln!("{} writing to '{}': {}", red("error:"), out_path.display(), e);
            std::process::exit(1);
        });
    } else {
        println!("{} LIR to stdout", status_label("Emitting"));
        println!("{}", json);
    }
}

fn cmd_trace_show(run_id: &str, trace_dir: &Path) {
    let path = trace_dir.join(format!("{}.jsonl", run_id));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            println!("{} trace for run {}", status_label("Showing"), cyan(run_id));
            for line in content.lines() {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Ok(pretty) = serde_json::to_string_pretty(&event) {
                        println!("{}", pretty);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{} cannot read trace '{}': {}", red("error:"), path.display(), e);
            std::process::exit(1);
        }
    }
}

fn cmd_cache_clear(cache_dir: &PathBuf) {
    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).unwrap_or_else(|e| {
            eprintln!("{} clearing cache: {}", red("error:"), e);
            std::process::exit(1);
        });
        println!("{} cache cleared", green("✓"));
    } else {
        println!("{} cache directory does not exist: {}", yellow("warning:"), cache_dir.display());
    }
}

fn cmd_init() {
    let path = PathBuf::from("lumen.toml");
    if path.exists() {
        eprintln!("{} lumen.toml already exists — not overwriting", red("error:"));
        std::process::exit(1);
    }
    std::fs::write(&path, config::LumenConfig::default_template()).unwrap_or_else(|e| {
        eprintln!("{} writing lumen.toml: {}", red("error:"), e);
        std::process::exit(1);
    });
    println!("{} lumen.toml", status_label("Created"));
}

fn cmd_fmt(files: Vec<PathBuf>, check: bool) {
    if files.is_empty() {
        eprintln!("{} no files specified", red("error:"));
        std::process::exit(1);
    }

    match fmt::format_files(&files, check) {
        Ok(needs_formatting) => {
            if check && needs_formatting {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}
