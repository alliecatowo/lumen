# Orbit Contracts

Tool declarations, provider-facing contracts, and routing helpers.

```lumen
import models: PromptPacket, ToolInvocationSpec

use tool llm.chat as OrbitPlannerChat
use tool llm.chat as OrbitReviewerChat
use tool http.get as OrbitHttpGet
use tool github.search_issues as GitHubOrbitIssues
use tool slack.post_message as SlackOrbitPost

bind effect llm to OrbitPlannerChat
bind effect llm to OrbitReviewerChat
bind effect http to OrbitHttpGet
bind effect mcp to GitHubOrbitIssues
bind effect mcp to SlackOrbitPost

grant OrbitPlannerChat
  model "gpt-4o-mini"
  max_tokens 1200
  temperature 0.1

grant OrbitReviewerChat
  model "gpt-4o"
  max_tokens 1500
  temperature 0.2

grant OrbitHttpGet
  allowed_domains ["api.github.com", "status.acme-orbit.example"]
  timeout_ms 4000

grant GitHubOrbitIssues
  timeout_ms 3500

grant SlackOrbitPost
  timeout_ms 2500

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

cell orbit_routes() -> list[ToolRoute]
  [
    ToolRoute(phase: "planning", tool_alias: "OrbitPlannerChat", purpose: "draft release route and risk map"),
    ToolRoute(phase: "research", tool_alias: "GitHubOrbitIssues", purpose: "gather active incidents from MCP GitHub"),
    ToolRoute(phase: "review", tool_alias: "OrbitReviewerChat", purpose: "cross-check blast radius and rollback hints"),
    ToolRoute(phase: "notify", tool_alias: "SlackOrbitPost", purpose: "publish launch update to operations channel")
  ]
end

cell orbit_bindings() -> list[ProviderBinding]
  [
    ProviderBinding(tool_alias: "OrbitPlannerChat", provider: "openai-compatible", policy_profile: "llm-deterministic"),
    ProviderBinding(tool_alias: "OrbitReviewerChat", provider: "openai-compatible", policy_profile: "llm-risk-audit"),
    ProviderBinding(tool_alias: "OrbitHttpGet", provider: "builtin-http", policy_profile: "read-only-status"),
    ProviderBinding(tool_alias: "GitHubOrbitIssues", provider: "mcp-bridge", policy_profile: "mcp-read-only"),
    ProviderBinding(tool_alias: "SlackOrbitPost", provider: "mcp-bridge", policy_profile: "mcp-notify-only")
  ]
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

cell plan_prompt(tenant: String, region: String, mission_count: Int) -> PromptPacket
  let system_prompt: String = "You are Orbit planner. Prioritize safe launch ordering and explicit rollback criteria."
  let user_prompt: String = "Build a release route for tenant " + tenant + " in " + region + " with " + string(mission_count) + " queued missions."
  PromptPacket(
    system_prompt: system_prompt,
    user_prompt: user_prompt,
    target_model: "gpt-4o-mini"
  )
end

cell make_research_invocation(research_tool: String, tenant: String) -> ToolInvocationSpec
  let payload: String = "{\"repo\":\"acme/orbit\",\"label\":\"incident\",\"state\":\"open\",\"tenant\":\"" + tenant + "\"}"
  ToolInvocationSpec(
    tool_alias: research_tool,
    payload_json: payload,
    fallback: "use last successful incident digest and require human review"
  )
end
```
