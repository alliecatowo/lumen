# Your First Program

Let's build a complete program that demonstrates Lumen's key features.

## The Goal

We'll create a simple task manager that:
1. Defines a `Task` record with validation
2. Uses pattern matching on task status
3. Handles errors with the result type
4. Demonstrates string interpolation

## Step 1: Define the Data Model

Create `tasks.lm.md`:

```lumen
enum Status
  Todo
  InProgress
  Done
end

record Task
  id: Int where id > 0
  title: String where length(title) > 0
  status: Status
end
```

The `where` clauses add runtime validation.

## Step 2: Create Helper Functions

```lumen
cell create_task(id: Int, title: String) -> Task
  return Task(id: id, title: title, status: Todo)
end

cell mark_progress(task: Task) -> Task
  return Task(id: task.id, title: task.title, status: InProgress)
end

cell mark_done(task: Task) -> Task
  return Task(id: task.id, title: task.title, status: Done)
end
```

## Step 3: Pattern Matching

```lumen
cell status_label(task: Task) -> String
  match task.status
    Todo -> return "⏳ Not started"
    InProgress -> return "🔄 In progress"
    Done -> return "✅ Completed"
  end
end
```

## Step 4: Error Handling

```lumen
cell find_task(tasks: list[Task], id: Int) -> result[Task, String]
  for task in tasks
    if task.id == id
      return ok(task)
    end
  end
  return err("Task not found: {id}")
end
```

## Step 5: Put It Together

```lumen
cell main() -> String
  let tasks = [
    create_task(1, "Learn Lumen"),
    create_task(2, "Build an agent"),
    create_task(3, "Deploy to production")
  ]
  
  let result = find_task(tasks, 2)
  
  match result
    ok(task) -> return status_label(task)
    err(msg) -> return "Error: {msg}"
  end
end
```

## Run It

```bash
lumen run tasks.lm.md
```

Output:
```
🔄 In progress
```

## Complete Code

```lumen
# Task Manager
# A simple example demonstrating records, enums, pattern matching, and error handling.

enum Status
  Todo
  InProgress
  Done
end

record Task
  id: Int where id > 0
  title: String where length(title) > 0
  status: Status
end

cell create_task(id: Int, title: String) -> Task
  return Task(id: id, title: title, status: Todo)
end

cell mark_progress(task: Task) -> Task
  return Task(id: task.id, title: task.title, status: InProgress)
end

cell mark_done(task: Task) -> Task
  return Task(id: task.id, title: task.title, status: Done)
end

cell status_label(task: Task) -> String
  match task.status
    Todo -> return "⏳ Not started"
    InProgress -> return "🔄 In progress"
    Done -> return "✅ Completed"
  end
end

cell find_task(tasks: list[Task], id: Int) -> result[Task, String]
  for task in tasks
    if task.id == id
      return ok(task)
    end
  end
  return err("Task not found: {id}")
end

cell main() -> String
  let tasks = [
    create_task(1, "Learn Lumen"),
    create_task(2, "Build an agent"),
    create_task(3, "Deploy to production")
  ]
  
  let result = find_task(tasks, 2)
  
  match result
    ok(task) -> return status_label(task)
    err(msg) -> return "Error: {msg}"
  end
end
```

## Key Concepts Learned

| Concept | Example |
|---------|---------|
| Enum | `enum Status ... end` |
| Record | `record Task ... end` |
| Where constraint | `id: Int where id > 0` |
| Pattern matching | `match task.status ... end` |
| Result type | `result[Task, String]` |
| String interpolation | `"Error: {msg}"` |
| List literal | `[task1, task2]` |

## Next Steps

- [Tutorial: Basics](/learn/tutorial/basics) — Deep dive into syntax
- [Pattern Matching](/learn/tutorial/pattern-matching) — More match patterns
- [Error Handling](/learn/tutorial/error-handling) — Working with results
