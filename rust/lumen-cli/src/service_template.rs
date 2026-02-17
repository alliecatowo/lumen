//! Service package template generator for Lumen.
//!
//! Scaffolds HTTP/API service projects with typed routes, schemas, and test fixtures.
//! Generates valid `.lm.md` source files using Lumen's type system (records, enums,
//! cells, effects, grants).

use std::collections::HashSet;
use std::fmt;

// =============================================================================
// Configuration Types
// =============================================================================

/// Top-level service template configuration.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Service project name (used for directory and package).
    pub name: String,
    /// Kind of service to generate.
    pub service_type: ServiceType,
    /// Port number the service listens on.
    pub port: u16,
    /// Route specifications.
    pub routes: Vec<RouteSpec>,
    /// Middleware stack.
    pub middleware: Vec<MiddlewareSpec>,
    /// Optional database configuration.
    pub database: Option<DatabaseConfig>,
}

/// Supported service types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    HttpApi,
    WebSocket,
    Grpc,
    GraphQl,
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceType::HttpApi => write!(f, "HTTP API"),
            ServiceType::WebSocket => write!(f, "WebSocket"),
            ServiceType::Grpc => write!(f, "gRPC"),
            ServiceType::GraphQl => write!(f, "GraphQL"),
        }
    }
}

/// A single route specification.
#[derive(Debug, Clone)]
pub struct RouteSpec {
    /// HTTP method.
    pub method: HttpMethod,
    /// URL path (must start with `/`).
    pub path: String,
    /// Handler cell name.
    pub handler: String,
    /// Optional request body type name.
    pub request_type: Option<String>,
    /// Optional response body type name.
    pub response_type: Option<String>,
    /// Human-readable description.
    pub description: String,
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Patch => write!(f, "PATCH"),
        }
    }
}

/// Middleware specification.
#[derive(Debug, Clone)]
pub struct MiddlewareSpec {
    /// Middleware name.
    pub name: String,
    /// Middleware kind.
    pub kind: MiddlewareKind,
}

/// Supported middleware kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiddlewareKind {
    Auth,
    Cors,
    RateLimit,
    Logging,
    Compression,
    Custom(String),
}

impl fmt::Display for MiddlewareKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MiddlewareKind::Auth => write!(f, "auth"),
            MiddlewareKind::Cors => write!(f, "cors"),
            MiddlewareKind::RateLimit => write!(f, "rate_limit"),
            MiddlewareKind::Logging => write!(f, "logging"),
            MiddlewareKind::Compression => write!(f, "compression"),
            MiddlewareKind::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Database configuration.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database engine.
    pub kind: DatabaseKind,
    /// Database name.
    pub name: String,
}

/// Supported database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    Sqlite,
    Postgres,
    InMemory,
}

impl fmt::Display for DatabaseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseKind::Sqlite => write!(f, "sqlite"),
            DatabaseKind::Postgres => write!(f, "postgres"),
            DatabaseKind::InMemory => write!(f, "in-memory"),
        }
    }
}

// =============================================================================
// Generated File
// =============================================================================

/// A generated file with its relative path and content.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Relative path from project root (e.g. `src/main.lm.md`).
    pub path: String,
    /// File content.
    pub content: String,
    /// Whether the file should be executable.
    pub is_executable: bool,
}

// =============================================================================
// Template Errors
// =============================================================================

/// Errors that can occur during template generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// General configuration error.
    InvalidConfig(String),
    /// Route path is malformed.
    InvalidRoutePath(String),
    /// Two routes share the same method+path.
    DuplicateRoute(String),
    /// Service/handler name is not a valid identifier.
    InvalidName(String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            TemplateError::InvalidRoutePath(msg) => write!(f, "invalid route path: {}", msg),
            TemplateError::DuplicateRoute(msg) => write!(f, "duplicate route: {}", msg),
            TemplateError::InvalidName(msg) => write!(f, "invalid name: {}", msg),
        }
    }
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Check if `name` is a valid Lumen identifier (snake_case cell name).
pub fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    // Reject reserved words used as identifiers
    !matches!(
        name,
        "cell"
            | "end"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "return"
            | "let"
            | "record"
            | "enum"
            | "effect"
            | "handle"
            | "perform"
            | "resume"
            | "grant"
            | "import"
            | "true"
            | "false"
            | "null"
            | "result"
            | "defer"
            | "yield"
            | "loop"
            | "break"
            | "continue"
            | "extern"
            | "comptime"
            | "when"
            | "process"
            | "machine"
            | "pipeline"
            | "memory"
    )
}

