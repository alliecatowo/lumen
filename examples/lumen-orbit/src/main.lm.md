# Lumen Orbit Example

Deterministic flagship workspace using imported planner/executor/recovery modules.

```lumen
import models: OrbitContext, Mission, MissionQueue, StageEvent, RunSummary
import contracts: orbit_routes, orbit_bindings, route_for_phase, provider_for_alias, plan_prompt, make_research_invocation
import planner: build_planner_snapshot
import executor: build_execution_command, simulate_execution
import recovery: build_recovery_snapshot

cell build_queue() -> MissionQueue
  let context: OrbitContext = OrbitContext(
    workspace: "orbit-release",
    tenant: "acme-space",
    region: "us-west",
    mode: "simulation",
    launch_window: "2026-03-15T08:00Z"
  )

  let missions: list[Mission] = [
    Mission(id: "ORB-101", title: "stage telemetry schema", priority: 8, owner: "planner", requires_human_approval: false, tags: ["schema", "planning"]),
    Mission(id: "ORB-102", title: "query incident context", priority: 9, owner: "executor", requires_human_approval: false, tags: ["mcp", "github"]),
    Mission(id: "ORB-103", title: "publish launch brief", priority: 6, owner: "notifier", requires_human_approval: false, tags: ["mcp", "slack"])
  ]

  MissionQueue(context: context, missions: missions)
end

cell build_summary(
  queue: MissionQueue,
  routes_total: Int,
  events: list[StageEvent],
  recovery_status: String
) -> RunSummary
  let notes: String = "recovery=" + recovery_status + ", window=" + queue.context.launch_window
  RunSummary(
    workspace: queue.context.workspace,
    mode: queue.context.mode,
    missions_total: len(queue.missions),
    routes_total: routes_total,
    stage_events: events,
    notes: notes
  )
end

cell render_summary(
  summary: RunSummary,
  planner_provider: String,
  execution_error: String
) -> String
  let out: String = "Lumen Orbit\n"
  let out: String = out + "workspace: " + summary.workspace + " (" + summary.mode + ")\n"
  let out: String = out + "missions: " + string(summary.missions_total) + ", routes: " + string(summary.routes_total) + "\n"
  let out: String = out + "planner provider: " + planner_provider + "\n"
  let out: String = out + "execution error: " + execution_error + "\n"
  out + "notes: " + summary.notes
end

cell run_orbit() -> String
  let queue: MissionQueue = build_queue()
  let routes = orbit_routes()
  let bindings = orbit_bindings()

  let planning_tool: String = route_for_phase(routes, "planning")
  let research_tool: String = route_for_phase(routes, "research")
  let notify_tool: String = route_for_phase(routes, "notify")
  let planner_provider: String = provider_for_alias(bindings, planning_tool)

  let prompt = plan_prompt(queue.context.tenant, queue.context.region, len(queue.missions))
  let invocation = make_research_invocation(research_tool, queue.context.tenant)

  let planner = build_planner_snapshot(
    queue.context.mode,
    queue.missions,
    prompt,
    planning_tool,
    research_tool,
    notify_tool
  )

  let command = build_execution_command(planner, invocation)
  let execution = simulate_execution(queue.context.mode, command)
  let recovery = build_recovery_snapshot(execution, invocation.fallback, notify_tool)

  let events = [
    StageEvent(stage: planner.stage, tool_alias: planner.selected_tool, outcome: planner.plan_preview),
    StageEvent(stage: execution.stage, tool_alias: execution.target_tool, outcome: execution.primary_outcome),
    StageEvent(stage: recovery.stage, tool_alias: recovery.recommended_tool, outcome: recovery.action_note)
  ]

  let summary: RunSummary = build_summary(queue, len(routes), events, recovery.status)
  render_summary(summary, planner_provider, execution.error_reason)
end

cell test_orbit_projection() -> Bool
  let report: String = run_orbit()
  contains(report, "workspace: orbit-release (simulation)")
end

cell test_orbit_report() -> Bool
  let report: String = run_orbit()
  contains(report, "recovery=degraded")
end

cell main() -> Null
  print(run_orbit())
  null
end
```
