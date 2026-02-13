# Task Utilities

Helper functions for task manipulation.

```lumen
import models.{Task, TaskId, Priority, TaskStatus}

# Convert priority to string representation
cell priority_to_string(p: Priority) -> string
  match p
    | Priority::Low => "low"
    | Priority::Medium => "medium"
    | Priority::High => "high"
    | Priority::Urgent => "urgent"
  end
end

# Get numeric score for priority
cell priority_score(p: Priority) -> int
  match p
    | Priority::Low => 1
    | Priority::Medium => 2
    | Priority::High => 3
    | Priority::Urgent => 4
  end
end

# Validate task title
cell validate_task_title(title: string) -> result[string, string]
  if len(title) == 0 then
    Err("Title cannot be empty")
  else if len(title) > 100 then
    Err("Title too long (max 100 characters)")
  else
    Ok(title)
  end
end

# Format task summary for display
cell format_task_summary(task: Task) -> string
  let priority_str = priority_to_string(task.priority)
  let tag_count = len(task.tags)
  "Task: " ++ task.title ++ " [" ++ priority_str ++ "] (" ++ string(tag_count) ++ " tags)"
end

# Get human-readable status description
cell task_status_description(status: TaskStatus) -> string
  match status
    | TaskStatus::Pending => "waiting to start"
    | TaskStatus::InProgress => "actively working"
    | TaskStatus::Completed => "finished"
    | TaskStatus::Cancelled => "no longer needed"
  end
end

# Extract completion timestamp if available
cell get_completed_timestamp(task: Task) -> string
  if let Ok(timestamp) = task.completed_at then
    "Completed at: " ++ string(timestamp)
  else
    "Not completed"
  end
end
```

**Note**: This file is a design target for when multi-file imports are fully supported. Currently not imported by main.lm.md.
