# Minimal Task Showcase

```lumen
enum Priority
  Low
  High
end

cell priority_score(p: Priority) -> int
  match p
    | Priority::Low => 1
    | Priority::High => 3
  end
end

cell main() -> string
  let p = Priority::High
  let score = priority_score(p)
  "Priority score: " ++ string(score)
end
```
