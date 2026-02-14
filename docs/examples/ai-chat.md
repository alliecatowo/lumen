# Examples: AI Chat

A complete AI chat agent example.

## Basic Chat Agent

```lumen
# chat.lm.md

use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 2048
  temperature 0.7

bind effect llm to Chat

agent ChatBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end

cell main() -> String / {llm}
  let bot = ChatBot()
  return bot.respond("What is the capital of France?")
end
```

## Chat with History

```lumen
use tool llm.chat as Chat

record Message
  role: String
  content: String

record Conversation
  messages: list[Message]

agent ConversationalBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o" max_tokens 4096
  
  cell chat(history: Conversation, message: String) -> Conversation / {llm}
    let updated = history.messages ++ [Message(role: "user", content: message)]
    
    role system: You are a helpful assistant.
    for msg in updated
      role {msg.role}: {msg.content}
    end
    
    let response = Chat(prompt: message)
    let final = updated ++ [Message(role: "assistant", content: response)]
    
    return Conversation(messages: final)
  end
  
  cell start() -> Conversation
    return Conversation(messages: [])
  end
end

cell main() -> String / {llm}
  let bot = ConversationalBot()
  let conv = bot.start()
  
  let conv2 = bot.chat(conv, "Hi, I'm learning Lumen")
  let conv3 = bot.chat(conv2, "Can you help me with pattern matching?")
  
  # Get last assistant message
  let last = conv3.messages[length(conv3.messages) - 1]
  return last.content
end
```

## Multi-Model Chat

```lumen
use tool llm.chat as Chat

agent GPTBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell respond(message: String) -> String / {llm}
    return Chat(prompt: message)
  end
end

agent ClaudeBot
  use tool llm.chat as Chat
  grant Chat model "claude-3-opus"
  
  cell respond(message: String) -> String / {llm}
    return Chat(prompt: message)
  end
end

orchestration EnsembleChat
  cell ask_all(question: String) -> EnsembleResponse / {llm}
    let gpt = GPTBot()
    let claude = ClaudeBot()
    
    let responses = await parallel
      gpt_answer = gpt.respond(question)
      claude_answer = claude.respond(question)
    end
    
    return EnsembleResponse(
      question: question,
      gpt4: gpt_answer,
      claude: claude_answer
    )
  end
end

record EnsembleResponse
  question: String
  gpt4: String
  claude: String

cell main() -> String / {llm}
  let ensemble = EnsembleChat()
  let result = ensemble.ask_all("What is 2 + 2?")
  
  return """
GPT-4: {result.gpt4}
Claude: {result.claude}
"""
end
```

## Tool-Using Agent

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch

grant Chat model "gpt-4o"
grant Fetch timeout_ms 5000

agent WebAssistant
  use tool llm.chat as Chat
  use tool http.get as Fetch
  
  grant Chat model "gpt-4o" max_tokens 2048
  grant Fetch domain "*.wikipedia.org"
  
  cell research(topic: String) -> String / {llm, http}
    let url = "https://en.wikipedia.org/api/rest_v1/page/summary/{topic}"
    let data = Fetch(url: url)
    
    role system: You summarize information. Be concise.
    role user: Summarize this: {data}
    
    return Chat(prompt: data)
  end
end

cell main() -> String / {llm, http}
  let assistant = WebAssistant()
  return assistant.research("Artificial_intelligence")
end
```

## Streaming Chat (Conceptual)

```lumen
agent StreamingBot
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell stream(message: String) -> String / {llm}
    # In production, this would yield chunks
    return Chat(prompt: message)
  end
end
```

## Configuration

`lumen.toml`:

```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"
```

## Next Example

[Code Reviewer](/examples/code-reviewer) — AI-powered code analysis
