//! Registry command handlers for authentication and package management.
//!
//! Commands:
//! - `lumen registry login` - Interactive login, saves token
//! - `lumen registry logout` - Removes stored token
//! - `lumen registry token` - List/manage tokens
//! - `lumen registry owner add <package> <email>` - Add owner
//! - `lumen registry owner remove <package> <email>` - Remove owner
//! - `lumen registry owner list <package>` - List owners

use crate::auth::{
    lumen_home_dir, mask_token, validate_token_format, AuthError, AuthenticatedClient,
    CredentialManager, OwnerRole, PackageOwner, TokenScope,
};
use crate::config::LumenConfig;
use std::collections::HashMap;
use std::io::Write;

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

// =============================================================================
// Registry Commands
// =============================================================================

/// Registry subcommands.
#[derive(Debug, Clone)]
pub enum RegistryCommands {
    /// Login to a registry
    Login {
        /// Registry URL (defaults to configured default)
        registry: Option<String>,
        /// Token to use (if provided, skip interactive prompt)
        token: Option<String>,
        /// Token name
        name: Option<String>,
    },
    /// Logout from a registry
    Logout {
        /// Registry URL (defaults to configured default)
        registry: Option<String>,
    },
    /// Show current authenticated user
    Whoami {
        /// Registry URL (defaults to configured default)
        registry: Option<String>,
    },
    /// Manage tokens
    Token {
        /// Token subcommand
        sub: TokenCommands,
    },
    /// Manage package owners
    Owner {
        /// Owner subcommand
        sub: OwnerCommands,
    },
}

/// Token management subcommands.
#[derive(Debug, Clone)]
pub enum TokenCommands {
    /// List stored tokens
    List,
    /// Add a new token
    Add {
        registry: String,
        token: String,
        name: Option<String>,
    },
    /// Remove a token
    Remove { registry: String },
}

/// Owner management subcommands.
#[derive(Debug, Clone)]
pub enum OwnerCommands {
    /// Add an owner to a package
    Add {
        package: String,
        email: String,
        /// Owner role (maintainer or owner)
        role: Option<String>,
    },
    /// Remove an owner from a package
    Remove { package: String, email: String },
    /// List owners of a package
    List { package: String },
}

// =============================================================================
// Command Handlers
// =============================================================================

/// Handle registry commands.
pub fn cmd_registry(sub: RegistryCommands) {
    match sub {
        RegistryCommands::Login {
            registry,
            token,
            name,
        } => cmd_registry_login(registry, token, name),
        RegistryCommands::Logout { registry } => cmd_registry_logout(registry),
        RegistryCommands::Whoami { registry } => cmd_registry_whoami(registry),
        RegistryCommands::Token { sub } => cmd_registry_token(sub),
        RegistryCommands::Owner { sub } => cmd_registry_owner(sub),
    }
}

