# Orbit Planner

Planner-stage logic for sequencing missions and setting guardrails.

```lumen
import models: Mission, PlannedStep, PlannerSnapshot, PromptPacket

cell make_step(step_no: Int, phase: String, tool_alias: String, summary: String) -> PlannedStep
  PlannedStep(
    step_no: step_no,
    phase: phase,
    tool_alias: tool_alias,
    summary: summary
  )
end

cell count_critical(missions: list[Mission]) -> Int
  let critical: Int = 0
  for mission in missions
    if mission.priority >= 8
      critical = critical + 1
    end
  end
  critical
end

cell requires_manual_review(mode: String, missions: list[Mission]) -> Bool
  if mode != "simulation"
    return true
  end

  for mission in missions
    if mission.requires_human_approval
      return true
    end
  end
  false
end

cell build_planner_snapshot(
  mode: String,
  missions: list[Mission],
  prompt: PromptPacket,
  planning_tool: String,
  research_tool: String,
  notify_tool: String
) -> PlannerSnapshot
  let steps: list[PlannedStep] = []
  steps = append(steps, make_step(1, "planning", planning_tool, "draft launch ordering and constraints"))
  steps = append(steps, make_step(2, "research", research_tool, "pull active incident context from GitHub MCP"))
  steps = append(steps, make_step(3, "notify", notify_tool, "publish launch readiness snapshot"))

  let critical: Int = count_critical(missions)
  let manual_review: Bool = requires_manual_review(mode, missions)
  let preview: String = "model " + prompt.target_model + " assembled " + string(len(steps)) + " steps with " + string(critical) + " critical missions"

  PlannerSnapshot(
    stage: "planning",
    selected_tool: planning_tool,
    plan_preview: preview,
    guardrail: "require human approval before any write-capable action",
    steps: steps,
    manual_review: manual_review,
    critical_count: critical
  )
end
```
