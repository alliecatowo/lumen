//! wares — Lumen Package Manager
//! CLI for publishing and managing Lumen wares.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wares", about = "Lumen Package Manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new ware
    Init {
        /// Ware name (creates a subdirectory; omit to init in current dir)
        name: Option<String>,
    },
    /// Build ware and dependencies
    Build,
    /// Type-check ware
    Check,
    /// Add a dependency (installs wares)
    Add {
        /// Ware name
        package: String,
        /// Path to the dependency
        #[arg(long)]
        path: Option<String>,
    },
    /// Remove a dependency (removes wares)
    Remove {
        /// Ware name
        package: String,
    },
    /// List dependencies (list installed wares)
    List,
    /// Install dependencies from lumen.toml (installs all wares)
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
    /// Search for ware in the registry
    Search {
        /// Search query
        query: String,
    },
    /// Inspect ware metadata
    Info {
        /// Optional path to ware directory or archive (.tar)
        target: Option<String>,
    },
    /// Create a deterministic package archive in dist/
    Pack,
    /// Publish ware to the registry
    Publish {
        /// Validate/package locally without uploading
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => lumen_cli::pkg::cmd_pkg_init(name),
        Commands::Build => lumen_cli::pkg::cmd_pkg_build(),
        Commands::Check => lumen_cli::pkg::cmd_pkg_check(),
        Commands::Add { package, path } => lumen_cli::pkg::cmd_pkg_add(&package, path.as_deref()),
        Commands::Remove { package } => lumen_cli::pkg::cmd_pkg_remove(&package),
        Commands::List => lumen_cli::pkg::cmd_pkg_list(),
        Commands::Install { frozen } => lumen_cli::pkg::cmd_pkg_install_with_lock(frozen),
        Commands::Update { frozen } => lumen_cli::pkg::cmd_pkg_update_with_lock(frozen),
        Commands::Search { query } => lumen_cli::pkg::cmd_pkg_search(&query),
        Commands::Info { target } => {
            lumen_cli::pkg::cmd_pkg_info(target.as_deref().unwrap_or(""), None)
        }
        Commands::Pack => lumen_cli::pkg::cmd_pkg_pack(),
        Commands::Publish { dry_run } => lumen_cli::pkg::cmd_pkg_publish(dry_run),
    }
}
