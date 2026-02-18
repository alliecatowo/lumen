# lumen-provider-gemini

Google Gemini AI tool provider for Lumen's runtime tool dispatch system.

## Overview

`lumen-provider-gemini` implements the `ToolProvider` trait to expose Google Gemini AI models as callable tools in Lumen programs. It provides text generation, multi-turn chat, and text embedding capabilities using the Gemini REST API.

The provider supports the Gemini 1.5 Flash and Pro models with configurable parameters including temperature, top-p, top-k, max tokens, and safety settings. API key authentication via environment variables.

## Provided Tools

| Tool | Description | Model |
|------|-------------|-------|
| `gemini.generate` | Single-shot text generation | gemini-1.5-flash |
| `gemini.chat` | Multi-turn conversational AI | gemini-1.5-flash |
| `gemini.embed` | Text embeddings (vectors) | text-embedding-004 |

## Usage in Lumen

### Text Generation

```lumen
use tool gemini.generate as Generate

grant Generate max_tokens 1024
grant Generate timeout_ms 30000

cell summarize(text: String) -> String / {gemini}
  let prompt = "Summarize the following text:\n\n{text}"
  let response = Generate(
    prompt: prompt,
    temperature: 0.7,
    max_tokens: 500
  )
  return response.text
end
```

### Multi-turn Chat

```lumen
use tool gemini.chat as Chat

cell assistant(messages: list[Json]) -> String / {gemini}
  let response = Chat(
    messages: messages,
    system: "You are a helpful coding assistant.",
    temperature: 0.9
  )
  return response.text
end

# Example messages format:
# [
#   {"role": "user", "content": "What is Rust?"},
#   {"role": "assistant", "content": "Rust is a systems programming language..."},
#   {"role": "user", "content": "How do I use it?"}
# ]
```

### Text Embeddings

```lumen
use tool gemini.embed as Embed

cell embed_documents(docs: list[String]) -> list[list[Float]] / {gemini}
  let embeddings = map(docs, fn(doc: String) -> list[Float] =>
    Embed(text: doc).embedding
  )
  return embeddings
end
```

## Tool Schemas

### gemini.generate

**Input**:
```json
{
  "prompt": "Write a haiku about programming",
  "system": "You are a creative poet",
  "temperature": 0.8,
  "max_tokens": 100,
  "top_p": 0.95,
  "top_k": 40
}
```

**Output**:
```json
{
  "text": "Code flows like water\nThrough silicon rivers deep\nLogic blooms in bits",
  "finish_reason": "STOP",
  "model": "gemini-1.5-flash"
}
```

### gemini.chat

**Input**:
```json
{
  "messages": [
    {"role": "user", "content": "Hello!"},
    {"role": "assistant", "content": "Hi! How can I help?"},
    {"role": "user", "content": "Tell me a joke"}
  ],
  "system": "You are a friendly assistant",
  "temperature": 0.9
}
```

**Output**:
```json
{
  "text": "Why did the programmer quit? They didn't get arrays!",
  "finish_reason": "STOP",
  "model": "gemini-1.5-flash"
}
```

### gemini.embed

**Input**:
```json
{
  "text": "Lumen is an AI-native programming language"
}
```

**Output**:
```json
{
  "embedding": [0.012, -0.034, 0.056, ...],  // 768 dimensions
  "model": "text-embedding-004"
}
```

## Configuration

In `lumen.toml`:

```toml
[providers]
gemini.generate = "gemini"
gemini.chat = "gemini"
gemini.embed = "gemini"

[providers.config.gemini]
api_key_env = "GEMINI_API_KEY"
base_url = "https://generativelanguage.googleapis.com/v1beta"
default_model = "gemini-1.5-flash"
timeout_ms = 30000
```

Set API key:

```bash
export GEMINI_API_KEY="your-api-key-here"
```

Get API key from [Google AI Studio](https://makersuite.google.com/app/apikey).

## Integration

Register with the runtime:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_gemini::GeminiProvider;

let api_key = std::env::var("GEMINI_API_KEY")?;
let mut registry = ProviderRegistry::new();

registry.register(Box::new(GeminiProvider::generate(api_key.clone())));
registry.register(Box::new(GeminiProvider::chat(api_key.clone())));
registry.register(Box::new(GeminiProvider::embed(api_key)));
```

## Parameters

### Generation Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `temperature` | Float | 0.7 | Randomness (0.0-2.0) |
| `top_p` | Float | 0.95 | Nucleus sampling threshold |
| `top_k` | Int | 40 | Top-k sampling limit |
| `max_tokens` | Int | 2048 | Maximum output length |

### Safety Settings

```json
{
  "safety_settings": [
    {
      "category": "HARM_CATEGORY_HATE_SPEECH",
      "threshold": "BLOCK_MEDIUM_AND_ABOVE"
    },
    {
      "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
      "threshold": "BLOCK_MEDIUM_AND_ABOVE"
    }
  ]
}
```

## Error Handling

Returns `ToolError` variants:
- **`AuthError`**: Invalid or missing API key
- **`RateLimit`**: Quota exceeded (includes `retry_after_ms`)
- **`ModelNotFound`**: Invalid model name
- **`ExecutionFailed`**: Network error, API error
- **`Timeout`**: Request exceeded time limit

## Testing

```bash
# Requires GEMINI_API_KEY environment variable
GEMINI_API_KEY=your-key cargo test -p lumen-provider-gemini

# Unit tests (mocked)
cargo test -p lumen-provider-gemini --lib

# Integration tests (ignored by default)
GEMINI_API_KEY=your-key cargo test -p lumen-provider-gemini -- --ignored
```

## Capabilities

The provider advertises these capabilities:
- `TextGeneration` — Single-shot text generation
- `Chat` — Multi-turn conversation
- `Embedding` — Text to vector embeddings

Check capabilities:

```rust
use lumen_rt::services::tools::Capability;

let provider = GeminiProvider::generate(api_key);
assert!(provider.capabilities().contains(&Capability::TextGeneration));
```

## Rate Limits

Gemini API free tier limits:
- 15 requests per minute
- 1 million tokens per minute
- 1,500 requests per day

The provider automatically handles rate limit errors with `retry_after_ms` hints.

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait
- **reqwest** — HTTP client for Gemini API
- **serde_json** — JSON serialization
- **lumen-cli** — Optional Gemini provider registration
