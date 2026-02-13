# Planner Workflow

Planner-stage records and helper cells used by the workspace pipeline.

```lumen
record PlannerSnapshot
  stage: String
  selected_tool: String
  plan_preview: String
  guardrail: String
end

cell build_planner_snapshot(stage: String, selected_tool: String, target_model: String) -> PlannerSnapshot
  let preview = "prompt model " + target_model + " routed to " + selected_tool
  return PlannerSnapshot(
    stage: stage,
    selected_tool: selected_tool,
    plan_preview: preview,
    guardrail: "require human approval before live mutations"
  )
end
```
