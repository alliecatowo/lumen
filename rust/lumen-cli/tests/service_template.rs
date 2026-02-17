//! Integration tests for the service template generator (wave24).

use lumen_cli::service_template::*;

// =============================================================================
// Validation: is_valid_identifier
// =============================================================================

#[test]
fn service_identifier_simple() {
    assert!(is_valid_identifier("foo"));
}

#[test]
fn service_identifier_snake_case() {
    assert!(is_valid_identifier("hello_world"));
}

#[test]
fn service_identifier_leading_underscore() {
    assert!(is_valid_identifier("_private"));
}

#[test]
fn service_identifier_with_digits() {
    assert!(is_valid_identifier("x2"));
    assert!(is_valid_identifier("handler_v3"));
}

#[test]
fn service_identifier_reject_empty() {
    assert!(!is_valid_identifier(""));
}

#[test]
fn service_identifier_reject_uppercase() {
    assert!(!is_valid_identifier("Foo"));
    assert!(!is_valid_identifier("ALLCAPS"));
}

#[test]
fn service_identifier_reject_digit_start() {
    assert!(!is_valid_identifier("123"));
    assert!(!is_valid_identifier("3abc"));
}

#[test]
fn service_identifier_reject_reserved() {
    assert!(!is_valid_identifier("cell"));
    assert!(!is_valid_identifier("return"));
    assert!(!is_valid_identifier("end"));
    assert!(!is_valid_identifier("if"));
    assert!(!is_valid_identifier("match"));
    assert!(!is_valid_identifier("record"));
    assert!(!is_valid_identifier("enum"));
    assert!(!is_valid_identifier("effect"));
    assert!(!is_valid_identifier("grant"));
    assert!(!is_valid_identifier("let"));
    assert!(!is_valid_identifier("true"));
    assert!(!is_valid_identifier("false"));
    assert!(!is_valid_identifier("null"));
    assert!(!is_valid_identifier("result"));
}

#[test]
fn service_identifier_reject_special_chars() {
    assert!(!is_valid_identifier("foo-bar"));
    assert!(!is_valid_identifier("foo.bar"));
    assert!(!is_valid_identifier("foo bar"));
}

// =============================================================================
// Validation: is_valid_type_name
// =============================================================================

#[test]
fn service_type_name_pascal() {
    assert!(is_valid_type_name("Foo"));
    assert!(is_valid_type_name("MyType"));
    assert!(is_valid_type_name("CreateItemRequest"));
}

#[test]
fn service_type_name_single_char() {
    assert!(is_valid_type_name("A"));
    assert!(is_valid_type_name("Z"));
}

#[test]
fn service_type_name_reject_lowercase() {
    assert!(!is_valid_type_name("foo"));
    assert!(!is_valid_type_name("myType"));
}

#[test]
fn service_type_name_reject_empty() {
    assert!(!is_valid_type_name(""));
}

// =============================================================================
// Validation: is_valid_service_name
// =============================================================================

#[test]
fn service_name_simple() {
    assert!(is_valid_service_name("api"));
    assert!(is_valid_service_name("my-svc"));
    assert!(is_valid_service_name("web-app-2"));
    assert!(is_valid_service_name("test_service"));
}

#[test]
fn service_name_reject_uppercase() {
    assert!(!is_valid_service_name("MyService"));
}

#[test]
fn service_name_reject_empty() {
    assert!(!is_valid_service_name(""));
}

#[test]
fn service_name_reject_start_dash() {
    assert!(!is_valid_service_name("-bad"));
}

#[test]
fn service_name_reject_too_long() {
    let long = "a".repeat(65);
    assert!(!is_valid_service_name(&long));
    // Exactly 64 should be fine
    let ok = "a".repeat(64);
    assert!(is_valid_service_name(&ok));
}

// =============================================================================
// Validation: validate_route_path
// =============================================================================

