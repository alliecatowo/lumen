# Showcase Models

Shared data types for the showcase project.

```lumen
type TaskId = String

record Task
  id: TaskId
  title: String
  done: Bool
  points: Int
end

record Board
  name: String
  tasks: list[Task]
end

record BoardStats
  total: Int
  done: Int
  open: Int
  points_done: Int
end
```
