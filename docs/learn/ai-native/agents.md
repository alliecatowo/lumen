# Tutorial: Agents

Agents encapsulate AI behavior with scoped capabilities.

## What Are Agents?

Agents are a way to organize:
- Tool usage
- Behavior (cells)
- Policy constraints (grants)
- Prompts (roles)

They compile to constructor-backed runtime records with callable methods.

## Basic Agent

```lumen
agent Greeter
  cell greet(name: String) -> String
    return "Hello, {name}!"
  end
end

cell main() -> String
  let greeter = Greeter()
  return greeter.greet("World")
end
```

## Agent with Tools

```lumen
use tool llm.chat as Chat

agent Assistant
  use tool llm.chat as Chat
  grant Chat
    model "gpt-4o"
    max_tokens 1024
  
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end

cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("Hello!")
end
```

## Multiple Methods

```lumen
use tool llm.chat as Chat

agent CodeHelper
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell explain(code: String) -> String / {llm}
    role system: You are a code explainer.
    role user: Explain this code:\n{code}
    return Chat(prompt: code)
  end
  
  cell review(code: String) -> String / {llm}
    role system: You are a code reviewer.
    role user: Review this code for issues:\n{code}
    return Chat(prompt: code)
  end
  
  cell fix(code: String, issue: String) -> String / {llm}
    role system: You are a code fixer.
    role user: Fix this issue in the code:\nIssue: {issue}\nCode:\n{code}
    return Chat(prompt: "{issue}\n{code}")
  end
end
```

## Scoped Grants

Grants inside an agent are scoped to that agent:

```lumen
use tool llm.chat as Chat

agent ConservativeBot
  use tool llm.chat as Chat
  grant Chat
    model "gpt-4o"
    max_tokens 100      # Very limited
    temperature 0.1     # Very deterministic

agent CreativeBot
  use tool llm.chat as Chat
  grant Chat
    model "gpt-4o"
    max_tokens 4000     # More room
    temperature 0.9     # More creative

cell main() -> String / {llm}
  let conservative = ConservativeBot()
  let creative = CreativeBot()
  
  # Each uses its own constraints
  let c1 = conservative.respond("Tell me a story")
  let c2 = creative.respond("Tell me a story")
  
  return c1 ++ "\n---\n" ++ c2
end
```

## Agent with State

```lumen
record Conversation
  messages: list[String]
end

agent ChatBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell chat(history: Conversation, message: String) -> Conversation / {llm}
    role system: You are a helpful assistant.
    let updated = history.messages ++ [message]
    let response = Chat(prompt: updated)
    return Conversation(messages: updated ++ [response])
  end
end
```

## Role Prompts

Agents can define role prompts:

```lumen
agent Expert
  cell answer(question: String) -> String / {llm}
    role system: |
      You are an expert software engineer with 20 years of experience.
      Provide detailed, accurate answers with code examples.
      Be concise but thorough.
    role user: {question}
    return Chat(prompt: question)
  end
end
```

## Inheritance Pattern

While Lumen doesn't have agent inheritance, you can compose:

```lumen
agent BaseAssistant
  cell base_prompt() -> String
    return "You are a helpful assistant."
  end
end

agent SpecializedAssistant
  cell system_prompt() -> String
    return "You are a specialized assistant for X."
  end
  
  cell respond(message: String) -> String / {llm}
    role system: {system_prompt()}
    role user: {message}
    return Chat(prompt: message)
  end
end
```

## Example: Invoice Processor

```lumen
use tool llm.chat as Chat

record Invoice
  vendor: String
  amount: Float
  date: String
  items: list[String]
end

agent InvoiceProcessor
  use tool llm.chat as Chat
  grant Chat
    model "gpt-4o"
    max_tokens 2000
  
  cell extract(text: String) -> Invoice / {llm}
    role system: |
      Extract invoice data from text.
      Return JSON with: vendor, amount, date, items.
    role user: {text}
    let result = Chat(prompt: text)
    return parse_invoice(result)
  end
  
  cell validate(invoice: Invoice) -> result[Invoice, String]
    if invoice.amount < 0
      return err("Invalid amount")
    end
    if length(invoice.vendor) == 0
      return err("Missing vendor")
    end
    return ok(invoice)
  end
  
  cell categorize(invoice: Invoice) -> String / {llm}
    role system: Categorize this invoice into one of: Office, Travel, Software, Hardware, Services.
    role user: Vendor: {invoice.vendor}, Items: {invoice.items}
    return Chat(prompt: invoice.vendor)
  end
end

cell main() -> String / {llm}
  let processor = InvoiceProcessor()
  let invoice = processor.extract("ACME Corp - $1,500 - 2024-01-15 - Office supplies")
  
  match invoice
    ok(inv) -> return processor.categorize(inv)
    err(msg) -> return "Error: {msg}"
  end
end
```

## Best Practices

1. **One responsibility per agent** — Keep focused
2. **Use scoped grants** — Limit capabilities appropriately
3. **Document with roles** — Clear system prompts
4. **Validate inputs/outputs** — Check before processing
5. **Handle errors** — Use result types

## Next Steps

- [Processes](/learn/ai-native/processes) — Stateful workflows
- [Pipelines](/learn/ai-native/pipelines) — Data processing
- [Agent Reference](/reference/agents) — Complete agent documentation
