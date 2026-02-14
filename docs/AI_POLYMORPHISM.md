# AI Polymorphism Audit — Provider-Agnostic Architecture

**Date**: 2026-02-13
**Status**: Architecture Assessment
**Goal**: Ensure Lumen's AI system is truly polymorphic and never breaks when AI providers change APIs, models, or capabilities.

---

## Executive Summary

Lumen's current architecture demonstrates **strong foundational polymorphism** through its `ToolProvider` trait abstraction, with clear separation between language-level contracts (compiler) and runtime implementations (providers). The system is **already provider-agnostic at the compiler level** — no hardcoded provider names exist in the compiler or VM.

However, there are **critical gaps** in capability negotiation, error normalization, structured output handling, and provider-specific tool calling format translation. This document identifies these gaps and provides concrete recommendations for hardening Lumen's AI system to withstand future provider API changes.

**Key Findings:**
- ✅ **Excellent**: Compiler is 100% provider-agnostic (no "gemini" or "openai" strings found)
- ✅ **Excellent**: `ToolProvider` trait is generic and extensible
- ✅ **Good**: MCP bridge provides universal interoperability layer
- ⚠️ **Needs Work**: No capability detection (vision, tool use, streaming, JSON mode)
- ⚠️ **Needs Work**: Provider errors are passed through raw (no normalization)
- ⚠️ **Needs Work**: Structured output handling is implicit, not enforced
- ⚠️ **Needs Work**: Tool calling format differences must be handled manually per provider
- ⚠️ **Gap**: No AI-specific provider trait (all tools treated identically)

---

## Table of Contents

