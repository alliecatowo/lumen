//! Lumen CLI — command-line interface for the Lumen language.

mod config;
mod doc;
mod fmt;
mod lint;
mod lockfile;
mod module_resolver;
mod pkg;
mod repl;
mod test_cmd;

use clap::{Parser as ClapParser, Subcommand, ValueEnum};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
#[command(
    name = "lumen",
    version,
    about = "The Lumen programming language — statically typed, AI-native systems",
    long_about = "Lumen is a statically typed programming language for AI-native systems.\n\n\
                  Learn more at: https://github.com/alliecatowo/lumen",
    help_template = "\
{before-help}{name} {version}
{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}

Examples:
  lumen check hello.lm                 Type-check a file
  lumen run hello.lumen                Compile and run (default: main cell)
  lumen run hello.lm.md --cell test    Run a specific cell
  lumen fmt *.lm.md                    Format source files
  lumen test                           Run all test_* cells
  lumen lint --strict src/             Lint with warnings as errors
"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Type-check a `.lm`, `.lumen`, `.lm.md`, or `.lumen.md` source file
    Check {
        /// Path to the source file
        #[arg()]
        file: PathBuf,
    },
    /// Compile and run a `.lm`, `.lumen`, `.lm.md`, or `.lumen.md` file
    Run {
        /// Path to the source file
        #[arg()]
        file: PathBuf,

        /// Entry cell name (default: main)
        #[arg(long, default_value = "main")]
        cell: String,

        /// Emit trace to the given directory
        #[arg(long)]
        trace_dir: Option<PathBuf>,
    },
    /// Compile a `.lm`, `.lumen`, `.lm.md`, or `.lumen.md` file to LIR JSON
    Emit {
        /// Path to the source file
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
    /// Lint Lumen source files
    Lint {
        /// Files to lint
        files: Vec<PathBuf>,
        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,
    },
    /// Run tests by discovering test_* cells
    Test {
        /// File or directory to test (default: current directory)
        path: Option<PathBuf>,
        /// Filter tests by name substring
        #[arg(long)]
        filter: Option<String>,
        /// Show additional details
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run CI-style quality gate (check + lint + test + doc sanity)
    Ci {
        /// File or directory to validate (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Build commands
    Build {
        #[command(subcommand)]
        sub: BuildCommands,
    },
}

#[derive(Subcommand)]
enum BuildCommands {
    /// Build for WebAssembly target
    Wasm {
        /// Target type (web, nodejs, or wasi)
        #[arg(long, default_value = "web")]
        target: String,
        /// Release build (optimized)
        #[arg(long)]
        release: bool,
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
        /// Output format
        #[arg(long, value_enum, default_value_t = TraceShowFormat::Pretty)]
        format: TraceShowFormat,
        /// Verify sequence and hash chain before rendering
        #[arg(long)]
        verify_chain: bool,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum TraceShowFormat {
    Pretty,
    Replay,
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
    /// Add a dependency to this package
    Add {
        /// Package name
        package: String,
        /// Path to the dependency
        #[arg(long)]
        path: Option<String>,
    },
    /// Remove a dependency from this package
    Remove {
        /// Package name
        package: String,
    },
    /// List dependencies
    List,
    /// Install dependencies from lumen.toml and write lumen.lock
    Install {
        /// Fail if lumen.lock would be changed
        #[arg(long, alias = "locked")]
        frozen: bool,
    },
    /// Update dependencies to latest compatible versions
    Update {
        /// Fail if lumen.lock would be changed
        #[arg(long, alias = "locked")]
        frozen: bool,
    },
    /// Search for a package in the registry
    Search {
        /// Search query
        query: String,
    },
    /// Create a deterministic package archive in dist/
    Pack,
    /// Validate package metadata/contents and (eventually) publish
    Publish {
        /// Validate/package locally without uploading
        #[arg(long)]
        dry_run: bool,
    },
}

/// Register all provider crates into the runtime registry.
fn register_providers(
    registry: &mut lumen_runtime::tools::ProviderRegistry,
    #[allow(unused_variables)] config: &config::LumenConfig,
) {
    // Auto-register built-in providers (always available by default)

    #[cfg(feature = "fs")]
    {
        registry.register("fs.read", Box::new(lumen_provider_fs::FsProvider::read()));
        registry.register("fs.write", Box::new(lumen_provider_fs::FsProvider::write()));
        registry.register(
            "fs.exists",
            Box::new(lumen_provider_fs::FsProvider::exists()),
        );
        registry.register("fs.list", Box::new(lumen_provider_fs::FsProvider::list()));
        registry.register("fs.mkdir", Box::new(lumen_provider_fs::FsProvider::mkdir()));
        registry.register(
            "fs.remove",
            Box::new(lumen_provider_fs::FsProvider::remove()),
        );
    }

    #[cfg(feature = "env")]
    {
        registry.register("env.get", Box::new(lumen_provider_env::EnvProvider::get()));
        registry.register("env.set", Box::new(lumen_provider_env::EnvProvider::set()));
        registry.register(
            "env.list",
            Box::new(lumen_provider_env::EnvProvider::list()),
        );
        registry.register("env.has", Box::new(lumen_provider_env::EnvProvider::has()));
        registry.register("env.cwd", Box::new(lumen_provider_env::EnvProvider::cwd()));
        registry.register(
            "env.home",
            Box::new(lumen_provider_env::EnvProvider::home()),
        );
        registry.register(
            "env.platform",
            Box::new(lumen_provider_env::EnvProvider::platform()),
        );
        registry.register(
            "env.args",
            Box::new(lumen_provider_env::EnvProvider::args()),
        );
    }

    #[cfg(feature = "json")]
    {
        registry.register("json", Box::new(lumen_provider_json::JsonProvider::new()));
    }

    #[cfg(feature = "crypto")]
    {
        registry.register(
            "crypto.sha256",
            Box::new(lumen_provider_crypto::CryptoProvider::sha256()),
        );
        registry.register(
            "crypto.sha512",
            Box::new(lumen_provider_crypto::CryptoProvider::sha512()),
        );
        registry.register(
            "crypto.md5",
            Box::new(lumen_provider_crypto::CryptoProvider::md5()),
        );
        registry.register(
            "crypto.base64_encode",
            Box::new(lumen_provider_crypto::CryptoProvider::base64_encode()),
        );
        registry.register(
            "crypto.base64_decode",
            Box::new(lumen_provider_crypto::CryptoProvider::base64_decode()),
        );
        registry.register(
            "crypto.uuid",
            Box::new(lumen_provider_crypto::CryptoProvider::uuid()),
        );
        registry.register(
            "crypto.random_int",
            Box::new(lumen_provider_crypto::CryptoProvider::random_int()),
        );
        registry.register(
            "crypto.hmac_sha256",
            Box::new(lumen_provider_crypto::CryptoProvider::hmac_sha256()),
        );
    }

    #[cfg(feature = "http")]
    {
        registry.register(
            "http.get",
            Box::new(lumen_provider_http::HttpProvider::get()),
        );
        registry.register(
            "http.post",
            Box::new(lumen_provider_http::HttpProvider::post()),
        );
        registry.register(
            "http.put",
            Box::new(lumen_provider_http::HttpProvider::put()),
        );
        registry.register(
            "http.delete",
            Box::new(lumen_provider_http::HttpProvider::delete()),
        );
    }

    // Register providers from config (these may override defaults or add new ones)
    #[cfg(feature = "gemini")]
    {
        // Check if gemini is configured in lumen.toml or via env var
        let api_key = config
            .providers
            .tools
            .get("gemini")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| std::env::var("GEMINI_API_KEY").ok());

        if let Some(key) = api_key {
            registry.register(
                "gemini.generate",
                Box::new(lumen_provider_gemini::GeminiProvider::generate(key.clone())),
            );
            registry.register(
                "gemini.chat",
                Box::new(lumen_provider_gemini::GeminiProvider::chat(key.clone())),
            );
            registry.register(
                "gemini.embed",
                Box::new(lumen_provider_gemini::GeminiProvider::embed(key)),
            );
        }
    }
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
            TraceCommands::Show {
                run_id,
                trace_dir,
                format,
                verify_chain,
            } => cmd_trace_show(&run_id, &trace_dir, format, verify_chain),
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
            PkgCommands::Add { package, path } => pkg::cmd_pkg_add(&package, path.as_deref()),
            PkgCommands::Remove { package } => pkg::cmd_pkg_remove(&package),
            PkgCommands::List => pkg::cmd_pkg_list(),
            PkgCommands::Install { frozen } => pkg::cmd_pkg_install_with_lock(frozen),
            PkgCommands::Update { frozen } => pkg::cmd_pkg_update_with_lock(frozen),
            PkgCommands::Search { query } => pkg::cmd_pkg_search(&query),
            PkgCommands::Pack => pkg::cmd_pkg_pack(),
            PkgCommands::Publish { dry_run } => pkg::cmd_pkg_publish(dry_run),
        },
        Commands::Fmt { files, check } => cmd_fmt(files, check),
        Commands::Doc {
            path,
            format,
            output,
        } => cmd_doc(&path, &format, output),
        Commands::Lint { files, strict } => cmd_lint(files, strict),
        Commands::Test {
            path,
            filter,
            verbose,
        } => cmd_test(path, filter, verbose),
        Commands::Ci { path } => cmd_ci(path),
        Commands::Build { sub } => match sub {
            BuildCommands::Wasm { target, release } => cmd_build_wasm(&target, release),
        },
    }
}

fn cmd_lint(files: Vec<PathBuf>, strict: bool) {
    let mode = if strict { "strict mode" } else { "standard" };
    println!(
        "{} {} {} ({})",
        status_label("Linting"),
        files.len(),
        if files.len() == 1 { "file" } else { "files" },
        mode
    );

    let start = std::time::Instant::now();
    match lint::cmd_lint(&files, strict) {
        Ok(summary) => {
            let elapsed = start.elapsed();
            if summary.total_warnings == 0 {
                println!(
                    "{} Finished in {:.2}s — no issues found",
                    green("✓"),
                    elapsed.as_secs_f64()
                );
            } else if summary.total_errors > 0 {
                println!(
                    "{} Finished in {:.2}s — {} warning(s), {} error(s)",
                    red("✗"),
                    elapsed.as_secs_f64(),
                    summary.total_warnings,
                    summary.total_errors
                );
                std::process::exit(1);
            } else if strict {
                println!(
                    "{} Finished in {:.2}s — {} warning(s) (strict mode)",
                    yellow("⚠"),
                    elapsed.as_secs_f64(),
                    summary.total_warnings
                );
                std::process::exit(1);
            } else {
                println!(
                    "{} Finished in {:.2}s — {} warning(s)",
                    yellow("⚠"),
                    elapsed.as_secs_f64(),
                    summary.total_warnings
                );
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("✗ Error:"), e);
            std::process::exit(1);
        }
    }
}

fn cmd_doc(path: &Path, format: &str, output: Option<PathBuf>) {
    match doc::cmd_doc(path, format, output.as_deref()) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

fn cmd_test(path: Option<PathBuf>, filter: Option<String>, verbose: bool) {
    test_cmd::cmd_test(path, filter, verbose);
}

const GATE_EXIT_CHECK: u8 = 1;
const GATE_EXIT_LINT: u8 = 2;
const GATE_EXIT_TEST: u8 = 4;
const GATE_EXIT_DOC: u8 = 8;
const GATE_EXIT_INPUT: u8 = 16;

fn cmd_ci(path: PathBuf) {
    if !path.exists() {
        eprintln!("{} path does not exist: {}", red("error:"), path.display());
        std::process::exit(i32::from(GATE_EXIT_INPUT));
    }

    let mut source_files = Vec::new();
    if path.is_file() {
        if !is_lumen_source(&path) {
            eprintln!(
                "{} expected a .lm, .lumen, .lm.md, or .lumen.md file, got '{}'",
                red("error:"),
                path.display()
            );
            std::process::exit(i32::from(GATE_EXIT_INPUT));
        }
        source_files.push(path.clone());
    } else if let Err(e) = collect_lumen_sources(&path, &mut source_files) {
        eprintln!("{} {}", red("error:"), e);
        std::process::exit(i32::from(GATE_EXIT_INPUT));
    }

    source_files.sort();
    if source_files.is_empty() {
        eprintln!(
            "{} no .lm/.lumen/.lm.md/.lumen.md files found under {}",
            red("error:"),
            path.display()
        );
        std::process::exit(i32::from(GATE_EXIT_INPUT));
    }

    let markdown_files: Vec<PathBuf> = source_files
        .iter()
        .filter(|p| is_markdown_source(p))
        .cloned()
        .collect();

    println!(
        "{} quality gate for {}",
        status_label("Running"),
        bold(&path.display().to_string())
    );

    let mut exit_code = 0u8;

    println!(
        "{} {} source file(s)",
        status_label("Checking"),
        source_files.len()
    );
    if !gate_check_sources(&source_files) {
        exit_code |= GATE_EXIT_CHECK;
    }

    println!("{} strict mode", status_label("Linting"));
    if !gate_lint_sources(&source_files) {
        exit_code |= GATE_EXIT_LINT;
    }

    let should_run_markdown_stages = !path.is_file() || is_markdown_source(&path);

    if should_run_markdown_stages {
        println!("{} {}", status_label("Testing"), path.display());
        if !gate_run_tests(&path) {
            exit_code |= GATE_EXIT_TEST;
        }

        println!("{} {}", status_label("Doc"), path.display());
        if !gate_doc_sanity(&markdown_files) {
            exit_code |= GATE_EXIT_DOC;
        }
    } else {
        println!(
            "{} skipping test/doc for non-markdown file '{}'",
            gray("info:"),
            path.display()
        );
    }

    if exit_code == 0 {
        println!("{} quality gate passed", green("✓"));
        return;
    }

    eprintln!(
        "{} quality gate failed (exit code {})",
        red("error:"),
        exit_code
    );
    if exit_code & GATE_EXIT_CHECK != 0 {
        eprintln!("  check failed");
    }
    if exit_code & GATE_EXIT_LINT != 0 {
        eprintln!("  lint failed");
    }
    if exit_code & GATE_EXIT_TEST != 0 {
        eprintln!("  test failed");
    }
    if exit_code & GATE_EXIT_DOC != 0 {
        eprintln!("  doc sanity failed");
    }
    std::process::exit(i32::from(exit_code));
}

fn gate_check_sources(files: &[PathBuf]) -> bool {
    let mut failures = 0usize;

    for file in files {
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "{} cannot read file '{}': {}",
                    red("error:"),
                    file.display(),
                    e
                );
                failures += 1;
                continue;
            }
        };

        let filename = file.display().to_string();
        if let Err(e) = compile_source_file(file, &source) {
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            failures += 1;
        }
    }

    if failures == 0 {
        println!("{} check passed", green("✓"));
        true
    } else {
        eprintln!("{} check failed ({} file(s))", red("error:"), failures);
        false
    }
}

