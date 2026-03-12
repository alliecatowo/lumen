//! HTTP client/server abstractions for the Lumen runtime.
//!
//! This module provides typed HTTP primitives — methods, headers, requests,
//! responses, a fluent request builder, and a server-side router with path
//! parameter extraction. These are *abstractions only*; actual network I/O is
//! wired through tool providers at a higher layer.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// HttpMethod
// ---------------------------------------------------------------------------

/// Standard HTTP methods.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    /// HTTP GET
    Get,
    /// HTTP POST
    Post,
    /// HTTP PUT
    Put,
    /// HTTP DELETE
    Delete,
    /// HTTP PATCH
    Patch,
    /// HTTP HEAD
    Head,
    /// HTTP OPTIONS
    Options,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        };
        f.write_str(s)
    }
}

impl HttpMethod {
    /// Parse an HTTP method from a string (case-insensitive).
    pub fn parse(s: &str) -> Result<Self, HttpError> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            "PATCH" => Ok(HttpMethod::Patch),
            "HEAD" => Ok(HttpMethod::Head),
            "OPTIONS" => Ok(HttpMethod::Options),
            _ => Err(HttpError::InvalidMethod(s.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// HttpHeader
// ---------------------------------------------------------------------------

/// A single HTTP header (name-value pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHeader {
    /// Header name (e.g. `Content-Type`).
    pub name: String,
    /// Header value (e.g. `application/json`).
    pub value: String,
}

// ---------------------------------------------------------------------------
// HttpRequest
// ---------------------------------------------------------------------------

/// An HTTP request ready to be dispatched by a tool provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// HTTP method.
    pub method: HttpMethod,
    /// Target URL.
    pub url: String,
    /// Request headers.
    pub headers: Vec<HttpHeader>,
    /// Optional request body.
    pub body: Option<String>,
    /// Optional timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// HttpResponse
// ---------------------------------------------------------------------------

/// An HTTP response returned by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code (e.g. 200).
    pub status: u16,
    /// Status reason phrase (e.g. "OK").
    pub status_text: String,
    /// Response headers.
    pub headers: Vec<HttpHeader>,
    /// Response body as a string.
    pub body: String,
    /// Wall-clock time the request took, in milliseconds.
    pub elapsed_ms: u64,
}

impl HttpResponse {
    /// Returns `true` if the status code is in the 2xx range.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 3xx range.
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 4xx range.
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 5xx range.
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Case-insensitive header lookup by name.
    ///
    /// Returns the value of the first matching header, or `None`.
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|h| h.name.to_lowercase() == lower)
            .map(|h| h.value.as_str())
    }

    /// Shortcut for retrieving the `Content-Type` header value.
    pub fn content_type(&self) -> Option<&str> {
        self.header("Content-Type")
    }
}

// ---------------------------------------------------------------------------
// RequestBuilder
// ---------------------------------------------------------------------------

/// Fluent builder for constructing [`HttpRequest`] values.
#[derive(Debug, Clone)]
pub struct RequestBuilder {
    method: HttpMethod,
    url: String,
    headers: Vec<HttpHeader>,
    body: Option<String>,
    timeout_ms: Option<u64>,
}

impl RequestBuilder {
    /// Create a new builder with the given method and URL.
    pub fn new(method: HttpMethod, url: &str) -> Self {
        Self {
            method,
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
            timeout_ms: None,
        }
    }

    /// Shortcut for a GET request builder.
    pub fn get(url: &str) -> Self {
        Self::new(HttpMethod::Get, url)
    }

    /// Shortcut for a POST request builder.
    pub fn post(url: &str) -> Self {
        Self::new(HttpMethod::Post, url)
    }

    /// Add a header to the request.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push(HttpHeader {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    /// Set the request body.
    pub fn body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    /// Set a JSON body and the `Content-Type: application/json` header.
    pub fn json(self, value: &str) -> Self {
        self.header("Content-Type", "application/json").body(value)
    }

    /// Set a request timeout in milliseconds.
    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Set an `Authorization: Bearer <token>` header.
    pub fn bearer_token(self, token: &str) -> Self {
        self.header("Authorization", &format!("Bearer {}", token))
    }

