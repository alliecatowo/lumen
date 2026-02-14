# Executor Workflow

Execution-stage records and helper cells for tool dispatch planning.

```lumen
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

cell build_execution_command(
  selected_tool: String,
  manual_review: Bool,
  requested_tool: String,
  payload_json: String
) -> ExecutionCommand
  let tool_alias = requested_tool
  if requested_tool == ""
    tool_alias = selected_tool
  end

  let expected = "remote-dispatch"
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
  let outcome = command.expected_outcome
  let retries = 0
  let fallback_used = false
  let error_reason = "none"

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
```