fn gate_lint_sources(files: &[PathBuf]) -> bool {
    match lint::cmd_lint(files, true) {
        Ok(_summary) => {
            println!("{} lint passed", green("✓"));
            true
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            false
        }
    }
}

fn gate_run_tests(path: &Path) -> bool {
    match test_cmd::run_tests(Some(path.to_path_buf()), None, false) {
        Ok(summary) => {
            if summary.is_success() {
                println!("{} test passed ({} total)", green("✓"), summary.total);
                true
            } else {
                eprintln!(
                    "{} test failed ({} passed, {} failed)",
                    red("error:"),
                    summary.passed,
                    summary.failed
                );
                false
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            false
        }
    }
}

fn gate_doc_sanity(markdown_files: &[PathBuf]) -> bool {
    if markdown_files.is_empty() {
        eprintln!(
            "{} no .lm.md/.lumen.md files found for doc sanity",
            red("error:")
        );
        return false;
    }

    let mut failures = 0usize;
    for (idx, file) in markdown_files.iter().enumerate() {
        let out_path = std::env::temp_dir().join(format!(
            "lumen-doc-sanity-{}-{}.json",
            std::process::id(),
            idx
        ));

        if let Err(e) = doc::cmd_doc(file, "json", Some(&out_path)) {
            eprintln!("{} {}: {}", red("error:"), file.display(), e);
            failures += 1;
        }

        let _ = std::fs::remove_file(&out_path);
    }

    if failures == 0 {
        println!(
            "{} doc sanity passed ({} file(s))",
            green("✓"),
            markdown_files.len()
        );
        true
    } else {
        eprintln!("{} doc sanity failed ({} file(s))", red("error:"), failures);
        false
    }
}

fn collect_lumen_sources(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_lumen_source(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("cannot read directory '{}': {}", path.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read directory entry: {}", e))?;
        collect_lumen_sources(&entry.path(), files)?;
    }

    Ok(())
}

fn is_lumen_source(path: &Path) -> bool {
    if is_markdown_source(path) {
        return true;
    }
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("lm") | Some("lumen")
    )
}

fn read_source(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!(
            "{} cannot read file '{}': {}",
            red("error:"),
            bold(&path.display().to_string()),
            e
        );
        std::process::exit(1);
    })
}

