# Orbit Recovery

Recovery-stage policy for fallback, escalation, and notification routing.

```lumen
import models: ExecutionSnapshot, RecoverySnapshot

cell build_recovery_snapshot(
  execution: ExecutionSnapshot,
  fallback_note: String,
  notify_tool: String
) -> RecoverySnapshot
  let status: String = "stable"
  let recommended_tool: String = execution.target_tool
  let action_note: String = "no recovery action required"
  let escalation_level: Int = 0

  if execution.fallback_used
    status = "degraded"
    recommended_tool = notify_tool
    action_note = fallback_note
    escalation_level = 1
  end

  if execution.primary_outcome == "dispatch-blocked"
    status = "blocked"
    recommended_tool = notify_tool
    action_note = "manual approval required before dispatch"
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