#[test]
fn service_route_path_ok() {
    assert!(validate_route_path("/").is_ok());
    assert!(validate_route_path("/api/items").is_ok());
    assert!(validate_route_path("/api/items/:id").is_ok());
    assert!(validate_route_path("/api/:org/:repo").is_ok());
}

#[test]
fn service_route_path_no_slash() {
    let r = validate_route_path("api/items");
    assert!(matches!(r, Err(TemplateError::InvalidRoutePath(_))));
}

#[test]
fn service_route_path_empty() {
    let r = validate_route_path("");
    assert!(matches!(r, Err(TemplateError::InvalidRoutePath(_))));
}

#[test]
fn service_route_path_duplicate_param() {
    let r = validate_route_path("/api/:id/sub/:id");
    assert!(matches!(r, Err(TemplateError::InvalidRoutePath(_))));
    if let Err(TemplateError::InvalidRoutePath(msg)) = r {
        assert!(msg.contains("duplicate"));
    }
}

#[test]
fn service_route_path_empty_param() {
    let r = validate_route_path("/api/:/other");
    assert!(matches!(r, Err(TemplateError::InvalidRoutePath(_))));
}

// =============================================================================
// Validation: validate_config
// =============================================================================

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
fn service_validate_minimal() {
    assert!(validate_config(&minimal_config()).is_ok());
}

#[test]
fn service_validate_bad_name() {
    let mut c = minimal_config();
    c.name = "BadName".to_string();
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidName(_))
    ));
}

#[test]
fn service_validate_zero_port() {
    let mut c = minimal_config();
    c.port = 0;
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidConfig(_))
    ));
}

#[test]
fn service_validate_bad_handler() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Get,
        path: "/test".to_string(),
        handler: "BadHandler".to_string(),
        request_type: None,
        response_type: None,
        description: "test".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidName(_))
    ));
}

#[test]
fn service_validate_bad_request_type() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Post,
        path: "/test".to_string(),
        handler: "test_handler".to_string(),
        request_type: Some("bad_type".to_string()),
        response_type: None,
        description: "test".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidName(_))
    ));
}

#[test]
fn service_validate_bad_response_type() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Get,
        path: "/test".to_string(),
        handler: "test_handler".to_string(),
        request_type: None,
        response_type: Some("bad_type".to_string()),
        description: "test".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidName(_))
    ));
}

#[test]
fn service_validate_duplicate_routes() {
    let mut c = minimal_config();
    let route = RouteSpec {
        method: HttpMethod::Get,
        path: "/api/items".to_string(),
        handler: "list_items".to_string(),
        request_type: None,
        response_type: None,
        description: "list".to_string(),
    };
    c.routes.push(route.clone());
    c.routes.push(RouteSpec {
        method: HttpMethod::Get,
        path: "/api/items".to_string(),
        handler: "list_items_v2".to_string(),
        request_type: None,
        response_type: None,
        description: "list v2".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::DuplicateRoute(_))
    ));
}

#[test]
fn service_validate_same_path_diff_method() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Get,
        path: "/api/items".to_string(),
        handler: "list_items".to_string(),
        request_type: None,
        response_type: None,
        description: "list".to_string(),
    });
    c.routes.push(RouteSpec {
        method: HttpMethod::Post,
        path: "/api/items".to_string(),
        handler: "create_item".to_string(),
        request_type: Some("CreateRequest".to_string()),
        response_type: None,
        description: "create".to_string(),
    });
    assert!(validate_config(&c).is_ok());
}

#[test]
fn service_validate_empty_db_name() {
    let mut c = minimal_config();
    c.database = Some(DatabaseConfig {
        kind: DatabaseKind::Sqlite,
        name: "".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidConfig(_))
    ));
}

// =============================================================================
// Preset: rest_api
// =============================================================================

#[test]
fn service_rest_api_preset() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    assert_eq!(config.name, "my-api");
    assert_eq!(config.service_type, ServiceType::HttpApi);
    assert_eq!(config.port, 8080);
    assert_eq!(config.routes.len(), 4);
    assert_eq!(config.middleware.len(), 2);
    assert!(config.database.is_none());
}