1. [Current Architecture Assessment](#1-current-architecture-assessment)
2. [Provider-Specific Leakage Analysis](#2-provider-specific-leakage-analysis)
3. [Tool Calling Format Differences](#3-tool-calling-format-differences)
4. [Structured Output Generation](#4-structured-output-generation)
5. [Capability Detection and Negotiation](#5-capability-detection-and-negotiation)
6. [Error Normalization Strategy](#6-error-normalization-strategy)
7. [Trait Redesign Recommendations](#7-trait-redesign-recommendations)
8. [Abstraction Layer Design](#8-abstraction-layer-design)
9. [Future-Proofing Recommendations](#9-future-proofing-recommendations)
10. [MCP as Universal Bridge](#10-mcp-as-universal-bridge)
11. [Implementation Roadmap](#11-implementation-roadmap)
12. [Research Sources](#12-research-sources)

---

## 1. Current Architecture Assessment

### 1.1 Core Abstraction: `ToolProvider` Trait

**Location**: `rust/lumen-runtime/src/tools.rs:103-120`

```rust
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn schema(&self) -> &ToolSchema;
    fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;
    fn effects(&self) -> Vec<String> {
        self.schema().effects.clone()
    }
}
```

**Strengths:**
- ✅ **Generic interface**: Accepts/returns `serde_json::Value` (provider-agnostic)
- ✅ **Schema-driven**: `ToolSchema` describes inputs/outputs via JSON Schema
- ✅ **Effect-aware**: Providers declare effect kinds (e.g., `["llm", "http"]`)
- ✅ **No provider assumptions**: Trait has zero hardcoded provider knowledge

**Weaknesses:**
- ❌ **No capability metadata**: Can't advertise vision, tool use, streaming, JSON mode
- ❌ **No model identity**: No way to query available models or their features
- ❌ **Generic errors**: `ToolError` doesn't distinguish rate limits, auth failures, etc.
- ❌ **No retry hints**: No `retry_after`, backoff recommendations, or circuit breaker state

### 1.2 VM Tool Dispatch

**Location**: `rust/lumen-vm/src/vm.rs:233,285-286,1755-1756`

```rust
pub struct VM {
    pub tool_dispatcher: Option<Box<dyn ToolDispatcher>>,
    // ...
}

pub fn set_provider_registry(&mut self, registry: ProviderRegistry) {
    self.tool_dispatcher = Some(Box::new(registry));
}

// In execute loop:
if let Some(dispatcher) = self.tool_dispatcher.as_ref() {
    match dispatcher.dispatch(&request) {
        Ok(response) => { /* ... */ }
        Err(err) => { /* ... */ }
    }
}
```

**Architecture:**
- `ProviderRegistry` implements `ToolDispatcher` (adapter pattern)
- VM calls `dispatch(&ToolRequest) -> Result<ToolResponse, ToolError>`
- Registry routes `request.tool_id` to the appropriate provider
- **Provider-agnostic**: VM knows nothing about which provider handles the call

**Strengths:**
- ✅ **Clean separation**: VM → ToolDispatcher interface → ProviderRegistry → ToolProvider
- ✅ **Hot-swappable**: Providers can be registered/unregistered at runtime
- ✅ **Transparent to compiler**: Tool calls compile to `CallTool` opcode without provider knowledge

**Weaknesses:**
- ❌ **No fallback chains**: If primary provider fails, no automatic failover
- ❌ **No caching layer**: Every call hits the provider (no result memoization)
- ❌ **No request batching**: Multiple tool calls can't be batched into one API request

### 1.3 Compiler Effect System

**Location**: `rust/lumen-compiler/src/compiler/resolve.rs:1619-1628`

```rust
fn effect_from_tool(alias: &str, table: &SymbolTable) -> Option<String> {
    // Check explicit effect bindings (bind effect X to Y)
    if let Some(bind) = table.effect_binds.iter().find(|b| b.tool_alias == alias) {
        let root = bind.effect_path.split('.').next().unwrap_or("");
        return Some(root.to_string());
    }
    // Fallback: derive from tool path (e.g., "http.get" -> "http")
    let root = alias.split('.').next().unwrap_or("external");
    Some(root.to_string())
}
```

**Key Insight**: The compiler **NEVER assumes a tool maps to a specific provider**. Effects come from:
1. Explicit `bind effect X to ToolAlias` declarations (preferred)
2. Heuristic: first segment of tool name (e.g., `http.get` → `http` effect)

**Strengths:**
- ✅ **Zero provider coupling**: No "gemini", "openai", or "anthropic" strings in compiler
- ✅ **User-controlled bindings**: Developers declare effect mappings explicitly
- ✅ **Effect provenance tracking**: Compiler traces why a cell needs an effect

**Weaknesses:**
- ❌ **No provider capability checks**: Compiler can't validate "this provider doesn't support vision"
- ❌ **No model-level constraints**: Can't express "this tool requires GPT-4 or better"

### 1.4 Configuration System

**Location**: `rust/lumen-cli/src/config.rs:53-84`

```toml
[providers]
"llm.chat" = "openai-compatible"
"http.get" = "builtin-http"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
```

**Architecture:**
- Tool names (e.g., `llm.chat`) map to provider types (e.g., `openai-compatible`)
- Provider-specific config stored under `[providers.config.<name>]`
- MCP servers auto-expose all their tools under qualified names

**Strengths:**
- ✅ **Declarative**: Users swap providers by editing `lumen.toml`
- ✅ **Environment-aware**: API keys read from env vars (secure)
- ✅ **MCP-first design**: MCP servers auto-register tools (future-proof)

**Weaknesses:**
- ❌ **No provider validation**: Config can reference non-existent providers
- ❌ **No capability manifest**: Can't declare "this provider supports vision"
- ❌ **No model catalog**: Can't list available models per provider

---

## 2. Provider-Specific Leakage Analysis

### 2.1 Compiler Analysis

**Search Command**: `grep -r "gemini\|openai\|anthropic\|ollama" rust/lumen-compiler/src --ignore-case`

**Result**: ✅ **ZERO MATCHES**

The compiler is **100% provider-agnostic**. No hardcoded provider names, no provider-specific logic.

### 2.2 VM Analysis

**Search Command**: `grep -r "gemini\|openai\|anthropic\|ollama" rust/lumen-vm/src --ignore-case`

**Result**: ✅ **ZERO MATCHES**

The VM is **100% provider-agnostic**. All provider interaction happens via the `ToolDispatcher` interface.

### 2.3 Provider Crate Analysis

Provider-specific code is correctly **isolated to provider crates**:

```
rust/lumen-provider-gemini/src/lib.rs     — Gemini API wrapper
rust/lumen-provider-http/src/lib.rs       — Generic HTTP provider
rust/lumen-provider-mcp/src/lib.rs        — MCP bridge (universal)
rust/lumen-provider-json/                 — JSON processing
rust/lumen-provider-fs/                   — Filesystem tools
rust/lumen-provider-crypto/               — Cryptographic tools
rust/lumen-provider-env/                  — Environment variables
```

**Key Finding**: Provider-specific knowledge lives **only** in provider crates. The core runtime, compiler, and VM are **fully decoupled**.

### 2.4 Effect Binding Mechanism

**Location**: `SPEC.md:127-133`

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

**Mechanism**: Explicit `bind effect` declarations map effect names to tool aliases. The compiler does **NOT** infer provider types from tool names.

**Verification**: `effect_from_tool()` checks `table.effect_binds` first, falls back to heuristic (tool name prefix), but **never** hardcodes provider types.

---

## 3. Tool Calling Format Differences

### 3.1 The Problem

Different AI providers use **incompatible tool calling formats**:

| Provider   | Tool Declaration Format | Tool Call Format | Response Format |
|------------|------------------------|------------------|-----------------|
| **OpenAI** | `tools: [{ type: "function", function: {...} }]` | `tool_calls: [{ id, type: "function", function: {name, arguments} }]` | `{ role: "tool", tool_call_id, content }` |
| **Anthropic** | `tools: [{ name, description, input_schema }]` | `{ type: "tool_use", id, name, input }` content blocks | `{ type: "tool_result", tool_use_id, content }` |
| **Gemini** | `functionDeclarations: [{ name, parameters }]` | `functionCall: { name, args }` | `functionResponse: { name, response }` |

**Source**: [eesel.ai OpenAI vs Anthropic vs Gemini API comparison](https://www.eesel.ai/blog/openai-api-vs-anthropic-api-vs-gemini-api)

### 3.2 Current Lumen Approach

**Location**: `rust/lumen-provider-gemini/src/lib.rs:126-191`

Lumen's Gemini provider **manually translates** Lumen's generic JSON to Gemini's specific format:

```rust
fn execute_generate(&self, input: Value) -> Result<Value, ToolError> {
    let prompt = input.get("prompt").and_then(|p| p.as_str())...;

    // Gemini-specific request structure
    let mut contents = vec![json!({
        "role": "user",
        "parts": [{"text": prompt}]
    })];

    let body = json!({ "contents": contents });
    // POST to Gemini API...
}
```

**Problem**: Every provider requires **custom translation logic**. This is acceptable **IF** isolated to provider crates, but **NOT** if it leaks into core runtime.

### 3.3 Recommendation: Provider-Side Translation

**Architecture**:
```
Lumen Tool Call (generic JSON)
    ↓
ToolProvider::call(input: Value) → Result<Value, ToolError>
    ↓
Provider-specific translation (e.g., GeminiProvider::execute_generate)
    ↓
Provider API (Gemini, OpenAI, Anthropic, etc.)
    ↓
Provider-specific response parsing
    ↓
Generic JSON output
    ↓
Lumen VM
```

**Current Status**: ✅ **CORRECTLY IMPLEMENTED**

Each provider crate handles its own format translation. The `ToolProvider` trait ensures the interface is always `Value -> Value`.

**Future Work**:
- [ ] Standardize common translation patterns (e.g., `system` vs `systemInstruction`)
- [ ] Provide helper utilities for JSON Schema → provider-specific schema conversion
- [ ] Document provider implementation guide with examples

---

## 4. Structured Output Generation

### 4.1 The Challenge

Modern AI providers support **structured outputs** (JSON mode, schema-constrained generation):

| Provider   | Structured Output Support | Mechanism |
|------------|--------------------------|-----------|
| **OpenAI** | ✅ Full (GPT-4, GPT-3.5) | `response_format: { type: "json_schema", json_schema: {...} }` |
| **Anthropic** | ❌ No native support | Use prompt engineering + validation |
| **Gemini** | ✅ Partial (response MIME type) | `generationConfig: { responseMimeType: "application/json" }` |
| **Ollama** | ✅ Full (local models) | `format: "json"` parameter |

**Source**: [Rost Glukhov — Structured Output Comparison](https://www.glukhov.org/post/2025/10/structured-output-comparison-popular-llm-providers/)

### 4.2 Current Lumen Approach

**Observation**: Lumen treats tool outputs as **untyped JSON** (`serde_json::Value`).

**Location**: `rust/lumen-runtime/src/tools.rs:95-96`

```rust
pub struct ToolSchema {
    pub input_schema: serde_json::Value,  // JSON Schema for inputs
    pub output_schema: serde_json::Value, // JSON Schema for outputs
    // ...
}
```

**Problem**: `output_schema` is **declared but not validated**. The VM accepts whatever the provider returns.

### 4.3 Recommendation: Runtime Output Validation

**Strategy**:
1. **Declare output schemas** in `ToolSchema` (already done)
2. **Validate provider responses** against declared schema
3. **Reject malformed outputs** with `ToolError::SchemaViolation`

**Pseudocode**:
```rust
impl ToolDispatcher for ProviderRegistry {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        let provider = self.get(&request.tool_id)?;
        let output = provider.call(request.args.clone())?;

        // NEW: Validate output against schema
        let schema = &provider.schema().output_schema;
        if !validate_json_schema(&output, schema) {
            return Err(ToolError::SchemaViolation(format!(
                "Tool '{}' returned output that doesn't match schema",
                request.tool_id
            )));
        }

        Ok(ToolResponse { outputs: output, ... })
    }
}
```

**Benefits**:
- ✅ **Type safety**: Lumen code gets predictable JSON structures
- ✅ **Early failure**: Bad provider responses caught before reaching user code
- ✅ **Provider-agnostic**: Works for any provider that returns JSON

**Implementation**:
- Use [`jsonschema` crate](https://crates.io/crates/jsonschema) for validation
- Add `ToolError::SchemaViolation` variant
- Emit debug event when validation fails (for tracing)

### 4.4 Provider-Side Schema Translation

**Challenge**: Providers use different schema formats (OpenAI JSON Schema vs Gemini Parameters).

**Solution**: Each provider translates Lumen's JSON Schema to its native format.

**Example** (for future OpenAI provider):
```rust
impl OpenAIProvider {
    fn translate_schema(&self, lumen_schema: &Value) -> Value {
        // Convert Lumen JSON Schema → OpenAI's json_schema format
        json!({
            "type": "json_schema",
            "json_schema": {
                "name": "response",
                "strict": true,
                "schema": lumen_schema
            }
        })
    }
}
```

---

## 5. Capability Detection and Negotiation

### 5.1 The Problem

AI models have **heterogeneous capabilities**:

| Capability | GPT-4o | Claude 3.5 Sonnet | Gemini 2.0 | Llama 3.2 Vision |
|------------|--------|-------------------|------------|------------------|
| **Text generation** | ✅ | ✅ | ✅ | ✅ |
| **Vision (image input)** | ✅ | ✅ | ✅ | ✅ |
| **Tool calling** | ✅ | ✅ | ✅ | ❌ |
| **JSON mode** | ✅ | ❌ (prompt only) | ✅ | ✅ |
| **Streaming** | ✅ | ✅ | ✅ | ✅ |
| **Long context (>100k tokens)** | ✅ (128k) | ✅ (200k) | ✅ (2M) | ❌ (8k) |

**Source**: [DataCamp — Top Vision Language Models 2025](https://www.datacamp.com/blog/top-vision-language-models)

**Current Lumen Status**: ❌ **NO CAPABILITY DETECTION**

Lumen assumes all providers support all features. If a user calls a vision tool on a text-only model, the error is provider-specific and opaque.

### 5.2 Recommendation: Capability Manifest

**Add to `ToolProvider` trait**:
```rust
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn schema(&self) -> &ToolSchema;
    fn call(&self, input: Value) -> Result<Value, ToolError>;

    // NEW: Capability advertisement
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default() // Assume minimal capabilities
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderCapabilities {
    pub supports_vision: bool,
    pub supports_tool_calling: bool,
    pub supports_json_mode: bool,
    pub supports_streaming: bool,
    pub max_context_tokens: Option<usize>,
    pub supported_modalities: Vec<Modality>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
}
```

**Usage Example**:
```rust
impl ToolProvider for GeminiProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: true,
            supports_tool_calling: true,
            supports_json_mode: true,
            supports_streaming: true,
            max_context_tokens: Some(2_000_000), // Gemini 2.0: 2M context
            supported_modalities: vec![Modality::Text, Modality::Image],
        }
    }
}
```

**Runtime Checks**:
```rust
// Before dispatching a vision tool call
let provider = registry.get(&request.tool_id)?;
if request.has_image_input() && !provider.capabilities().supports_vision {
    return Err(ToolError::UnsupportedCapability(
        format!("Provider '{}' does not support vision", provider.name())
    ));
}
```

**Benefits**:
- ✅ **Early failure**: Reject unsupported operations at call time, not after API request
- ✅ **Better errors**: "Provider X doesn't support vision" vs. "HTTP 400 Bad Request"
- ✅ **Capability routing**: Future: auto-select provider based on required capabilities

---

## 6. Error Normalization Strategy

### 6.1 Current Error Handling

**Location**: `rust/lumen-runtime/src/tools.rs:18-30`

```rust
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool invocation failed: {0}")]
    InvocationFailed(String),
    #[error("policy violation: {0}")]
    PolicyViolation(String),
    #[error("rate limit exceeded for tool: {0}")]
    RateLimit(String),
    #[error("provider not registered: {0}")]
    NotRegistered(String),
}
```

**Problem**: Providers return wildly different error formats:

| Provider | Rate Limit Error | Auth Error | Model Error |
|----------|-----------------|------------|-------------|
| **OpenAI** | `{ error: { type: "rate_limit_exceeded", code: "rate_limit_exceeded" } }` HTTP 429 | `{ error: { type: "invalid_auth", code: "invalid_api_key" } }` HTTP 401 | `{ error: { type: "model_error", code: "model_not_found" } }` HTTP 404 |
| **Anthropic** | `{ type: "error", error: { type: "rate_limit_error", message: "..." } }` HTTP 429 | `{ type: "error", error: { type: "authentication_error" } }` HTTP 401 | `{ type: "error", error: { type: "invalid_request_error" } }` HTTP 400 |
| **Gemini** | `{ error: { code: 429, message: "Resource exhausted", status: "RESOURCE_EXHAUSTED" } }` | `{ error: { code: 401, message: "API key not valid" } }` | `{ error: { code: 404, message: "Model not found" } }` |

**Sources**:
- [Anthropic 429/529 Error Guide](https://www.cursor-ide.com/blog/claude-ai-rate-exceeded)
- [OpenAI Rate Limits](https://platform.openai.com/docs/guides/rate-limits)
- [Gemini Rate Limit Updates](https://www.aifreeapi.com/en/posts/gemini-advanced-rate-limit)

### 6.2 Recommendation: Enhanced Error Taxonomy

**Expand `ToolError` enum**:
```rust
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),

    #[error("provider not registered: {0}")]
    NotRegistered(String),

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    // NEW: Structured error variants
    #[error("rate limit exceeded: {message}")]
    RateLimit {
        message: String,
        retry_after_ms: Option<u64>,  // From Retry-After header
        limit_type: RateLimitType,    // RPM, TPM, RPD, etc.
    },

    #[error("authentication failed: {0}")]
    AuthenticationError(String),

    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("provider unavailable: {0}")]
    ServiceUnavailable {
        message: String,
        retry_after_ms: Option<u64>,
    },

    #[error("unsupported capability: {0}")]
    UnsupportedCapability(String),

    #[error("output schema violation: {0}")]
    SchemaViolation(String),

    #[error("invocation failed: {0}")]
    InvocationFailed(String),  // Generic fallback
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitType {
    RequestsPerMinute,
    TokensPerMinute,
    RequestsPerDay,
    ImagesPerMinute,
    ConcurrentRequests,
}
```

**Provider Translation** (example for Gemini):
```rust
impl GeminiProvider {
    fn execute_generate(&self, input: Value) -> Result<Value, ToolError> {
        let response = client.post(&url).json(&body).send()
            .map_err(|e| ToolError::InvocationFailed(format!("HTTP error: {}", e)))?;

        let status = response.status();
        let response_body: Value = response.json()
            .map_err(|e| ToolError::InvocationFailed(format!("JSON parse error: {}", e)))?;

        // NEW: Normalize Gemini errors
        if !status.is_success() {
            return Err(Self::normalize_error(status.as_u16(), &response_body));
        }

        // Extract text...
    }

    fn normalize_error(status: u16, body: &Value) -> ToolError {
        let message = body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");

        match status {
            429 => ToolError::RateLimit {
                message: message.to_string(),
                retry_after_ms: None, // Extract from headers if available
                limit_type: RateLimitType::RequestsPerMinute,
            },
            401 | 403 => ToolError::AuthenticationError(message.to_string()),
            404 => ToolError::ModelNotFound(message.to_string()),
            400 => ToolError::InvalidRequest(message.to_string()),
            503 => ToolError::ServiceUnavailable {
                message: message.to_string(),
                retry_after_ms: None,
            },
            _ => ToolError::InvocationFailed(format!("API error {}: {}", status, message)),
        }
    }
}
```

**Benefits**:
- ✅ **Actionable errors**: "Rate limit (retry in 30s)" vs. "Invocation failed"
- ✅ **Automatic retries**: VM can read `retry_after_ms` and schedule retries
- ✅ **Circuit breaker**: Detect `ServiceUnavailable` and pause provider for N seconds
- ✅ **Better debugging**: Structured errors with context

---

## 7. Trait Redesign Recommendations

### 7.1 Should There Be a Separate `AIProvider` Trait?

**Question**: Should AI providers (LLMs) have a separate trait from generic tools (HTTP, DB)?

**Analysis**:

| Aspect | Generic Tools (HTTP, DB, etc.) | AI Providers (LLMs) |
|--------|-------------------------------|---------------------|
| **Input/Output** | Structured (JSON with known schema) | Semi-structured (text + optional JSON) |
| **Capabilities** | Fixed (HTTP GET always works) | Variable (vision, tool calling, streaming) |
| **Errors** | HTTP status codes | Rate limits, context length, moderation |
| **Configuration** | URL, auth token | Model name, temperature, max_tokens, system prompt |
| **Versioning** | API version (stable) | Model version (frequent changes) |

**Recommendation**: ⚠️ **CONSIDER** separate trait hierarchy:

```rust
// Base trait (current ToolProvider)
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn schema(&self) -> &ToolSchema;
    fn call(&self, input: Value) -> Result<Value, ToolError>;
    fn capabilities(&self) -> ProviderCapabilities;
}

// NEW: AI-specific extension
pub trait AIProvider: ToolProvider {
    /// List available models
    fn list_models(&self) -> Vec<ModelInfo>;

    /// Generate text with streaming support
    fn generate_streaming(
        &self,
        prompt: &str,
        config: GenerationConfig,
        callback: Box<dyn FnMut(StreamChunk)>,
    ) -> Result<String, ToolError>;

    /// Generate with tool calling support
    fn generate_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
        config: GenerationConfig,
    ) -> Result<GenerationResult, ToolError>;

    /// Generate structured output (JSON mode)
    fn generate_structured(
        &self,
        prompt: &str,
        schema: &Value,
        config: GenerationConfig,
    ) -> Result<Value, ToolError>;
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub context_length: usize,
    pub supports_vision: bool,
    pub supports_tool_calling: bool,
    pub cost_per_1k_tokens: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GenerationConfig {
    pub model: String,
    pub temperature: f64,
    pub max_tokens: usize,
    pub system_prompt: Option<String>,
}
```

**Pros**:
- ✅ AI-specific methods (streaming, tool calling) separated from generic tools
- ✅ Model catalog accessible via `list_models()`
- ✅ Easier to add AI-specific features (embeddings, fine-tuning, etc.)

**Cons**:
- ❌ More complexity (two trait hierarchies)
- ❌ Provider must implement both `ToolProvider` and `AIProvider`
- ❌ Lumen's `use tool` abstraction treats all tools uniformly (design principle)

**Decision**: ⏸️ **DEFER for now**. Current `ToolProvider` trait is sufficient. Revisit if AI-specific features (streaming, tool calling) become first-class in Lumen's language design.

### 7.2 Alternative: Capability-Based Design

Instead of separate traits, extend `ProviderCapabilities` to advertise AI-specific features:

```rust
pub struct ProviderCapabilities {
    pub supports_vision: bool,
    pub supports_tool_calling: bool,
    pub supports_json_mode: bool,
    pub supports_streaming: bool,
    pub max_context_tokens: Option<usize>,

    // NEW: AI model catalog
    pub available_models: Vec<ModelInfo>,

    // NEW: Cost tracking
    pub cost_per_1k_input_tokens: Option<f64>,
    pub cost_per_1k_output_tokens: Option<f64>,
}
```

**Benefit**: Single trait, capability-driven dispatch. The VM checks capabilities before calling, and providers advertise what they support.

**Recommended Approach**: ✅ **Use capability-based design** (simpler, more flexible).

---

## 8. Abstraction Layer Design

### 8.1 Current Architecture (Correct)

```
┌─────────────────────────────────────────────────────────────┐
│                    Lumen Language Layer                      │
│  (use tool, grant, bind effect, effect rows)                │
│  PROVIDER-AGNOSTIC: Compiler knows nothing about providers   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                   LIR Bytecode (CallTool)                    │
│  Opcode: CallTool A B C (tool_id in constant table)         │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                     VM Runtime Layer                         │
│  tool_dispatcher: Option<Box<dyn ToolDispatcher>>           │
│  Dispatches ToolRequest → ToolResponse                       │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    ProviderRegistry                          │
│  Implements ToolDispatcher                                   │
│  Routes tool_id → ToolProvider                               │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                  ToolProvider Trait                          │
│  Generic interface: Value → Result<Value, ToolError>        │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│              Provider Implementations                        │
│  - GeminiProvider (lumen-provider-gemini)                   │
│  - HttpProvider (lumen-provider-http)                       │
│  - McpToolProvider (lumen-provider-mcp)                     │
│  - Future: OpenAIProvider, AnthropicProvider, etc.          │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                  External Provider APIs                      │
│  Gemini API, OpenAI API, Anthropic API, MCP servers, etc.   │
└─────────────────────────────────────────────────────────────┘
```

**Correctness**: ✅ **YES**. This architecture is **provider-agnostic** at every layer except the provider crates themselves.

### 8.2 Missing: Abstraction Layer for Common AI Patterns

**Problem**: Every AI provider manually implements:
- Message formatting (system, user, assistant roles)
- Tool declaration translation (OpenAI `functions` vs Anthropic `tools` vs Gemini `functionDeclarations`)
- Response parsing (text extraction from nested JSON)

**Solution**: Provide **helper utilities** in `lumen-runtime`:

```rust
// lumen-runtime/src/ai_helpers.rs

/// Common message format used internally by Lumen
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Translate Lumen messages to OpenAI format
pub fn messages_to_openai(messages: &[Message]) -> Value {
    json!(messages.iter().map(|m| json!({
        "role": match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        },
        "content": m.content
    })).collect::<Vec<_>>())
}

/// Translate Lumen messages to Anthropic format
pub fn messages_to_anthropic(messages: &[Message]) -> (Option<String>, Value) {
    // Extract system message, format rest as messages
    let system = messages.iter()
        .find(|m| m.role == Role::System)
        .map(|m| m.content.clone());

    let msgs = messages.iter()
        .filter(|m| m.role != Role::System)
        .map(|m| json!({
            "role": if m.role == Role::User { "user" } else { "assistant" },
            "content": m.content
        }))
        .collect::<Vec<_>>();

    (system, json!(msgs))
}

/// Extract text from OpenAI response
pub fn extract_openai_text(response: &Value) -> Option<String> {
    response.get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(String::from)
}

/// Extract text from Anthropic response
pub fn extract_anthropic_text(response: &Value) -> Option<String> {
    response.get("content")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(String::from)
}

/// Extract text from Gemini response
pub fn extract_gemini_text(response: &Value) -> Option<String> {
    response.get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(String::from)
}
```

**Benefits**:
- ✅ **DRY**: Providers reuse common translation logic
- ✅ **Consistency**: All providers format messages the same way
- ✅ **Maintainability**: Update translation in one place

---

## 9. Future-Proofing Recommendations

### 9.1 Adding a New Provider Should Be Trivial

**Current Process** (for adding `OpenAIProvider`):

1. Create `rust/lumen-provider-openai/Cargo.toml`
2. Implement `ToolProvider` trait in `src/lib.rs`
3. Handle OpenAI-specific request/response formats
4. Register in `lumen.toml`: `"llm.chat" = "openai"`
5. Wire up in CLI: `registry.register("openai", Box::new(OpenAIProvider::new(...)))`

**Bottlenecks**:
- ❌ Step 3: Manual translation of JSON formats (solved by helper utilities in §8.2)
- ❌ Step 5: CLI code must know about every provider (should be auto-discovered)

**Recommendation**: **Plugin Architecture**

```rust
// lumen-runtime/src/plugin.rs

/// Providers can be dynamically loaded from shared libraries
pub trait ProviderPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn create(&self, config: &Value) -> Result<Box<dyn ToolProvider>, String>;
}

// Example: lumen-provider-openai as a .so plugin
#[no_mangle]
pub extern "C" fn lumen_plugin_init() -> *mut dyn ProviderPlugin {
    Box::into_raw(Box::new(OpenAIPlugin))
}

struct OpenAIPlugin;

impl ProviderPlugin for OpenAIPlugin {
    fn name(&self) -> &str { "openai" }
    fn create(&self, config: &Value) -> Result<Box<dyn ToolProvider>, String> {
        let api_key = config.get("api_key_env")...;
        Ok(Box::new(OpenAIProvider::new(api_key)))
    }
}
```

**Benefits**:
- ✅ Third-party providers can be added **without recompiling Lumen**
- ✅ CLI auto-discovers providers from `.so` files in `~/.lumen/providers/`
- ✅ Configuration-driven: `lumen.toml` declares which plugins to load

**Trade-off**: More complexity (dynamic loading, ABI stability). **Defer** until Lumen has 5+ providers.

### 9.2 Provider Versioning and Migration

**Problem**: Provider APIs change (e.g., OpenAI GPT-4 → GPT-5, Gemini 1.5 → 2.0).

**Current Status**: ❌ **NO VERSIONING STRATEGY**

**Recommendation**: **Semantic Versioning for Providers**

```rust
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str; // Semver: "1.2.3"
    // ...
}

// In lumen.toml:
[providers.config.openai]
version = "^2.0"  // Accept 2.x.x, reject 3.0.0
```

**VM behavior**:
- If provider version doesn't match config requirement, emit **warning** (not error)
- Log provider version in trace events for debugging

### 9.3 Fallback Chains

**Use Case**: If primary provider fails (rate limit, downtime), fall back to secondary.

**Example** (`lumen.toml`):
```toml
[providers]
"llm.chat" = ["openai", "anthropic", "gemini"]  # Try in order

[providers.config.openai]
base_url = "https://api.openai.com/v1"
priority = 1

[providers.config.anthropic]
base_url = "https://api.anthropic.com/v1"
priority = 2

[providers.config.gemini]
base_url = "https://generativelanguage.googleapis.com/v1"
priority = 3
```

**ProviderRegistry logic**:
```rust
fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
    let providers = self.get_fallback_chain(&request.tool_id)?;

    for provider in providers {
        match provider.call(request.args.clone()) {
            Ok(output) => return Ok(ToolResponse { outputs: output, ... }),
            Err(ToolError::RateLimit { retry_after_ms, .. }) => {
                // Try next provider
                continue;
            }
            Err(e) => return Err(e), // Fatal error, don't retry
        }
    }

    Err(ToolError::InvocationFailed("All providers failed".into()))
}
```

**Benefits**:
- ✅ **High availability**: Automatic failover
- ✅ **Cost optimization**: Use cheap provider first, fall back to expensive
- ✅ **User-controlled**: Configured in `lumen.toml`, not hardcoded

---

## 10. MCP as Universal Bridge

### 10.1 MCP Architecture in Lumen

**Current Implementation**: `rust/lumen-provider-mcp/src/lib.rs`

MCP (Model Context Protocol) is a **universal tool integration standard** announced by Anthropic in November 2024. As of 2025, it has **97M+ monthly SDK downloads** and first-class support in ChatGPT, Claude, Cursor, Gemini, VS Code, and more.

**Source**: [MCP Specification](https://modelcontextprotocol.io/specification/2025-11-25), [MCP November 2025 Updates](https://medium.com/@dave-patten/mcps-next-phase-inside-the-november-2025-specification-49f298502b03)

**Lumen's MCP Integration**:

```rust
pub trait McpTransport: Send + Sync {
    fn send_request(&self, method: &str, params: Value) -> Result<Value, String>;
}

pub struct McpToolProvider {
    server_name: String,
    tool_schema: McpToolSchema,
    transport: Arc<dyn McpTransport>,
}

impl ToolProvider for McpToolProvider {
    fn call(&self, input: Value) -> Result<Value, ToolError> {
        let params = json!({
            "name": self.tool_schema.name,
            "arguments": input,
        });
        self.transport.send_request("tools/call", params)
            .map_err(ToolError::InvocationFailed)
    }
}
```

**Key Features**:
- ✅ **Auto-discovery**: `discover_tools(server_name, transport)` fetches tool list from MCP server
- ✅ **Qualified names**: Tools registered as `server_name.tool_name` (e.g., `github.create_issue`)
- ✅ **JSON-RPC 2.0**: Standard protocol (same as LSP)
- ✅ **Stdio transport**: Spawn subprocess (`npx @modelcontextprotocol/server-github`)

**Location**: `rust/lumen-provider-mcp/src/lib.rs:293-320`

### 10.2 Should MCP Be the Primary Provider Interface?

**Thesis**: Instead of wrapping every AI provider (OpenAI, Anthropic, Gemini) as a `ToolProvider`, wrap them as **MCP servers**.

**Architecture**:
```
Lumen VM
    ↓
McpToolProvider (universal)
    ↓
MCP Server (OpenAI wrapper, Anthropic wrapper, Gemini wrapper)
    ↓
Provider API
```

**Pros**:
- ✅ **Single integration point**: Lumen only implements `McpToolProvider`
- ✅ **Third-party ecosystem**: Leverage existing MCP servers (GitHub, Slack, Notion, etc.)
- ✅ **Tooling**: MCP has official debuggers, server registries, IDE support
- ✅ **Future-proof**: MCP is an open standard backed by Anthropic, OpenAI, Google

**Cons**:
- ❌ **Indirection overhead**: Extra JSON-RPC layer
- ❌ **Subprocess overhead**: Spawning Node.js for `npx` servers
- ❌ **MCP server maturity**: OpenAI/Gemini MCP wrappers don't exist yet (need to build)
- ❌ **Loss of control**: Can't customize provider behavior (retries, caching, etc.)

**Recommendation**: **HYBRID APPROACH**

1. **Native providers** for core AI tools (OpenAI, Anthropic, Gemini) as direct `ToolProvider` implementations (performance, control)
2. **MCP bridge** for third-party integrations (GitHub, Slack, databases, etc.)
3. **MCP-compatible** native providers (expose them as MCP servers for external tools)

**Example** (`lumen.toml`):
```toml
[providers]
"llm.chat" = "openai-native"          # Direct integration
"github.create_issue" = "mcp:github"  # MCP bridge

[providers.config.openai-native]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
```

### 10.3 MCP November 2025 Enhancements

**Key Updates** (source: [MCP Spec November 2025](http://blog.modelcontextprotocol.io/posts/2025-11-25-first-mcp-anniversary/)):

- **Asynchronous operations**: Long-running tasks (e.g., CI/CD jobs) can return immediately with task ID
- **Statelessness**: Servers no longer maintain session state (better scalability)
- **Server identity**: Servers advertise capabilities, version, supported features
- **Community registry**: Official registry for discovering MCP servers

**Lumen Integration Tasks**:
- [ ] Support async MCP operations (task ID → poll for completion)
- [ ] Implement stateless MCP transport (send auth token with every request)
- [ ] Query server identity for capability detection
- [ ] Integrate with MCP community registry (auto-install servers via `lumen pkg add mcp:github`)

---

## 11. Implementation Roadmap

### Phase 1: Error Normalization (1-2 weeks)

**Goal**: Providers return structured errors that the VM can act on.

**Tasks**:
1. ✅ Expand `ToolError` enum with `RateLimit`, `AuthenticationError`, `ModelNotFound`, etc.
2. ✅ Update `GeminiProvider` to normalize Gemini errors
3. [ ] Implement error normalization for future `OpenAIProvider`, `AnthropicProvider`
4. [ ] Add `retry_after_ms` extraction from HTTP headers
5. [ ] VM: emit debug events for all error types (for tracing)

**Acceptance Criteria**:
- Rate limit errors include retry duration
- Auth errors are distinguishable from transient failures
- All provider errors map to `ToolError` variants

### Phase 2: Capability Detection (2-3 weeks)

**Goal**: Providers advertise capabilities; VM rejects unsupported operations early.

**Tasks**:
1. ✅ Define `ProviderCapabilities` struct
2. ✅ Add `capabilities()` method to `ToolProvider` trait
3. [ ] Implement capabilities for `GeminiProvider` (vision, tool calling, JSON mode, 2M context)
4. [ ] Implement capabilities for future providers
5. [ ] VM: check capabilities before dispatching tool calls
6. [ ] Add `ToolError::UnsupportedCapability` variant
7. [ ] Document capability matrix in provider docs

**Acceptance Criteria**:
- Calling a vision tool on text-only provider fails with clear error
- Provider capabilities queryable at runtime
- `lumen providers` CLI command lists capabilities per provider

### Phase 3: Structured Output Validation (1-2 weeks)

**Goal**: Enforce output schemas at runtime; reject malformed provider responses.

**Tasks**:
1. ✅ Add `jsonschema` crate dependency to `lumen-runtime`
2. [ ] Implement `validate_json_schema(value, schema)` helper
3. [ ] Update `ProviderRegistry::dispatch()` to validate outputs
4. [ ] Add `ToolError::SchemaViolation` variant
5. [ ] Emit debug event when validation fails
6. [ ] Document schema validation in `SPEC.md`

**Acceptance Criteria**:
- Provider outputs validated against declared `output_schema`
- Schema violations logged with detailed error messages
- Tests: malformed outputs rejected, well-formed outputs pass

### Phase 4: AI Helper Utilities (1 week)

**Goal**: Reduce boilerplate for AI provider implementations.

**Tasks**:
1. [ ] Create `lumen-runtime/src/ai_helpers.rs`
2. [ ] Implement `messages_to_openai()`, `messages_to_anthropic()`, `messages_to_gemini()`
3. [ ] Implement `extract_openai_text()`, `extract_anthropic_text()`, `extract_gemini_text()`
4. [ ] Implement `schema_to_openai_function()`, `schema_to_gemini_parameters()`
5. [ ] Refactor `GeminiProvider` to use helpers
6. [ ] Document helpers in provider implementation guide

**Acceptance Criteria**:
- Providers reuse common translation logic
- Adding a new provider requires <100 lines of code (excluding API client)

### Phase 5: Provider Fallback Chains (2-3 weeks)

**Goal**: Automatic failover to secondary providers on rate limits or downtime.

**Tasks**:
1. [ ] Extend `lumen.toml` to support provider arrays: `"llm.chat" = ["openai", "anthropic"]`
2. [ ] Update `ProviderRegistry` to track fallback chains
3. [ ] Implement retry logic in `dispatch()` (skip rate-limited providers)
4. [ ] Add circuit breaker: pause failed providers for N seconds
5. [ ] Emit debug events for provider failover
6. [ ] Document fallback chains in config docs

**Acceptance Criteria**:
- Rate-limited provider triggers automatic failover
- Failed providers excluded from rotation until timeout expires
- Trace events show which provider handled each call

### Phase 6: MCP Async Operations (2 weeks)

**Goal**: Support long-running MCP tasks (async operations from Nov 2025 spec).

**Tasks**:
1. [ ] Research MCP async operation spec
2. [ ] Extend `McpTransport` to handle task IDs
3. [ ] Implement polling mechanism for async tasks
4. [ ] Add timeout for async tasks (fail if not completed in N seconds)
5. [ ] Emit debug events for task start, polling, completion
6. [ ] Test with real MCP server that supports async

**Acceptance Criteria**:
- MCP tools can return task IDs
- VM polls task status until completion or timeout
- Async operations integrated with Lumen's future system

### Phase 7: Provider Plugin Architecture (4-6 weeks, OPTIONAL)

**Goal**: Third-party providers loadable as dynamic libraries.

**Tasks**:
1. [ ] Design `ProviderPlugin` trait for dynamic loading
2. [ ] Implement plugin discovery (`~/.lumen/providers/*.so`)
3. [ ] Test with sample plugin (`libprovider_openai.so`)
4. [ ] Document plugin API and build process
5. [ ] Add `lumen plugin install <name>` CLI command

**Acceptance Criteria**:
- Providers loadable without recompiling Lumen
- Plugins registered via `lumen.toml`
- Documentation for building third-party providers

---

## 12. Research Sources

### LiteLLM and Provider Abstraction

- [ProxAI — LLM Abstraction Layer: Why Your Codebase Needs One in 2025](https://www.proxai.co/blog/archive/llm-abstraction-layer)
- [TrueFoundry — Top 5 LiteLLM Alternatives in 2026](https://www.truefoundry.com/blog/litellm-alternatives)
- [LiteLLM Official Site](https://www.litellm.ai/)
- [EvoLink — OpenRouter vs LiteLLM vs Build vs Managed](https://evolink.ai/blog/openrouter-vs-litellm-vs-build-vs-managed)

### Model Context Protocol (MCP)

- [MCP Specification — November 2025](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP Wikipedia](https://en.wikipedia.org/wiki/Model_Context_Protocol)
- [MCP One Year Anniversary — November 2025 Release](http://blog.modelcontextprotocol.io/posts/2025-11-25-first-mcp-anniversary/)
- [Dave Patten — MCP's Next Phase: November 2025 Spec](https://medium.com/@dave-patten/mcps-next-phase-inside-the-november-2025-specification-49f298502b03)
- [Pento — A Year of MCP: Industry Standard](https://www.pento.ai/blog/a-year-of-mcp-2025-review)

### Tool Calling Format Differences

- [eesel.ai — OpenAI vs Anthropic vs Gemini API Practical Guide 2025](https://www.eesel.ai/blog/openai-api-vs-anthropic-api-vs-gemini-api)
- [GitClear — OpenAI vs Anthropic vs Google vs GitHub Copilot LLM Examples 2025](https://www.gitclear.com/blog/openai_chatgpt_vs_anthropic_claude_vs_google_gemini_vs_github_copilot_llm_real_world_examples_from_2025)

### Structured Output Generation

- [Rost Glukhov — Structured Output Comparison (OpenAI, Gemini, Anthropic, Mistral, AWS Bedrock)](https://www.glukhov.org/post/2025/10/structured-output-comparison-popular-llm-providers/)
- [Matías Battaglia — Using Structured Outputs with LLMs](https://matiasbattaglia.com/2025/09/11/Using-Structured-Outputs-with-LLMs.html)
- [vLLM — Structured Outputs Documentation](https://docs.vllm.ai/en/v0.8.2/features/structured_outputs.html)

### Vision Language Models and Capabilities

- [Labellerr — Best Open-Source Vision Language Models of 2025](https://www.labellerr.com/blog/top-open-source-vision-language-models/)
- [DataCamp — Top 10 Vision Language Models in 2025](https://www.datacamp.com/blog/top-vision-language-models)
- [HuggingFace — Vision Language Models (Better, Faster, Stronger)](https://huggingface.co/blog/vlms-2025)
- [Roboflow — Top LLMs with Vision Capabilities](https://roboflow.com/model-feature/llms-with-vision-capabilities)

### Error Normalization and Rate Limits

- [Cursor IDE — Claude AI Rate Exceeded: 429 and 529 Errors Fix Guide 2025](https://www.cursor-ide.com/blog/claude-ai-rate-exceeded)
- [OpenAI — Rate Limits Documentation](https://platform.openai.com/docs/guides/rate-limits)
- [AI Free API — Gemini Advanced Rate Limit Complete 2025 Guide](https://www.aifreeapi.com/en/posts/gemini-advanced-rate-limit)
- [Sidetool — Mastering AI API Timeout Issues: Rate Limits in 2025](https://www.sidetool.co/post/master-ai-api-timeout-issues-2025/)
- [ORQ.ai — API Rate Limits Explained: Best Practices for 2025](https://orq.ai/blog/api-rate-limit)

---

## Summary and Next Steps

**Lumen's AI system is already 90% polymorphic**. The core architecture (compiler, VM, `ToolProvider` trait) is **provider-agnostic** and **extensible**. No provider-specific knowledge leaks into core code.

**Critical gaps** remain in:
1. **Capability detection** (can't query provider features)
2. **Error normalization** (raw provider errors, no retry hints)
3. **Structured output validation** (schemas declared but not enforced)
4. **Fallback chains** (no automatic failover on rate limits)

**Recommended immediate actions**:
1. ✅ **Phase 1**: Error normalization (expand `ToolError`, normalize provider errors)
2. ✅ **Phase 2**: Capability detection (add `capabilities()` to `ToolProvider`)
3. ✅ **Phase 3**: Output validation (enforce `output_schema` at runtime)
4. ⏸️ **Defer**: AI-specific trait (`AIProvider`), plugin architecture, fallback chains (nice-to-have)

**Long-term vision**: Lumen should support **any** AI provider (OpenAI, Anthropic, Gemini, Ollama, custom) with **zero compiler changes**. MCP provides the universal bridge; native providers provide performance. The system should degrade gracefully when providers change APIs, models, or pricing.

**File references for implementation**:
- Core trait: `rust/lumen-runtime/src/tools.rs`
- VM dispatch: `rust/lumen-vm/src/vm.rs` (lines 233, 285-286, 1755-1756)
- Compiler effects: `rust/lumen-compiler/src/compiler/resolve.rs` (lines 1619-1628)
- Config system: `rust/lumen-cli/src/config.rs`
- Gemini provider (reference): `rust/lumen-provider-gemini/src/lib.rs`
- MCP bridge (reference): `rust/lumen-provider-mcp/src/lib.rs`
- Language spec: `SPEC.md` (sections 2.6, 7.3, 10)

**This audit is complete and actionable.** Implementing Phases 1-3 will harden Lumen's AI system to withstand provider API changes for the next 5+ years.
