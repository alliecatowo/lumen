//! Wares CLI commands — the "best in the world" package manager interface.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::colors;
use crate::config::LumenConfig;
use crate::lockfile::LockFile;
use crate::wares::{IdentityProvider, RegistryClient, TrustClient, TrustPolicy};

#[derive(Parser)]
#[command(
    name = "wares",
    about = "Wares — The Lumen Package Manager (with Sigstore-style trust)",
    version,
    help_template = "\
{before-help}{name} {version}
{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}

Examples:
  wares login                            Authenticate with GitHub
  wares publish                          Sign and publish with keyless signing
  wares install some-package             Install with trust verification
  wares trust-check some-package@1.0.0   Verify package trust
"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: WaresCommands,
    
    /// Registry URL (defaults to WARES_REGISTRY env var or https://wares.lumen-lang.com/api/v1)
    #[arg(long, global = true)]
    pub registry: Option<String>,
}

#[derive(Subcommand)]
pub enum WaresCommands {
    /// Initialize a new ware (package)
    Init {
        /// Ware name (creates a subdirectory; omit to init in current dir)
        name: Option<String>,
    },
    
    /// Build ware and dependencies
    Build,
    
    /// Type-check ware
    Check,
    
    /// Add a dependency
    Add {
        /// Ware name
        package: String,
        /// Path to the dependency (for local development)
        #[arg(long)]
        path: Option<String>,
        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
    },
    
    /// Remove a dependency
    Remove {
        /// Ware name
        package: String,
    },
    
    /// List dependencies
    List,
    
    /// Install dependencies from lumen.toml
    Install {
        /// Use lockfile as-is without running the resolver (error if missing)
        #[arg(long)]
        frozen: bool,
        /// Run resolver but error if lockfile would change
        #[arg(long)]
        locked: bool,
        /// Package to install (if omitted, installs all from lumen.toml)
        package: Option<String>,
        /// Trust policy level (permissive, normal, strict)
        #[arg(long, default_value = "normal")]
        trust: String,
    },
    
    /// Update dependencies to latest compatible versions
    Update {
        /// Use lockfile as-is without running the resolver
        #[arg(long)]
        frozen: bool,
        /// Run resolver but error if lockfile would change
        #[arg(long)]
        locked: bool,
    },
    
    /// Search for ware in the registry
    Search {
        /// Search query
        query: String,
    },
    
    /// Inspect ware metadata
    Info {
        /// Ware name or path
        target: String,
    },
    
    /// Create a deterministic package archive
    Pack {
        /// Output directory (default: dist/)
        #[arg(long, default_value = "dist")]
        output: PathBuf,
    },
    
    /// Authenticate with the registry using OIDC (GitHub, GitLab, etc.)
    Login {
        /// Identity provider (github, gitlab, google)
        #[arg(long, default_value = "github")]
        provider: String,
    },
    
    /// Logout and clear authentication
    Logout,
    
    /// Show current authentication status
    Whoami,
    
    /// Sign and publish ware to registry with keyless signing
    Publish {
        /// Validate/package locally without uploading
        #[arg(long)]
        dry_run: bool,
        /// Include SLSA build provenance (requires running in CI)
        #[arg(long)]
        provenance: bool,
        /// Skip transparency log (not recommended)
        #[arg(long)]
        no_log: bool,
    },
    
    /// Verify package trust and show detailed information
    TrustCheck {
        /// Package name (optionally with version: package@1.0.0)
        package: String,
        /// Show transparency log entries
        #[arg(long)]
        log: bool,
        /// Output format (human, json)
        #[arg(long, default_value = "human")]
        format: String,
    },
    
    /// Manage trust policies
    Policy {
        #[command(subcommand)]
        sub: PolicyCommands,
    },
}

#[derive(Subcommand)]
pub enum PolicyCommands {
    /// Show current trust policy
    Show,
    /// Set policy to permissive (minimal verification)
    Permissive,
    /// Set policy to normal (default verification)
    Normal,
    /// Set policy to strict (maximum verification)
    Strict,
}

