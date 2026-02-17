//! Comprehensive tests for the `lumen_runtime::http` module.

use lumen_runtime::http::*;

// ---------------------------------------------------------------------------
// HttpMethod tests
// ---------------------------------------------------------------------------

#[test]
fn http_method_display_get() {
    assert_eq!(HttpMethod::Get.to_string(), "GET");
}

#[test]
fn http_method_display_post() {
    assert_eq!(HttpMethod::Post.to_string(), "POST");
}

#[test]
fn http_method_display_put() {
    assert_eq!(HttpMethod::Put.to_string(), "PUT");
}

#[test]
fn http_method_display_delete() {
    assert_eq!(HttpMethod::Delete.to_string(), "DELETE");
}

#[test]
fn http_method_display_patch() {
    assert_eq!(HttpMethod::Patch.to_string(), "PATCH");
}

#[test]
fn http_method_display_head() {
    assert_eq!(HttpMethod::Head.to_string(), "HEAD");
}

#[test]
fn http_method_display_options() {
    assert_eq!(HttpMethod::Options.to_string(), "OPTIONS");
}

#[test]
fn http_method_parse_valid() {
    assert_eq!(HttpMethod::parse("get").unwrap(), HttpMethod::Get);
    assert_eq!(HttpMethod::parse("POST").unwrap(), HttpMethod::Post);
    assert_eq!(HttpMethod::parse("Put").unwrap(), HttpMethod::Put);
    assert_eq!(HttpMethod::parse("DELETE").unwrap(), HttpMethod::Delete);
    assert_eq!(HttpMethod::parse("patch").unwrap(), HttpMethod::Patch);
    assert_eq!(HttpMethod::parse("HEAD").unwrap(), HttpMethod::Head);
    assert_eq!(HttpMethod::parse("options").unwrap(), HttpMethod::Options);
}

#[test]
fn http_method_parse_invalid() {
    let err = HttpMethod::parse("FOOBAR").unwrap_err();
    assert_eq!(err, HttpError::InvalidMethod("FOOBAR".to_string()));
}

// ---------------------------------------------------------------------------
// RequestBuilder tests
// ---------------------------------------------------------------------------

#[test]
fn request_builder_get_shortcut() {
    let req = RequestBuilder::get("https://example.com").build();
    assert_eq!(req.method, HttpMethod::Get);
    assert_eq!(req.url, "https://example.com");
    assert!(req.headers.is_empty());
    assert!(req.body.is_none());
    assert!(req.timeout_ms.is_none());
}

#[test]
fn request_builder_post_with_json() {
    let req = RequestBuilder::post("https://api.example.com/data")
        .json(r#"{"key":"value"}"#)
        .build();
    assert_eq!(req.method, HttpMethod::Post);
    assert_eq!(req.body.as_deref(), Some(r#"{"key":"value"}"#));
    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].name, "Content-Type");
    assert_eq!(req.headers[0].value, "application/json");
}

#[test]
fn request_builder_fluent_chaining() {
    let req = RequestBuilder::new(HttpMethod::Put, "https://api.example.com/item/1")
        .header("Accept", "application/json")
        .header("X-Custom", "test")
        .body("hello")
        .timeout(5000)
        .build();

    assert_eq!(req.method, HttpMethod::Put);
    assert_eq!(req.url, "https://api.example.com/item/1");
    assert_eq!(req.headers.len(), 2);
    assert_eq!(req.body.as_deref(), Some("hello"));
    assert_eq!(req.timeout_ms, Some(5000));
}

#[test]
fn request_builder_bearer_token() {
    let req = RequestBuilder::get("https://api.example.com/me")
        .bearer_token("abc123")
        .build();
    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].name, "Authorization");
    assert_eq!(req.headers[0].value, "Bearer abc123");
}

#[test]
fn request_builder_multiple_headers_and_timeout() {
    let req = RequestBuilder::post("https://example.com")
        .header("X-A", "1")
        .header("X-B", "2")
        .header("X-C", "3")
        .timeout(100)
        .build();
    assert_eq!(req.headers.len(), 3);
    assert_eq!(req.timeout_ms, Some(100));
}

