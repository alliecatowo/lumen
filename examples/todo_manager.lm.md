# Todo Manager

> A task management system demonstrating records, enums, lists, and pattern matching.
> Shows Lumen's type system and data modeling capabilities.

```lumen
enum Priority
  Low
  Medium
  High
  Critical
end

record Task
  id: Int
  title: String
  priority: String
  done: Bool
end

cell create_task(id: Int, title: String, priority: String) -> Task
  return Task(id: id, title: title, priority: priority, done: false)
end

cell priority_label(p: String) -> String
  match p
    "Critical" -> return "🔴 CRITICAL"
    "High"     -> return "🟠 High"
    "Medium"   -> return "🟡 Medium"
    "Low"      -> return "🟢 Low"
    _          -> return "⚪ Unknown"
  end
end

cell format_task(task: Task) -> String
  let status = "[x]"
  if not task.done
    status = "[ ]"
  end
  let label = priority_label(task.priority)
  return status + " " + label + " — " + task.title
end

cell count_done(tasks: list[Task]) -> Int
  let count = 0
  for task in tasks
    if task.done
      count = count + 1
    end
  end
  return count
end

cell count_by_priority(tasks: list[Task], priority: String) -> Int
  let count = 0
  for task in tasks
    if task.priority == priority
      count = count + 1
    end
  end
  return count
end

cell filter_pending(tasks: list[Task]) -> list[Task]
  let items = []
  for task in tasks
    if not task.done
      items = append(items, task)
    end
  end
  return items
end

cell main() -> Null
  print("═══════════════════════════════")
  print("  📋 Lumen Todo Manager")
  print("═══════════════════════════════")
  print("")

  let tasks = [
    Task(id: 1, title: "Design Lumen type system", priority: "Critical", done: true),
    Task(id: 2, title: "Implement register VM", priority: "Critical", done: true),
    Task(id: 3, title: "Build trace system", priority: "High", done: true),
    Task(id: 4, title: "Add MCP bridge", priority: "High", done: false),
    Task(id: 5, title: "Write conformance tests", priority: "Medium", done: false),
    Task(id: 6, title: "Create LSP server", priority: "Medium", done: false),
    Task(id: 7, title: "Add syntax highlighting", priority: "Low", done: false),
    Task(id: 8, title: "Write documentation", priority: "High", done: false)
  ]

  print("All tasks:")
  for task in tasks
    print("  " + format_task(task))
  end

  let done = count_done(tasks)
  let total = len(tasks)
  print("")
  print("Progress: " + to_string(done) + "/" + to_string(total) + " complete")

  print("")
  print("By priority:")
  print("  🔴 Critical: " + to_string(count_by_priority(tasks, "Critical")))
  print("  🟠 High:     " + to_string(count_by_priority(tasks, "High")))
  print("  🟡 Medium:   " + to_string(count_by_priority(tasks, "Medium")))
  print("  🟢 Low:      " + to_string(count_by_priority(tasks, "Low")))

  print("")
  let pending = filter_pending(tasks)
  print("Pending tasks (" + to_string(len(pending)) + "):")
  for task in pending
    print("  → " + task.title)
  end

  return null
end
```
