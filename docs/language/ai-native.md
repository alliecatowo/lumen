# AI-Native Features

Lumen includes agent and tool primitives in the language and runtime instead of framework-specific wrappers.

## Tools and grants

Declare tools and constrain behavior with grants:

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 512
  temperature 0.2
```

Runtime policy enforcement validates configured constraints before tool dispatch.

## Roles and prompt structure

Role prompts can be declared directly in cells:

```lumen
cell summarize(text: String) -> String
  role system: You are a concise assistant.
  role user: Summarize this text: {text}
  return "summary"
end
```

## Orchestration and async execution

Lumen runtime supports futures and orchestration constructs with deterministic semantics.

- `Spawn` creates future handles.
- `Await` resolves futures and propagates errors.
- Scheduling can run in eager or deterministic deferred modes.

See [Runtime Model](/RUNTIME) for runtime details.

## Determinism profile

`@deterministic` enables strict checks that reject nondeterministic operations/effects where required.

This improves reproducibility for audit-heavy or safety-critical workflows.

## Process runtime objects

Lumen lowers process-family declarations into runtime-backed records with callable methods, including:

- `pipeline`
- `memory`
- `machine`

## Continue

- Browser execution: [Browser WASM Guide](/guide/wasm-browser)
- Language syntax: [Language Tour](/language/tour)
- Commands and tooling: [CLI Reference](/CLI)
