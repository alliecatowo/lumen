# Provider Contracts

Provider and tool contracts for realistic LLM + MCP workflows.

```lumen
use tool llm.chat as PlannerChat
use tool llm.chat as ReviewerChat
use tool http.get as HttpFetch
use tool mcp.github.search_issues as GitHubIssues
use tool mcp.slack.post_message as SlackPost

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

cell mock_llm_preview(packet: PromptPacket) -> String
  role system: You are a planning assistant for safe deployments.
  role user: {packet.user_prompt}
  return "stubbed-plan: prioritize deterministic checks before live tool calls"
end

cell mock_mcp_preview(spec: ToolInvocationSpec) -> String
  return "stubbed-mcp-call: " + spec.tool_alias + " with payload " + spec.payload_json
end
```
