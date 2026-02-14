# Planner Workflow

Planner-stage records and helper cells used by the workspace pipeline.

```lumen
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

  let preview = "model " + target_model + " prepared " + string(len(steps)) + " staged actions for " + string(item_count) + " tasks"
  let requires_manual_review = false
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
```