fn is_markdown_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".lm.md") || n.ends_with(".lumen.md"))
        .unwrap_or(false)
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

fn cmd_check(file: &PathBuf) {
    let source = read_source(file);
    let filename = file.display().to_string();

    println!("{} {}", status_label("Checking"), bold(&filename));
    let start = std::time::Instant::now();

    match compile_source_file(file, &source) {
        Ok(_module) => {
            let elapsed = start.elapsed();
            println!(
                "{} Finished in {:.2}s — no errors",
                green("✓"),
                elapsed.as_secs_f64()
            );
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

    println!("{} {}", status_label("Compiling"), bold(&filename));
    let start = std::time::Instant::now();
    let module = match compile_source_file(file, &source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} compilation failed", red("✗ Error:"));
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    };

    // Load project config and build provider registry
    let config = config::LumenConfig::load();
    let mut registry = lumen_runtime::tools::ProviderRegistry::new();
    register_providers(&mut registry, &config);

    // Optionally set up tracing
    let trace_store = trace_dir.map(|dir| {
        Arc::new(Mutex::new(lumen_runtime::trace::store::TraceStore::new(
            &dir,
        )))
    });
    let mut trace_run_id: Option<String> = None;

    if let Some(trace_store) = trace_store.as_ref() {
        if let Ok(mut ts) = trace_store.lock() {
            trace_run_id = Some(ts.start_run(&module.doc_hash));
            ts.cell_start(cell);
        }
    }

    println!("{} {}", status_label("Running"), cyan(cell));
    let mut vm = lumen_vm::vm::VM::new();
    if let Some(run_id) = trace_run_id.as_ref() {
        vm.set_trace_id(run_id.clone());
    }
    vm.set_provider_registry(registry);
    if let Some(trace_store) = trace_store.as_ref() {
        let trace_store = Arc::clone(trace_store);
        vm.debug_callback = Some(Box::new(move |event| {
            let Ok(mut ts) = trace_store.lock() else {
                return;
            };
            match event {
                lumen_vm::vm::DebugEvent::Step {
                    cell_name,
                    ip,
                    opcode,
                } => ts.vm_step(cell_name, *ip, opcode),
                lumen_vm::vm::DebugEvent::CallEnter { cell_name } => ts.call_enter(cell_name),
                lumen_vm::vm::DebugEvent::CallExit { cell_name, result } => {
                    ts.call_exit(cell_name, result.type_name())
                }
                lumen_vm::vm::DebugEvent::ToolCall {
                    cell_name,
                    tool_id,
                    tool_version,
                    latency_ms,
                    success,
                    message,
                } => ts.tool_call(
                    cell_name,
                    tool_id,
                    tool_version,
                    *latency_ms,
                    false,
                    *success,
                    message.as_deref(),
                ),
                lumen_vm::vm::DebugEvent::SchemaValidate {
                    cell_name,
                    schema,
                    valid,
                } => ts.schema_validate(cell_name, schema, *valid),
            }
        }));
    }
    vm.load(module);
    match vm.execute(cell, vec![]) {
        Ok(result) => {
            let elapsed = start.elapsed();
            if let Some(trace_store) = trace_store.as_ref() {
                if let Ok(mut ts) = trace_store.lock() {
                    ts.cell_end(cell);
                    ts.end_run();
                    let run_id = ts.run_id().to_string();
                    println!("{} {}", gray("trace:"), run_id);
                }
            }
            println!("\n{}", result);
            println!("{} Finished in {:.2}s", green("✓"), elapsed.as_secs_f64());
        }
        Err(e) => {
            if let Some(trace_store) = trace_store.as_ref() {
                if let Ok(mut ts) = trace_store.lock() {
                    ts.error(Some(cell), &format!("{}", e));
                    ts.end_run();
                }
            }
            eprintln!("{} {}", red("✗ Error:"), e);
            std::process::exit(1);
        }
    }
}

fn cmd_emit(file: &PathBuf, output: Option<PathBuf>) {
    let source = read_source(file);
    let filename = file.display().to_string();

    println!("{} {}", status_label("Compiling"), filename);
    let module = match compile_source_file(file, &source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} compilation failed", red("error:"));
            let formatted = lumen_compiler::format_error(&e, &source, &filename);
            eprint!("{}", formatted);
            std::process::exit(1);
        }
    };

    let json = lumen_compiler::compiler::emit::emit_json(&module).unwrap_or_else(|e| {
        eprintln!("{} emit failed: {}", red("error:"), e);
        std::process::exit(1);
    });

    if let Some(ref out_path) = output {
        println!("{} LIR to {}", status_label("Emitting"), out_path.display());
        std::fs::write(out_path, &json).unwrap_or_else(|e| {
            eprintln!(
                "{} writing to '{}': {}",
                red("error:"),
                out_path.display(),
                e
            );
            std::process::exit(1);
        });
    } else {
        println!("{} LIR to stdout", status_label("Emitting"));
        println!("{}", json);
    }
}

