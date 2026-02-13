# Task Models

Type definitions for the task management system.

```lumen
# Type alias for task IDs
type TaskId = string

# Enum for task priority
enum Priority
  Low
  Medium
  High
  Urgent
end

# Enum for task status
enum TaskStatus
  Pending
  InProgress
  Completed
  Cancelled
end

# Core task record
record Task
  id: TaskId
  title: string
  description: string
  priority: Priority
  status: TaskStatus
  tags: list[string]
  created_at: int where created_at >= 0
  completed_at: result[int, string]
end

# Statistics record
record TaskStats
  total: int
  pending: int
  in_progress: int
  completed: int
  cancelled: int
end
```

**Note**: This file is a design target for when multi-file imports are fully supported. Currently not imported by main.lm.md.
