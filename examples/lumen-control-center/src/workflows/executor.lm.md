# Executor Workflow

Execution-stage records and helper cells for tool dispatch planning.

```lumen
record ExecutionSnapshot
  stage: String
  target_tool: String
  retries: Int
  fallback_used: Bool
end

cell build_execution_snapshot(target_tool: String, retries: Int, fallback_used: Bool) -> ExecutionSnapshot
  return ExecutionSnapshot(
    stage: "execution",
    target_tool: target_tool,
    retries: retries,
    fallback_used: fallback_used
  )
end
```