fn cmd_trace_show(run_id: &str, trace_dir: &Path, format: TraceShowFormat, verify_chain: bool) {
    let path = trace_dir.join(format!("{}.jsonl", run_id));
    match read_trace_events(&path) {
        Ok(events) => {
            println!("{} trace for run {}", status_label("Showing"), cyan(run_id));

            if verify_chain {
                match verify_trace_chain(&events) {
                    Ok(()) => println!("{} trace chain verified", green("✓")),
                    Err(msg) => {
                        eprintln!("{} {}", red("error:"), msg);
                        std::process::exit(1);
                    }
                }
            }

            match format {
                TraceShowFormat::Pretty => {
                    for event in &events {
                        if let Ok(pretty) = serde_json::to_string_pretty(event) {
                            println!("{}", pretty);
                        }
                    }
                }
                TraceShowFormat::Replay => {
                    for event in &events {
                        println!("{}", replay_line(event));
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "{} cannot read trace '{}': {}",
                red("error:"),
                path.display(),
                e
            );
            std::process::exit(1);
        }
    }
}

fn read_trace_events(path: &Path) -> Result<Vec<lumen_runtime::trace::events::TraceEvent>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read trace '{}': {}", path.display(), e))?;

    content
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            serde_json::from_str::<lumen_runtime::trace::events::TraceEvent>(line)
                .map_err(|e| format!("invalid JSON in trace at line {}: {}", idx + 1, e))
        })
        .collect()
}

