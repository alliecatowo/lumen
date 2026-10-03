# Orbit Executor

Execution-stage command assembly and deterministic simulation logic.

```lumen
import models: PlannerSnapshot, ToolInvocationSpec, ExecutionCommand, ExecutionSnapshot

cell build_execution_command(planner: PlannerSnapshot, invocation: ToolInvocationSpec) -> ExecutionCommand
  let mut expected: String = "remote-dispatch"
  if planner.manual_review
    expected = "dispatch-blocked"
  end

  ExecutionCommand(
    attempt: 1,
    tool_alias: invocation.tool_alias,
    payload_json: invocation.payload_json,
    expected_outcome: expected
  )
end

cell simulate_execution(mode: String, command: ExecutionCommand) -> ExecutionSnapshot
  let mut retries: Int = 0
  let mut fallback_used: Bool = false
  let mut outcome: String = command.expected_outcome
  let mut error_reason: String = "none"

  if command.expected_outcome == "dispatch-blocked"
    outcome = "dispatch-blocked"
    error_reason = "manual-approval-required"
  else
    if mode == "simulation"
      if command.tool_alias == "GitHubOrbitIssues"
        outcome = "provider-timeout"
        retries = 1
        fallback_used = true
        error_reason = "provider-timeout"
      else
        outcome = "simulated-ok"
      end
    else
      outcome = "remote-ok"
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
```
