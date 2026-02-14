# Tutorial: Orchestration

Orchestration coordinates multiple async operations and agents.

## What Is Orchestration?

Orchestration handles:
- Running multiple operations in parallel
- Racing operations for the first result
- Voting across multiple sources
- Timeout handling
- Selecting from multiple futures

## Parallel Execution

Run operations concurrently:

```lumen
cell fetch_all(urls: list[String]) -> list[String] / {http}
  await parallel for url in urls
    fetch(url)
  end
end
```

### Parallel with Collection

```lumen
cell analyze_sources(sources: list[String]) -> list[Analysis] / {llm}
  await parallel for source in sources
    analyze(source)
  end
end
```

### Parallel Block

```lumen
cell multi_task() -> tuple[String, Int, Bool] / {http, db}
  await parallel
    result1 = fetch_from_api()
    result2 = query_database()
    result3 = check_feature_flag()
  end
  
  return (result1, result2, result3)
end
```

## Race

Return the first completed result:

```lumen
cell fast_fetch(url1: String, url2: String) -> String / {http}
  await race
    fetch(url1)
    fetch(url2)
  end
end
```

### Race with Timeout

```lumen
cell fetch_with_timeout(url: String, timeout_ms: Int) -> String | Null / {http}
  await race
    fetch(url)
    sleep(timeout_ms) |> fn(_) => null
  end
end
```

## Vote

Get consensus from multiple sources:

```lumen
cell get_answer(question: String) -> String / {llm}
  await vote
    model_a(question)
    model_b(question)
    model_c(question)
  end
end
```

Voting returns the most common result.

## Select

Wait for first available from multiple channels:

```lumen
cell handle_events() -> String
  await select
    event1 = channel_a.receive()
    event2 = channel_b.receive()
    timeout = sleep(5000)
  end
  
  match event
    e: Event1 -> handle1(e)
    e: Event2 -> handle2(e)
    _ -> "timeout"
  end
end
```

## Timeout

Add timeout constraints:

```lumen
cell bounded_fetch(url: String) -> result[String, String] / {http}
  let result = await timeout(5000, fetch(url))
  
  match result
    data: String -> return ok(data)
    null -> return err("Timeout")
  end
end
```

## Orchestration Process

Combine orchestration patterns:

```lumen
orchestration ResearchTeam
  use tool llm.chat as Chat
  use tool http.get as Fetch
  
  grant Chat model "gpt-4o"
  
  cell research(topic: String) -> Report / {llm, http}
    # Gather sources in parallel
    let sources = await parallel for engine in ["google", "bing", "duckduckgo"]
      search(engine, topic)
    end
    
    # Race for quick summary
    let quick = await race
      summarize_fast(sources)
      summarize_thorough(sources)
    end
    
    # Vote on best insights
    let insights = await vote
      extract_insights_a(sources)
      extract_insights_b(sources)
      extract_insights_c(sources)
    end
    
    return combine(quick, insights)
  end
  
  cell search(engine: String, topic: String) -> SearchResult / {http}
    # Search implementation
  end
  
  cell summarize_fast(sources: list[SearchResult]) -> String / {llm}
    # Fast summarization
  end
  
  cell summarize_thorough(sources: list[SearchResult]) -> String / {llm}
    # Thorough summarization
  end
end
```

## Spawning Futures

Create background tasks:

```lumen
cell background_processing() -> Null
  let future1 = spawn(long_task_a())
  let future2 = spawn(long_task_b())
  
  # Do other work...
  
  let result1 = await future1
  let result2 = await future2
  
  return null
end
```

### Spawn List

```lumen
cell process_all(items: list[Item]) -> list[Result]
  let futures = spawn([
    process(items[0]),
    process(items[1]),
    process(items[2])
  ])
  
  return await futures
end
```

## Future States

Futures have explicit states:
- `Pending` — Not yet complete
- `Completed(value)` — Finished with result
- `Error(message)` — Failed

```lumen
cell check_future(f: Future) -> String
  match f.state
    Pending -> "Still running"
    Completed(v) -> "Got: {v}"
    Error(msg) -> "Failed: {msg}"
  end
end
```

## Deterministic Scheduling

With `@deterministic true`:

```lumen
@deterministic true

cell ordered_execution() -> list[Int]
  # Futures execute in order, not concurrently
  let f1 = spawn(task_a())
  let f2 = spawn(task_b())
  
  return [await f1, await f2]  # Always [a_result, b_result]
end
```

## Example: Multi-Model Analysis

```lumen
use tool llm.chat as Chat

agent ModelA
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell analyze(text: String) -> Analysis / {llm}
    # GPT-4 analysis
  end
end

agent ModelB
  use tool llm.chat as Chat
  grant Chat model "claude-3-opus"
  
  cell analyze(text: String) -> Analysis / {llm}
    # Claude analysis
  end
end

orchestration EnsembleAnalyzer
  cell analyze(text: String) -> ConsensusAnalysis / {llm}
    let a = ModelA()
    let b = ModelB()
    
    # Get analyses in parallel
    let analyses = await parallel
      r1 = a.analyze(text)
      r2 = b.analyze(text)
    end
    
    # Vote on sentiment
    let sentiment = await vote
      r1.sentiment
      r2.sentiment
    
    return ConsensusAnalysis(
      sentiment: sentiment,
      details: combine(r1, r2)
    )
  end
end
```

## Best Practices

1. **Use parallel for independent work** — Don't serialize unnecessarily
2. **Add timeouts** — Prevent hanging on slow responses
3. **Handle failures** — Races can fail if all fail
4. **Use vote for consensus** — Better than single-source decisions
5. **Test determinism** — Ensure reproducible results when needed

## Next Steps

- [Advanced Effects](../advanced/effects) — Effect system deep dive
- [Async & Futures](../advanced/async) — Async programming
