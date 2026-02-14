//! HTTP provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose HTTP operations as tools:
//! - `http.get` — GET request
//! - `http.post` — POST request with body
//! - `http.put` — PUT request with body
//! - `http.delete` — DELETE request
//!
//! Each tool accepts a JSON object with `url`, optional `headers`, and optional `body`,
//! and returns a JSON object with `status`, `body`, and `headers`.

use lumen_runtime::tools::{ToolError, ToolProvider, ToolSchema};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Request/Response schemas
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpRequest {
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpResponse {
    status: u16,
    body: String,
    headers: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// HTTP method enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    fn tool_name(&self) -> &'static str {
        match self {
            Method::Get => "http.get",
            Method::Post => "http.post",
            Method::Put => "http.put",
            Method::Delete => "http.delete",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Method::Get => "Perform an HTTP GET request",
            Method::Post => "Perform an HTTP POST request with optional body",
            Method::Put => "Perform an HTTP PUT request with optional body",
            Method::Delete => "Perform an HTTP DELETE request",
        }
    }
}

// ---------------------------------------------------------------------------
// HttpProvider implementation
// ---------------------------------------------------------------------------

/// HTTP provider implementing the `ToolProvider` trait.
pub struct HttpProvider {
    method: Method,
    schema: ToolSchema,
    client: Client,
}

impl HttpProvider {
    /// Create a new HTTP provider for the given method.
    fn new(method: Method) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        let schema = ToolSchema {
            name: method.tool_name().to_string(),
            description: method.description().to_string(),
            input_schema: json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Target URL for the HTTP request"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional HTTP headers as key-value pairs",
                        "additionalProperties": {"type": "string"}
                    },
                    "body": {
                        "type": "string",
                        "description": "Optional request body (for POST/PUT)"
                    }
                }
            }),
            output_schema: json!({
                "type": "object",
                "required": ["status", "body", "headers"],
                "properties": {
                    "status": {
                        "type": "number",
                        "description": "HTTP status code"
                    },
                    "body": {
                        "type": "string",
                        "description": "Response body as text"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Response headers as key-value pairs",
                        "additionalProperties": {"type": "string"}
                    }
                }
            }),
            effects: vec!["http".to_string()],
        };

        Self {
            method,
            schema,
            client,
        }
    }

    /// Create a GET provider.
    pub fn get() -> Self {
        Self::new(Method::Get)
    }

    /// Create a POST provider.
    pub fn post() -> Self {
        Self::new(Method::Post)
    }

    /// Create a PUT provider.
    pub fn put() -> Self {
        Self::new(Method::Put)
    }

    /// Create a DELETE provider.
    pub fn delete() -> Self {
        Self::new(Method::Delete)
    }

    /// Execute the HTTP request with the given method.
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, ToolError> {
        // Validate URL
        if request.url.is_empty() {
            return Err(ToolError::InvocationFailed("URL cannot be empty".into()));
        }

        // Parse URL to validate format
        let url = reqwest::Url::parse(&request.url).map_err(|e| {
            ToolError::InvocationFailed(format!("Invalid URL '{}': {}", request.url, e))
        })?;

        // Build request
        let mut req = match self.method {
            Method::Get => self.client.get(url),
            Method::Post => self.client.post(url),
            Method::Put => self.client.put(url),
            Method::Delete => self.client.delete(url),
        };

        // Add headers
        for (key, value) in &request.headers {
            req = req.header(key, value);
        }

        // Add body for POST/PUT
        if matches!(self.method, Method::Post | Method::Put) {
            if let Some(body) = &request.body {
                req = req.body(body.clone());
            }
        }

        // Execute request
        let response = req
            .send()
            .map_err(|e| ToolError::InvocationFailed(format!("HTTP request failed: {}", e)))?;

        // Extract status
        let status = response.status().as_u16();

        // Extract headers
        let mut headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // Extract body
        let body = response.text().map_err(|e| {
            ToolError::InvocationFailed(format!("Failed to read response body: {}", e))
        })?;

        Ok(HttpResponse {
            status,
            body,
            headers,
        })
    }
}

