# Advanced: Async & Futures

Lumen provides first-class async programming with futures.

## Futures

Futures represent values that will be available later:

```lumen
let future = spawn(long_running_task())
# ... do other work ...
let result = await future
```

## Creating Futures

### spawn

Create a future from a callable:

```lumen
let f1 = spawn(compute(42))
let f2 = spawn(fetch_data("https://api.example.com"))
```

### spawn with closure

```lumen
let future = spawn(fn() -> String
  let a = compute_step_1()
  let b = compute_step_2(a)
  return b
end)
```

### spawn list

Create multiple futures:

```lumen
let futures = spawn([
  task_a(),
  task_b(),
  task_c()
])

let results = await futures  # list[Result]
```

## Awaiting Futures

### Basic await

```lumen
let result = await future
```

### Await resolves nested futures

```lumen
let nested = [spawn(a()), spawn(b()), spawn(c())]
let results = await nested  # Automatically awaits each
```

## Future States

```lumen
enum FutureState
  Pending
  Completed(value: T)
  Error(message: String)
end
```

Check state:

```lumen
match future.state
  Pending -> "Still running"
  Completed(v) -> "Done: {v}"
  Error(msg) -> "Failed: {msg}"
end
```

## Scheduling Modes

### Eager (default)

Futures start immediately when spawned:

```lumen
let f = spawn(task())  # Starts now
# ... other work ...
let r = await f        # Waits if not done
```

### Deferred FIFO

With `@deterministic true`:

```lumen
@deterministic true

cell example() -> list[Int]
  let f1 = spawn(task_a())  # Queued
  let f2 = spawn(task_b())  # Queued
  
  # Execute in order: f1, then f2
  return [await f1, await f2]
end
```

## Orchestration Patterns

### Parallel

Run multiple futures concurrently:

```lumen
await parallel for item in items
  process(item)
end
```

Block form:

```lumen
await parallel
  a = fetch_a()
  b = fetch_b()
  c = fetch_c()
end

# a, b, c are all available
return a ++ b ++ c
```

### Race

First to complete wins:

```lumen
let result = await race
  fetch_from_primary()
  fetch_from_backup()
end
```

### Vote

Consensus from multiple sources:

```lumen
let answer = await vote
  model_a(question)
  model_b(question)
  model_c(question)
end
```

### Select

Wait for first available:

```lumen
await select
  msg = channel.receive()
  timeout = sleep(1000)
end

match msg
  m: Message -> handle(m)
  _ -> print("timeout")
end
```

### Timeout

Add time limit:

```lumen
let result = await timeout(5000, slow_operation())

match result
  v: T -> print("Got {v}")
  null -> print("Timed out")
end
```

## Error Handling

Futures can fail:

```lumen
let future = spawn(risky_operation())

match await future
  value: T -> print("Success: {value}")
  error: String -> print("Failed: {error}")
end
```

With try:

```lumen
cell safe_fetch() -> result[Data, String]
  let future = spawn(fetch_data())
  let result = await future
  
  match result
    data: Data -> return ok(data)
    err: String -> return err(err)
  end
end
```

## Async Cells

Mark cells as async:

```lumen
async cell fetch_user(id: String) -> User
  let data = await fetch("/users/{id}")
  return parse_user(data)
end
```

Async cells can use await directly.

## Practical Example: Web Scraper

```lumen
async cell scrape_site(base_url: String) -> list[Page]
  let index = await fetch("{base_url}/index")
  let links = extract_links(index)
  
  let pages = await parallel for link in links
    fetch("{base_url}/{link}")
  end
  
  return pages
end
```

## Practical Example: Multi-Model Query

```lumen
use tool llm.chat as Chat

async cell query_ensemble(question: String) -> Consensus / {llm}
  # Get answers from multiple models in parallel
  let answers = await parallel
    gpt4 = query_model("gpt-4o", question)
    claude = query_model("claude-3-opus", question)
    gemini = query_model("gemini-pro", question)
  end
  
  # Vote on best answer
  let consensus = await vote
    gpt4.answer
    claude.answer
    gemini.answer
  end
  
  return Consensus(
    answer: consensus,
    models: [gpt4, claude, gemini]
  )
end

cell query_model(model: String, question: String) -> Answer / {llm}
  role system: You are a helpful assistant.
  role user: {question}
  return Chat(prompt: question, model: model)
end
```

## Best Practices

1. **Use parallel for independence** — Don't serialize unnecessarily
2. **Add timeouts** — Prevent hanging
3. **Handle errors** — Futures can fail
4. **Test with deterministic mode** — Ensure reproducibility
5. **Avoid blocking in futures** — Let the scheduler manage

## Next Steps

- [Orchestration](../ai-native/orchestration) — Full orchestration guide
- [Effects](./effects) — Effect system
