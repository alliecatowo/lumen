# Tutorial: Pipelines

Pipelines define sequential data processing stages.

## What Are Pipelines?

Pipelines chain processing stages where each stage:
- Takes exactly one data argument
- Produces output for the next stage
- Has a typed interface

## Basic Pipeline

```lumen
pipeline TextProcessor
  stages:
    -> read
    -> clean
    -> tokenize
    -> analyze
  
  cell read(source: String) -> String
    return file_read(source)
  end
  
  cell clean(text: String) -> String
    return text.lower().trim()
  end
  
  cell tokenize(text: String) -> list[String]
    return split(text, " ")
  end
  
  cell analyze(tokens: list[String]) -> map[String, Int]
    # Count word frequencies
  end
end

cell main() -> map[String, Int]
  let processor = TextProcessor()
  return processor.run("input.txt")
end
```

## Stage Interface Rules

Each stage must:
1. Take exactly one data parameter
2. Return a value (the input to the next stage)
3. Have types that chain correctly

```lumen
# Valid: String -> list[Int] -> Int -> String
stage1(input: String) -> list[Int]
stage2(numbers: list[Int]) -> Int
stage3(count: Int) -> String

# Invalid: Types don't chain
stage1(input: String) -> Int
stage2(data: String) -> Int    # Error: expected Int, got String
```

## Auto-Generated Run

If you don't define `run`, Lumen generates:

```lumen
# Generated from stages declaration
cell run(input: InputType) -> OutputType
  let a = stage1(input)
  let b = stage2(a)
  let c = stage3(b)
  return c
end
```

## Custom Run

Override for error handling or control flow:

```lumen
pipeline SafeProcessor
  stages:
    -> validate
    -> transform
    -> save
  
  cell validate(input: RawData) -> result[ValidData, String]
    # Returns ok or err
  end
  
  cell transform(data: ValidData) -> result[Output, String]
    # Returns ok or err
  end
  
  cell save(output: Output) -> result[String, String]
    # Returns ok or err
  end
  
  cell run(input: RawData) -> result[String, String]
    let valid = try validate(input)
    let output = try transform(valid)
    let saved = try save(output)
    return ok(saved)
  end
end
```

## Parallel Stages

For stages that can run independently:

```lumen
pipeline Analyzer
  stages:
    -> fetch_data
    -> parallel_analyze
    -> combine
  
  cell fetch_data(sources: list[String]) -> list[Json]
    # Fetch from multiple sources
  end
  
  cell parallel_analyze(datasets: list[Json]) -> list[Analysis]
    await parallel for data in datasets
      analyze_single(data)
    end
  end
  
  cell combine(results: list[Analysis]) -> Report
    # Combine into final report
  end
end
```

## AI-Enhanced Pipeline

```lumen
use tool llm.chat as Chat

pipeline DocumentProcessor
  use tool llm.chat as Chat
  grant Chat model "gpt-4o" max_tokens 2048
  
  stages:
    -> extract_text
    -> chunk
    -> summarize_chunks
    -> combine_summaries
  
  cell extract_text(file: String) -> String
    return file_read(file)
  end
  
  cell chunk(text: String) -> list[String]
    # Split into manageable chunks
    return split_chunks(text, 2000)
  end
  
  cell summarize_chunks(chunks: list[String]) -> list[String] / {llm}
    await parallel for chunk in chunks
      role system: Summarize this text concisely.
      role user: {chunk}
      Chat(prompt: chunk)
    end
  end
  
  cell combine_summaries(summaries: list[String]) -> String / {llm}
    role system: Combine these summaries into a coherent overview.
    role user: {join(summaries, "\n\n")}
    return Chat(prompt: join(summaries, "\n\n"))
  end
end
```

## Pipeline with Branching

For conditional processing:

```lumen
pipeline SmartProcessor
  stages:
    -> classify
    -> route
    -> process
  
  cell classify(input: Data) -> ClassifiedData
    # Add classification metadata
  end
  
  cell route(data: ClassifiedData) -> ProcessedData
    match data.type
      "text" -> return process_text(data)
      "image" -> return process_image(data)
      "audio" -> return process_audio(data)
      _ -> return process_default(data)
    end
  end
  
  cell process(data: ProcessedData) -> Output
    # Final processing
  end
end
```

## Pipeline Composition

Combine multiple pipelines:

```lumen
pipeline PreProcessor
  stages:
    -> clean
    -> validate
  # ...
end

pipeline MainProcessor
  stages:
    -> transform
    -> enrich
  # ...
end

cell process_all(input: RawData) -> Output
  let pre = PreProcessor()
  let main = MainProcessor()
  
  let cleaned = pre.run(input)
  return main.run(cleaned)
end
```

## Best Practices

1. **Keep stages focused** — One responsibility per stage
2. **Type strictly** — Clear input/output types
3. **Handle errors** — Use result types in custom run
4. **Document stages** — Comment what each does
5. **Consider parallelism** — Use `await parallel` for independent work

## Next Steps

- [Orchestration](/learn/ai-native/orchestration) — Multi-agent coordination
- [Process Reference](/reference/processes) — Complete pipeline documentation