/// Login to a registry.
fn cmd_registry_login(
    registry: Option<String>,
    provided_token: Option<String>,
    token_name: Option<String>,
) {
    // Get registry URL
    let registry_url = get_effective_registry(registry);

    println!("{} {}", status_label("Logging in"), cyan(&registry_url));

    // Get token - either provided or interactive
    let token = match provided_token {
        Some(t) => t,
        None => {
            // Interactive prompt
            print!("{} Paste your API token: ", status_label("Prompt"));
            std::io::stdout().flush().unwrap();

            // Read token without echoing
            let input = rpassword::read_password()
                .map_err(|e| {
                    eprintln!("{} Failed to read token: {}", red("error:"), e);
                    std::process::exit(1);
                })
                .unwrap();

            let trimmed = input.trim();
            if trimmed.is_empty() {
                eprintln!("{} Token cannot be empty", red("error:"));
                std::process::exit(1);
            }
            trimmed.to_string()
        }
    };

    // Validate token format
    if let Err(e) = validate_token_format(&token) {
        eprintln!("{} {}", yellow("warning:"), e);
    }

    // Get token name if not provided
    let name = match token_name {
        Some(n) => n,
        None => {
            print!(
                "{} Token name (e.g., 'laptop', 'ci'): ",
                status_label("Prompt")
            );
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let trimmed = input.trim();
            if trimmed.is_empty() {
                "default".to_string()
            } else {
                trimmed.to_string()
            }
        }
    };

    // Store the token temporarily for validation
    let cred_manager = match CredentialManager::new() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("{} Failed to access credential store: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    // Store token first so AuthenticatedClient can pick it up
    if let Err(e) = cred_manager.store_token(&registry_url, &token, Some(&name)) {
        eprintln!("{} Failed to store token: {}", red("error:"), e);
        std::process::exit(1);
    }

    // Test the token by making an authenticated request
    println!("{} validating token...", status_label("Validating"));

    let client = match AuthenticatedClient::new(registry_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            // Clean up stored token on failure
            let _ = cred_manager.remove_token(&registry_url);
            eprintln!("{} Failed to create client: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    // Validate token with registry
    match client.validate_token() {
        Ok(validation) => {
            if validation.valid {
                if let Some(user_info) = validation.user_info {
                    println!("{} authenticated as {}", green("✓"), bold(&user_info.email));
                    if !user_info.scopes.is_empty() {
                        println!(
                            "  {} {}",
                            gray("Scopes:"),
                            gray(&user_info.scopes.join(", "))
                        );
                    }
                } else {
                    println!("{} token is valid", green("✓"));
                }
            } else {
                // Clean up stored token on validation failure
                let _ = cred_manager.remove_token(&registry_url);
                eprintln!("{} Token validation failed", red("error:"));
                if let Some(err) = validation.error {
                    eprintln!("  {}", err);
                }
                std::process::exit(1);
            }
        }
        Err(e) => {
            // Clean up stored token on validation error
            let _ = cred_manager.remove_token(&registry_url);
            eprintln!("{} Failed to validate token: {}", red("error:"), e);
            std::process::exit(1);
        }
    }

    // Generate signing key for future uploads
    match cred_manager.get_or_create_signing_key(&registry_url) {
        Ok(keypair) => {
            println!(
                "{} generated signing key {}",
                status_label("Keys"),
                gray(&keypair.key_id)
            );
        }
        Err(e) => {
            eprintln!(
                "{} Failed to generate signing key: {}",
                yellow("warning:"),
                e
            );
        }
    }

    println!("{} logged in as '{}'", green("✓"), bold(&name));
    println!("  {} {}", gray("Registry:"), cyan(&registry_url));
    println!("  {} {}", gray("Token:"), gray(&mask_token(&token)));
}

/// Logout from a registry.
fn cmd_registry_logout(registry: Option<String>) {
    let registry_url = get_effective_registry(registry);

    let cred_manager = match CredentialManager::new() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("{} Failed to access credential store: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    match cred_manager.remove_token(&registry_url) {
        Ok(true) => {
            println!("{} logged out from {}", green("✓"), cyan(&registry_url));
        }
        Ok(false) => {
            println!(
                "{} no credentials found for {}",
                yellow("⚠"),
                cyan(&registry_url)
            );
        }
        Err(e) => {
            eprintln!("{} Failed to remove credentials: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// Show current authenticated user.
fn cmd_registry_whoami(registry: Option<String>) {
    let registry_url = get_effective_registry(registry);

    println!("{} {}", status_label("Checking"), cyan(&registry_url));

    let client = match AuthenticatedClient::new(registry_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to create client: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    if !client.is_authenticated() {
        eprintln!(
            "{} Not authenticated for {}",
            red("error:"),
            cyan(&registry_url)
        );
        eprintln!("  Run {} to login", cyan("lumen registry login"));
        std::process::exit(1);
    }

    match client.whoami() {
        Ok(user_info) => {
            println!("{} authenticated", green("✓"));
            println!();
            println!("  {:<20} {}", bold("User ID:"), cyan(&user_info.user_id));
            println!("  {:<20} {}", bold("Email:"), cyan(&user_info.email));
            if let Some(name) = &user_info.name {
                println!("  {:<20} {}", bold("Name:"), name);
            }
            if !user_info.scopes.is_empty() {
                let scopes: Vec<String> = user_info
                    .scopes
                    .iter()
                    .map(|s| green(s).to_string())
                    .collect();
                println!("  {:<20} {}", bold("Token Scopes:"), scopes.join(", "));
            }
            if !user_info.organizations.is_empty() {
                println!("  {}", bold("Organizations:"));
                for org in &user_info.organizations {
                    println!("    - {} ({})", cyan(&org.name), gray(&org.role));
                }
            }
            if let Some(expires) = user_info.expires_at {
                let now = chrono::Utc::now();
                let duration = expires.signed_duration_since(now);
                let expiry_str = if duration.num_days() < 0 {
                    format!(
                        "{} (expired)",
                        red(&expires.format("%Y-%m-%d %H:%M UTC").to_string())
                    )
                } else if duration.num_days() < 7 {
                    format!(
                        "{} (expires in {} days)",
                        yellow(&expires.format("%Y-%m-%d %H:%M UTC").to_string()),
                        duration.num_days()
                    )
                } else {
                    expires.format("%Y-%m-%d %H:%M UTC").to_string()
                };
                println!("  {:<20} {}", bold("Token Expires:"), expiry_str);
            }
        }
        Err(AuthError::NotAuthenticated(msg)) => {
            eprintln!("{} {}", red("error:"), msg);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{} Failed to get user info: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// Handle token management commands.
fn cmd_registry_token(sub: TokenCommands) {
    match sub {
        TokenCommands::List => cmd_token_list(),
        TokenCommands::Add {
            registry,
            token,
            name,
        } => cmd_token_add(registry, token, name),
        TokenCommands::Remove { registry } => cmd_token_remove(registry),
    }
}

/// List stored tokens.
fn cmd_token_list() {
    let cred_manager = match CredentialManager::new() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("{} Failed to access credential store: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    let credentials = match cred_manager.list_credentials() {
        Ok(creds) => creds,
        Err(e) => {
            eprintln!("{} Failed to load credentials: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    if credentials.is_empty() {
        println!("{} no stored tokens", gray("info:"));
        println!();
        println!("Run {} to authenticate", cyan("lumen registry login"));
        return;
    }

    println!("{} stored credentials:", status_label("Found"));
    println!();
    println!(
        "  {:<30} {:<20} {}",
        bold("Registry"),
        bold("Name"),
        bold("Saved")
    );
    println!("  {}", gray(&"─".repeat(70)));

    for cred in &credentials {
        let token_display = if cred.token == "__keyring__" {
            "[keyring]".to_string()
        } else if cred.token.is_empty() {
            "[none]".to_string()
        } else {
            mask_token(&cred.token)
        };

        let name = cred.token_name.as_deref().unwrap_or("default");
        let saved = cred.saved_at.format("%Y-%m-%d %H:%M UTC").to_string();

        println!(
            "  {:<30} {:<20} {}",
            cyan(&truncate(&cred.registry, 30)),
            name,
            gray(&saved)
        );
        println!("    {} {}", gray("Token:"), gray(&token_display));

        if let Some(key_id) = &cred.signing_key_id {
            println!("    {} {}", gray("Key ID:"), gray(key_id));
        }
    }

    println!();
    println!(
        "Credentials stored at: {}",
        gray(&cred_manager.credentials_path().display().to_string())
    );
}

/// Add a token manually.
fn cmd_token_add(registry: String, token: String, name: Option<String>) {
    let name = name.unwrap_or_else(|| "manual".to_string());

    let cred_manager = match CredentialManager::new() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("{} Failed to access credential store: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    if let Err(e) = cred_manager.store_token(&registry, &token, Some(&name)) {
        eprintln!("{} Failed to store token: {}", red("error:"), e);
        std::process::exit(1);
    }

    println!("{} added token for {}", green("✓"), cyan(&registry));
    println!("  {} {}", gray("Name:"), &name);
}

/// Remove a token.
fn cmd_token_remove(registry: String) {
    cmd_registry_logout(Some(registry));
}

/// Handle owner management commands.
fn cmd_registry_owner(sub: OwnerCommands) {
    match sub {
        OwnerCommands::Add {
            package,
            email,
            role,
        } => cmd_owner_add(package, email, role),
        OwnerCommands::Remove { package, email } => cmd_owner_remove(package, email),
        OwnerCommands::List { package } => cmd_owner_list(package),
    }
}

/// Add an owner to a package.
fn cmd_owner_add(package: String, email: String, role: Option<String>) {
    let config = LumenConfig::load();
    let registry_url = config.registry_url();

    let client = match AuthenticatedClient::new(registry_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to create client: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    if !client.is_authenticated() {
        eprintln!(
            "{} Not authenticated. Run: lumen registry login",
            red("error:")
        );
        std::process::exit(1);
    }

    let role = match role {
        Some(r) => match OwnerRole::from_str(&r) {
            Some(role) => role,
            None => {
                eprintln!(
                    "{} Invalid role '{}'. Use 'maintainer' or 'owner'",
                    red("error:"),
                    r
                );
                std::process::exit(1);
            }
        },
        None => OwnerRole::Maintainer,
    };

    println!(
        "{} adding owner to {}...",
        status_label("Adding"),
        bold(&package)
    );
    println!("  {} {}", gray("Email:"), cyan(&email));
    println!("  {} {}", gray("Role:"), cyan(&role.to_string()));

    // Prepare request body
    let request = serde_json::json!({
        "email": email,
        "role": role,
    });

    let body = serde_json::to_vec(&request).unwrap();
    let path = format!("/v1/wares/{}/owners", package);

    match client.post(&path, body) {
        Ok(resp) => {
            if resp.status().is_success() {
                println!(
                    "{} added {} as {}",
                    green("✓"),
                    bold(&email),
                    cyan(&role.to_string())
                );
            } else if let Some(msg) = AuthenticatedClient::check_auth_error(&resp) {
                eprintln!("{} {}", red("error:"), msg);
                if resp.status() == reqwest::StatusCode::CONFLICT {
                    eprintln!("  The user may already be an owner of this package.");
                }
                std::process::exit(1);
            } else {
                eprintln!("{} Failed to add owner: {}", red("error:"), resp.status());
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{} Request failed: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// Remove an owner from a package.
fn cmd_owner_remove(package: String, email: String) {
    let config = LumenConfig::load();
    let registry_url = config.registry_url();

    let client = match AuthenticatedClient::new(registry_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to create client: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    if !client.is_authenticated() {
        eprintln!(
            "{} Not authenticated. Run: lumen registry login",
            red("error:")
        );
        std::process::exit(1);
    }

    println!(
        "{} removing owner from {}...",
        status_label("Removing"),
        bold(&package)
    );
    println!("  {} {}", gray("Email:"), cyan(&email));

    // Confirm removal
    print!(
        "{} Are you sure? This cannot be undone [y/N]: ",
        yellow("⚠")
    );
    std::io::stdout().flush().unwrap();

    let mut confirm = String::new();
    std::io::stdin().read_line(&mut confirm).unwrap();

    if !confirm.trim().eq_ignore_ascii_case("y") {
        println!("{} cancelled", gray("info:"));
        return;
    }

    let path = format!("/v1/wares/{}/owners/{}", package, email);

    match client.delete(&path) {
        Ok(resp) => {
            if resp.status().is_success() {
                println!(
                    "{} removed {} from {}",
                    green("✓"),
                    bold(&email),
                    bold(&package)
                );
            } else if let Some(msg) = AuthenticatedClient::check_auth_error(&resp) {
                eprintln!("{} {}", red("error:"), msg);
                std::process::exit(1);
            } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                eprintln!("{} {} is not an owner of {}", red("error:"), email, package);
                std::process::exit(1);
            } else {
                eprintln!(
                    "{} Failed to remove owner: {}",
                    red("error:"),
                    resp.status()
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{} Request failed: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

/// List owners of a package.
fn cmd_owner_list(package: String) {
    let config = LumenConfig::load();
    let registry_url = config.registry_url();

    let client = match AuthenticatedClient::new(registry_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to create client: {}", red("error:"), e);
            std::process::exit(1);
        }
    };

    // Note: Listing owners may be public, so we don't require auth
    let path = format!("/v1/wares/{}/owners", package);

    match client.get(&path) {
        Ok(resp) => {
            if resp.status().is_success() {
                let owners: Vec<PackageOwner> = match resp.json() {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!("{} Failed to parse response: {}", red("error:"), e);
                        std::process::exit(1);
                    }
                };

                if owners.is_empty() {
                    println!("{} no owners found for {}", gray("info:"), bold(&package));
                    return;
                }

                println!("{} owners of {}:", status_label("Found"), bold(&package));
                println!();
                println!(
                    "  {:<30} {:<15} {}",
                    bold("Email"),
                    bold("Role"),
                    bold("Added")
                );
                println!("  {}", gray(&"─".repeat(70)));

                for owner in &owners {
                    let role_color = match owner.role {
                        OwnerRole::Owner => cyan(&owner.role.to_string()),
                        OwnerRole::Maintainer => green(&owner.role.to_string()),
                    };

                    println!(
                        "  {:<30} {:<15} {}",
                        cyan(&owner.email),
                        role_color,
                        gray(&owner.added_at.format("%Y-%m-%d").to_string())
                    );
                }
            } else if let Some(msg) = AuthenticatedClient::check_auth_error(&resp) {
                eprintln!("{} {}", red("error:"), msg);
                std::process::exit(1);
            } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                eprintln!("{} package '{}' not found", red("error:"), package);
                std::process::exit(1);
            } else {
                eprintln!("{} Failed to list owners: {}", red("error:"), resp.status());
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{} Request failed: {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the effective registry URL.
fn get_effective_registry(provided: Option<String>) -> String {
    if let Some(url) = provided {
        return url;
    }

    let config = LumenConfig::load();
    config.registry_url()
}

/// Mask a token for display.
fn mask_token_display(token: &str) -> String {
    if token.len() < 12 {
        "***".to_string()
    } else {
        format!("{}...{}", &token[..6], &token[token.len() - 4..])
    }
}

/// Truncate a string to max length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// =============================================================================
// Integration with pkg publish
// =============================================================================

/// Publish a package with authentication.
pub fn publish_with_auth(
    registry_url: &str,
    package_name: &str,
    version: &str,
    archive_data: Vec<u8>,
    proof: Option<serde_json::Value>,
) -> Result<(), String> {
    let client = AuthenticatedClient::new(registry_url.to_string())
        .map_err(|e| format!("Failed to create authenticated client: {}", e))?;

    if !client.is_authenticated() {
        return Err("Not authenticated. Run 'lumen registry login' first.".to_string());
    }

    println!(
        "{} {}@{} to {}",
        status_label("Publishing"),
        bold(package_name),
        gray(version),
        cyan(registry_url)
    );

    let path = "/v1/wares".to_string();

    match client.put_signed(&path, archive_data, package_name, version, proof) {
        Ok(resp) => {
            if resp.status().is_success() {
                println!(
                    "{} published {}@{}",
                    green("✓"),
                    bold(package_name),
                    gray(version)
                );
                Ok(())
            } else if let Some(msg) = AuthenticatedClient::check_auth_error(&resp) {
                Err(msg)
            } else {
                Err(format!("Publish failed: {}", resp.status()))
            }
        }
        Err(e) => Err(format!("Request failed: {}", e)),
    }
}

/// Get authentication headers for a request.
pub fn get_auth_headers(registry_url: &str) -> Result<HashMap<String, String>, String> {
    let cred_manager = CredentialManager::new()
        .map_err(|e| format!("Failed to access credential store: {}", e))?;

    let token = cred_manager
        .get_token(registry_url)
        .map_err(|e| format!("Failed to retrieve token: {}", e))?
        .ok_or_else(|| "No token found. Run 'lumen registry login' first.".to_string())?;

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", token));

    Ok(headers)
}

/// Check if authenticated for a registry.
pub fn is_authenticated(registry_url: &str) -> bool {
    // Check env var first for CI/testing
    if std::env::var("LUMEN_AUTH_TOKEN").is_ok() {
        return true;
    }

    let cred_manager = match CredentialManager::new() {
        Ok(cm) => cm,
        Err(_) => return false,
    };

    cred_manager
        .get_token(registry_url)
        .ok()
        .flatten()
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_token() {
        assert_eq!(mask_token("lm_abc123xyz789"), "lm_abc...9789");
        assert_eq!(mask_token("short"), "***");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world this is long", 10), "hello w...");
    }

    #[test]
    fn test_get_effective_registry() {
        // This test may fail if no config exists, so we just test the provided case
        assert_eq!(
            get_effective_registry(Some("https://example.com".to_string())),
            "https://example.com"
        );
    }
}
