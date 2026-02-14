---
layout: page
---

# Playground

Try Lumen directly in your browser.

<div id="playground-container">
  <div id="editor-container">
    <div id="editor-header">
      <span>main.lm.md</span>
      <button id="run-button" onclick="runCode()">Run</button>
    </div>
    <div id="editor"></div>
  </div>
  <div id="output-container">
    <div id="output-header">Output</div>
    <pre id="output"></pre>
  </div>
</div>

<style>
#playground-container {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
  height: 500px;
  margin: 1rem 0;
}

#editor-container, #output-container {
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  overflow: hidden;
}

#editor-header, #output-header {
  background: var(--vp-c-bg-soft);
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--vp-c-divider);
  font-weight: 500;
}

#editor {
  height: calc(100% - 40px);
}

#output {
  padding: 1rem;
  margin: 0;
  height: calc(100% - 40px);
  overflow: auto;
  font-family: monospace;
  white-space: pre-wrap;
}

#run-button {
  background: var(--vp-c-brand);
  color: white;
  border: none;
  padding: 0.25rem 1rem;
  border-radius: 4px;
  cursor: pointer;
  float: right;
}

#run-button:hover {
  opacity: 0.9;
}

@media (max-width: 768px) {
  #playground-container {
    grid-template-columns: 1fr;
    height: auto;
  }
  
  #editor-container, #output-container {
    height: 300px;
  }
}
</style>

## Example Code

```lumen
# Try editing this code!

cell greet(name: String) -> String
  return "Hello, {name}!"
end

cell fibonacci(n: Int) -> Int
  if n <= 1
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end

cell main() -> String
  let greeting = greet("World")
  let fib10 = fibonacci(10)
  
  return """
{greeting}
The 10th Fibonacci number is: {fib10}
"""
end
```

## More Examples

- [Hello World](/examples/hello-world) — Basic program
- [Calculator](/examples/calculator) — Arithmetic operations
- [AI Chat](/examples/ai-chat) — AI-powered chatbot
- [Pattern Matching](/examples/language-features) — Advanced patterns

## Running Locally

To run Lumen locally:

```bash
# Install
cargo install lumen-lang

# Create a file
echo 'cell main() -> String
  return "Hello, World!"
end' > hello.lm.md

# Run it
lumen run hello.lm.md
```

## WASM Integration

The playground uses the Lumen WASM build. You can embed Lumen in your own applications:

```javascript
import init, { check, run } from 'lumen-wasm';

await init();

const result = check(`
  cell main() -> String
    return "Hello!"
  end
`);

if (result.is_ok()) {
  const output = run(result.code, 'main');
  console.log(output);
}
```

<script setup>
import { onMounted } from 'vue';

// Placeholder for actual WASM integration
// In production, this would load the actual lumen-wasm module

onMounted(() => {
  // Initialize editor
  // For now, show static content
});
</script>
