# LOC Counter

A lines-of-code counter for Lumen projects, written in Lumen.

```lumen
cell count_lines(path: string) -> int
  let content = read_file(path)
  let lines = split(content, "\n")
  let count = 0
  let i = 0
  while i < len(lines)
    let line = lines[i]
    let trimmed = trim(line)
    if len(trimmed) > 0
      count = count + 1
    end
    i = i + 1
  end
  count
end

cell main() -> Null
  let files = glob("**/*.rs")
  let total = 0
  let i = 0
  while i < len(files)
    let f = files[i]
    let count = count_lines(f)
    print("{f}: {count} lines")
    total = total + count
    i = i + 1
  end
  print("Total: {total} lines")
end
```