#[test]
fn service_rest_api_validates() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    assert!(validate_config(&config).is_ok());
}

#[test]
fn service_rest_api_generates() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let files = ServiceTemplateGenerator::generate(&config).unwrap();
    assert!(files.len() >= 9);
    // Check file paths
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"src/main.lm.md"));
    assert!(paths.contains(&"src/routes.lm.md"));
    assert!(paths.contains(&"src/handlers.lm.md"));
    assert!(paths.contains(&"src/types.lm.md"));
    assert!(paths.contains(&"src/middleware.lm.md"));
    assert!(paths.contains(&"tests/service_test.lm.md"));
    assert!(paths.contains(&"lumen.toml"));
    assert!(paths.contains(&"Dockerfile"));
    assert!(paths.contains(&"README.md"));
}

// =============================================================================
// Preset: websocket_server
// =============================================================================

#[test]
fn service_ws_preset() {
    let config = ServiceTemplateGenerator::websocket_server("ws-server");
    assert_eq!(config.name, "ws-server");
    assert_eq!(config.service_type, ServiceType::WebSocket);
    assert_eq!(config.port, 8081);
    assert_eq!(config.routes.len(), 2);
    assert_eq!(config.middleware.len(), 1);
}

#[test]
fn service_ws_validates() {
    let config = ServiceTemplateGenerator::websocket_server("ws-server");
    assert!(validate_config(&config).is_ok());
}

// =============================================================================
// Preset: crud_service
// =============================================================================

#[test]
fn service_crud_preset() {
    let config = ServiceTemplateGenerator::crud_service("user-svc", "user");
    assert_eq!(config.name, "user-svc");
    assert_eq!(config.routes.len(), 5);
    assert!(config.database.is_some());
    let db = config.database.as_ref().unwrap();
    assert_eq!(db.kind, DatabaseKind::Sqlite);
    assert_eq!(db.name, "user_db");
}

#[test]
fn service_crud_validates() {
    let config = ServiceTemplateGenerator::crud_service("user-svc", "user");
    assert!(validate_config(&config).is_ok());
}

#[test]
fn service_crud_generates_db_module() {
    let config = ServiceTemplateGenerator::crud_service("user-svc", "user");
    let files = ServiceTemplateGenerator::generate(&config).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"src/db.lm.md"));
}

#[test]
fn service_crud_route_names() {
    let config = ServiceTemplateGenerator::crud_service("item-svc", "item");
    let handlers: Vec<&str> = config.routes.iter().map(|r| r.handler.as_str()).collect();
    assert!(handlers.contains(&"list_item"));
    assert!(handlers.contains(&"create_item"));
    assert!(handlers.contains(&"get_item"));
    assert!(handlers.contains(&"update_item"));
    assert!(handlers.contains(&"delete_item"));
}

// =============================================================================
// Generator: generate_main
// =============================================================================

#[test]
fn service_main_has_lumen_block() {
    let config = minimal_config();
    let main = ServiceTemplateGenerator::generate_main(&config);
    assert!(main.contains("```lumen"));
    assert!(main.contains("```\n"));
}

#[test]
fn service_main_has_cell() {
    let config = minimal_config();
    let main = ServiceTemplateGenerator::generate_main(&config);
    assert!(main.contains("cell main()"));
    assert!(main.contains("end\n"));
}

#[test]
fn service_main_has_imports() {
    let config = minimal_config();
    let main = ServiceTemplateGenerator::generate_main(&config);
    assert!(main.contains("import routes: *"));
    assert!(main.contains("import handlers: *"));
    assert!(main.contains("import types: *"));
    assert!(main.contains("import middleware: *"));
}

#[test]
fn service_main_has_port() {
    let mut c = minimal_config();
    c.port = 9090;
    let main = ServiceTemplateGenerator::generate_main(&c);
    assert!(main.contains("port: 9090"));
}

