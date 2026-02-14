use lumen_runtime::tools::*;
use serde_json::{json, Value};

/// Gemini tool type — each gets its own provider instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeminiTool {
    Generate,
    Chat,
    Embed,
}

impl GeminiTool {
    fn tool_name(&self) -> &'static str {
        match self {
            GeminiTool::Generate => "gemini.generate",
            GeminiTool::Chat => "gemini.chat",
            GeminiTool::Embed => "gemini.embed",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            GeminiTool::Generate => "Generate text using Gemini",
            GeminiTool::Chat => "Multi-turn chat with Gemini",
            GeminiTool::Embed => "Generate text embeddings",
        }
    }
}

pub struct GeminiProvider {
    tool: GeminiTool,
    api_key: String,
    model: String,
    base_url: String,
    schema: ToolSchema,
}

impl GeminiProvider {
    fn new(tool: GeminiTool, api_key: String) -> Self {
        let schema = match tool {
            GeminiTool::Generate => ToolSchema {
                name: tool.tool_name().to_string(),
                description: tool.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string", "description": "The prompt to send" },
                        "system": { "type": "string", "description": "Optional system instruction" },
                        "max_tokens": { "type": "integer", "description": "Max output tokens" },
                        "temperature": { "type": "number", "description": "Sampling temperature (0-2)" }
                    },
                    "required": ["prompt"]
                }),
                output_schema: json!({ "type": "string" }),
                effects: vec!["llm".to_string()],
            },
            GeminiTool::Chat => ToolSchema {
                name: tool.tool_name().to_string(),
                description: tool.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "messages": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "role": { "type": "string", "enum": ["user", "model"] },
                                    "content": { "type": "string" }
                                }
                            }
                        },
                        "system": { "type": "string" },
                        "temperature": { "type": "number" }
                    },
                    "required": ["messages"]
                }),
                output_schema: json!({ "type": "string" }),
                effects: vec!["llm".to_string()],
            },
            GeminiTool::Embed => ToolSchema {
                name: tool.tool_name().to_string(),
                description: tool.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" }
                    },
                    "required": ["text"]
                }),
                output_schema: json!({ "type": "array", "items": { "type": "number" } }),
                effects: vec!["llm".to_string()],
            },
        };

        Self {
            tool,
            api_key,
            model: "gemini-2.0-flash".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            schema,
        }
    }

    /// Create a new provider for gemini.generate
    pub fn generate(api_key: String) -> Self {
        Self::new(GeminiTool::Generate, api_key)
    }

    /// Create a new provider for gemini.chat
    pub fn chat(api_key: String) -> Self {
        Self::new(GeminiTool::Chat, api_key)
    }

    /// Create a new provider for gemini.embed
    pub fn embed(api_key: String) -> Self {
        Self::new(GeminiTool::Embed, api_key)
    }

    /// Override the default model
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    fn execute_generate(&self, input: Value) -> Result<Value, ToolError> {
        let prompt = input
            .get("prompt")
            .and_then(|p| p.as_str())
            .ok_or_else(|| ToolError::InvocationFailed("missing 'prompt' field".to_string()))?;
        let system = input.get("system").and_then(|s| s.as_str());
        let temperature = input.get("temperature").and_then(|t| t.as_f64());

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let mut contents = vec![];
        if let Some(sys) = system {
            // Gemini uses systemInstruction field
            contents.push(json!({
                "role": "user",
                "parts": [{"text": format!("System: {}\n\n{}", sys, prompt)}]
            }));
        } else {
            contents.push(json!({
                "role": "user",
                "parts": [{"text": prompt}]
            }));
        }

        let mut body = json!({ "contents": contents });

        if let Some(temp) = temperature {
            body["generationConfig"] = json!({ "temperature": temp });
        }

        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| ToolError::InvocationFailed(format!("HTTP error: {}", e)))?;

        let status = response.status();
        let response_body: Value = response
            .json()
            .map_err(|e| ToolError::InvocationFailed(format!("JSON parse error: {}", e)))?;

        if !status.is_success() {
            return Err(ToolError::InvocationFailed(format!(
                "API error {}: {}",
                status, response_body
            )));
        }

        // Extract text from Gemini response
        let text = response_body
            .get("candidates")
            .and_then(|c: &Value| c.get(0))
            .and_then(|c: &Value| c.get("content"))
            .and_then(|c: &Value| c.get("parts"))
            .and_then(|p: &Value| p.get(0))
            .and_then(|p: &Value| p.get("text"))
            .and_then(|t: &Value| t.as_str())
            .unwrap_or("")
            .to_string();

        Ok(json!(text))
    }

    fn execute_chat(&self, input: Value) -> Result<Value, ToolError> {
        let messages = input
            .get("messages")
            .and_then(|m| m.as_array())
            .ok_or_else(|| ToolError::InvocationFailed("missing 'messages' array".to_string()))?;

        let contents: Vec<Value> = messages
            .iter()
            .map(|m| {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                json!({
                    "role": role,
                    "parts": [{"text": content}]
                })
            })
            .collect();

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let body = json!({ "contents": contents });

        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| ToolError::InvocationFailed(format!("HTTP error: {}", e)))?;

        let response_body: Value = response
            .json()
            .map_err(|e| ToolError::InvocationFailed(format!("JSON parse error: {}", e)))?;

        let text = response_body
            .get("candidates")
            .and_then(|c: &Value| c.get(0))
            .and_then(|c: &Value| c.get("content"))
            .and_then(|c: &Value| c.get("parts"))
            .and_then(|p: &Value| p.get(0))
            .and_then(|p: &Value| p.get("text"))
            .and_then(|t: &Value| t.as_str())
            .unwrap_or("")
            .to_string();

        Ok(json!(text))
    }

    fn execute_embed(&self, input: Value) -> Result<Value, ToolError> {
        let text = input
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| ToolError::InvocationFailed("missing 'text' field".to_string()))?;

        let url = format!(
            "{}/models/text-embedding-004:embedContent?key={}",
            self.base_url, self.api_key
        );

        let body = json!({
            "model": "models/text-embedding-004",
            "content": {
                "parts": [{"text": text}]
            }
        });

        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| ToolError::InvocationFailed(format!("HTTP error: {}", e)))?;

        let response_body: Value = response
            .json()
            .map_err(|e| ToolError::InvocationFailed(format!("JSON parse error: {}", e)))?;

        let embedding = response_body
            .get("embedding")
            .and_then(|e: &Value| e.get("values"))
            .cloned()
            .unwrap_or(json!([]));

        Ok(embedding)
    }
}