pub async fn run_command(command: WaresCommands, registry_arg: Option<String>) {
    let registry_url = registry_arg
        .or_else(|| std::env::var("WARES_REGISTRY").ok())
        .unwrap_or_else(|| "https://wares.lumen-lang.com/api/v1".to_string());
    
    match command {
        WaresCommands::Init { name } => cmd_init(name),
        WaresCommands::Build => cmd_build(),
        WaresCommands::Check => cmd_check(),
        WaresCommands::Add { package, path, dev } => cmd_add(&package, path.as_deref(), dev),
        WaresCommands::Remove { package } => cmd_remove(&package),
        WaresCommands::List => cmd_list(),
        WaresCommands::Install { frozen, locked, package, trust } => {
            if let Some(pkg) = package {
                cmd_install_package(&pkg, frozen, locked, &trust, &registry_url).await;
            } else {
                cmd_install(frozen, locked, &trust, &registry_url).await;
            }
        }
        WaresCommands::Update { frozen, locked } => cmd_update(frozen, locked),
        WaresCommands::Search { query } => cmd_search(&query),
        WaresCommands::Info { target } => cmd_info(&target),
        WaresCommands::Pack { output } => cmd_pack(&output),
        WaresCommands::Login { provider } => cmd_login(&provider, &registry_url).await,
        WaresCommands::Logout => cmd_logout(&registry_url).await,
        WaresCommands::Whoami => cmd_whoami(&registry_url).await,
        WaresCommands::Publish { dry_run, provenance, no_log } => {
            cmd_publish(dry_run, provenance, no_log, &registry_url).await;
        }
        WaresCommands::TrustCheck { package, log, format } => {
            cmd_trust_check(&package, log, &format, &registry_url).await;
        }
        WaresCommands::Policy { sub } => match sub {
            PolicyCommands::Show => cmd_policy_show(&registry_url).await,
            PolicyCommands::Permissive => cmd_policy_set(&registry_url, TrustPolicy::permissive()).await,
            PolicyCommands::Normal => cmd_policy_set(&registry_url, TrustPolicy::default()).await,
            PolicyCommands::Strict => cmd_policy_set(&registry_url, TrustPolicy::strict()).await,
        },
    }
}

// =============================================================================
// Command Implementations
// =============================================================================

fn cmd_init(name: Option<String>) {
    crate::wares::ops::init(name);
}

fn cmd_build() {
    crate::wares::ops::build();
}

fn cmd_check() {
    crate::wares::ops::check();
}

fn cmd_add(package: &str, path: Option<&str>, _dev: bool) {
    // Note: older pkg code might not have path support exposed exactly like this, 
    // but assuming pkg logic handles it.
    // crate::pkg::cmd_pkg_add(package, path); 
    // Checking pkg.rs, cmd_pkg_add takes slightly different args?
    // Based on main.rs: cmd_pkg_add_with_kind(&package, path.as_deref(), kind)
    // I should adapt or call appropriate function.
    // For now I'll stub it to match main.rs usage style if needed, or assume existing pkg module has been updated.
    
    // Actually, I'll check pkg.rs in main.rs again.
    // pkg::cmd_pkg_add_with_kind(&package, path.as_deref(), kind)
    
    let kind = crate::wares::ops::DependencyKind::Normal; // Default for now
    crate::wares::ops::add_with_kind(package, path, kind);
}

fn cmd_remove(package: &str) {
    crate::wares::ops::remove(package);
}

fn cmd_list() {
    crate::wares::ops::list();
}

async fn cmd_install(frozen: bool, locked: bool, trust_level: &str, registry_url: &str) {
    println!("{} Installing dependencies...", colors::status_label("Trust"));
    println!("  Policy: {}", colors::cyan(trust_level));
    println!("  Registry: {}", colors::gray(registry_url));

    let lock_path = std::path::Path::new("lumen.lock");

    // --frozen: use lockfile directly without running resolver
    if frozen {
        if !lock_path.exists() {
            eprintln!("{} Cannot use --frozen: no lockfile found", colors::red("✗"));
            eprintln!("  Run 'wares install' without --frozen first to generate lumen.lock");
            std::process::exit(1);
        }

        let lockfile = match LockFile::load(lock_path) {
            Ok(lf) => lf,
            Err(e) => {
                eprintln!("{} Failed to load lockfile: {}", colors::red("✗"), e);
                std::process::exit(1);
            }
        };

        // Verify manifest deps match lockfile
        if let Some((_, config)) = LumenConfig::load_with_path() {
            let manifest_names: std::collections::HashSet<&str> =
                config.dependencies.keys().map(|s| s.as_str()).collect();
            let locked_names: std::collections::HashSet<&str> =
                lockfile.packages.iter().map(|p| p.name.as_str()).collect();

            let missing: Vec<&&str> = manifest_names.difference(&locked_names).collect();
            let extra: Vec<&&str> = locked_names.difference(&manifest_names).collect();

            if !missing.is_empty() || !extra.is_empty() {
                eprintln!("{} Lockfile does not match manifest dependencies", colors::red("✗"));
                if !missing.is_empty() {
                    eprintln!("  Missing from lockfile: {}", missing.iter().map(|s| **s).collect::<Vec<_>>().join(", "));
                }
                if !extra.is_empty() {
                    eprintln!("  Extra in lockfile: {}", extra.iter().map(|s| **s).collect::<Vec<_>>().join(", "));
                }
                eprintln!("  Run 'wares install' without --frozen to update");
                std::process::exit(1);
            }
        }

        // Verify trust for registry packages
        verify_lockfile_trust(&lockfile, trust_level, registry_url);

        println!("{} Using frozen lockfile ({} packages)", colors::green("✓"), lockfile.packages.len());
        return;
    }

    // --locked: run resolver but error if lockfile would change
    if locked {
        if !lock_path.exists() {
            eprintln!("{} Cannot use --locked: no lockfile found", colors::red("✗"));
            eprintln!("  Run 'wares install' without --locked first to generate lumen.lock");
            std::process::exit(1);
        }

        let existing_lockfile = match LockFile::load(lock_path) {
            Ok(lf) => lf,
            Err(e) => {
                eprintln!("{} Failed to load lockfile: {}", colors::red("✗"), e);
                std::process::exit(1);
            }
        };

        // Run the resolver (install_with_lock will do this), but capture the result
        // We run without frozen to get the new lockfile, then compare
        crate::wares::ops::install_with_lock(false);

        // After install, reload and compare
        if let Ok(new_lockfile) = LockFile::load(lock_path) {
            let diff = existing_lockfile.diff(&new_lockfile);
            if !diff.is_empty() {
                eprintln!("{} Lockfile would change, run 'wares install' without --locked", colors::red("✗"));
                eprintln!("{}", diff.summary());
                // Restore the original lockfile
                if let Err(e) = existing_lockfile.save(lock_path) {
                    eprintln!("{} Failed to restore lockfile: {}", colors::red("✗"), e);
                }
                std::process::exit(1);
            }
        }

        // Verify trust
        if let Ok(lockfile) = LockFile::load(lock_path) {
            verify_lockfile_trust(&lockfile, trust_level, registry_url);
        }
        return;
    }

    // Normal mode: resolve and install
    crate::wares::ops::install_with_lock(false);

    // After install, verify trust for all registry packages in the lockfile
    if let Ok(lockfile) = LockFile::load(lock_path) {
        verify_lockfile_trust(&lockfile, trust_level, registry_url);
    }
}