    /// Consume the builder and produce an [`HttpRequest`].
    pub fn build(self) -> HttpRequest {
        HttpRequest {
            method: self.method,
            url: self.url,
            headers: self.headers,
            body: self.body,
            timeout_ms: self.timeout_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Router / Route / RouteMatch
// ---------------------------------------------------------------------------

/// A route binding a method + path pattern to a named handler cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// HTTP method for this route.
    pub method: HttpMethod,
    /// Path pattern (e.g. `/users/:id`).
    pub path: String,
    /// Name of the Lumen cell that handles this route.
    pub handler_name: String,
    /// Optional Lumen type name for the request body.
    pub request_type: Option<String>,
    /// Optional Lumen type name for the response body.
    pub response_type: Option<String>,
}

/// Result of a successful route match, carrying extracted path parameters.
#[derive(Debug)]
pub struct RouteMatch<'a> {
    /// The matched route definition.
    pub route: &'a Route,
    /// Path parameters extracted from `:param` segments.
    pub params: HashMap<String, String>,
}

/// A simple path-based HTTP router.
#[derive(Debug, Default)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// Create an empty router.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a route. Returns `&mut Self` for chaining.
    pub fn add_route(&mut self, route: Route) -> &mut Self {
        self.routes.push(route);
        self
    }

    /// Shortcut: register a GET route.
    pub fn get(&mut self, path: &str, handler: &str) -> &mut Self {
        self.add_route(Route {
            method: HttpMethod::Get,
            path: path.to_string(),
            handler_name: handler.to_string(),
            request_type: None,
            response_type: None,
        })
    }

    /// Shortcut: register a POST route.
    pub fn post(&mut self, path: &str, handler: &str) -> &mut Self {
        self.add_route(Route {
            method: HttpMethod::Post,
            path: path.to_string(),
            handler_name: handler.to_string(),
            request_type: None,
            response_type: None,
        })
    }

    /// Attempt to match a request method + path against registered routes.
    ///
    /// Path segments starting with `:` are treated as named parameters.
    /// Returns `None` if no route matches.
    pub fn match_route(&self, method: &HttpMethod, path: &str) -> Option<RouteMatch<'_>> {
        let req_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for route in &self.routes {
            if route.method != *method {
                continue;
            }

            let route_segments: Vec<&str> =
                route.path.split('/').filter(|s| !s.is_empty()).collect();

            if route_segments.len() != req_segments.len() {
                continue;
            }

            let mut params = HashMap::new();
            let mut matched = true;

            for (rs, qs) in route_segments.iter().zip(req_segments.iter()) {
                if let Some(param_name) = rs.strip_prefix(':') {
                    params.insert(param_name.to_string(), qs.to_string());
                } else if rs != qs {
                    matched = false;
                    break;
                }
            }

            if matched {
                return Some(RouteMatch { route, params });
            }
        }

        None
    }

    /// Return the number of registered routes.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Return `true` if no routes are registered.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

// ---------------------------------------------------------------------------
// HttpError
// ---------------------------------------------------------------------------

/// Errors that can occur in the HTTP abstraction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// The provided URL is malformed.
    InvalidUrl(String),
    /// The request timed out.
    Timeout {
        /// URL that was being requested.
        url: String,
        /// Configured timeout in milliseconds.
        timeout_ms: u64,
    },
    /// A connection-level failure occurred.
    ConnectionError(String),
    /// The HTTP method string is not recognised.
    InvalidMethod(String),
    /// A header name or value is invalid.
    InvalidHeader(String),
    /// The server returned a non-success status.
    ResponseError {
        /// HTTP status code.
        status: u16,
        /// Response body (may be empty).
        body: String,
    },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::InvalidUrl(url) => write!(f, "invalid URL: {}", url),
            HttpError::Timeout { url, timeout_ms } => {
                write!(f, "request to {} timed out after {}ms", url, timeout_ms)
            }
            HttpError::ConnectionError(msg) => write!(f, "connection error: {}", msg),
            HttpError::InvalidMethod(m) => write!(f, "invalid HTTP method: {}", m),
            HttpError::InvalidHeader(msg) => write!(f, "invalid header: {}", msg),
            HttpError::ResponseError { status, body } => {
                write!(f, "HTTP error {}: {}", status, body)
            }
        }
    }
}

impl std::error::Error for HttpError {}
