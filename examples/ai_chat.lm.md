# AI Chat Example
Demonstrates using the Gemini provider for AI-powered chat.

```lumen
use tool gemini.generate as Generate

grant Generate {
  max_tokens: 1000
}

cell main() -> String
  let response = Generate({
    prompt: "Explain what Lumen is in one sentence",
    temperature: 0.7
  })
  return response
end
```