fn verify_trace_chain(events: &[lumen_runtime::trace::events::TraceEvent]) -> Result<(), String> {
    lumen_runtime::trace::store::verify_event_chain(events)
}

fn replay_line(event: &lumen_runtime::trace::events::TraceEvent) -> String {
    let kind = match event.kind {
        lumen_runtime::trace::events::TraceEventKind::RunStart => "run_start",
        lumen_runtime::trace::events::TraceEventKind::CellStart => "cell_start",
        lumen_runtime::trace::events::TraceEventKind::CellEnd => "cell_end",
        lumen_runtime::trace::events::TraceEventKind::CallEnter => "call_enter",
        lumen_runtime::trace::events::TraceEventKind::CallExit => "call_exit",
        lumen_runtime::trace::events::TraceEventKind::VmStep => "vm_step",
        lumen_runtime::trace::events::TraceEventKind::ToolCall => "tool_call",
        lumen_runtime::trace::events::TraceEventKind::SchemaValidate => "schema_validate",
        lumen_runtime::trace::events::TraceEventKind::Error => "error",
        lumen_runtime::trace::events::TraceEventKind::RunEnd => "run_end",
    };

    let mut parts = vec![format!("{:06}", event.seq), kind.to_string()];

    if let Some(cell) = event.cell.as_deref() {
        parts.push(format!("cell={}", cell));
    }
    if let Some(tool_id) = event.tool_id.as_deref() {
        parts.push(format!("tool={}", tool_id));
    }
    if let Some(tool_version) = event.tool_version.as_deref() {
        parts.push(format!("version={}", tool_version));
    }
    if let Some(latency_ms) = event.latency_ms {
        parts.push(format!("latency_ms={}", latency_ms));
    }
    if let Some(cached) = event.cached {
        parts.push(format!("cached={}", cached));
    }
    if let Some(message) = event.message.as_ref() {
        parts.push(format!(
            "message={}",
            serde_json::to_string(message).unwrap_or_else(|_| "\"<invalid>\"".to_string())
        ));
    }

    if let Some(details) = event.details.as_ref() {
        match event.kind {
            lumen_runtime::trace::events::TraceEventKind::VmStep => {
                if let Some(ip) = details.get("ip").and_then(|value| value.as_u64()) {
                    parts.push(format!("ip={}", ip));
                }
                if let Some(opcode) = details.get("opcode").and_then(|value| value.as_str()) {
                    parts.push(format!("opcode={}", opcode));
                }
            }
            lumen_runtime::trace::events::TraceEventKind::SchemaValidate => {
                if let Some(schema) = details.get("schema").and_then(|value| value.as_str()) {
                    parts.push(format!("schema={}", schema));
                }
                if let Some(valid) = details.get("valid").and_then(|value| value.as_bool()) {
                    parts.push(format!("valid={}", valid));
                }
            }
            lumen_runtime::trace::events::TraceEventKind::CallExit => {
                if let Some(result_type) =
                    details.get("result_type").and_then(|value| value.as_str())
                {
                    parts.push(format!("result_type={}", result_type));
                }
            }
            lumen_runtime::trace::events::TraceEventKind::ToolCall => {
                if let Some(success) = details.get("success").and_then(|value| value.as_bool()) {
                    parts.push(format!("success={}", success));
                }
            }
            _ => {}
        }
    }

    parts.push(format!("hash={}", event.hash));
    parts.join(" ")
}

