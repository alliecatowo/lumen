# @deterministic Directive

Enable deterministic execution mode.

## Syntax

```lumen
@deterministic true
```

## Purpose

The `@deterministic` directive rejects nondeterministic operations at compile time and ensures reproducible execution.

## What Is Rejected

When `@deterministic true` is active:

| Operation | Effect | Status |
|-----------|--------|--------|
| `uuid()` | random | Rejected |
| `uuid_v4()` | random | Rejected |
| `timestamp()` | time | Rejected |
| `timestamp_ms()` | time | Rejected |
| Unknown tool calls | external | Rejected |
| `random()` | random | Rejected |
| `random_int()` | random | Rejected |

## Example

```lumen
@deterministic true

cell main() -> String
  return uuid()  # ERROR: Nondeterministic operation in deterministic mode
end
```

## Runtime Behavior

With `@deterministic true`:

1. **Future scheduling** defaults to `DeferredFifo` (ordered execution)
2. **Random operations** are rejected at compile time
3. **Time functions** are rejected at compile time
4. **External tool calls** require explicit effect declarations

## When to Use

Use `@deterministic true` for:

- **Testing** — Reproducible test results
- **Auditing** — Traceable execution
- **Debugging** — Consistent behavior
- **Safety-critical** — Predictable outcomes
- **CI/CD** — Deterministic builds

## When Not to Use

Don't use for:

- **Interactive apps** — Need timestamps, UUIDs
- **Random sampling** — Need randomness
- **Real-time systems** — Need current time

## Example: Deterministic Pipeline

```lumen
@deterministic true

pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> load
  
  cell extract(source: String) -> list[Json]
    # Deterministic extraction
  end
  
  cell transform(data: list[Json]) -> list[Record]
    # Deterministic transformation
  end
  
  cell load(records: list[Record]) -> Int
    # Deterministic loading
  end
end

cell main() -> Int
  let processor = DataProcessor()
  return processor.run("input.json")
end
```

## Comparison: With vs Without

```lumen
# Without @deterministic
cell main() -> String
  let id = uuid()           # OK - random UUID
  let now = timestamp()     # OK - current time
  return "{id} at {now}"
end

# With @deterministic true
@deterministic true

cell main() -> String
  let id = uuid()           # ERROR
  let now = timestamp()     # ERROR
  return "{id} at {now}"
end
```

## Deterministic Future Scheduling

```lumen
@deterministic true

cell main() -> list[Int]
  # Futures execute in order, not concurrently
  let f1 = spawn(task_a())  # Queued first
  let f2 = spawn(task_b())  # Queued second
  
  # Always: [a_result, b_result] in order
  return [await f1, await f2]
end
```

## External Tool Calls

With `@deterministic`, external tool calls must be declared:

```lumen
@deterministic true

use tool llm.chat as Chat
bind effect llm to Chat

cell main() -> String / {llm}  # Must declare effect
  return Chat(prompt: "Hello")
end

# Without effect declaration: ERROR
cell bad() -> String
  return Chat(prompt: "Hello")  # ERROR: Unknown external call
end
```

## Best Practices

1. **Enable for tests** — Ensure reproducibility
2. **Enable for auditing** — Traceable execution
3. **Document exceptions** — If you must disable
4. **Use with effects** — Declare all effects explicitly
