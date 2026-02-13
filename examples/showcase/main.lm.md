# Task Management System - Lumen Showcase

This comprehensive Lumen program demonstrates all language features through a practical task management system.

```lumen
# Type alias for task IDs
type TaskId = string

# Enum for task priority with exhaustive pattern matching
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

# Record with typed fields and constraints
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

# Record for task statistics
record TaskStats
  total: int
  pending: int
  in_progress: int
  completed: int
  cancelled: int
end

# Cell demonstrating string operations and priority conversion
cell priority_to_string(p: Priority) -> string
  match p
    | Priority::Low => "low"
    | Priority::Medium => "medium"
    | Priority::High => "high"
    | Priority::Urgent => "urgent"
  end
end

# Cell demonstrating numeric operations and comparisons
cell priority_score(p: Priority) -> int
  match p
    | Priority::Low => 1
    | Priority::Medium => 2
    | Priority::High => 3
    | Priority::Urgent => 4
  end
end

# Cell with error handling using result types
cell validate_task_title(title: string) -> result[string, string]
  if len(title) == 0 then
    Err("Title cannot be empty")
  else if len(title) > 100 then
    Err("Title too long (max 100 characters)")
  else
    Ok(title)
  end
end

# Cell demonstrating closures and lambdas as filters
cell filter_by_priority(tasks: list[Task], min_priority: Priority) -> list[Task]
  let min_score = priority_score(min_priority)
  let filter_fn = fn(t: Task) -> bool => priority_score(t.priority) >= min_score
  let result = []
  let i = 0
  while i < len(tasks)
    let task = get(tasks, i)
    if filter_fn(task) then
      result = append(result, task)
    end
    i = i + 1
  end
  result
end

# Cell demonstrating list operations with for loop
cell count_by_status(tasks: list[Task], status: TaskStatus) -> int
  let count = 0
  let i = 0
  while i < len(tasks)
    let task = get(tasks, i)
    if task.status == status then
      count = count + 1
    end
    i = i + 1
  end
  count
end

# Cell computing statistics with multiple field accesses
cell compute_stats(tasks: list[Task]) -> TaskStats
  TaskStats {
    total: len(tasks),
    pending: count_by_status(tasks, TaskStatus::Pending),
    in_progress: count_by_status(tasks, TaskStatus::InProgress),
    completed: count_by_status(tasks, TaskStatus::Completed),
    cancelled: count_by_status(tasks, TaskStatus::Cancelled)
  }
end

# Cell demonstrating string operations and interpolation
cell format_task_summary(task: Task) -> string
  let priority_str = priority_to_string(task.priority)
  let tag_count = len(task.tags)
  "Task: " ++ task.title ++ " [" ++ priority_str ++ "] (" ++ string(tag_count) ++ " tags)"
end

# Cell with while loop and mutation
cell find_task_by_id(tasks: list[Task], id: TaskId) -> result[Task, string]
  let i = 0
  while i < len(tasks)
    let task = get(tasks, i)
    if task.id == id then
      return Ok(task)
    end
    i = i + 1
  end
  Err("Task not found")
end

# Cell demonstrating nested match expressions
cell task_status_description(status: TaskStatus) -> string
  match status
    | TaskStatus::Pending => "waiting to start"
    | TaskStatus::InProgress => "actively working"
    | TaskStatus::Completed => "finished"
    | TaskStatus::Cancelled => "no longer needed"
  end
end

# Cell with if-let pattern matching
cell get_completed_timestamp(task: Task) -> string
  if let Ok(timestamp) = task.completed_at then
    "Completed at: " ++ string(timestamp)
  else
    "Not completed"
  end
end

# Cell for creating tasks with generated IDs
cell create_task_sequential(title: string, desc: string, prio: Priority, seq: int) -> Task
  Task {
    id: "task_" ++ string(seq),
    title: title,
    description: desc,
    priority: prio,
    status: TaskStatus::Pending,
    tags: [],
    created_at: seq * 1000,
    completed_at: Err("not completed")
  }
end

# Cell demonstrating map pattern with list
cell extract_task_titles(tasks: list[Task]) -> list[string]
  let result = []
  let i = 0
  while i < len(tasks)
    let task = get(tasks, i)
    result = append(result, task.title)
    i = i + 1
  end
  result
end

# Cell demonstrating reduce pattern
cell total_tasks_by_priority(tasks: list[Task], target_priority: Priority) -> int
  let count = 0
  let i = 0
  while i < len(tasks)
    let task = get(tasks, i)
    if task.priority == target_priority then
      count = count + 1
    end
    i = i + 1
  end
  count
end

# Cell with nested loops demonstrating sort logic (bubble sort)
cell sort_tasks_by_priority(tasks: list[Task]) -> list[Task]
  let sorted = tasks
  let n = len(sorted)
  let i = 0
  while i < n
    let j = 0
    while j < n - i - 1
      let task1 = get(sorted, j)
      let task2 = get(sorted, j + 1)
      if priority_score(task1.priority) < priority_score(task2.priority) then
        sorted = set(sorted, j, task2)
        sorted = set(sorted, j + 1, task1)
      end
      j = j + 1
    end
    i = i + 1
  end
  sorted
end

# Cell with multiple cells calling each other
cell analyze_tasks(tasks: list[Task]) -> string
  let stats = compute_stats(tasks)
  let high_priority = filter_by_priority(tasks, Priority::High)
  let high_count = len(high_priority)
  let summary = "Total tasks: " ++ string(stats.total) ++ "\n"
  let summary2 = summary ++ "Pending: " ++ string(stats.pending) ++ "\n"
  let summary3 = summary2 ++ "In Progress: " ++ string(stats.in_progress) ++ "\n"
  let summary4 = summary3 ++ "Completed: " ++ string(stats.completed) ++ "\n"
  let summary5 = summary4 ++ "High Priority: " ++ string(high_count)
  summary5
end

# Main entry point demonstrating all features
cell main() -> string
  let task1 = Task {
    id: "task001",
    title: "Implement user authentication",
    description: "Add login and registration with OAuth support",
    priority: Priority::High,
    status: TaskStatus::InProgress,
    tags: ["security", "backend", "urgent"],
    created_at: 1707840000,
    completed_at: Err("in progress")
  }
  let task2 = Task {
    id: "task002",
    title: "Design landing page",
    description: "Create responsive design for marketing site",
    priority: Priority::Medium,
    status: TaskStatus::Pending,
    tags: ["frontend", "design"],
    created_at: 1707843600,
    completed_at: Err("not started")
  }
  let task3 = Task {
    id: "task003",
    title: "Fix memory leak in worker",
    description: "Profile and fix memory issue in background processor",
    priority: Priority::Urgent,
    status: TaskStatus::Completed,
    tags: ["bug", "performance", "backend"],
    created_at: 1707750000,
    completed_at: Ok(1707836400)
  }
  let task4 = Task {
    id: "task004",
    title: "Update documentation",
    description: "Refresh API docs and add examples",
    priority: Priority::Low,
    status: TaskStatus::Cancelled,
    tags: ["docs"],
    created_at: 1707840000,
    completed_at: Err("cancelled")
  }
  let all_tasks = [task1, task2, task3, task4]
  let validation_result = validate_task_title(task1.title)
  let validation_msg = match validation_result
    | Ok(title) => "Valid title: " ++ title
    | Err(msg) => "Invalid title: " ++ msg
  end
  let high_priority_tasks = filter_by_priority(all_tasks, Priority::High)
  let analysis = analyze_tasks(all_tasks)
  let lookup_result = find_task_by_id(all_tasks, "task003")
  let lookup_msg = match lookup_result
    | Ok(task) => "Found: " ++ format_task_summary(task)
    | Err(msg) => "Lookup failed: " ++ msg
  end
  let sorted_tasks = sort_tasks_by_priority(all_tasks)
  let first_task = get(sorted_tasks, 0)
  let top_priority_msg = "Top priority: " ++ format_task_summary(first_task)
  let completion_msg = get_completed_timestamp(task3)
  let output = "=== Task Management System ===\n\n"
  let output2 = output ++ validation_msg ++ "\n\n"
  let output3 = output2 ++ "Analysis:\n" ++ analysis ++ "\n\n"
  let output4 = output3 ++ "High Priority Count: " ++ string(len(high_priority_tasks)) ++ "\n"
  let output5 = output4 ++ lookup_msg ++ "\n"
  let output6 = output5 ++ top_priority_msg ++ "\n"
  let output7 = output6 ++ completion_msg ++ "\n\n"
  let output8 = output7 ++ "Status Descriptions:\n"
  let output9 = output8 ++ "- Pending: " ++ task_status_description(TaskStatus::Pending) ++ "\n"
  let output10 = output9 ++ "- In Progress: " ++ task_status_description(TaskStatus::InProgress) ++ "\n"
  let output11 = output10 ++ "- Completed: " ++ task_status_description(TaskStatus::Completed) ++ "\n"
  output11
end

# Test cases
cell test_priority_scoring() -> bool
  let low_score = priority_score(Priority::Low)
  let urgent_score = priority_score(Priority::Urgent)
  low_score == 1 and urgent_score == 4
end

cell test_title_validation() -> bool
  let valid = validate_task_title("Valid Task")
  let empty = validate_task_title("")
  match valid
    | Ok(_) => match empty
      | Err(_) => true
      | Ok(_) => false
    end
    | Err(_) => false
  end
end

cell test_task_counting() -> bool
  let task = Task {
    id: "test",
    title: "Test Task",
    description: "Testing",
    priority: Priority::Low,
    status: TaskStatus::Pending,
    tags: [],
    created_at: 0,
    completed_at: Err("n/a")
  }
  let tasks = [task]
  let count = count_by_status(tasks, TaskStatus::Pending)
  count == 1
end

cell run_tests() -> string
  let test1 = test_priority_scoring()
  let test2 = test_title_validation()
  let test3 = test_task_counting()
  if test1 and test2 and test3 then
    "Tests: 3/3 passed"
  else
    "Tests: 0/3 passed"
  end
end
```