#[test]
fn service_main_has_db_import() {
    let mut c = minimal_config();
    c.database = Some(DatabaseConfig {
        kind: DatabaseKind::InMemory,
        name: "mydb".to_string(),
    });
    let main = ServiceTemplateGenerator::generate_main(&c);
    assert!(main.contains("import db: *"));
}

#[test]
fn service_main_effects() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let main = ServiceTemplateGenerator::generate_main(&config);
    // Should have effect row for http
    assert!(main.contains("http"));
}

// =============================================================================
// Generator: generate_routes
// =============================================================================

#[test]
fn service_routes_has_record() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let routes = ServiceTemplateGenerator::generate_routes(&config);
    assert!(routes.contains("record Route"));
    assert!(routes.contains("method: String"));
    assert!(routes.contains("path: String"));
}

#[test]
fn service_routes_empty() {
    let config = minimal_config();
    let routes = ServiceTemplateGenerator::generate_routes(&config);
    assert!(routes.contains("return []"));
}

// =============================================================================
// Generator: generate_handlers
// =============================================================================

#[test]
fn service_handlers_has_grant() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let handlers = ServiceTemplateGenerator::generate_handlers(&config);
    assert!(handlers.contains("grant HttpResponse"));
}

#[test]
fn service_handlers_has_effect_row() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let handlers = ServiceTemplateGenerator::generate_handlers(&config);
    assert!(handlers.contains("/ {http}"));
}

#[test]
fn service_handlers_typed_request() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Post,
        path: "/items".to_string(),
        handler: "create_item".to_string(),
        request_type: Some("CreateRequest".to_string()),
        response_type: Some("ItemResponse".to_string()),
        description: "Create item".to_string(),
    });
    let handlers = ServiceTemplateGenerator::generate_handlers(&c);
    assert!(handlers.contains("cell create_item(request: CreateRequest) -> ItemResponse"));
}

// =============================================================================
// Generator: generate_types
// =============================================================================

#[test]
fn service_types_has_status_enum() {
    let config = minimal_config();
    let types = ServiceTemplateGenerator::generate_types(&config);
    assert!(types.contains("enum HttpStatus"));
    assert!(types.contains("Ok"));
    assert!(types.contains("NotFound"));
    assert!(types.contains("InternalError"));
}

#[test]
fn service_types_has_api_response() {
    let config = minimal_config();
    let types = ServiceTemplateGenerator::generate_types(&config);
    assert!(types.contains("record ApiResponse"));
    assert!(types.contains("record ApiError"));
}

#[test]
fn service_types_generates_route_types() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let types = ServiceTemplateGenerator::generate_types(&config);
    assert!(types.contains("record HealthResponse"));
    assert!(types.contains("record ItemResponse"));
    assert!(types.contains("record CreateItemRequest"));
    assert!(types.contains("record ItemListResponse"));
}

#[test]
fn service_types_no_duplicates() {
    // ItemResponse appears in multiple routes but should only be generated once
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let types = ServiceTemplateGenerator::generate_types(&config);
    let count = types.matches("record ItemResponse").count();
    assert_eq!(count, 1);
}

#[test]
fn service_types_db_model() {
    let mut c = minimal_config();
    c.database = Some(DatabaseConfig {
        kind: DatabaseKind::Postgres,
        name: "user_data".to_string(),
    });
    let types = ServiceTemplateGenerator::generate_types(&c);
    assert!(types.contains("record UserDataModel"));
}

// =============================================================================
// Generator: generate_middleware
// =============================================================================

#[test]
fn service_middleware_empty() {
    let config = minimal_config();
    let mw = ServiceTemplateGenerator::generate_middleware(&config);
    assert!(mw.contains("return []"));
}

#[test]
fn service_middleware_entries() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let mw = ServiceTemplateGenerator::generate_middleware(&config);
    assert!(mw.contains("request_logger"));
    assert!(mw.contains("cors_policy"));
    assert!(mw.contains("cell apply_middleware()"));
}

// =============================================================================
// Generator: generate_tests
// =============================================================================

