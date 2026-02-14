# Orbit Types

Shared records for the Orbit workspace pipeline.

```lumen
record OrbitContext
  workspace: String
  tenant: String
  region: String
  mode: String
  launch_window: String
end

record Mission
  id: String
  title: String
  priority: Int
  owner: String
  requires_human_approval: Bool
  tags: list[String]
end

record MissionQueue
  context: OrbitContext
  missions: list[Mission]
end

record PlannedStep
  step_no: Int
  phase: String
  tool_alias: String
  summary: String
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

record PlannerSnapshot
  stage: String
  selected_tool: String
  plan_preview: String
  guardrail: String
  steps: list[PlannedStep]
  manual_review: Bool
  critical_count: Int
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

record StageEvent
  stage: String
  tool_alias: String
  outcome: String
end

record RunSummary
  workspace: String
  mode: String
  missions_total: Int
  routes_total: Int
  stage_events: list[StageEvent]
  notes: String
end
```