// ---------------------------------------------------------------------------
// HttpRequest construction test
// ---------------------------------------------------------------------------

#[test]
fn http_request_construction_and_fields() {
    let req = HttpRequest {
        method: HttpMethod::Delete,
        url: "https://example.com/item/42".to_string(),
        headers: vec![HttpHeader {
            name: "Authorization".to_string(),
            value: "Bearer tok".to_string(),
        }],
        body: None,
        timeout_ms: Some(3000),
    };
    assert_eq!(req.method, HttpMethod::Delete);
    assert_eq!(req.url, "https://example.com/item/42");
    assert_eq!(req.headers.len(), 1);
    assert!(req.body.is_none());
    assert_eq!(req.timeout_ms, Some(3000));
}

// ---------------------------------------------------------------------------
// HttpResponse helper tests
// ---------------------------------------------------------------------------

fn make_response(status: u16) -> HttpResponse {
    HttpResponse {
        status,
        status_text: String::new(),
        headers: Vec::new(),
        body: String::new(),
        elapsed_ms: 0,
    }
}

#[test]
fn response_is_success() {
    assert!(make_response(200).is_success());
    assert!(make_response(201).is_success());
    assert!(make_response(204).is_success());
    assert!(make_response(299).is_success());
    assert!(!make_response(199).is_success());
    assert!(!make_response(300).is_success());
}

#[test]
fn response_is_redirect() {
    assert!(make_response(301).is_redirect());
    assert!(make_response(302).is_redirect());
    assert!(make_response(307).is_redirect());
    assert!(make_response(399).is_redirect());
    assert!(!make_response(200).is_redirect());
    assert!(!make_response(400).is_redirect());
}

#[test]
fn response_is_client_error() {
    assert!(make_response(400).is_client_error());
    assert!(make_response(404).is_client_error());
    assert!(make_response(429).is_client_error());
    assert!(make_response(499).is_client_error());
    assert!(!make_response(399).is_client_error());
    assert!(!make_response(500).is_client_error());
}

#[test]
fn response_is_server_error() {
    assert!(make_response(500).is_server_error());
    assert!(make_response(502).is_server_error());
    assert!(make_response(503).is_server_error());
    assert!(make_response(599).is_server_error());
    assert!(!make_response(499).is_server_error());
    assert!(!make_response(600).is_server_error());
}

#[test]
fn response_header_case_insensitive() {
    let resp = HttpResponse {
        status: 200,
        status_text: "OK".to_string(),
        headers: vec![
            HttpHeader {
                name: "Content-Type".to_string(),
                value: "application/json".to_string(),
            },
            HttpHeader {
                name: "X-Request-Id".to_string(),
                value: "abc123".to_string(),
            },
        ],
        body: "{}".to_string(),
        elapsed_ms: 42,
    };

    assert_eq!(resp.header("content-type"), Some("application/json"));
    assert_eq!(resp.header("CONTENT-TYPE"), Some("application/json"));
    assert_eq!(resp.header("Content-Type"), Some("application/json"));
    assert_eq!(resp.header("x-request-id"), Some("abc123"));
    assert_eq!(resp.header("X-Missing"), None);
}

#[test]
fn response_content_type_shortcut() {
    let resp = HttpResponse {
        status: 200,
        status_text: "OK".to_string(),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html".to_string(),
        }],
        body: "<h1>hi</h1>".to_string(),
        elapsed_ms: 10,
    };
    assert_eq!(resp.content_type(), Some("text/html"));
}

#[test]
fn response_content_type_missing() {
    let resp = make_response(204);
    assert_eq!(resp.content_type(), None);
}

// ---------------------------------------------------------------------------
// Router tests
// ---------------------------------------------------------------------------

#[test]
fn router_new_is_empty() {
    let router = Router::new();
    assert!(router.is_empty());
    assert_eq!(router.len(), 0);
}

#[test]
fn router_add_route_and_match_exact() {
    let mut router = Router::new();
    router.get("/health", "health_check");

    let m = router.match_route(&HttpMethod::Get, "/health").unwrap();
    assert_eq!(m.route.handler_name, "health_check");
    assert!(m.params.is_empty());
}