#[test]
fn service_tests_has_health() {
    let config = minimal_config();
    let tests = ServiceTemplateGenerator::generate_tests(&config);
    assert!(tests.contains("cell test_health()"));
    assert!(tests.contains("cell main()"));
}

#[test]
fn service_tests_per_handler() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let tests = ServiceTemplateGenerator::generate_tests(&config);
    assert!(tests.contains("cell test_health_check()"));
    assert!(tests.contains("cell test_list_items()"));
    assert!(tests.contains("cell test_create_item()"));
    assert!(tests.contains("cell test_get_item()"));
}

// =============================================================================
// Generator: generate_config_toml
// =============================================================================

#[test]
fn service_toml_package() {
    let config = minimal_config();
    let toml = ServiceTemplateGenerator::generate_config_toml(&config);
    assert!(toml.contains("[package]"));
    assert!(toml.contains("name = \"@service/test-svc\""));
    assert!(toml.contains("version = \"0.1.0\""));
}

#[test]
fn service_toml_sections() {
    let config = minimal_config();
    let toml = ServiceTemplateGenerator::generate_config_toml(&config);
    assert!(toml.contains("[toolchain]"));
    assert!(toml.contains("[features]"));
    assert!(toml.contains("[dependencies]"));
    assert!(toml.contains("[dev-dependencies]"));
}

#[test]
fn service_toml_db_provider() {
    let mut c = minimal_config();
    c.database = Some(DatabaseConfig {
        kind: DatabaseKind::Sqlite,
        name: "mydb".to_string(),
    });
    let toml = ServiceTemplateGenerator::generate_config_toml(&c);
    assert!(toml.contains("[providers]"));
    assert!(toml.contains("\"db.sqlite\""));
}

// =============================================================================
// Generator: generate_dockerfile
// =============================================================================

#[test]
fn service_dockerfile_expose() {
    let mut c = minimal_config();
    c.port = 4000;
    let df = ServiceTemplateGenerator::generate_dockerfile(&c);
    assert!(df.contains("EXPOSE 4000"));
    assert!(df.contains("FROM lumen/runtime:latest"));
    assert!(df.contains("CMD [\"lumen\", \"run\", \"src/main.lm.md\"]"));
}

// =============================================================================
// Generator: generate_readme
// =============================================================================

#[test]
fn service_readme_title() {
    let config = minimal_config();
    let readme = ServiceTemplateGenerator::generate_readme(&config);
    assert!(readme.contains("# test-svc"));
}

#[test]
fn service_readme_routes_table() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let readme = ServiceTemplateGenerator::generate_readme(&config);
    assert!(readme.contains("| Method | Path | Handler | Description |"));
    assert!(readme.contains("GET"));
    assert!(readme.contains("POST"));
}

#[test]
fn service_readme_middleware_section() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let readme = ServiceTemplateGenerator::generate_readme(&config);
    assert!(readme.contains("## Middleware"));
}

#[test]
fn service_readme_db_section() {
    let config = ServiceTemplateGenerator::crud_service("item-svc", "item");
    let readme = ServiceTemplateGenerator::generate_readme(&config);
    assert!(readme.contains("## Database"));
    assert!(readme.contains("sqlite"));
}

// =============================================================================
// to_pascal_case
// =============================================================================

#[test]
fn service_pascal_case() {
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
    assert_eq!(
        ServiceTemplateGenerator::to_pascal_case("user_data"),
        "UserData"
    );
}

// =============================================================================
// Display implementations
// =============================================================================

#[test]
fn service_display_service_type() {
    assert_eq!(format!("{}", ServiceType::HttpApi), "HTTP API");
    assert_eq!(format!("{}", ServiceType::WebSocket), "WebSocket");
    assert_eq!(format!("{}", ServiceType::Grpc), "gRPC");
    assert_eq!(format!("{}", ServiceType::GraphQl), "GraphQL");
}