fn cmd_cache_clear(cache_dir: &PathBuf) {
    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).unwrap_or_else(|e| {
            eprintln!("{} clearing cache: {}", red("error:"), e);
            std::process::exit(1);
        });
        println!("{} cache cleared", green("✓"));
    } else {
        println!(
            "{} cache directory does not exist: {}",
            yellow("warning:"),
            cache_dir.display()
        );
    }
}

fn cmd_init() {
    let path = PathBuf::from("lumen.toml");
    if path.exists() {
        eprintln!(
            "{} lumen.toml already exists — not overwriting",
            red("error:")
        );
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
        eprintln!("{} no files specified", red("✗ Error:"));
        std::process::exit(1);
    }

    let action = if check { "Checking" } else { "Formatting" };
    println!(
        "{} {} {}",
        status_label(action),
        files.len(),
        if files.len() == 1 { "file" } else { "files" }
    );

    let start = std::time::Instant::now();
    match fmt::format_files(&files, check) {
        Ok((needs_formatting, reformatted_count)) => {
            let elapsed = start.elapsed();
            if check {
                if needs_formatting {
                    println!(
                        "{} {} file(s) need formatting",
                        yellow("⚠"),
                        reformatted_count
                    );
                    std::process::exit(1);
                } else {
                    println!(
                        "{} Finished in {:.2}s — all files formatted",
                        green("✓"),
                        elapsed.as_secs_f64()
                    );
                }
            } else {
                println!(
                    "{} Finished in {:.2}s — {} file(s) reformatted",
                    green("✓"),
                    elapsed.as_secs_f64(),
                    reformatted_count
                );
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("✗ Error:"), e);
            std::process::exit(1);
        }
    }
}

