# Orbit Types

Shared records for the Orbit workspace pipeline.

```lumen
pub record OrbitContext
  workspace: String
  tenant: String
  region: String
  mode: String
  launch_window: String
end

pub record Mission
  id: String
  title: String
  priority: Int
  owner: String
  requires_human_approval: Bool
  tags: list[String]
end

pub record MissionQueue
  context: OrbitContext
  missions: list[Mission]
end

pub record PlannedStep
  step_no: Int
  phase: String
  tool_alias: String
  summary: String
end

pub record PromptPacket
  system_prompt: String
  user_prompt: String
  target_model: String
end

pub record ToolInvocationSpec
  tool_alias: String
  payload_json: String
  fallback: String
end

pub record PlannerSnapshot
  stage: String
  selected_tool: String
  plan_preview: String
  guardrail: String
  steps: list[PlannedStep]
  manual_review: Bool
  critical_count: Int
end

pub record ExecutionCommand
  attempt: Int
  tool_alias: String
  payload_json: String
  expected_outcome: String
end

pub record ExecutionSnapshot
  stage: String
  target_tool: String
  retries: Int
  fallback_used: Bool
  primary_outcome: String
  error_reason: String
end

pub record RecoverySnapshot
  stage: String
  status: String
  recommended_tool: String
  action_note: String
  escalation_level: Int
end

pub record StageEvent
  stage: String
  tool_alias: String
  outcome: String
end

pub record RunSummary
  workspace: String
  mode: String
  missions_total: Int
  routes_total: Int
  stage_events: list[StageEvent]
  notes: String
end
```
