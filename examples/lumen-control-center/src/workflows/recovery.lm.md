# Recovery Workflow

Recovery-stage records and helper cells for fallback and escalation paths.

```lumen
record RecoverySnapshot
  stage: String
  status: String
  recommended_tool: String
  action_note: String
  escalation_level: Int
end

cell build_recovery_snapshot(
  primary_outcome: String,
  fallback_used: Bool,
  target_tool: String,
  fallback_note: String,
  notify_tool: String
) -> RecoverySnapshot
  let status = "stable"
  let recommended_tool = target_tool
  let action_note = "no recovery action required"
  let escalation_level = 0

  if fallback_used
    status = "degraded"
    recommended_tool = notify_tool
    action_note = fallback_note
    escalation_level = 1
  end

  if primary_outcome == "dispatch-denied"
    status = "blocked"
    recommended_tool = notify_tool
    action_note = "manual approval required before remote dispatch"
    escalation_level = 2
  end

  RecoverySnapshot(
    stage: "recovery",
    status: status,
    recommended_tool: recommended_tool,
    action_note: action_note,
    escalation_level: escalation_level
  )
end
```