fn cmd_build_wasm(target: &str, release: bool) {
    // Check if wasm-pack is installed
    let wasm_pack_check = std::process::Command::new("wasm-pack")
        .arg("--version")
        .output();

    match wasm_pack_check {
        Ok(output) if output.status.success() => {
            // wasm-pack is installed
            let version = String::from_utf8_lossy(&output.stdout);
            println!("{} wasm-pack {}", status_label("Found"), version.trim());
        }
        _ => {
            eprintln!("{} wasm-pack not found", yellow("warning:"));
            eprintln!("\nInstall wasm-pack to build WASM targets:");
            eprintln!("  {}", cyan("cargo install wasm-pack"));
            eprintln!("\nAlternatively, build manually:");
            match target {
                "web" | "nodejs" => {
                    eprintln!(
                        "  {}",
                        cyan("cd rust/lumen-wasm && wasm-pack build --target web")
                    );
                }
                "wasi" => {
                    eprintln!(
                        "  {}",
                        cyan("cd rust/lumen-wasm && cargo build --target wasm32-wasi")
                    );
                }
                _ => {}
            }
            std::process::exit(1);
        }
    }

    let wasm_crate_dir = PathBuf::from("rust/lumen-wasm");
    if !wasm_crate_dir.exists() {
        eprintln!(
            "{} lumen-wasm crate not found at rust/lumen-wasm",
            red("error:")
        );
        eprintln!("\nThe WASM compilation target is still in development.");
        eprintln!("See docs/WASM_STRATEGY.md for more information.");
        std::process::exit(1);
    }

    println!("{} WASM target: {}", status_label("Building"), cyan(target));

    let mut cmd = std::process::Command::new("wasm-pack");
    cmd.arg("build");
    cmd.arg("--target").arg(target);

    if release {
        cmd.arg("--release");
    }

    cmd.current_dir(&wasm_crate_dir);

    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("{} executing wasm-pack: {}", red("error:"), e);
        std::process::exit(1);
    });

    if !status.success() {
        eprintln!("{} wasm-pack build failed", red("error:"));
        std::process::exit(1);
    }

    println!("{} WASM build complete", green("✓"));
    println!("\nOutput in: {}", bold("rust/lumen-wasm/pkg/"));

    match target {
        "web" => {
            println!("\nUsage in browser:");
            println!(
                "  {}",
                cyan("import init, {{ run, compile, check }} from './pkg/lumen_wasm.js';")
            );
            println!("  {}", cyan("await init();"));
            println!("  {}", cyan("const result = run(sourceCode, 'main');"));
        }
        "nodejs" => {
            println!("\nUsage in Node.js:");
            println!(
                "  {}",
                cyan("const {{ run, compile, check }} = require('./pkg/lumen_wasm.js');")
            );
            println!("  {}", cyan("const result = run(sourceCode, 'main');"));
        }
        "wasi" => {
            println!("\nRun with Wasmtime:");
            println!(
                "  {}",
                cyan("wasmtime rust/lumen-wasm/target/wasm32-wasi/release/lumen_wasm.wasm")
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_runtime::trace::events::TraceEventKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_lumen_file(contents: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        path.push(format!(
            "lumen-recovery-{}-{}.lm",
            std::process::id(),
            timestamp
        ));
        std::fs::write(&path, contents).expect("failed to write temp lumen file");
        path
    }

    fn write_temp_trace_events() -> Vec<lumen_runtime::trace::events::TraceEvent> {
        let mut base = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        base.push(format!(
            "lumen-trace-test-{}-{}",
            std::process::id(),
            timestamp
        ));
        let trace_dir = base.join("trace");
        std::fs::create_dir_all(&trace_dir).expect("trace dir should be created");

        let mut store = lumen_runtime::trace::store::TraceStore::new(&base);
        let run_id = store.start_run("doc-123");
        store.cell_start("main");
        store.end_run();

        let path = trace_dir.join(format!("{}.jsonl", run_id));
        let events = read_trace_events(&path).expect("trace events should be readable");
        let _ = std::fs::remove_dir_all(&base);
        events
    }

    #[test]
    fn parses_test_command_with_defaults() {
        let cli = Cli::try_parse_from(["lumen", "test"]).expect("test command should parse");
        match cli.command {
            Commands::Test {
                path,
                filter,
                verbose,
            } => {
                assert!(path.is_none());
                assert!(filter.is_none());
                assert!(!verbose);
            }
            _ => panic!("expected test command"),
        }
    }

    #[test]
    fn parses_test_command_with_flags() {
        let cli = Cli::try_parse_from([
            "lumen",
            "test",
            "examples/demo.lm.md",
            "--filter",
            "auth",
            "--verbose",
        ])
        .expect("test command with flags should parse");

        match cli.command {
            Commands::Test {
                path,
                filter,
                verbose,
            } => {
                assert_eq!(path, Some(PathBuf::from("examples/demo.lm.md")));
                assert_eq!(filter, Some("auth".to_string()));
                assert!(verbose);
            }
            _ => panic!("expected test command"),
        }
    }

    #[test]
    fn parses_ci_command_default_path() {
        let cli = Cli::try_parse_from(["lumen", "ci"]).expect("ci command should parse");
        match cli.command {
            Commands::Ci { path } => {
                assert_eq!(path, PathBuf::from("."));
            }
            _ => panic!("expected ci command"),
        }
    }

    #[test]
    fn check_path_collects_multiple_parse_diagnostics_for_one_file() {
        let source = include_str!("../tests/fixtures/recovery_multi_diag.lm");
        let temp_file = write_temp_lumen_file(source);
        let filename = temp_file.display().to_string();

        let result = compile_source_file(&temp_file, source);
        let _ = std::fs::remove_file(&temp_file);

        let err = result.expect_err("expected malformed file to fail compilation");
        let parse_count = match &err {
            lumen_compiler::CompileError::Parse(errors) => errors.len(),
            other => panic!("expected parse errors, got {:?}", other),
        };
        assert!(
            parse_count >= 3,
            "expected at least 3 parse errors, got {}",
            parse_count
        );

        let rendered = lumen_compiler::format_error(&err, source, &filename);
        let rendered_count = rendered.matches("PARSE ERROR").count();
        assert!(
            rendered_count >= 3,
            "expected at least 3 rendered parse diagnostics, got {}",
            rendered_count
        );
    }

    #[test]
    fn verify_trace_chain_accepts_valid_hash_chain() {
        let events = write_temp_trace_events();
        verify_trace_chain(&events).expect("valid chain should pass");
    }

    #[test]
    fn verify_trace_chain_rejects_tampered_payload() {
        let mut events = write_temp_trace_events();
        let target = events
            .iter_mut()
            .find(|event| event.kind == TraceEventKind::CellStart)
            .expect("cell start should exist");
        target.message = Some("tampered".to_string());
        let err = verify_trace_chain(&events).expect_err("tampered event should fail");
        assert!(
            err.contains("trace event hash mismatch"),
            "unexpected error: {}",
            err
        );
    }
}
