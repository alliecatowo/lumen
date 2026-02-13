//! lpm — Lumen Package Manager
//! Alias for `lumen pkg` subcommands.

mod config;
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
    }
}
