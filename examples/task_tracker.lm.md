# Task Tracker

A simple task management system demonstrating records, enums, lists, and state management.

This example showcases how Lumen's type system enables building structured applications
with clear data models and state transformations.

```lumen
enum Priority
    Low
    Medium
    High
end

enum Status
    Todo
    InProgress
    Done
end

record Task
    id: int
    title: string
    priority: Priority
    status: Status
end

cell create_task(id: int, title: string, priority: Priority) -> Task
    Task {
        id: id,
        title: title,
        priority: priority,
        status: Todo
    }
end

cell start_task(task: Task) -> Task
    Task {
        id: task.id,
        title: task.title,
        priority: task.priority,
        status: InProgress
    }
end

cell complete_task(task: Task) -> Task
    Task {
        id: task.id,
        title: task.title,
        priority: task.priority,
        status: Done
    }
end

cell priority_name(p: Priority) -> string
    match p
        Low -> "Low"
        Medium -> "Medium"
        High -> "High"
    end
end

cell status_name(s: Status) -> string
    match s
        Todo -> "Todo"
        InProgress -> "In Progress"
        Done -> "Done"
    end
end

cell display_task(task: Task)
    let priority_str = priority_name(task.priority)
    let status_str = status_name(task.status)
    print("  [{task.id}] {task.title}")
    print("      Priority: {priority_str} | Status: {status_str}")
end

cell status_equals(s1: Status, s2: Status) -> bool
    match s1
        Todo -> match s2
            Todo -> true
            InProgress -> false
            Done -> false
        end
        InProgress -> match s2
            Todo -> false
            InProgress -> true
            Done -> false
        end
        Done -> match s2
            Todo -> false
            InProgress -> false
            Done -> true
        end
    end
end

cell count_by_status(tasks: list[Task], target: Status) -> int
    let count = 0
    let i = 0
    while i < len(tasks)
        let task = tasks[i]
        if status_equals(task.status, target)
            let count = count + 1
        end
        let i = i + 1
    end
    count
end

cell display_all_tasks(tasks: list[Task])
    let i = 0
    while i < len(tasks)
        let task = tasks[i]
        display_task(task)
        let i = i + 1
    end
end

cell main()
    print("=== Task Tracker Demo ===")
    print("")

    # Create initial tasks
    let task1 = create_task(1, "Design database schema", High)
    let task2 = create_task(2, "Write API documentation", Medium)
    let task3 = create_task(3, "Update README", Low)

    # Start working on some tasks
    let task1 = start_task(task1)
    let task2 = start_task(task2)

    # Complete a task
    let task1 = complete_task(task1)

    # Build task list
    let tasks = [task1, task2, task3]

    print("Current Tasks:")
    display_all_tasks(tasks)

    print("")
    print("Summary:")
    let todo_count = count_by_status(tasks, Todo)
    let in_progress_count = count_by_status(tasks, InProgress)
    let done_count = count_by_status(tasks, Done)

    print("  Todo: {todo_count}")
    print("  In Progress: {in_progress_count}")
    print("  Done: {done_count}")
end
```