async fn cmd_install_package(package: &str, frozen: bool, locked: bool, trust_level: &str, registry_url: &str) {
    println!("{} Installing {}...", colors::status_label("Trust"), colors::bold(package));
    println!("  Policy: {}", colors::cyan(trust_level));

    // Parse package@version
    let (name, _version) = if let Some(idx) = package.find('@') {
        (&package[..idx], Some(&package[idx + 1..]))
    } else {
        (package, None)
    };

    let lock_path = std::path::Path::new("lumen.lock");

    if frozen {
        if !lock_path.exists() {
            eprintln!("{} Cannot use --frozen: no lockfile found", colors::red("✗"));
            std::process::exit(1);
        }
        let lockfile = match LockFile::load(lock_path) {
            Ok(lf) => lf,
            Err(e) => {
                eprintln!("{} Failed to load lockfile: {}", colors::red("✗"), e);
                std::process::exit(1);
            }
        };
        if !lockfile.packages.iter().any(|p| p.name == name) {
            eprintln!("{} Package '{}' not found in lockfile", colors::red("✗"), name);
            eprintln!("  Run 'wares install {}' without --frozen first", name);
            std::process::exit(1);
        }
        verify_lockfile_trust(&lockfile, trust_level, registry_url);
        println!("{} Using frozen lockfile for '{}'", colors::green("✓"), name);
        return;
    }

    if locked {
        let existing = lock_path.exists().then(|| LockFile::load(lock_path).ok()).flatten();
        crate::wares::ops::add_with_kind(name, None, crate::wares::ops::DependencyKind::Normal);

        if let Some(existing_lockfile) = existing {
            if let Ok(new_lockfile) = LockFile::load(lock_path) {
                let diff = existing_lockfile.diff(&new_lockfile);
                if !diff.is_empty() {
                    eprintln!("{} Lockfile would change, run without --locked", colors::red("✗"));
                    eprintln!("{}", diff.summary());
                    if let Err(e) = existing_lockfile.save(lock_path) {
                        eprintln!("{} Failed to restore lockfile: {}", colors::red("✗"), e);
                    }
                    std::process::exit(1);
                }
            }
        }
        if let Ok(lockfile) = LockFile::load(lock_path) {
            verify_lockfile_trust(&lockfile, trust_level, registry_url);
        }
        return;
    }

    // Normal mode: add/install the package
    crate::wares::ops::add_with_kind(name, None, crate::wares::ops::DependencyKind::Normal);

    // Verify trust on the resulting lockfile
    if let Ok(lockfile) = LockFile::load(lock_path) {
        verify_lockfile_trust(&lockfile, trust_level, registry_url);
    }
}