#[test]
fn router_match_with_path_params() {
    let mut router = Router::new();
    router.get("/users/:id", "get_user");

    let m = router.match_route(&HttpMethod::Get, "/users/42").unwrap();
    assert_eq!(m.route.handler_name, "get_user");
    assert_eq!(m.params.get("id"), Some(&"42".to_string()));
}

#[test]
fn router_match_multiple_path_params() {
    let mut router = Router::new();
    router.add_route(Route {
        method: HttpMethod::Get,
        path: "/orgs/:org/repos/:repo".to_string(),
        handler_name: "get_repo".to_string(),
        request_type: None,
        response_type: None,
    });

    let m = router
        .match_route(&HttpMethod::Get, "/orgs/acme/repos/widgets")
        .unwrap();
    assert_eq!(m.params.get("org"), Some(&"acme".to_string()));
    assert_eq!(m.params.get("repo"), Some(&"widgets".to_string()));
}

#[test]
fn router_method_mismatch_returns_none() {
    let mut router = Router::new();
    router.get("/users", "list_users");

    assert!(router.match_route(&HttpMethod::Post, "/users").is_none());
}

#[test]
fn router_path_not_found_returns_none() {
    let mut router = Router::new();
    router.get("/users", "list_users");

    assert!(router
        .match_route(&HttpMethod::Get, "/nonexistent")
        .is_none());
}

#[test]
fn router_post_shortcut() {
    let mut router = Router::new();
    router.post("/items", "create_item");

    let m = router.match_route(&HttpMethod::Post, "/items").unwrap();
    assert_eq!(m.route.handler_name, "create_item");
    assert_eq!(m.route.method, HttpMethod::Post);
}

#[test]
fn router_length_tracking() {
    let mut router = Router::new();
    assert_eq!(router.len(), 0);
    router.get("/a", "ha");
    assert_eq!(router.len(), 1);
    router.post("/b", "hb");
    assert_eq!(router.len(), 2);
}

#[test]
fn router_typed_route() {
    let mut router = Router::new();
    router.add_route(Route {
        method: HttpMethod::Post,
        path: "/users".to_string(),
        handler_name: "create_user".to_string(),
        request_type: Some("CreateUserRequest".to_string()),
        response_type: Some("User".to_string()),
    });

    let m = router.match_route(&HttpMethod::Post, "/users").unwrap();
    assert_eq!(m.route.request_type.as_deref(), Some("CreateUserRequest"));
    assert_eq!(m.route.response_type.as_deref(), Some("User"));
}

// ---------------------------------------------------------------------------
// HttpError tests
// ---------------------------------------------------------------------------

#[test]
fn http_error_display_invalid_url() {
    let err = HttpError::InvalidUrl("not a url".to_string());
    assert_eq!(err.to_string(), "invalid URL: not a url");
}

#[test]
fn http_error_display_timeout() {
    let err = HttpError::Timeout {
        url: "https://slow.example.com".to_string(),
        timeout_ms: 5000,
    };
    assert_eq!(
        err.to_string(),
        "request to https://slow.example.com timed out after 5000ms"
    );
}

#[test]
fn http_error_display_connection_error() {
    let err = HttpError::ConnectionError("DNS resolution failed".to_string());
    assert_eq!(err.to_string(), "connection error: DNS resolution failed");
}

#[test]
fn http_error_display_invalid_method() {
    let err = HttpError::InvalidMethod("FOOBAR".to_string());
    assert_eq!(err.to_string(), "invalid HTTP method: FOOBAR");
}

#[test]
fn http_error_display_invalid_header() {
    let err = HttpError::InvalidHeader("header name contains null".to_string());
    assert_eq!(err.to_string(), "invalid header: header name contains null");
}

#[test]
fn http_error_display_response_error() {
    let err = HttpError::ResponseError {
        status: 404,
        body: "Not Found".to_string(),
    };
    assert_eq!(err.to_string(), "HTTP error 404: Not Found");
}

#[test]
fn http_error_is_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(HttpError::ConnectionError("oops".to_string()));
    assert!(err.to_string().contains("oops"));
}