/// Check if `name` is a valid PascalCase type name.
pub fn is_valid_type_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if `name` is a valid Lumen service name (lowercase, hyphens, underscores).
pub fn is_valid_service_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Validate a route path segment. Must start with `/`, params use `:param`.
pub fn validate_route_path(path: &str) -> Result<(), TemplateError> {
    if path.is_empty() || !path.starts_with('/') {
        return Err(TemplateError::InvalidRoutePath(format!(
            "route path must start with '/': '{}'",
            path
        )));
    }

    // Check for duplicate parameter names
    let mut params = HashSet::new();
    for segment in path.split('/') {
        if let Some(param) = segment.strip_prefix(':') {
            if param.is_empty() {
                return Err(TemplateError::InvalidRoutePath(format!(
                    "empty parameter name in path '{}'",
                    path
                )));
            }
            if !params.insert(param.to_string()) {
                return Err(TemplateError::InvalidRoutePath(format!(
                    "duplicate parameter '{}' in path '{}'",
                    param, path
                )));
            }
        }
    }
    Ok(())
}

/// Validate the entire service config.
pub fn validate_config(config: &ServiceConfig) -> Result<(), TemplateError> {
    // Validate service name
    if !is_valid_service_name(&config.name) {
        return Err(TemplateError::InvalidName(format!(
            "invalid service name '{}': must be lowercase alphanumeric with hyphens/underscores",
            config.name
        )));
    }

    // Validate port
    if config.port == 0 {
        return Err(TemplateError::InvalidConfig(
            "port must be non-zero".to_string(),
        ));
    }

    // Validate each route
    for route in &config.routes {
        validate_route_path(&route.path)?;

        if !is_valid_identifier(&route.handler) {
            return Err(TemplateError::InvalidName(format!(
                "invalid handler name '{}': must be a valid Lumen identifier",
                route.handler
            )));
        }

        if let Some(ref req) = route.request_type {
            if !is_valid_type_name(req) {
                return Err(TemplateError::InvalidName(format!(
                    "invalid request type '{}': must be PascalCase",
                    req
                )));
            }
        }

        if let Some(ref resp) = route.response_type {
            if !is_valid_type_name(resp) {
                return Err(TemplateError::InvalidName(format!(
                    "invalid response type '{}': must be PascalCase",
                    resp
                )));
            }
        }
    }

    // Check for duplicate routes (same method + path)
    let mut seen = HashSet::new();
    for route in &config.routes {
        let key = format!("{} {}", route.method, route.path);
        if !seen.insert(key.clone()) {
            return Err(TemplateError::DuplicateRoute(key));
        }
    }

    // Validate database name if present
    if let Some(ref db) = config.database {
        if db.name.is_empty() {
            return Err(TemplateError::InvalidConfig(
                "database name must not be empty".to_string(),
            ));
        }
    }

    Ok(())
}

// =============================================================================
// ServiceTemplateGenerator
// =============================================================================

/// Generator that produces a complete service project scaffold.
pub struct ServiceTemplateGenerator;

