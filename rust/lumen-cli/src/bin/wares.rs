//! Wares CLI — package manager for the Lumen language.

use lumen_cli::{registry_cmd, wares};

use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(
    name = "wares",
    version,
    about = "The package manager for the Lumen programming language",
    long_about = "Wares is the official package manager for Lumen.\n\n\
                  Learn more at: https://github.com/alliecatowo/lumen",
    help_template = "\
{before-help}{name} {version}
{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}

Examples:
  wares init                   Initialize a new package
  wares install                Install dependencies
  wares add package-name       Add a dependency
  wares publish                Publish to the registry
  wares login                  Authenticate with the registry
"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Lumen package
    Init {
        /// Package name
        name: Option<String>,
    },
    /// Compile the package and all dependencies
    Build,
    /// Type-check the package and all dependencies
    Check,
    /// Add a dependency
    Add {
        /// Package name
        package: String,
        /// Path to the dependency
        #[arg(long)]
        path: Option<String>,
        /// Add as a dev dependency
        #[arg(long)]
        dev: bool,
        /// Add as a build dependency
        #[arg(long)]
        build: bool,
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
        /// Install dev dependencies only
        #[arg(long)]
        dev: bool,
        /// Install build dependencies only
        #[arg(long)]
        build: bool,
    },
    /// Update dependencies
    Update {
        /// Fail if lumen.lock would be changed
        #[arg(long, alias = "locked")]
        frozen: bool,
    },
    /// Search for a package
    Search {
        /// Search query
        query: String,
    },
    /// Create a package archive
    Pack,
    /// Publish to the registry
    Publish {
        /// Validate only
        #[arg(long)]
        dry_run: bool,
    },
    /// Login to a registry
    Login {
        #[arg(long)]
        registry: Option<String>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// Provider (github, etc)
        #[arg(long, default_value = "github")]
        provider: String,
    },
    /// Logout from a registry
    Logout {
        #[arg(long)]
        registry: Option<String>,
    },
    /// Show current authenticated user
    Whoami {
        #[arg(long)]
        registry: Option<String>,
    },
    /// Inspect ware metadata
    Info {
        /// Ware name or path
        target: String,
        #[arg(long)]
        registry: Option<String>,
    },
    /// Verify package trust
    TrustCheck {
        /// Ware name or path
        target: String,
        #[arg(long)]
        registry: Option<String>,
    },
    /// Manage trust policies
    Policy {
        #[command(subcommand)]
        sub: PolicyCommands,
    },
    /// Manage tokens
    Token {
        #[command(subcommand)]
        sub: TokenCommands,
    },
    /// Manage owners
    Owner {
        #[command(subcommand)]
        sub: OwnerCommands,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    Set { scope: String, level: String },
    Get { scope: String },
    List,
}

#[derive(Subcommand)]
enum TokenCommands {
    List,
    Add {
        registry: String,
        token: String,
        name: Option<String>,
    },
    Remove {
        registry: String,
    },
}

#[derive(Subcommand)]
enum OwnerCommands {
    Add {
        package: String,
        email: String,
        role: String,
    },
    Remove {
        package: String,
        email: String,
    },
    List {
        package: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => wares::ops::init(name),
        Commands::Build => wares::ops::build(),
        Commands::Check => wares::ops::check(),
        Commands::Add {
            package,
            path,
            dev,
            build,
        } => {
            let kind = if build {
                wares::ops::DependencyKind::Build
            } else if dev {
                wares::ops::DependencyKind::Dev
            } else {
                wares::ops::DependencyKind::Normal
            };
            wares::ops::add_with_kind(&package, path.as_deref(), kind)
        }
        Commands::Remove { package } => wares::ops::remove(&package),
        Commands::List => wares::ops::list(),
        Commands::Install { frozen, dev, build } => {
            let kind = if build {
                wares::ops::DependencyKind::Build
            } else if dev {
                wares::ops::DependencyKind::Dev
            } else {
                wares::ops::DependencyKind::Normal
            };
            if kind != wares::ops::DependencyKind::Normal {
                wares::ops::install_with_kind(kind, frozen)
            } else {
                wares::ops::install_with_lock(frozen)
            }
        }
        Commands::Update { frozen } => wares::ops::update_with_lock(frozen),
        Commands::Search { query } => wares::ops::search(&query),
        Commands::Pack => wares::ops::pack(),
        Commands::Publish { dry_run } => wares::ops::publish(dry_run),
        Commands::Login {
            registry,
            token,
            name,
            provider: _,
        } => {
            // For now mapping to registry_cmd logic
            registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Login {
                registry,
                token,
                name,
            })
        }
        Commands::Logout { registry } => {
            registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Logout { registry })
        }
        Commands::Whoami { registry } => {
            registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Whoami { registry })
        }
        Commands::Info {
            target,
            registry: _,
        } => {
            // TODO: expose a cleaner API for info or support registry arg
            wares::ops::info(&target, None);
        }
        Commands::TrustCheck {
            target,
            registry: _,
        } => {
            // Placeholder for trust check
            println!("Trust check for {}", target);
        }
        Commands::Policy { sub: _ } => {
            // Placeholder
            println!("Policy command");
        }
        Commands::Token { sub } => match sub {
            TokenCommands::List => {
                registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Token {
                    sub: registry_cmd::TokenCommands::List,
                })
            }
            TokenCommands::Add {
                registry,
                token,
                name,
            } => registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Token {
                sub: registry_cmd::TokenCommands::Add {
                    registry,
                    token,
                    name,
                },
            }),
            TokenCommands::Remove { registry } => {
                registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Token {
                    sub: registry_cmd::TokenCommands::Remove { registry },
                })
            }
        },
        Commands::Owner { sub } => match sub {
            OwnerCommands::Add {
                package,
                email,
                role,
            } => registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Owner {
                sub: registry_cmd::OwnerCommands::Add {
                    package,
                    email,
                    role: Some(role),
                },
            }),
            OwnerCommands::Remove { package, email } => {
                registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Owner {
                    sub: registry_cmd::OwnerCommands::Remove { package, email },
                })
            }
            OwnerCommands::List { package } => {
                registry_cmd::cmd_registry(registry_cmd::RegistryCommands::Owner {
                    sub: registry_cmd::OwnerCommands::List { package },
                })
            }
        },
    }
}
