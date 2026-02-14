# Gemini Hello World

Demonstrates sending a message to Gemini and getting a response back.

```lumen
use tool gemini.generate as Generate

grant Generate 
  max_tokens 100
  temperature 0.9

cell main() -> String
  let response = Generate(
    prompt: "Say hello to the Lumen developer in a very creative and futuristic way.",
    system: "You are a helpful AI assistant that is part of the Lumen language ecosystem."
  )
  return response
end
```
