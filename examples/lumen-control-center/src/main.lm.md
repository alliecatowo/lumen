# Lumen Control Center Main

Runnable entrypoint for a realistic AI workspace.

```lumen
record WorkspaceContext
  workspace: String
  tenant: String
  mode: String
end

record WorkItem
  id: String
  title: String
  owner: String
  tags: list[String]
  effort: Int
end

record WorkQueue
  context: WorkspaceContext
  items: list[WorkItem]
end

record StageEvent
  stage: String
  tool_alias: String
  outcome: String
end

record RunSummary
  workspace: String
  mode: String
  tasks_total: Int
  routes_total: Int
  stage_events: list[StageEvent]
  notes: String
end

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

record PlannedStep
  step_no: Int
  phase: String
  tool_alias: String
  summary: String
end

record PlannerSnapshot
  stage: String
  selected_tool: String
  plan_preview: String
  guardrail: String
  steps: list[PlannedStep]
  manual_review: Bool
end

record ExecutionCommand
  attempt: Int
  tool_alias: String
  payload_json: String
  expected_outcome: String
end

record ExecutionSnapshot
  stage: String
  target_tool: String
  retries: Int
  fallback_used: Bool
  primary_outcome: String
  error_reason: String
end

record RecoverySnapshot
  stage: String
  status: String
  recommended_tool: String
  action_note: String
  escalation_level: Int
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

cell make_step(step_no: Int, phase: String, tool_alias: String, summary: String) -> PlannedStep
  PlannedStep(
    step_no: step_no,
    phase: phase,
    tool_alias: tool_alias,
    summary: summary
  )
end

cell build_planner_snapshot(
  mode: String,
  item_count: Int,
  target_model: String,
  planning_tool: String,
  research_tool: String,
  notify_tool: String
) -> PlannerSnapshot
  let steps: list[PlannedStep] = []
  steps = append(steps, make_step(1, "planning", planning_tool, "draft rollout plan and risk priorities"))
  steps = append(steps, make_step(2, "research", research_tool, "collect active incident context from issue tracker"))
  steps = append(steps, make_step(3, "notify", notify_tool, "publish dry-run summary to release channel"))

  let preview: String = "model " + target_model + " prepared " + string(len(steps)) + " staged actions for " + string(item_count) + " tasks"
  let requires_manual_review: Bool = false
  if mode != "dry-run"
    requires_manual_review = true
  end

  PlannerSnapshot(
    stage: "planning",
    selected_tool: planning_tool,
    plan_preview: preview,
    guardrail: "require human approval before non-read-only actions",
    steps: steps,
    manual_review: requires_manual_review
  )
end

cell build_execution_command(
  selected_tool: String,
  manual_review: Bool,
  requested_tool: String,
  payload_json: String
) -> ExecutionCommand
  let tool_alias: String = requested_tool
  if requested_tool == ""
    tool_alias = selected_tool
  end

  let expected: String = "remote-dispatch"
  if manual_review
    expected = "dispatch-denied"
  end

  ExecutionCommand(
    attempt: 1,
    tool_alias: tool_alias,
    payload_json: payload_json,
    expected_outcome: expected
  )
end

cell build_execution_snapshot(mode: String, command: ExecutionCommand) -> ExecutionSnapshot
  let outcome: String = command.expected_outcome
  let retries: Int = 0
  let fallback_used: Bool = false
  let error_reason: String = "none"

  if mode == "dry-run"
    if command.tool_alias == "GitHubIssues"
      outcome = "provider-timeout"
      retries = 1
      fallback_used = true
      error_reason = "provider-timeout"
    else
      outcome = "simulated-ok"
    end
  end

  ExecutionSnapshot(
    stage: "execution",
    target_tool: command.tool_alias,
    retries: retries,
    fallback_used: fallback_used,
    primary_outcome: outcome,
    error_reason: error_reason
  )
end

cell build_recovery_snapshot(
  primary_outcome: String,
  fallback_used: Bool,
  target_tool: String,
  fallback_note: String,
  notify_tool: String
) -> RecoverySnapshot
  let status: String = "stable"
  let recommended_tool: String = target_tool
  let action_note: String = "no recovery action required"
  let escalation_level: Int = 0

  if fallback_used
    status = "degraded"
    recommended_tool = notify_tool
    action_note = fallback_note
    escalation_level = 1
  end

  if primary_outcome == "dispatch-denied"
    status = "blocked"
    recommended_tool = notify_tool
    action_note = "manual approval required before remote dispatch"
    escalation_level = 2
  end

  RecoverySnapshot(
    stage: "recovery",
    status: status,
    recommended_tool: recommended_tool,
    action_note: action_note,
    escalation_level: escalation_level
  )
end

cell build_queue() -> WorkQueue
  let context: WorkspaceContext = WorkspaceContext(
    workspace: "release-ops",
    tenant: "acme-platform",
    mode: "dry-run"
  )

  let items: list[WorkItem] = [
    WorkItem(id: "WK-101", title: "Collect deployment context", owner: "platform-bot", tags: ["intake", "config"], effort: 2),
    WorkItem(id: "WK-102", title: "Draft rollout plan with LLM", owner: "planner-agent", tags: ["llm", "planning"], effort: 5),
    WorkItem(id: "WK-103", title: "Query GitHub issues via MCP", owner: "executor-agent", tags: ["mcp", "github"], effort: 3),
    WorkItem(id: "WK-104", title: "Broadcast recovery status", owner: "notify-agent", tags: ["mcp", "slack"], effort: 1)
  ]

  WorkQueue(context: context, items: items)
end

cell build_routes() -> list[ToolRoute]
  [
    ToolRoute(phase: "planning", tool_alias: "PlannerChat", purpose: "generate candidate rollout plan"),
    ToolRoute(phase: "research", tool_alias: "GitHubIssues", purpose: "pull open incident context"),
    ToolRoute(phase: "review", tool_alias: "ReviewerChat", purpose: "cross-check risk assumptions"),
    ToolRoute(phase: "notify", tool_alias: "SlackPost", purpose: "publish dry-run summary")
  ]
end

cell build_bindings() -> list[ProviderBinding]
  [
    ProviderBinding(tool_alias: "PlannerChat", provider: "openai-compatible", policy_profile: "llm-safe-default"),
    ProviderBinding(tool_alias: "ReviewerChat", provider: "openai-compatible", policy_profile: "llm-review"),
    ProviderBinding(tool_alias: "HttpFetch", provider: "builtin-http", policy_profile: "status-read-only"),
    ProviderBinding(tool_alias: "GitHubIssues", provider: "mcp-bridge", policy_profile: "read-only-mcp"),
    ProviderBinding(tool_alias: "SlackPost", provider: "mcp-bridge", policy_profile: "notify-only")
  ]
end

cell stage_event(stage: String, alias: String, outcome: String) -> StageEvent
  StageEvent(stage: stage, tool_alias: alias, outcome: outcome)
end

cell render(summary: RunSummary, routes: list[ToolRoute], bindings: list[ProviderBinding]) -> String
  let out: String = "Lumen Control Center Demo\n"
  out = out + "workspace: " + summary.workspace + " (" + summary.mode + ")\n"
  out = out + "tasks: " + string(summary.tasks_total) + ", routes: " + string(summary.routes_total) + "\n"
  out = out + "notes: " + summary.notes + "\n\n"

  out = out + "Routes:\n"
  for route in routes
    out = out + "- " + route.phase + " -> " + route.tool_alias + " (" + route.purpose + ")\n"
  end

  out = out + "\nBindings:\n"
  for binding in bindings
    out = out + "- " + binding.tool_alias + " via " + binding.provider + " [" + binding.policy_profile + "]\n"
  end

  out = out + "\nEvents:\n"
  for ev in summary.stage_events
    out = out + "- " + ev.stage + ": " + ev.outcome + " [" + ev.tool_alias + "]\n"
  end

  out
end

cell main() -> String
  let queue: WorkQueue = build_queue()
  let routes: list[ToolRoute] = build_routes()
  let bindings: list[ProviderBinding] = build_bindings()

  let planning_tool: String = route_for_phase(routes, "planning")
  let research_tool: String = route_for_phase(routes, "research")
  let notify_tool: String = route_for_phase(routes, "notify")

  let prompt: PromptPacket = PromptPacket(
    system_prompt: "You are a release planner that prefers deterministic actions first.",
    user_prompt: "Create a dry-run rollout plan for release window 2026-02.",
    target_model: "gpt-4o-mini"
  )

  let invocation: ToolInvocationSpec = ToolInvocationSpec(
    tool_alias: research_tool,
    payload_json: "{\"repo\":\"acme/platform\",\"label\":\"incident\",\"state\":\"open\"}",
    fallback: "use cached issue digest"
  )

  let planner: PlannerSnapshot = build_planner_snapshot(
    queue.context.mode,
    len(queue.items),
    prompt.target_model,
    planning_tool,
    research_tool,
    notify_tool
  )

  let command: ExecutionCommand = build_execution_command(
    planner.selected_tool,
    planner.manual_review,
    invocation.tool_alias,
    invocation.payload_json
  )
  let executor: ExecutionSnapshot = build_execution_snapshot(queue.context.mode, command)
  let recovery: RecoverySnapshot = build_recovery_snapshot(
    executor.primary_outcome,
    executor.fallback_used,
    executor.target_tool,
    invocation.fallback,
    notify_tool
  )

  let planner_binding: String = provider_for_alias(bindings, planner.selected_tool)
  let research_binding: String = provider_for_alias(bindings, executor.target_tool)
  let notify_binding: String = provider_for_alias(bindings, recovery.recommended_tool)

  let events: list[StageEvent] = [
    stage_event("planning", planner.selected_tool, planner.plan_preview),
    stage_event("execution", executor.target_tool, executor.primary_outcome),
    stage_event("recovery", recovery.recommended_tool, recovery.status + ": " + recovery.action_note)
  ]

  let notes: String = "plan=" + planner_binding
  notes = notes + ", exec=" + research_binding
  notes = notes + ", notify=" + notify_binding
  notes = notes + ", fallback=" + invocation.fallback
  notes = notes + ", escalations=" + string(recovery.escalation_level)
  notes = notes + ", payload=" + command.payload_json

  let summary: RunSummary = RunSummary(
    workspace: queue.context.workspace,
    mode: queue.context.mode,
    tasks_total: len(queue.items),
    routes_total: len(routes),
    stage_events: events,
    notes: notes
  )
  render(summary, routes, bindings)
end
```