impl ToolProvider for HttpProvider {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        // Parse input
        let request: HttpRequest = serde_json::from_value(input)
            .map_err(|e| ToolError::InvocationFailed(format!("Invalid request format: {}", e)))?;

        // Execute request
        let response = self.execute(request)?;

        // Serialize response
        serde_json::to_value(response).map_err(|e| {
            ToolError::InvocationFailed(format!("Failed to serialize response: {}", e))
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_metadata() {
        let provider = HttpProvider::get();
        assert_eq!(provider.name(), "http.get");
        assert_eq!(provider.version(), "1.0.0");
        assert_eq!(provider.schema().name, "http.get");
        assert_eq!(provider.schema().effects, vec!["http"]);
    }

    #[test]
    fn all_methods_have_correct_metadata() {
        let providers = vec![
            (HttpProvider::get(), "http.get", "GET"),
            (HttpProvider::post(), "http.post", "POST"),
            (HttpProvider::put(), "http.put", "PUT"),
            (HttpProvider::delete(), "http.delete", "DELETE"),
        ];

        for (provider, expected_name, method) in providers {
            assert_eq!(provider.name(), expected_name);
            assert!(provider.schema().description.contains(method));
        }
    }

    #[test]
    fn schema_has_required_url() {
        let provider = HttpProvider::get();
        let schema = provider.schema();
        let input_schema = &schema.input_schema;

        assert_eq!(input_schema["type"], "object");
        assert_eq!(input_schema["required"], json!(["url"]));
        assert!(input_schema["properties"]["url"].is_object());
    }

    #[test]
    fn schema_output_structure() {
        let provider = HttpProvider::get();
        let schema = provider.schema();
        let output_schema = &schema.output_schema;

        assert_eq!(output_schema["type"], "object");
        assert_eq!(
            output_schema["required"],
            json!(["status", "body", "headers"])
        );
        assert!(output_schema["properties"]["status"].is_object());
        assert!(output_schema["properties"]["body"].is_object());
        assert!(output_schema["properties"]["headers"].is_object());
    }

    #[test]
    fn invalid_url_returns_error() {
        let provider = HttpProvider::get();
        let input = json!({
            "url": "not-a-valid-url"
        });

        let result = provider.call(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvocationFailed(msg) => {
                assert!(msg.contains("Invalid URL"));
            }
            other => panic!("Expected InvocationFailed, got: {:?}", other),
        }
    }

    #[test]
    fn empty_url_returns_error() {
        let provider = HttpProvider::get();
        let input = json!({
            "url": ""
        });

        let result = provider.call(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvocationFailed(msg) => {
                assert!(msg.contains("URL cannot be empty"));
            }
            other => panic!("Expected InvocationFailed, got: {:?}", other),
        }
    }

    #[test]
    fn missing_url_field_returns_error() {
        let provider = HttpProvider::get();
        let input = json!({
            "headers": {}
        });

        let result = provider.call(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvocationFailed(msg) => {
                assert!(msg.contains("Invalid request format"));
            }
            other => panic!("Expected InvocationFailed, got: {:?}", other),
        }
    }

    #[test]
    fn invalid_json_returns_error() {
        let provider = HttpProvider::get();
        let input = json!("not an object");

        let result = provider.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn http_request_deserialization() {
        let json = json!({
            "url": "https://example.com",
            "headers": {
                "Authorization": "Bearer token"
            },
            "body": "test body"
        });

        let request: HttpRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.url, "https://example.com");
        assert_eq!(
            request.headers.get("Authorization").unwrap(),
            "Bearer token"
        );
        assert_eq!(request.body.as_ref().unwrap(), "test body");
    }

    #[test]
    fn http_request_optional_fields() {
        let json = json!({
            "url": "https://example.com"
        });

        let request: HttpRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.url, "https://example.com");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
    }

    #[test]
    fn http_response_serialization() {
        let response = HttpResponse {
            status: 200,
            body: "OK".to_string(),
            headers: {
                let mut map = HashMap::new();
                map.insert("content-type".to_string(), "text/plain".to_string());
                map
            },
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["status"], 200);
        assert_eq!(json["body"], "OK");
        assert_eq!(json["headers"]["content-type"], "text/plain");
    }
}
