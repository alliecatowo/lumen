# Source Reader

Reads a `.lm.md` file and extracts fenced Lumen code blocks, printing each
block with its number. Demonstrates file I/O, string operations, and
control flow in Lumen.

```lumen
cell main() -> Int
  let path = "tools/source_reader.lm.md"
  let content = read_file(path)
  let lines = split(content, "\n")
  let in_block = false
  let block_num = 0
  let i = 0

  while i < len(lines)
    let line = lines[i]
    let trimmed = trim(line)

    if in_block == false
      if starts_with(trimmed, "```lumen")
        in_block = true
        block_num = block_num + 1
        print("--- Block {to_string(block_num)} ---")
      end
    else
      if trimmed == "```"
        in_block = false
        print("--- End Block ---")
        print("")
      else
        print(line)
      end
    end

    i = i + 1
  end

  print("Extracted {to_string(block_num)} code blocks from {path}")
  0
end
```
