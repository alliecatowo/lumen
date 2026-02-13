# Minimal Showcase

Smallest runnable import-based example.

```lumen
import models: Task

cell main() -> String
  let task = Task(id: "M-1", title: "Minimal import demo", done: false, points: 1)
  return task.id + ": " + task.title
end
```
