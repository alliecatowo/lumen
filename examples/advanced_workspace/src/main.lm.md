# Advanced Workspace Main

Runnable entrypoint for a realistic multi-module AI workspace.

```lumen
import domain.models: WorkspaceContext, WorkItem, WorkQueue
import domain.events: StageEvent, RunSummary
import providers.contracts: ToolRoute, ProviderBinding, PromptPacket, ToolInvocationSpec
import workflows.planner: PlannerSnapshot
import workflows.executor: ExecutionSnapshot

cell build_queue() -> WorkQueue
  let context = WorkspaceContext(
    workspace: "release-ops",
    tenant: "acme-platform",
    mode: "dry-run"
  )

  let items = [
    WorkItem(id: "WK-101", title: "Collect deployment context", owner: "platform-bot", tags: ["intake", "config"], effort: 2),
    WorkItem(id: "WK-102", title: "Draft rollout plan with LLM", owner: "planner-agent", tags: ["llm", "planning"], effort: 5),
    WorkItem(id: "WK-103", title: "Query GitHub issues via MCP", owner: "executor-agent", tags: ["mcp", "github"], effort: 3)
  ]

  return WorkQueue(context: context, items: items)
end

cell build_routes() -> list[ToolRoute]
  return [
    ToolRoute(phase: "planning", tool_alias: "PlannerChat", purpose: "generate candidate rollout plan"),
    ToolRoute(phase: "research", tool_alias: "GitHubIssues", purpose: "pull open incident context"),
    ToolRoute(phase: "notify", tool_alias: "SlackPost", purpose: "publish dry-run summary")
  ]
end

cell build_bindings() -> list[ProviderBinding]
  return [
    ProviderBinding(tool_alias: "PlannerChat", provider: "openai-compatible", policy_profile: "llm-safe-default"),
    ProviderBinding(tool_alias: "GitHubIssues", provider: "mcp-bridge", policy_profile: "read-only-mcp"),
    ProviderBinding(tool_alias: "SlackPost", provider: "mcp-bridge", policy_profile: "notify-only")
  ]
end

cell stage_event(stage: String, alias: String, outcome: String) -> StageEvent
  return StageEvent(stage: stage, tool_alias: alias, outcome: outcome)
end

cell summarize(
  queue: WorkQueue,
  routes: list[ToolRoute],
  prompt: PromptPacket,
  invocation: ToolInvocationSpec,
  planner: PlannerSnapshot,
  executor: ExecutionSnapshot
) -> RunSummary
  let events = [
    stage_event("planning", planner.selected_tool, "plan preview generated"),
    stage_event("research", invocation.tool_alias, "mcp request assembled"),
    stage_event("execution", executor.target_tool, "dry-run execution staged")
  ]

  let notes = "model=" + prompt.target_model + ", fallback=" + invocation.fallback

  return RunSummary(
    workspace: queue.context.workspace,
    mode: queue.context.mode,
    tasks_total: len(queue.items),
    routes_total: len(routes),
    stage_events: events,
    notes: notes
  )
end

cell render(
  summary: RunSummary,
  planner: PlannerSnapshot,
  executor: ExecutionSnapshot,
  routes: list[ToolRoute],
  bindings: list[ProviderBinding]
) -> String
  let out = "Advanced Workspace Demo\n"
  let out = out + "workspace: " + summary.workspace + " (" + summary.mode + ")\n"
  let out = out + "tasks: " + string(summary.tasks_total) + ", routes: " + string(summary.routes_total) + "\n"
  let out = out + "planner: " + planner.selected_tool + " -> " + planner.plan_preview + "\n"
  let out = out + "executor: " + executor.target_tool + ", retries=" + string(executor.retries) + "\n"
  let out = out + "notes: " + summary.notes + "\n\n"

  let out = out + "Routes:\n"
  for route in routes
    out = out + "- " + route.phase + " -> " + route.tool_alias + " (" + route.purpose + ")\n"
  end

  let out = out + "\nBindings:\n"
  for binding in bindings
    out = out + "- " + binding.tool_alias + " via " + binding.provider + " [" + binding.policy_profile + "]\n"
  end

  let out = out + "\nEvents:\n"
  for ev in summary.stage_events
    out = out + "- " + ev.stage + ": " + ev.outcome + " [" + ev.tool_alias + "]\n"
  end

  return out
end

cell main() -> String
  let queue = build_queue()
  let routes = build_routes()
  let bindings = build_bindings()

  let prompt = PromptPacket(
    system_prompt: "You are a release planner that prefers deterministic actions first.",
    user_prompt: "Create a dry-run rollout plan for release window 2026-02.",
    target_model: "gpt-4o-mini"
  )

  let invocation = ToolInvocationSpec(
    tool_alias: "GitHubIssues",
    payload_json: "{\"repo\":\"acme/platform\",\"label\":\"incident\",\"state\":\"open\"}",
    fallback: "use cached issue digest"
  )

  let planner = PlannerSnapshot(
    stage: "planning",
    selected_tool: "PlannerChat",
    plan_preview: "collect signals -> rank risks -> prepare rollout checklist",
    guardrail: "require human approval before live mutations"
  )

  let executor = ExecutionSnapshot(
    stage: "execution",
    target_tool: "GitHubIssues",
    retries: 1,
    fallback_used: true
  )

  let summary = summarize(queue, routes, prompt, invocation, planner, executor)
  return render(summary, planner, executor, routes, bindings)
end
```