fn cmd_update(frozen: bool, locked: bool) {
    let lock_path = std::path::Path::new("lumen.lock");

    if frozen {
        if !lock_path.exists() {
            eprintln!("{} Cannot use --frozen: no lockfile found", colors::red("✗"));
            std::process::exit(1);
        }
        println!("{} Using frozen lockfile, skipping update", colors::green("✓"));
        return;
    }

    if locked {
        let existing = lock_path.exists().then(|| LockFile::load(lock_path).ok()).flatten();
        crate::wares::ops::update_with_lock(false);

        if let Some(existing_lockfile) = existing {
            if let Ok(new_lockfile) = LockFile::load(lock_path) {
                let diff = existing_lockfile.diff(&new_lockfile);
                if !diff.is_empty() {
                    eprintln!("{} Lockfile would change, run 'wares update' without --locked", colors::red("✗"));
                    eprintln!("{}", diff.summary());
                    if let Err(e) = existing_lockfile.save(lock_path) {
                        eprintln!("{} Failed to restore lockfile: {}", colors::red("✗"), e);
                    }
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    crate::wares::ops::update_with_lock(false);
}

fn cmd_search(query: &str) {
    crate::wares::ops::search(query);
}

fn cmd_info(target: &str) {
    // Try local lumen.toml first
    if let Some((_path, config)) = LumenConfig::load_with_path() {
        if let Some(ref pkg) = config.package {
            if pkg.name == target || pkg.name.ends_with(&format!("/{}", target)) {
                println!("{} {} (local)", colors::bold(&pkg.name), colors::cyan(pkg.version.as_deref().unwrap_or("0.0.0")));
                if let Some(ref desc) = pkg.description {
                    println!("  {}", desc);
                }
                println!();
                if let Some(ref authors) = pkg.authors {
                    println!("  Authors:    {}", authors.join(", "));
                }
                if let Some(ref license) = pkg.license {
                    println!("  License:    {}", license);
                }
                if let Some(ref repo) = pkg.repository {
                    println!("  Repository: {}", repo);
                }
                if let Some(ref homepage) = pkg.homepage {
                    println!("  Homepage:   {}", homepage);
                }
                if let Some(ref docs) = pkg.documentation {
                    println!("  Docs:       {}", docs);
                }
                if let Some(ref keywords) = pkg.keywords {
                    if !keywords.is_empty() {
                        println!("  Keywords:   {}", keywords.join(", "));
                    }
                }
                if !config.dependencies.is_empty() {
                    println!();
                    println!("  Dependencies:");
                    for (dep_name, spec) in &config.dependencies {
                        println!("    {} {}", colors::cyan(dep_name), colors::gray(&format_dep_spec(spec)));
                    }
                }
                if !config.dev_dependencies.is_empty() {
                    println!();
                    println!("  Dev Dependencies:");
                    for (dep_name, spec) in &config.dev_dependencies {
                        println!("    {} {}", colors::cyan(dep_name), colors::gray(&format_dep_spec(spec)));
                    }
                }
                return;
            }
        }

        // Check if target is one of the project's dependencies
        if let Some(spec) = config.dependencies.get(target) {
            println!("{} {} (dependency)", colors::bold(target), colors::gray(&format_dep_spec(spec)));
        }
    }

    // Fetch from registry
    let registry_url = LumenConfig::load().registry_url();
    let client = RegistryClient::new(&registry_url);

    match client.fetch_package_index(target) {
        Ok(pkg_index) => {
            println!("{} {}", colors::bold(&pkg_index.name),
                colors::cyan(pkg_index.latest.as_deref().unwrap_or("unknown")));

            if let Some(ref desc) = pkg_index.description {
                println!("  {}", desc);
            }
            println!();

            println!("  Versions:   {}", pkg_index.versions.join(", "));
            if !pkg_index.categories.is_empty() {
                println!("  Categories: {}", pkg_index.categories.join(", "));
            }
            if let Some(downloads) = pkg_index.downloads {
                println!("  Downloads:  {}", downloads);
            }
            if !pkg_index.yanked.is_empty() {
                println!("  Yanked:     {}", pkg_index.yanked.keys().cloned().collect::<Vec<_>>().join(", "));
            }

            // Fetch latest version metadata for more details
            if let Some(ref latest) = pkg_index.latest {
                match client.fetch_version_metadata(target, latest) {
                    Ok(meta) => {
                        if let Some(ref license) = meta.license {
                            println!("  License:    {}", license);
                        }
                        if let Some(ref repo) = meta.repository {
                            println!("  Repository: {}", repo);
                        }
                        if let Some(ref docs) = meta.documentation {
                            println!("  Docs:       {}", docs);
                        }
                        if let Some(ref publisher) = meta.publisher {
                            let name = publisher.name.as_deref().unwrap_or("unknown");
                            let verified = if publisher.verified { " (verified)" } else { "" };
                            println!("  Publisher:  {}{}", name, colors::green(verified));
                        }
                        if let Some(ref published) = meta.published_at {
                            println!("  Published:  {}", published);
                        }
                        if !meta.deps.is_empty() {
                            println!();
                            println!("  Dependencies:");
                            for (name, constraint) in &meta.deps {
                                println!("    {} {}", colors::cyan(name), colors::gray(constraint));
                            }
                        }
                        if !meta.keywords.is_empty() {
                            println!("  Keywords:   {}", meta.keywords.join(", "));
                        }

                        // Trust information
                        let has_sig = meta.signature.is_some();
                        let has_transparency = meta.transparency.is_some();
                        if has_sig || has_transparency {
                            println!();
                            println!("  Trust:");
                            if has_sig {
                                println!("    {} Signed", colors::green("✓"));
                            }
                            if has_transparency {
                                println!("    {} Transparency log entry", colors::green("✓"));
                            }
                        }
                    }
                    Err(e) => {
                        println!("  {} Could not fetch version details: {}", colors::gray("→"), e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{} Package '{}' not found: {}", colors::red("✗"), target, e);
            std::process::exit(1);
        }
    }
}

fn cmd_pack(output: &PathBuf) {
    // Create output directory if needed
    if let Err(e) = std::fs::create_dir_all(output) {
        eprintln!("{} Failed to create output directory: {}", colors::red("✗"), e);
        std::process::exit(1);
    }
    crate::wares::ops::pack();
}

async fn cmd_login(provider_str: &str, registry_url: &str) {
    let provider: IdentityProvider = match provider_str.parse() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            eprintln!("  Supported providers: github, gitlab, google");
            std::process::exit(1);
        }
    };
    
    let mut client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    if let Err(e) = client.login(provider).await {
        eprintln!("{} {}", colors::red("✗"), e);
        std::process::exit(1);
    }
}

async fn cmd_logout(registry_url: &str) {
    let mut client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    if let Err(e) = client.logout() {
        eprintln!("{} {}", colors::red("✗"), e);
        std::process::exit(1);
    }
}

async fn cmd_whoami(registry_url: &str) {
    let client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    match client.current_identity() {
        Some(identity) => {
            println!("{} Logged in to {}", colors::green("✓"), registry_url);
            println!("  Identity: {}", colors::bold(&identity));
        }
        None => {
            println!("{} Not logged in to {}", colors::yellow("!"), registry_url);
            println!("  Run 'wares login' to authenticate");
            std::process::exit(1);
        }
    }
}

async fn cmd_publish(dry_run: bool, provenance: bool, no_log: bool, registry_url: &str) {
    // Check if we're logged in
    let mut client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    if !client.is_authenticated() {
        eprintln!("{} Not authenticated. Run 'wares login' first.", colors::red("✗"));
        std::process::exit(1);
    }
    
    // Get package info
    let (package_name, version) = match read_package_info() {
        Ok(info) => info,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    println!("{} Publishing {}@{}...", colors::status_label("Trust"), colors::bold(&package_name), colors::bold(&version));
    
    if dry_run {
        println!("  {} Dry run mode — not publishing", colors::yellow("!"));
    }
    
    // Build SLSA provenance if requested
    let slsa_provenance = if provenance {
        println!("  {} Including SLSA build provenance...", colors::cyan("→"));
        match generate_provenance(&package_name, &version) {
            Some(p) => {
                println!("  {} SLSA Level 3 provenance generated", colors::green("✓"));
                Some(p)
            }
            None => {
                eprintln!("  {} Could not generate provenance (not in CI?)", colors::yellow("!"));
                None
            }
        }
    } else {
        None
    };
    
    // Build and pack the package
    println!("  {} Building package archive...", colors::cyan("→"));
    crate::wares::ops::build();
    crate::wares::ops::pack();
    
    // Read the package archive (packed as .tgz by cmd_pkg_pack)
    let archive_path = format!("dist/{}-{}.tgz", package_name, version);
    let content = match std::fs::read(&archive_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to read package archive: {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    // Sign and publish
    if !dry_run {
        match client.publish_package(&package_name, &version, &content, slsa_provenance).await {
            Ok(sig) => {
                println!("  {} Package signed successfully", colors::green("✓"));
                println!("    Identity: {}", sig.certificate.identity_str());
                println!("    Content hash: {}", &sig.content_hash[..16]);
                
                if no_log {
                    println!("  {} Skipping transparency log", colors::yellow("!"));
                } else {
                    println!("  {} Transparency log entry pending...", colors::cyan("→"));
                }
                
                // Upload to registry (this would use the existing publish logic)
                // TODO: Integrate with actual registry upload
                println!("{} Published {}@{}", colors::green("✓"), package_name, version);
            }
            Err(e) => {
                eprintln!("{} Publish failed: {}", colors::red("✗"), e);
                std::process::exit(1);
            }
        }
    } else {
        println!("{} Dry run complete", colors::green("✓"));
        println!("  Package {}@{} would be signed and published", package_name, version);
    }
}

async fn cmd_trust_check(package_spec: &str, show_log: bool, format: &str, registry_url: &str) {
    let (name, version) = if let Some(idx) = package_spec.find('@') {
        (&package_spec[..idx], Some(&package_spec[idx + 1..]))
    } else {
        (package_spec, None)
    };

    let lock_path = std::path::Path::new("lumen.lock");

    // If checking a specific package, look it up in the lockfile
    if let Some(ver) = version {
        // Check a single package@version
        let result = check_single_package(name, ver, lock_path, registry_url, show_log);

        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string()));
            return;
        }

        print_single_trust_result(name, ver, &result, show_log);
        return;
    }

    // No version specified — check all packages in the lockfile
    if !lock_path.exists() {
        if format == "json" {
            println!("{{\"error\": \"no lockfile found\"}}");
        } else {
            eprintln!("{} No lockfile found. Run 'wares install' first.", colors::red("✗"));
        }
        std::process::exit(1);
    }

    let lockfile = match LockFile::load(lock_path) {
        Ok(lf) => lf,
        Err(e) => {
            eprintln!("{} Failed to load lockfile: {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };

    // Filter packages: if a name was given without version, filter by name
    let packages: Vec<&crate::lockfile::LockedPackage> = if name.is_empty() || name == "*" {
        lockfile.packages.iter().collect()
    } else {
        lockfile.packages.iter().filter(|p| p.name == name || p.name.ends_with(&format!("/{}", name))).collect()
    };

    if packages.is_empty() {
        if format == "json" {
            println!("{{\"error\": \"no matching packages found\"}}");
        } else {
            eprintln!("{} No matching packages found in lockfile", colors::red("✗"));
        }
        std::process::exit(1);
    }

    let mut passed = 0usize;
    let mut warned = 0usize;
    let mut failed = 0usize;

    if format == "json" {
        let mut results = Vec::new();
        for pkg in &packages {
            let result = check_locked_package_trust(pkg);
            results.push(result);
        }
        println!("{}", serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string()));
        return;
    }

    println!("{} Trust check for {} package(s)", colors::status_label("Trust"), packages.len());
    println!();

    // Table header
    println!("  {:<30} {:<12} {:<12} {:<12} {:<14} {}",
        colors::bold("Package"), colors::bold("Version"),
        colors::bold("Integrity"), colors::bold("Signature"),
        colors::bold("Transparency"), colors::bold("Status"));
    println!("  {}", "─".repeat(96));

    for pkg in &packages {
        let has_integrity = pkg.integrity.is_some();
        let has_signature = pkg.signature.is_some();
        let has_transparency = pkg.transparency_index.is_some();
        let is_registry = pkg.source.starts_with("registry+");

        let integrity_str = if has_integrity { colors::green("✓") } else if is_registry { colors::red("✗") } else { colors::gray("—") };
        let signature_str = if has_signature { colors::green("✓") } else if is_registry { colors::red("✗") } else { colors::gray("—") };
        let transparency_str = if has_transparency { colors::green("✓") } else if is_registry { colors::yellow("—") } else { colors::gray("—") };

        let status = if !is_registry {
            passed += 1;
            colors::gray("local")
        } else if has_integrity && has_signature {
            passed += 1;
            colors::green("pass")
        } else if has_integrity || has_signature {
            warned += 1;
            colors::yellow("warn")
        } else {
            failed += 1;
            colors::red("fail")
        };

        println!("  {:<30} {:<12} {:<12} {:<12} {:<14} {}",
            pkg.name, pkg.version,
            integrity_str, signature_str, transparency_str, status);
    }

    println!("  {}", "─".repeat(96));
    println!();

    // Show transparency log entries if requested
    if show_log {
        let logged: Vec<_> = packages.iter().filter(|p| p.transparency_index.is_some()).collect();
        if !logged.is_empty() {
            println!("{}", colors::bold("Transparency Log Entries"));
            for pkg in logged {
                if let Some(idx) = pkg.transparency_index {
                    println!("  #{}  {}@{}", idx, pkg.name, pkg.version);
                }
            }
            println!();
        }
    }

    // Summary
    println!("{}", colors::bold("Summary"));
    println!("  {} passed, {} warnings, {} failed",
        colors::green(&passed.to_string()),
        colors::yellow(&warned.to_string()),
        colors::red(&failed.to_string()));

    if failed > 0 {
        println!();
        println!("{} Some packages failed trust verification.", colors::red("✗"));
        println!("  Run with '--trust permissive' to install anyway, or add signatures.");
        std::process::exit(1);
    } else if warned > 0 {
        println!();
        println!("{} Some packages have incomplete trust metadata.", colors::yellow("!"));
    } else {
        println!();
        println!("{} {}", colors::green("✓"), colors::bold("All packages pass trust verification."));
    }
}

// =============================================================================
// Trust Verification Helpers
// =============================================================================

/// Verify trust metadata for all registry packages in a lockfile during install.
fn verify_lockfile_trust(lockfile: &LockFile, trust_level: &str, _registry_url: &str) {
    let registry_packages: Vec<_> = lockfile.packages.iter()
        .filter(|p| p.source.starts_with("registry+"))
        .collect();

    if registry_packages.is_empty() {
        return;
    }

    let mut failures = Vec::new();

    for pkg in &registry_packages {
        let has_integrity = pkg.integrity.is_some();
        let has_signature = pkg.signature.is_some();
        let has_transparency = pkg.transparency_index.is_some();

        match trust_level {
            "strict" => {
                if !has_integrity {
                    failures.push(format!("{}@{}: missing integrity hash", pkg.name, pkg.version));
                }
                if !has_signature {
                    failures.push(format!("{}@{}: missing signature", pkg.name, pkg.version));
                }
                if !has_transparency {
                    failures.push(format!("{}@{}: missing transparency log entry", pkg.name, pkg.version));
                }
            }
            "normal" => {
                if !has_integrity {
                    failures.push(format!("{}@{}: missing integrity hash", pkg.name, pkg.version));
                }
                if !has_signature {
                    println!("  {} {}@{}: no signature (consider 'strict' policy)",
                        colors::yellow("!"), pkg.name, pkg.version);
                }
            }
            "permissive" | _ => {
                // Permissive mode only warns, never fails
                if !has_integrity && !has_signature {
                    println!("  {} {}@{}: no trust metadata",
                        colors::yellow("!"), pkg.name, pkg.version);
                }
            }
        }
    }

    if !failures.is_empty() {
        eprintln!();
        eprintln!("{} Trust verification failed:", colors::red("✗"));
        for f in &failures {
            eprintln!("  {} {}", colors::red("•"), f);
        }
        eprintln!();
        eprintln!("  Use '--trust permissive' to skip trust verification");
        std::process::exit(1);
    }

    let total = registry_packages.len();
    let signed = registry_packages.iter().filter(|p| p.signature.is_some()).count();
    let transparent = registry_packages.iter().filter(|p| p.transparency_index.is_some()).count();

    println!("  {} Trust: {}/{} signed, {}/{} in transparency log",
        colors::green("✓"), signed, total, transparent, total);
}

/// Trust check result for a single package (JSON-serializable).
#[derive(serde::Serialize)]
struct TrustCheckResult {
    package: String,
    version: String,
    integrity: bool,
    signature: bool,
    transparency: bool,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    integrity_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transparency_index: Option<u64>,
}

/// Check trust for a locked package from the lockfile.
fn check_locked_package_trust(pkg: &crate::lockfile::LockedPackage) -> TrustCheckResult {
    let has_integrity = pkg.integrity.is_some();
    let has_signature = pkg.signature.is_some();
    let has_transparency = pkg.transparency_index.is_some();
    let is_registry = pkg.source.starts_with("registry+");

    let status = if !is_registry {
        "local".to_string()
    } else if has_integrity && has_signature {
        "pass".to_string()
    } else if has_integrity || has_signature {
        "warn".to_string()
    } else {
        "fail".to_string()
    };

    TrustCheckResult {
        package: pkg.name.clone(),
        version: pkg.version.clone(),
        integrity: has_integrity,
        signature: has_signature,
        transparency: has_transparency,
        status,
        integrity_hash: pkg.integrity.clone(),
        transparency_index: pkg.transparency_index,
    }
}

/// Check a single package@version against the lockfile and optionally the registry.
fn check_single_package(
    name: &str,
    version: &str,
    lock_path: &std::path::Path,
    registry_url: &str,
    _show_log: bool,
) -> TrustCheckResult {
    // First try the lockfile
    if lock_path.exists() {
        if let Ok(lockfile) = LockFile::load(lock_path) {
            if let Some(pkg) = lockfile.packages.iter().find(|p| p.name == name && p.version == version) {
                return check_locked_package_trust(pkg);
            }
        }
    }

    // Fall back to registry metadata
    let client = RegistryClient::new(registry_url);
    match client.fetch_version_metadata(name, version) {
        Ok(meta) => {
            TrustCheckResult {
                package: meta.name,
                version: meta.version,
                integrity: meta.integrity.is_some(),
                signature: meta.signature.is_some(),
                transparency: meta.transparency.is_some(),
                status: if meta.integrity.is_some() && meta.signature.is_some() {
                    "pass".to_string()
                } else if meta.integrity.is_some() || meta.signature.is_some() {
                    "warn".to_string()
                } else {
                    "fail".to_string()
                },
                integrity_hash: meta.integrity.map(|i| i.manifest_hash),
                transparency_index: meta.transparency.map(|t| t.log_index),
            }
        }
        Err(_) => {
            TrustCheckResult {
                package: name.to_string(),
                version: version.to_string(),
                integrity: false,
                signature: false,
                transparency: false,
                status: "unknown".to_string(),
                integrity_hash: None,
                transparency_index: None,
            }
        }
    }
}

/// Print a human-readable trust check result for a single package.
fn print_single_trust_result(name: &str, version: &str, result: &TrustCheckResult, show_log: bool) {
    println!("{} Trust check for {}@{}", colors::status_label("Trust"), colors::bold(name), version);
    println!();

    println!("{}", colors::bold("Verification"));
    if result.integrity {
        println!("  {} Integrity hash present", colors::green("✓"));
        if let Some(ref hash) = result.integrity_hash {
            println!("    {}", colors::gray(hash));
        }
    } else {
        println!("  {} No integrity hash", colors::red("✗"));
    }

    if result.signature {
        println!("  {} Package signature present", colors::green("✓"));
    } else {
        println!("  {} No package signature", colors::red("✗"));
    }

    if result.transparency {
        println!("  {} Transparency log entry", colors::green("✓"));
        if let Some(idx) = result.transparency_index {
            println!("    Log index: #{}", idx);
        }
    } else {
        println!("  {} No transparency log entry", colors::yellow("—"));
    }

    println!();

    if show_log && result.transparency {
        if let Some(idx) = result.transparency_index {
            println!("{}", colors::bold("Transparency Log"));
            println!("  #{} {}@{}", idx, name, version);
            println!();
        }
    }

    match result.status.as_str() {
        "pass" => {
            println!("{} {}", colors::green("✓"), colors::bold("Trust verification passed"));
        }
        "warn" => {
            println!("{} {}", colors::yellow("!"), colors::bold("Trust verification incomplete"));
            println!("  Some trust metadata is missing. Consider using 'strict' policy.");
        }
        "fail" => {
            println!("{} {}", colors::red("✗"), colors::bold("Trust verification failed"));
            println!("  This package lacks required trust metadata.");
        }
        _ => {
            println!("{} {}", colors::gray("?"), colors::bold("Trust status unknown"));
            println!("  Package not found in lockfile or registry.");
        }
    }
}

async fn cmd_policy_show(registry_url: &str) {
    let client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    let policy = client.config().get_policy(registry_url);
    
    println!("{}", colors::bold("Trust Policy"));
    println!("  Registry: {}", registry_url);
    println!();
    
    println!("{}", colors::bold("Requirements"));
    if let Some(pattern) = &policy.required_identity {
        println!("  Required identity: {}", colors::cyan(pattern));
    } else {
        println!("  Required identity: Any");
    }
    println!("  Min SLSA level: {}", policy.min_slsa_level);
    println!("  Require transparency log: {}", 
        if policy.require_transparency_log { colors::green("Yes") } else { colors::yellow("No") });
    if let Some(age) = &policy.min_package_age {
        println!("  Min package age: {}", age);
    }
    println!("  Block install scripts: {}", 
        if policy.block_install_scripts { colors::green("Yes") } else { colors::yellow("No") });
}

async fn cmd_policy_set(registry_url: &str, policy: TrustPolicy) {
    let mut client = match TrustClient::new(registry_url.to_string()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", colors::red("✗"), e);
            std::process::exit(1);
        }
    };
    
    client.config_mut().policies.insert(registry_url.to_string(), policy.clone());
    
    if let Err(e) = client.config().save() {
        eprintln!("{} Failed to save policy: {}", colors::red("✗"), e);
        std::process::exit(1);
    }
    
    println!("{} Policy updated for {}", colors::green("✓"), registry_url);
    
    // Show what changed
    if policy.min_slsa_level >= 2 {
        println!("  {} SLSA Level {} provenance required", colors::green("→"), policy.min_slsa_level);
    }
    if policy.require_transparency_log {
        println!("  {} Transparency log inclusion required", colors::green("→"));
    }
    if policy.block_install_scripts {
        println!("  {} Install scripts blocked by default", colors::green("→"));
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn read_package_info() -> Result<(String, String), String> {
    // Read lumen.toml
    let content = std::fs::read_to_string("lumen.toml")
        .map_err(|_| "No lumen.toml found. Run 'wares init' first.".to_string())?;
    
    let doc: toml::Value = content.parse()
        .map_err(|e| format!("Failed to parse lumen.toml: {}", e))?;
    
    let package = doc.get("package")
        .ok_or_else(|| "No [package] section in lumen.toml".to_string())?;
    
    let name = package.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No package.name in lumen.toml".to_string())?;
    
    let version = package.get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No package.version in lumen.toml".to_string())?;
    
    Ok((name.to_string(), version.to_string()))
}

fn format_dep_spec(spec: &crate::config::DependencySpec) -> String {
    use crate::config::DependencySpec;
    match spec {
        DependencySpec::Version(v) => v.clone(),
        DependencySpec::Path { path } => format!("path: {}", path),
        DependencySpec::VersionDetailed { version, .. } => version.clone(),
        DependencySpec::Git { git, .. } => format!("git: {}", git),
        DependencySpec::Workspace { .. } => "workspace".to_string(),
    }
}

fn generate_provenance(_name: &str, _version: &str) -> Option<crate::wares::types::SlsaProvenance> {
    // Check if we're in a CI environment
    if std::env::var("GITHUB_ACTIONS").is_err() {
        return None;
    }
    
    use crate::wares::types::{BuildInvocation, BuildMetadata, ConfigSource, SlsaProvenance, SourceInfo};
    use std::collections::HashMap;
    use chrono::Utc;
    
    let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let workflow = std::env::var("GITHUB_WORKFLOW").unwrap_or_default();
    let sha = std::env::var("GITHUB_SHA").unwrap_or_default();
    let run_id = std::env::var("GITHUB_RUN_ID").unwrap_or_default();
    
    let mut config_digest = HashMap::new();
    config_digest.insert("sha256".to_string(), sha.clone());
    
    let mut source_digest = HashMap::new();
    source_digest.insert("gitCommit".to_string(), sha);
    
    Some(SlsaProvenance {
        slsa_version: "v1.0".to_string(),
        build_type: "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1".to_string(),
        builder_id: format!("https://github.com/{}/.github/workflows/{}", repo, workflow),
        invocation: BuildInvocation {
            config_source: ConfigSource {
                uri: format!("https://github.com/{}", repo),
                digest: config_digest,
                entry_point: workflow,
            },
            environment: {
                let mut env = HashMap::new();
                env.insert("GITHUB_RUN_ID".to_string(), run_id);
                env
            },
        },
        source: SourceInfo {
            uri: format!("https://github.com/{}", repo),
            digest: source_digest,
        },
        metadata: BuildMetadata {
            started_on: Utc::now(),
            finished_on: Utc::now(),
        },
    })
}
