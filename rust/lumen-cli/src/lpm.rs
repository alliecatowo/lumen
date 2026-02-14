//! lpm — Lumen Package Manager
//! Alias for `lumen pkg` subcommands.

mod config;
mod lockfile;
mod pkg;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lpm", about = "Lumen Package Manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new package
    Init {
        /// Package name (creates a subdirectory; omit to init in current dir)
        name: Option<String>,
    },
    /// Build package and dependencies
    Build,
    /// Type-check package
    Check,
    /// Add a dependency
    Add {
        /// Package name
        package: String,
        /// Path to the dependency
        #[arg(long)]
        path: Option<String>,
    },
    /// Remove a dependency
    Remove {
        /// Package name
        package: String,
    },
    /// List dependencies
    List,
    /// Install dependencies from lumen.toml
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
    /// Search for packages in the registry
    Search {
        /// Search query
        query: String,
    },
    /// Inspect package metadata for a local package directory or .tar archive
    Info {
        /// Optional path to package directory or archive (.tar)
        target: Option<String>,
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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => pkg::cmd_pkg_init(name),
        Commands::Build => pkg::cmd_pkg_build(),
        Commands::Check => pkg::cmd_pkg_check(),
        Commands::Add { package, path } => pkg::cmd_pkg_add(&package, path.as_deref()),
        Commands::Remove { package } => pkg::cmd_pkg_remove(&package),
        Commands::List => pkg::cmd_pkg_list(),
        Commands::Install { frozen } => pkg::cmd_pkg_install_with_lock(frozen),
        Commands::Update { frozen } => pkg::cmd_pkg_update_with_lock(frozen),
        Commands::Search { query } => pkg::cmd_pkg_search(&query),
        Commands::Info { target } => pkg::cmd_pkg_info(target.as_deref()),
        Commands::Pack => pkg::cmd_pkg_pack(),
        Commands::Publish { dry_run } => pkg::cmd_pkg_publish(dry_run),
    }
}
