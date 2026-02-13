//! Lumen CLI — command-line interface for the Lumen language.

use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { file } => cmd_check(&file),
        Commands::Run { file, cell, trace_dir } => cmd_run(&file, &cell, trace_dir),
        Commands::Emit { file, output } => cmd_emit(&file, output),
        Commands::Trace { sub } => match sub {
            TraceCommands::Show { run_id, trace_dir } => cmd_trace_show(&run_id, &trace_dir),
        },
        Commands::Cache { sub } => match sub {
            CacheCommands::Clear { cache_dir } => cmd_cache_clear(&cache_dir),
        },
    }
}

fn read_source(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read file '{}': {}", path.display(), e);
        std::process::exit(1);
    })
}

fn cmd_check(file: &PathBuf) {
    let source = read_source(file);
    match lumen_compiler::compile(&source) {
        Ok(_module) => {
            println!("✓ {} — no errors", file.display());
        }
        Err(e) => {
            eprintln!("✗ {} — {}", file.display(), e);
            std::process::exit(1);
        }
    }
}

fn cmd_run(file: &PathBuf, cell: &str, trace_dir: Option<PathBuf>) {
    let source = read_source(file);
    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("compile error: {}", e);
            std::process::exit(1);
        }
    };

    // Optionally set up tracing
    let mut trace_store = trace_dir.map(|dir| {
        lumen_runtime::trace::store::TraceStore::new(&dir)
    });

    if let Some(ref mut ts) = trace_store {
        ts.start_run(&module.doc_hash);
        ts.cell_start(cell);
    }

    let mut vm = lumen_vm::vm::VM::new();
    vm.load(module);
    match vm.execute(cell, vec![]) {
        Ok(result) => {
            if let Some(ref mut ts) = trace_store {
                ts.cell_end(cell);
                ts.end_run();
                println!("trace: {}", ts.run_id());
            }
            println!("{}", result);
        }
        Err(e) => {
            if let Some(ref mut ts) = trace_store {
                ts.error(Some(cell), &format!("{}", e));
                ts.end_run();
            }
            eprintln!("runtime error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_emit(file: &PathBuf, output: Option<PathBuf>) {
    let source = read_source(file);
    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("compile error: {}", e);
            std::process::exit(1);
        }
    };

    let json = lumen_compiler::compiler::emit::emit_json(&module);

    if let Some(ref out_path) = output {
        std::fs::write(out_path, &json).unwrap_or_else(|e| {
            eprintln!("error writing to '{}': {}", out_path.display(), e);
            std::process::exit(1);
        });
        println!("wrote LIR to {}", out_path.display());
    } else {
        println!("{}", json);
    }
}

fn cmd_trace_show(run_id: &str, trace_dir: &PathBuf) {
    let path = trace_dir.join(format!("{}.jsonl", run_id));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            for line in content.lines() {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Ok(pretty) = serde_json::to_string_pretty(&event) {
                        println!("{}", pretty);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: cannot read trace '{}': {}", path.display(), e);
            std::process::exit(1);
        }
    }
}

fn cmd_cache_clear(cache_dir: &PathBuf) {
    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).unwrap_or_else(|e| {
            eprintln!("error clearing cache: {}", e);
            std::process::exit(1);
        });
        println!("cache cleared");
    } else {
        println!("cache directory does not exist: {}", cache_dir.display());
    }
}
