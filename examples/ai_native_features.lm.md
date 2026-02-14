# AI-Native Features Examples

This file demonstrates Lumen's AI-native language features that make it uniquely suited for AI systems.

## Setup

```lumen
use tool llm.chat as Chat
use tool http.get as HttpGet

grant Chat max_tokens 2000
grant HttpGet timeout_ms 5000

bind effect llm to Chat
bind effect http to HttpGet
```

## 1. Scored Values

Records that wrap values with confidence scores for tracking uncertainty:

```lumen
record ScoredString
  value: String
  confidence: Float
end

cell classify_sentiment(text: String) -> ScoredString / {llm}
  let result = Chat(prompt: text)
  return ScoredString(value: result, confidence: 0.95)
end

cell get_best_answer(candidates: list[ScoredString]) -> String
  let best = candidates[0]
  for candidate in candidates
    if candidate.confidence > best.confidence
      best = candidate
    end
  end
  return best.value
end
```

## 2. Tool Calls with Effects

Effect-tracked tool calls ensure all external interactions are visible in the type signature:

```lumen
cell fetch_data(url: String) -> String / {http}
  return HttpGet(url: url)
end

cell ask_llm(question: String) -> String / {llm}
  return Chat(prompt: question)
end
```

## 3. Complete AI Workflow Example

Combining scored values with tool calls:

```lumen
cell analyze_document(doc: String) -> ScoredString / {llm}
  let raw_result = Chat(prompt: "Extract key facts from: " ++ doc)
  let confidence = calculate_confidence(raw_result)
  return ScoredString(value: raw_result, confidence: confidence)
end

cell calculate_confidence(result: String) -> Float
  let field_count = length(result)
  return min(1.0, float(field_count) / 10.0)
end

cell main() -> String / {llm}
  let doc = "The annual report shows revenue of $5M..."
  let analysis = analyze_document(doc)

  if analysis.confidence > 0.8
    return "High confidence: " ++ analysis.value
  else
    return "Low confidence, needs review"
  end
end
```

## Benefits of AI-Native Features

1. **Type Safety**: Tool call parameters are validated at compile time
2. **Effect Tracking**: All external interactions declared in cell signatures
3. **Transparency**: Confidence scores track uncertainty
4. **Composability**: Records, effects, and tool calls work together seamlessly

These features make Lumen well-suited for building production AI systems.
