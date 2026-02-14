# Provider Contracts

Provider and tool contracts for realistic LLM + MCP workflows.

```lumen
use tool llm.chat as PlannerChat
use tool llm.chat as ReviewerChat
use tool http.get as HttpFetch
use tool github.search_issues as GitHubIssues
use tool slack.post_message as SlackPost

bind effect llm to PlannerChat
bind effect llm to ReviewerChat
bind effect http to HttpFetch
bind effect mcp to GitHubIssues
bind effect mcp to SlackPost

grant PlannerChat
  model "gpt-4o-mini"
  max_tokens 1200
  temperature 0.1

grant ReviewerChat
  model "gpt-4o"
  max_tokens 1800
  temperature 0.2

grant HttpFetch
  allowed_domains ["api.github.com", "status.example.com"]
  timeout_ms 5000

grant GitHubIssues
  timeout_ms 4000

grant SlackPost
  timeout_ms 3000

record ToolRoute
  phase: String
  tool_alias: String
  purpose: String
end

record ProviderBinding
  tool_alias: String
  provider: String
  policy_profile: String
end

record PromptPacket
  system_prompt: String
  user_prompt: String
  target_model: String
end

record ToolInvocationSpec
  tool_alias: String
  payload_json: String
  fallback: String
end

cell route_for_phase(routes: list[ToolRoute], phase: String) -> String
  for route in routes
    if route.phase == phase
      return route.tool_alias
    end
  end
  "unassigned"
end

cell provider_for_alias(bindings: list[ProviderBinding], alias: String) -> String
  for binding in bindings
    if binding.tool_alias == alias
      return binding.provider
    end
  end
  "unbound"
end
```