#[test]
fn service_display_http_method() {
    assert_eq!(format!("{}", HttpMethod::Get), "GET");
    assert_eq!(format!("{}", HttpMethod::Post), "POST");
    assert_eq!(format!("{}", HttpMethod::Put), "PUT");
    assert_eq!(format!("{}", HttpMethod::Delete), "DELETE");
    assert_eq!(format!("{}", HttpMethod::Patch), "PATCH");
}

#[test]
fn service_display_middleware_kind() {
    assert_eq!(format!("{}", MiddlewareKind::Auth), "auth");
    assert_eq!(format!("{}", MiddlewareKind::Cors), "cors");
    assert_eq!(format!("{}", MiddlewareKind::RateLimit), "rate_limit");
    assert_eq!(format!("{}", MiddlewareKind::Logging), "logging");
    assert_eq!(format!("{}", MiddlewareKind::Compression), "compression");
    assert_eq!(
        format!("{}", MiddlewareKind::Custom("custom_mw".to_string())),
        "custom_mw"
    );
}

#[test]
fn service_display_db_kind() {
    assert_eq!(format!("{}", DatabaseKind::Sqlite), "sqlite");
    assert_eq!(format!("{}", DatabaseKind::Postgres), "postgres");
    assert_eq!(format!("{}", DatabaseKind::InMemory), "in-memory");
}

#[test]
fn service_display_template_error() {
    let e = TemplateError::InvalidConfig("bad".to_string());
    assert_eq!(format!("{}", e), "invalid config: bad");

    let e = TemplateError::InvalidRoutePath("no slash".to_string());
    assert_eq!(format!("{}", e), "invalid route path: no slash");

    let e = TemplateError::DuplicateRoute("GET /foo".to_string());
    assert_eq!(format!("{}", e), "duplicate route: GET /foo");

    let e = TemplateError::InvalidName("bad".to_string());
    assert_eq!(format!("{}", e), "invalid name: bad");
}

// =============================================================================
// Generated file properties
// =============================================================================

#[test]
fn service_files_not_executable() {
    let config = ServiceTemplateGenerator::rest_api("my-api");
    let files = ServiceTemplateGenerator::generate(&config).unwrap();
    for f in &files {
        assert!(!f.is_executable, "file {} should not be executable", f.path);
    }
}

#[test]
fn service_generated_file_count() {
    // Without db: 9 files
    let config = minimal_config();
    let files = ServiceTemplateGenerator::generate(&config).unwrap();
    assert_eq!(files.len(), 9);

    // With db: 10 files
    let config = ServiceTemplateGenerator::crud_service("user-svc", "user");
    let files = ServiceTemplateGenerator::generate(&config).unwrap();
    assert_eq!(files.len(), 10);
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn service_handler_reserved_as_keyword() {
    let mut c = minimal_config();
    c.routes.push(RouteSpec {
        method: HttpMethod::Get,
        path: "/test".to_string(),
        handler: "cell".to_string(),
        request_type: None,
        response_type: None,
        description: "test".to_string(),
    });
    assert!(matches!(
        validate_config(&c),
        Err(TemplateError::InvalidName(_))
    ));
}

#[test]
fn service_route_all_methods() {
    let mut c = minimal_config();
    let methods = [
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Delete,
        HttpMethod::Patch,
    ];
    for (i, method) in methods.iter().enumerate() {
        c.routes.push(RouteSpec {
            method: *method,
            path: "/test".to_string(),
            handler: format!("handler_{}", i),
            request_type: None,
            response_type: None,
            description: "test".to_string(),
        });
    }
    assert!(validate_config(&c).is_ok());
}

#[test]
fn service_crud_pascal_resource() {
    let config = ServiceTemplateGenerator::crud_service("order-svc", "order");
    let types = ServiceTemplateGenerator::generate_types(&config);
    assert!(types.contains("record OrderResponse"));
    assert!(types.contains("record CreateOrderRequest"));
    assert!(types.contains("record UpdateOrderRequest"));
    assert!(types.contains("record OrderListResponse"));
}
