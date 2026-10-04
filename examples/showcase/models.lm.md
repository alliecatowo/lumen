# Showcase Models

Shared data types for the showcase project.

```lumen
type TaskId = String

pub record Task
  id: TaskId
  title: String
  done: Bool
  points: Int
end

pub record Board
  name: String
  tasks: list[Task]
end

pub record BoardStats
  total: Int
  done: Int
  open: Int
  points_done: Int
end
```
