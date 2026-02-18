---
name: runtime-tools
description: Reference for Lumen's runtime tool system - providers, dispatch, policies, error types, capabilities, and retry
---

# Lumen Runtime Tool System

## Architecture (`rust/lumen-rt/src/services/tools.rs`)

### Tool Dispatch Flow
1. Look up tool alias in `ProviderRegistry`
2. Merge and validate grant policies via `validate_tool_policy()`
3. Execute via provider's `call()` method
4. Record trace event (tool name, input, output, duration, provider)
5. Return result

### Key Types
- `ToolDispatcher` (trait): Interface for tool providers
- `ProviderRegistry`: Registry of available providers
- `ToolRequest`: Input to a tool call
- `ToolResponse`: Output from a tool call

### ToolError Variants
- `NotFound(String)` — Tool not registered
- `InvalidArgs(String)` — Missing or malformed input
- `ExecutionFailed(String)` — Generic execution error
- `RateLimit { retry_after_ms, message }` — Rate limit exceeded
- `AuthError { message }` — Auth failure
- `ModelNotFound { model, provider }` — Model unavailable
- `Timeout { elapsed_ms, limit_ms }` — Time exceeded
- `ProviderUnavailable { provider, reason }` — Service down
- `OutputValidationFailed { expected_schema, actual }` — Schema mismatch

### Provider Capabilities
- `TextGeneration`, `Chat`, `Embedding`, `Vision`, `ToolUse`, `StructuredOutput`, `Streaming`

### RetryPolicy
- `max_retries: u32` (default: 3)
- `base_delay_ms: u64` (default: 100ms)
- `max_delay_ms: u64` (default: 10s)
- Strategies: Exponential, Fibonacci backoff

## Tool Providers
| Crate | Purpose |
|-------|---------|
| `lumen-provider-http` | HTTP requests |
| `lumen-provider-json` | JSON operations |
| `lumen-provider-fs` | Filesystem operations |
| `lumen-provider-mcp` | Model Context Protocol bridge |
| `lumen-provider-gemini` | Google Gemini AI |
| `lumen-provider-env` | Environment variables |
| `lumen-provider-crypto` | Cryptographic operations |

## Grant Policies
```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000
grant Chat max_tokens 4096
grant Chat domain "api.openai.com"
```
Constraint keys: `domain` (URL pattern), `timeout_ms`, `max_tokens`, custom keys.

## MCP Server Bridge
MCP servers exposed as Lumen tool providers. Each tool becomes `server.tool_name` with `"mcp"` effect kind.

## Runtime Services
- **Scheduler** (`services/scheduler.rs`): M:N work-stealing with crossbeam-deque
- **Trace** (`services/trace/`): Structured event recording, DAG visualization
- **Schema Drift** (`services/schema_drift.rs`): Detects API shape changes
- **Crypto** (`services/crypto.rs`): SHA-256, BLAKE3, HMAC, HKDF, Ed25519, UUID
- **HTTP** (`services/http.rs`): RequestBuilder, Router with path params
- **FS Async** (`services/fs_async.rs`): Async file ops, batch, file watcher
- **Net** (`services/net.rs`): IP/Socket, TCP/UDP config, DNS