impl ServiceTemplateGenerator {
    /// Generate a complete set of project files from a service configuration.
    pub fn generate(config: &ServiceConfig) -> Result<Vec<GeneratedFile>, TemplateError> {
        validate_config(config)?;

        let mut files = vec![
            GeneratedFile {
                path: "src/main.lm.md".to_string(),
                content: Self::generate_main(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "src/routes.lm.md".to_string(),
                content: Self::generate_routes(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "src/handlers.lm.md".to_string(),
                content: Self::generate_handlers(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "src/types.lm.md".to_string(),
                content: Self::generate_types(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "src/middleware.lm.md".to_string(),
                content: Self::generate_middleware(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "tests/service_test.lm.md".to_string(),
                content: Self::generate_tests(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "lumen.toml".to_string(),
                content: Self::generate_config_toml(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "Dockerfile".to_string(),
                content: Self::generate_dockerfile(config),
                is_executable: false,
            },
            GeneratedFile {
                path: "README.md".to_string(),
                content: Self::generate_readme(config),
                is_executable: false,
            },
        ];

        // Database migration file
        if config.database.is_some() {
            files.push(GeneratedFile {
                path: "src/db.lm.md".to_string(),
                content: Self::generate_db_module(config),
                is_executable: false,
            });
        }

        Ok(files)
    }

    /// Generate the main entry point (`src/main.lm.md`).
    pub fn generate_main(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Main Entry Point\n\n", config.name));
        out.push_str(&format!(
            "Service type: {} on port {}.\n\n",
            config.service_type, config.port
        ));

        out.push_str("```lumen\n");
        // Imports
        out.push_str("import routes: *\n");
        out.push_str("import handlers: *\n");
        out.push_str("import types: *\n");
        out.push_str("import middleware: *\n");
        if config.database.is_some() {
            out.push_str("import db: *\n");
        }
        out.push('\n');

        // Server config record
        out.push_str("record ServerConfig\n");
        out.push_str("  port: Int\n");
        out.push_str("  host: String\n");
        out.push_str("end\n\n");

        // Main cell with effects
        let effects = Self::collect_effects(config);
        let effect_row = if effects.is_empty() {
            String::new()
        } else {
            format!(" / {{{}}}", effects.join(", "))
        };

        out.push_str(&format!("cell main() -> Int{}\n", effect_row));
        out.push_str(&format!(
            "  let config = ServerConfig(port: {}, host: \"0.0.0.0\")\n",
            config.port
        ));
        out.push_str("  print(\"Starting server on {config.host}:{config.port}\")\n");

        // Apply middleware
        if !config.middleware.is_empty() {
            out.push_str("  let app = apply_middleware()\n");
            out.push_str("  print(\"Middleware applied: {app}\")\n");
        }

        // Register routes
        if !config.routes.is_empty() {
            out.push_str("  let router = register_routes()\n");
            out.push_str("  print(\"Routes registered: {router}\")\n");
        }

        out.push_str("  return 0\n");
        out.push_str("end\n");
        out.push_str("```\n");
        out
    }

    /// Generate the routes module (`src/routes.lm.md`).
    pub fn generate_routes(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Route Definitions\n\n", config.name));
        out.push_str("Route table mapping HTTP methods and paths to handler cells.\n\n");

        out.push_str("```lumen\n");

        // Route record
        out.push_str("record Route\n");
        out.push_str("  method: String\n");
        out.push_str("  path: String\n");
        out.push_str("  handler_name: String\n");
        out.push_str("  description: String\n");
        out.push_str("end\n\n");

        // register_routes cell
        out.push_str("cell register_routes() -> list[Route]\n");
        if config.routes.is_empty() {
            out.push_str("  return []\n");
        } else {
            out.push_str("  let routes = [\n");
            for (i, route) in config.routes.iter().enumerate() {
                let comma = if i + 1 < config.routes.len() { "," } else { "" };
                out.push_str(&format!(
                    "    Route(method: \"{}\", path: \"{}\", handler_name: \"{}\", description: \"{}\"){}\n",
                    route.method, route.path, route.handler, route.description, comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("  return routes\n");
        }
        out.push_str("end\n");

        out.push_str("```\n");
        out
    }

    /// Generate handler cells (`src/handlers.lm.md`).
    pub fn generate_handlers(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Request Handlers\n\n", config.name));
        out.push_str("Handler cells for each route.\n\n");

        out.push_str("```lumen\n");
        out.push_str("import types: *\n\n");

        // Grant for HTTP tool access
        out.push_str("grant HttpResponse to http_respond\n");
        out.push_str("  domain: \"*\"\n");
        out.push_str("end\n\n");

        if config.routes.is_empty() {
            out.push_str("# No routes configured.\n");
        }

        for route in &config.routes {
            let req_param = if let Some(ref req_type) = route.request_type {
                format!("request: {}", req_type)
            } else {
                String::new()
            };

            let ret_type = if let Some(ref resp_type) = route.response_type {
                resp_type.clone()
            } else {
                "String".to_string()
            };

            // Effect row for handlers doing I/O
            let effect_row = " / {http}";

            out.push_str(&format!(
                "# {} {} — {}\n",
                route.method, route.path, route.description
            ));
            out.push_str(&format!(
                "cell {}({}) -> {}{}\n",
                route.handler, req_param, ret_type, effect_row
            ));

            // Default body
            if let Some(ref resp_type) = route.response_type {
                // Build default record construction
                out.push_str(&format!("  # TODO: implement {} handler\n", route.handler));
                out.push_str(&format!(
                    "  let response = {}()\n",
                    Self::default_constructor(resp_type)
                ));
                out.push_str("  return response\n");
            } else {
                out.push_str(&format!(
                    "  return \"{} {} handler\"\n",
                    route.method, route.path
                ));
            }

            out.push_str("end\n\n");
        }

        out.push_str("```\n");
        out
    }

    /// Generate type definitions (`src/types.lm.md`).
    pub fn generate_types(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Type Definitions\n\n", config.name));
        out.push_str("Shared records, enums, and type aliases.\n\n");

        out.push_str("```lumen\n");

        // Status enum
        out.push_str("enum HttpStatus\n");
        out.push_str("  Ok\n");
        out.push_str("  Created\n");
        out.push_str("  BadRequest\n");
        out.push_str("  NotFound\n");
        out.push_str("  InternalError\n");
        out.push_str("end\n\n");

        // Response wrapper
        out.push_str("record ApiResponse\n");
        out.push_str("  status: HttpStatus\n");
        out.push_str("  body: String\n");
        out.push_str("end\n\n");

        // Error record
        out.push_str("record ApiError\n");
        out.push_str("  code: Int\n");
        out.push_str("  message: String\n");
        out.push_str("end\n\n");

        // Collect unique request/response type names
        let mut type_names: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for route in &config.routes {
            if let Some(ref t) = route.request_type {
                if seen.insert(t.clone()) {
                    type_names.push(t.clone());
                }
            }
            if let Some(ref t) = route.response_type {
                if seen.insert(t.clone()) {
                    type_names.push(t.clone());
                }
            }
        }

        for type_name in &type_names {
            out.push_str(&format!("record {}\n", type_name));
            out.push_str("  id: String\n");
            out.push_str("  data: String\n");
            out.push_str("end\n\n");
        }

        // Database model if configured
        if let Some(ref db) = config.database {
            out.push_str(&format!("# Database model for {} ({})\n", db.name, db.kind));
            out.push_str(&format!("record {}Model\n", Self::to_pascal_case(&db.name)));
            out.push_str("  id: String\n");
            out.push_str("  created_at: String\n");
            out.push_str("  updated_at: String\n");
            out.push_str("end\n\n");
        }

        out.push_str("```\n");
        out
    }

    /// Generate the middleware module (`src/middleware.lm.md`).
    pub fn generate_middleware(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Middleware\n\n", config.name));
        out.push_str("Middleware stack for request/response processing.\n\n");

        out.push_str("```lumen\n");

        // Middleware record
        out.push_str("record MiddlewareEntry\n");
        out.push_str("  name: String\n");
        out.push_str("  enabled: Bool\n");
        out.push_str("end\n\n");

        // Individual middleware cells
        for mw in &config.middleware {
            out.push_str(&format!(
                "cell {}_middleware(request: String) -> String\n",
                mw.kind
            ));
            out.push_str(&format!("  # {} middleware: {}\n", mw.kind, mw.name));
            out.push_str("  return request\n");
            out.push_str("end\n\n");
        }

        // apply_middleware cell
        out.push_str("cell apply_middleware() -> list[MiddlewareEntry]\n");
        if config.middleware.is_empty() {
            out.push_str("  return []\n");
        } else {
            out.push_str("  let stack = [\n");
            for (i, mw) in config.middleware.iter().enumerate() {
                let comma = if i + 1 < config.middleware.len() {
                    ","
                } else {
                    ""
                };
                out.push_str(&format!(
                    "    MiddlewareEntry(name: \"{}\", enabled: true){}\n",
                    mw.name, comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("  return stack\n");
        }
        out.push_str("end\n");

        out.push_str("```\n");
        out
    }

    /// Generate test fixtures (`tests/service_test.lm.md`).
    pub fn generate_tests(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} — Tests\n\n", config.name));
        out.push_str("Test suite for the service.\n\n");

        out.push_str("```lumen\n");
        out.push_str("import types: *\n");
        out.push_str("import handlers: *\n\n");

        // A test cell for each handler
        for route in &config.routes {
            out.push_str(&format!("cell test_{}() -> Bool\n", route.handler));

            if let Some(ref req_type) = route.request_type {
                out.push_str(&format!(
                    "  let req = {}(id: \"test-1\", data: \"test-data\")\n",
                    req_type
                ));
                out.push_str(&format!("  let resp = {}(req)\n", route.handler));
            } else {
                out.push_str(&format!("  let resp = {}()\n", route.handler));
            }

            out.push_str("  print(\"Test {}: passed\")\n");
            out.push_str("  return true\n");
            out.push_str("end\n\n");
        }

        // Health-check test
        out.push_str("cell test_health() -> Bool\n");
        out.push_str("  print(\"Health check: ok\")\n");
        out.push_str("  return true\n");
        out.push_str("end\n\n");

        // main test runner
        out.push_str("cell main() -> Int\n");
        out.push_str("  print(\"Running service tests...\")\n");
        for route in &config.routes {
            out.push_str(&format!("  test_{}()\n", route.handler));
        }
        out.push_str("  test_health()\n");
        out.push_str("  print(\"All tests passed.\")\n");
        out.push_str("  return 0\n");
        out.push_str("end\n");

        out.push_str("```\n");
        out
    }

    /// Generate `lumen.toml` project configuration.
    pub fn generate_config_toml(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str("# Lumen Service Configuration\n\n");

        out.push_str("[package]\n");
        out.push_str(&format!("name = \"@service/{}\"\n", config.name));
        out.push_str("version = \"0.1.0\"\n");
        out.push_str(&format!(
            "description = \"{} service built with Lumen\"\n",
            config.service_type
        ));
        out.push_str("license = \"MIT\"\n");
        out.push_str("edition = \"2024\"\n\n");

        out.push_str("[package.exports]\n");
        out.push_str("default = \"main\"\n\n");

        out.push_str("[toolchain]\n");
        out.push_str("lumen = \">=0.1.0\"\n\n");

        out.push_str("[features]\n");
        out.push_str("default = []\n\n");

        out.push_str("[dependencies]\n\n");
        out.push_str("[dev-dependencies]\n\n");

        if let Some(ref db) = config.database {
            out.push_str("[providers]\n");
            out.push_str(&format!("\"db.{}\" = \"{}\"\n", db.kind, db.name));
        }

        out
    }

    /// Generate a Dockerfile for the service.
    pub fn generate_dockerfile(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str("# Dockerfile for Lumen service\n");
        out.push_str("FROM lumen/runtime:latest\n\n");
        out.push_str("WORKDIR /app\n\n");
        out.push_str("COPY lumen.toml .\n");
        out.push_str("COPY src/ ./src/\n");
        out.push_str("COPY tests/ ./tests/\n\n");
        out.push_str("RUN lumen pkg build\n\n");
        out.push_str(&format!("EXPOSE {}\n\n", config.port));
        out.push_str("CMD [\"lumen\", \"run\", \"src/main.lm.md\"]\n");
        out
    }

    /// Generate a README for the service.
    pub fn generate_readme(config: &ServiceConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", config.name));
        out.push_str(&format!(
            "A {} service built with [Lumen](https://lumen-lang.com).\n\n",
            config.service_type
        ));

        out.push_str("## Getting Started\n\n");
        out.push_str("```bash\n");
        out.push_str("lumen run src/main.lm.md\n");
        out.push_str("```\n\n");

        out.push_str("## Routes\n\n");
        out.push_str("| Method | Path | Handler | Description |\n");
        out.push_str("|--------|------|---------|-------------|\n");
        for route in &config.routes {
            out.push_str(&format!(
                "| {} | `{}` | `{}` | {} |\n",
                route.method, route.path, route.handler, route.description
            ));
        }
        out.push('\n');

        if !config.middleware.is_empty() {
            out.push_str("## Middleware\n\n");
            for mw in &config.middleware {
                out.push_str(&format!("- **{}** ({})\n", mw.name, mw.kind));
            }
            out.push('\n');
        }

        if let Some(ref db) = config.database {
            out.push_str("## Database\n\n");
            out.push_str(&format!("- Engine: {}\n- Name: {}\n\n", db.kind, db.name));
        }

        out.push_str("## Testing\n\n");
        out.push_str("```bash\n");
        out.push_str("lumen run tests/service_test.lm.md\n");
        out.push_str("```\n\n");

        out.push_str("## Docker\n\n");
        out.push_str("```bash\n");
        out.push_str(&format!("docker build -t {} .\n", config.name));
        out.push_str(&format!(
            "docker run -p {}:{} {}\n",
            config.port, config.port, config.name
        ));
        out.push_str("```\n");
        out
    }

    // =========================================================================
    // Preset Templates
    // =========================================================================

    /// Create a REST API preset config.
    pub fn rest_api(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            service_type: ServiceType::HttpApi,
            port: 8080,
            routes: vec![
                RouteSpec {
                    method: HttpMethod::Get,
                    path: "/health".to_string(),
                    handler: "health_check".to_string(),
                    request_type: None,
                    response_type: Some("HealthResponse".to_string()),
                    description: "Health check endpoint".to_string(),
                },
                RouteSpec {
                    method: HttpMethod::Get,
                    path: "/api/items".to_string(),
                    handler: "list_items".to_string(),
                    request_type: None,
                    response_type: Some("ItemListResponse".to_string()),
                    description: "List all items".to_string(),
                },
                RouteSpec {
                    method: HttpMethod::Post,
                    path: "/api/items".to_string(),
                    handler: "create_item".to_string(),
                    request_type: Some("CreateItemRequest".to_string()),
                    response_type: Some("ItemResponse".to_string()),
                    description: "Create a new item".to_string(),
                },
                RouteSpec {
                    method: HttpMethod::Get,
                    path: "/api/items/:id".to_string(),
                    handler: "get_item".to_string(),
                    request_type: None,
                    response_type: Some("ItemResponse".to_string()),
                    description: "Get item by ID".to_string(),
                },
            ],
            middleware: vec![
                MiddlewareSpec {
                    name: "request_logger".to_string(),
                    kind: MiddlewareKind::Logging,
                },
                MiddlewareSpec {
                    name: "cors_policy".to_string(),
                    kind: MiddlewareKind::Cors,
                },
            ],
            database: None,
        }
    }

    /// Create a WebSocket server preset config.
    pub fn websocket_server(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            service_type: ServiceType::WebSocket,
            port: 8081,
            routes: vec![
                RouteSpec {
                    method: HttpMethod::Get,
                    path: "/ws".to_string(),
                    handler: "ws_connect".to_string(),
                    request_type: None,
                    response_type: Some("WsResponse".to_string()),
                    description: "WebSocket connection endpoint".to_string(),
                },
                RouteSpec {
                    method: HttpMethod::Get,
                    path: "/health".to_string(),
                    handler: "health_check".to_string(),
                    request_type: None,
                    response_type: Some("HealthResponse".to_string()),
                    description: "Health check endpoint".to_string(),
                },
            ],
            middleware: vec![MiddlewareSpec {
                name: "ws_auth".to_string(),
                kind: MiddlewareKind::Auth,
            }],
            database: None,
        }
    }

    /// Create a CRUD service preset config for a named resource.
    pub fn crud_service(name: &str, resource: &str) -> ServiceConfig {
        let pascal = Self::to_pascal_case(resource);
        let lower = resource.to_ascii_lowercase();
        ServiceConfig {
            name: name.to_string(),
            service_type: ServiceType::HttpApi,
            port: 8080,
            routes: vec![
                RouteSpec {
                    method: HttpMethod::Get,
                    path: format!("/api/{}", lower),
                    handler: format!("list_{}", lower),
                    request_type: None,
                    response_type: Some(format!("{}ListResponse", pascal)),
                    description: format!("List all {}s", lower),
                },
                RouteSpec {
                    method: HttpMethod::Post,
                    path: format!("/api/{}", lower),
                    handler: format!("create_{}", lower),
                    request_type: Some(format!("Create{}Request", pascal)),
                    response_type: Some(format!("{}Response", pascal)),
                    description: format!("Create a new {}", lower),
                },
                RouteSpec {
                    method: HttpMethod::Get,
                    path: format!("/api/{}/:id", lower),
                    handler: format!("get_{}", lower),
                    request_type: None,
                    response_type: Some(format!("{}Response", pascal)),
                    description: format!("Get {} by ID", lower),
                },
                RouteSpec {
                    method: HttpMethod::Put,
                    path: format!("/api/{}/:id", lower),
                    handler: format!("update_{}", lower),
                    request_type: Some(format!("Update{}Request", pascal)),
                    response_type: Some(format!("{}Response", pascal)),
                    description: format!("Update {} by ID", lower),
                },
                RouteSpec {
                    method: HttpMethod::Delete,
                    path: format!("/api/{}/:id", lower),
                    handler: format!("delete_{}", lower),
                    request_type: None,
                    response_type: Some(format!("{}Response", pascal)),
                    description: format!("Delete {} by ID", lower),
                },
            ],
            middleware: vec![
                MiddlewareSpec {
                    name: "request_logger".to_string(),
                    kind: MiddlewareKind::Logging,
                },
                MiddlewareSpec {
                    name: "cors_policy".to_string(),
                    kind: MiddlewareKind::Cors,
                },
                MiddlewareSpec {
                    name: "rate_limiter".to_string(),
                    kind: MiddlewareKind::RateLimit,
                },
            ],
            database: Some(DatabaseConfig {
                kind: DatabaseKind::Sqlite,
                name: format!("{}_db", lower),
            }),
        }
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Convert a snake_case or lowercase string to PascalCase.
    pub fn to_pascal_case(s: &str) -> String {
        s.split(['_', '-'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let mut word = first.to_uppercase().to_string();
                        word.extend(chars);
                        word
                    }
                }
            })
            .collect()
    }

    /// Collect effect names used in the config.
    fn collect_effects(config: &ServiceConfig) -> Vec<String> {
        let mut effects = Vec::new();
        if !config.routes.is_empty() {
            effects.push("http".to_string());
        }
        if config.database.is_some() {
            effects.push("db".to_string());
        }
        for mw in &config.middleware {
            match mw.kind {
                MiddlewareKind::Logging => {
                    if !effects.contains(&"trace".to_string()) {
                        effects.push("trace".to_string());
                    }
                }
                MiddlewareKind::Auth => {
                    if !effects.contains(&"auth".to_string()) {
                        effects.push("auth".to_string());
                    }
                }
                _ => {}
            }
        }
        effects
    }

    /// Build a default constructor expression for a type name.
    fn default_constructor(type_name: &str) -> String {
        format!("{}(id: \"default\", data: \"\")", type_name)
    }

    /// Generate database module.
    fn generate_db_module(config: &ServiceConfig) -> String {
        let db = match &config.database {
            Some(db) => db,
            None => return String::new(),
        };

        let mut out = String::new();
        out.push_str(&format!("# {} — Database Module\n\n", config.name));
        out.push_str(&format!(
            "Database layer for {} ({}).\n\n",
            db.name, db.kind
        ));

        out.push_str("```lumen\n");
        out.push_str("import types: *\n\n");

        out.push_str(&format!("grant DbAccess to db_{}\n", db.kind));
        out.push_str(&format!("  domain: \"{}\"\n", db.name));
        out.push_str("end\n\n");

        let model_name = format!("{}Model", Self::to_pascal_case(&db.name));

        out.push_str("cell db_connect() -> String / {db}\n");
        out.push_str(&format!("  return \"connected to {}\"\n", db.name));
        out.push_str("end\n\n");

        out.push_str(&format!(
            "cell db_find_all() -> list[{}] / {{db}}\n",
            model_name
        ));
        out.push_str("  return []\n");
        out.push_str("end\n\n");

        out.push_str(&format!(
            "cell db_find_by_id(id: String) -> {} / {{db}}\n",
            model_name
        ));
        out.push_str(&format!(
            "  return {}(id: id, created_at: \"\", updated_at: \"\")\n",
            model_name
        ));
        out.push_str("end\n");

        out.push_str("```\n");
        out
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config() -> ServiceConfig {
        ServiceConfig {
            name: "test-svc".to_string(),
            service_type: ServiceType::HttpApi,
            port: 3000,
            routes: vec![],
            middleware: vec![],
            database: None,
        }
    }

    #[test]
    fn test_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("hello_world"));
        assert!(is_valid_identifier("_private"));
        assert!(is_valid_identifier("x2"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("Foo"));
        assert!(!is_valid_identifier("123"));
        assert!(!is_valid_identifier("cell"));
        assert!(!is_valid_identifier("return"));
    }

    #[test]
    fn test_valid_type_name() {
        assert!(is_valid_type_name("Foo"));
        assert!(is_valid_type_name("MyType"));
        assert!(is_valid_type_name("A"));
        assert!(!is_valid_type_name(""));
        assert!(!is_valid_type_name("foo"));
        assert!(!is_valid_type_name("123"));
    }

    #[test]
    fn test_valid_service_name() {
        assert!(is_valid_service_name("my-svc"));
        assert!(is_valid_service_name("api"));
        assert!(is_valid_service_name("web-app-2"));
        assert!(!is_valid_service_name(""));
        assert!(!is_valid_service_name("MyService"));
        assert!(!is_valid_service_name("-bad"));
    }

    #[test]
    fn test_validate_route_path_ok() {
        assert!(validate_route_path("/").is_ok());
        assert!(validate_route_path("/api/items").is_ok());
        assert!(validate_route_path("/api/items/:id").is_ok());
        assert!(validate_route_path("/api/:org/:repo").is_ok());
    }

    #[test]
    fn test_validate_route_path_missing_slash() {
        assert!(validate_route_path("").is_err());
        assert!(validate_route_path("api/items").is_err());
    }

    #[test]
    fn test_validate_route_path_duplicate_param() {
        let r = validate_route_path("/api/:id/sub/:id");
        assert!(r.is_err());
        match r {
            Err(TemplateError::InvalidRoutePath(msg)) => {
                assert!(msg.contains("duplicate"));
            }
            _ => panic!("expected InvalidRoutePath"),
        }
    }

    #[test]
    fn test_validate_route_path_empty_param() {
        let r = validate_route_path("/api/:/other");
        assert!(r.is_err());
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(ServiceTemplateGenerator::to_pascal_case("hello"), "Hello");
        assert_eq!(
            ServiceTemplateGenerator::to_pascal_case("hello_world"),
            "HelloWorld"
        );
        assert_eq!(
            ServiceTemplateGenerator::to_pascal_case("my-service"),
            "MyService"
        );
        assert_eq!(ServiceTemplateGenerator::to_pascal_case("a"), "A");
        assert_eq!(ServiceTemplateGenerator::to_pascal_case(""), "");
    }

    #[test]
    fn test_generate_minimal() {
        let config = minimal_config();
        let files = ServiceTemplateGenerator::generate(&config).unwrap();
        assert!(files.len() >= 9);
    }

    #[test]
    fn test_generate_main_content() {
        let config = minimal_config();
        let main = ServiceTemplateGenerator::generate_main(&config);
        assert!(main.contains("```lumen"));
        assert!(main.contains("cell main()"));
        assert!(main.contains("ServerConfig"));
        assert!(main.contains("port: 3000"));
    }

    #[test]
    fn test_generate_routes_content() {
        let mut config = minimal_config();
        config.routes.push(RouteSpec {
            method: HttpMethod::Get,
            path: "/health".to_string(),
            handler: "health_check".to_string(),
            request_type: None,
            response_type: None,
            description: "Health check".to_string(),
        });
        let routes = ServiceTemplateGenerator::generate_routes(&config);
        assert!(routes.contains("record Route"));
        assert!(routes.contains("cell register_routes()"));
        assert!(routes.contains("/health"));
    }

    #[test]
    fn test_generate_handlers_content() {
        let mut config = minimal_config();
        config.routes.push(RouteSpec {
            method: HttpMethod::Post,
            path: "/api/items".to_string(),
            handler: "create_item".to_string(),
            request_type: Some("CreateRequest".to_string()),
            response_type: Some("ItemResponse".to_string()),
            description: "Create item".to_string(),
        });
        let handlers = ServiceTemplateGenerator::generate_handlers(&config);
        assert!(handlers.contains("cell create_item(request: CreateRequest)"));
        assert!(handlers.contains("grant HttpResponse"));
    }

    #[test]
    fn test_generate_types_content() {
        let config = minimal_config();
        let types = ServiceTemplateGenerator::generate_types(&config);
        assert!(types.contains("enum HttpStatus"));
        assert!(types.contains("record ApiResponse"));
        assert!(types.contains("record ApiError"));
    }

    #[test]
    fn test_generate_middleware_content() {
        let mut config = minimal_config();
        config.middleware.push(MiddlewareSpec {
            name: "cors_policy".to_string(),
            kind: MiddlewareKind::Cors,
        });
        let mw = ServiceTemplateGenerator::generate_middleware(&config);
        assert!(mw.contains("record MiddlewareEntry"));
        assert!(mw.contains("cell apply_middleware()"));
        assert!(mw.contains("cors_policy"));
    }

    #[test]
    fn test_generate_tests_content() {
        let mut config = minimal_config();
        config.routes.push(RouteSpec {
            method: HttpMethod::Get,
            path: "/health".to_string(),
            handler: "health_check".to_string(),
            request_type: None,
            response_type: None,
            description: "Health check".to_string(),
        });
        let tests = ServiceTemplateGenerator::generate_tests(&config);
        assert!(tests.contains("cell test_health_check()"));
        assert!(tests.contains("cell test_health()"));
        assert!(tests.contains("cell main()"));
    }

    #[test]
    fn test_generate_config_toml() {
        let config = minimal_config();
        let toml = ServiceTemplateGenerator::generate_config_toml(&config);
        assert!(toml.contains("[package]"));
        assert!(toml.contains("@service/test-svc"));
        assert!(toml.contains("version = \"0.1.0\""));
    }

    #[test]
    fn test_generate_dockerfile() {
        let config = minimal_config();
        let df = ServiceTemplateGenerator::generate_dockerfile(&config);
        assert!(df.contains("FROM lumen/runtime:latest"));
        assert!(df.contains("EXPOSE 3000"));
    }

    #[test]
    fn test_generate_readme() {
        let mut config = minimal_config();
        config.routes.push(RouteSpec {
            method: HttpMethod::Get,
            path: "/health".to_string(),
            handler: "health_check".to_string(),
            request_type: None,
            response_type: None,
            description: "Health check".to_string(),
        });
        let readme = ServiceTemplateGenerator::generate_readme(&config);
        assert!(readme.contains("# test-svc"));
        assert!(readme.contains("## Routes"));
        assert!(readme.contains("health_check"));
    }
}
