# Domain Events

Records used to report stage-level outcomes and full pipeline summaries.

```lumen
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
```
