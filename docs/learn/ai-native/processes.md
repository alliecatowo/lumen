# Tutorial: Processes

Processes provide structured runtime objects with built-in methods.

## What Are Processes?

Processes are special declarations that compile to runtime-backed records with:
- Instance-scoped state
- Built-in methods
- Type-safe interfaces

## Process Kinds

| Kind | Purpose | Built-in Methods |
|------|---------|------------------|
| `memory` | Key-value storage | append, recent, recall, get, query, store |
| `machine` | State machine | run, start, step, is_terminal, current_state |
| `pipeline` | Data pipeline | run (auto-generated from stages) |
| `orchestration` | Multi-agent coordination | run |

## Memory

Memory processes provide key-value storage:

```lumen
memory ConversationBuffer

cell main() -> Int
  let buffer = ConversationBuffer()
  
  buffer.append("Hello")
  buffer.append("How are you?")
  buffer.append("Goodbye")
  
  let recent = buffer.recent(2)  # Last 2 messages
  return length(recent)          # 2
end
```

### Memory Methods

| Method | Description |
|--------|-------------|
| `append(entry)` | Add entry to memory |
| `recent(n)` | Get last n entries |
| `recall(query)` | Search memory |
| `get(key)` | Get by key |
| `query(filter)` | Filter entries |
| `store(key, value)` | Store key-value pair |
| `upsert(key, value)` | Update or insert |

### Isolation

Each instance has isolated state:

```lumen
memory Buf

cell main() -> Int
  let a = Buf()
  let b = Buf()
  
  a.append("in A")
  b.append("in B")
  
  return length(a.recent(10)) + length(b.recent(10))  # 2 (1 + 1)
end
```

## Machine

Machines are state machines with typed states:

```lumen
machine OrderWorkflow
  initial: Created
  
  state Created(order: Order)
    transition Process(order)
  end
  
  state Process(order: Order)
    guard: order.total > 0
    transition Shipped(order.tracking)
  end
  
  state Shipped(tracking: String)
    terminal: true
  end
end
```

### Machine States

```lumen
state StateName(payload: Type)
  guard: boolean_expression      # Optional guard
  terminal: true                 # Mark as terminal
  transition NextState(args)     # Define transitions
end
```

### Machine Methods

| Method | Description |
|--------|-------------|
| `run(input)` | Execute machine to completion |
| `start(input)` | Start machine, return first state |
| `step()` | Advance to next state |
| `is_terminal()` | Check if in terminal state |
| `current_state()` | Get current state name |
| `resume_from(state)` | Resume from saved state |

### Example: Task States

```lumen
machine TaskLifecycle
  initial: Todo
  
  state Todo(title: String)
    transition InProgress(title)
  end
  
  state InProgress(title: String)
    transition Done(title)
    transition Blocked(title)
  end
  
  state Blocked(title: String)
    transition InProgress(title)
  end
  
  state Done(title: String)
    terminal: true
  end
end

cell main() -> String
  let workflow = TaskLifecycle()
  workflow.start("Build feature")
  
  while not workflow.is_terminal()
    workflow.step()
  end
  
  return workflow.current_state()  # "Done"
end
```

### Guards

Guards control transitions:

```lumen
machine PaymentFlow
  initial: Pending
  
  state Pending(amount: Float)
    guard: amount > 0
    transition Processing(amount)
    transition Rejected("Invalid amount")
  end
  
  state Processing(amount: Float)
    transition Completed(amount)
  end
  
  state Completed(amount: Float)
    terminal: true
  end
  
  state Rejected(reason: String)
    terminal: true
  end
end
```

## Pipeline

Pipelines chain stages:

```lumen
pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> load
  
  cell extract(source: String) -> list[Json]
    # Extract data from source
  end
  
  cell transform(data: list[Json]) -> list[Record]
    # Transform JSON to records
  end
  
  cell load(records: list[Record]) -> Int
    # Load records, return count
  end
end

cell main() -> Int
  let processor = DataProcessor()
  return processor.run("data.csv")
end
```

### Auto-Generated Run

If no `run` cell is defined, one is generated:

```lumen
# Auto-generated:
cell run(source: String) -> Int
  let data = extract(source)
  let records = transform(data)
  return load(records)
end
```

### Custom Run

Override the default:

```lumen
pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> validate
    -> load
  
  # ... stage cells ...
  
  cell run(source: String) -> result[Int, String]
    let data = extract(source)
    let records = transform(data)
    
    for record in records
      match validate(record)
        err(msg) -> return err(msg)
        _ -> ()
      end
    end
    
    return ok(load(records))
  end
end
```

## Orchestration

Orchestration processes coordinate multiple agents:

```lumen
orchestration AnalysisTeam
  use tool llm.chat as Chat
  
  cell analyze(text: String) -> String / {llm}
    await parallel
      sentiment = analyze_sentiment(text)
      topics = extract_topics(text)
      summary = summarize(text)
    end
    
    return combine(sentiment, topics, summary)
  end
  
  cell analyze_sentiment(text: String) -> String / {llm}
    # ...
  end
  
  cell extract_topics(text: String) -> list[String] / {llm}
    # ...
  end
  
  cell summarize(text: String) -> String / {llm}
    # ...
  end
end
```

## Combining Processes

```lumen
memory ConversationHistory

machine SupportFlow
  initial: Greeting
  
  state Greeting()
    transition CollectingIssue()
  end
  
  state CollectingIssue()
    transition Searching()
  end
  
  state Searching()
    transition Resolved()
    transition Escalated()
  end
  
  state Resolved()
    terminal: true
  end
  
  state Escalated()
    terminal: true
  end
end

agent SupportBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  memory ConversationHistory
  machine SupportFlow
  
  cell handle(message: String) -> String / {llm}
    history.append(message)
    
    match current_state()
      Greeting -> return greet()
      CollectingIssue -> return collect_issue(message)
      Searching -> return search(message)
      _ -> return "Goodbye!"
    end
  end
end
```

## Best Practices

1. **Use memory for conversation history**
2. **Use machines for multi-step workflows**
3. **Use pipelines for data transformation**
4. **Keep state types simple**
5. **Validate at state transitions**

## Next Steps

- [Pipelines](./pipelines) — Detailed pipeline guide
- [Agents](./agents) — Agent definition
- [Pipelines](./pipelines) — Pipeline composition
