# Showcase Utilities

Utility-facing types and helper cells used by the showcase.

```lumen
type SectionTitle = String

record RenderOptions
  show_points: Bool
  prefix_done: String
  prefix_open: String
end

record DisplayLine
  text: String
  priority: Int
end

cell default_options() -> RenderOptions
  return RenderOptions(
    show_points: true,
    prefix_done: "[x]",
    prefix_open: "[ ]"
  )
end

cell choose_prefix(done: Bool, options: RenderOptions) -> String
  if done
    return options.prefix_done
  end
  return options.prefix_open
end

cell make_line(text: String, priority: Int) -> DisplayLine
  return DisplayLine(text: text, priority: priority)
end

cell main() -> String
  let options = default_options()
  let prefix = choose_prefix(false, options)
  let line = make_line(prefix + " utility smoke test", 1)
  return line.text
end
```