impl ToolProvider for GeminiProvider {
    fn name(&self) -> &str {
        self.tool.tool_name()
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        match self.tool {
            GeminiTool::Generate => self.execute_generate(input),
            GeminiTool::Chat => self.execute_chat(input),
            GeminiTool::Embed => self.execute_embed(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_metadata() {
        let provider = GeminiProvider::generate("test_key".to_string());
        assert_eq!(provider.name(), "gemini.generate");
        assert_eq!(provider.version(), "0.1.0");
    }

    #[test]
    fn test_schema_generation() {
        let generate_provider = GeminiProvider::generate("test_key".to_string());
        let schema = generate_provider.schema();
        assert_eq!(schema.name, "gemini.generate");
        assert_eq!(schema.description, "Generate text using Gemini");
        assert_eq!(schema.effects, vec!["llm"]);

        let chat_provider = GeminiProvider::chat("test_key".to_string());
        let schema = chat_provider.schema();
        assert_eq!(schema.name, "gemini.chat");
        assert_eq!(schema.description, "Multi-turn chat with Gemini");

        let embed_provider = GeminiProvider::embed("test_key".to_string());
        let schema = embed_provider.schema();
        assert_eq!(schema.name, "gemini.embed");
        assert_eq!(schema.description, "Generate text embeddings");
    }

    #[test]
    fn test_effects_include_llm() {
        let provider = GeminiProvider::generate("test_key".to_string());
        assert_eq!(provider.effects(), vec!["llm"]);
    }

    #[test]
    fn test_generate_missing_prompt() {
        let provider = GeminiProvider::generate("test_key".to_string());
        let result = provider.call(json!({}));
        assert!(matches!(result, Err(ToolError::InvocationFailed(_))));
        if let Err(ToolError::InvocationFailed(msg)) = result {
            assert!(msg.contains("missing 'prompt' field"));
        }
    }

    #[test]
    fn test_chat_missing_messages() {
        let provider = GeminiProvider::chat("test_key".to_string());
        let result = provider.call(json!({}));
        assert!(matches!(result, Err(ToolError::InvocationFailed(_))));
        if let Err(ToolError::InvocationFailed(msg)) = result {
            assert!(msg.contains("missing 'messages' array"));
        }
    }

    #[test]
    fn test_embed_missing_text() {
        let provider = GeminiProvider::embed("test_key".to_string());
        let result = provider.call(json!({}));
        assert!(matches!(result, Err(ToolError::InvocationFailed(_))));
        if let Err(ToolError::InvocationFailed(msg)) = result {
            assert!(msg.contains("missing 'text' field"));
        }
    }

    #[test]
    fn test_with_model() {
        let provider = GeminiProvider::generate("test_key".to_string()).with_model("gemini-pro");
        assert_eq!(provider.model, "gemini-pro");
    }

    #[test]
    #[ignore] // Run with: cargo test -p lumen-provider-gemini -- --ignored
    fn test_real_gemini_generate() {
        let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY not set");
        let provider = GeminiProvider::generate(api_key);
        let result = provider.call(json!({
            "prompt": "Say hello in exactly 3 words",
            "temperature": 0.0
        }));
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.as_str().map(|s| !s.is_empty()).unwrap_or(false));
    }
}
