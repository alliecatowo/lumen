# Markdown Table Generator

Takes structured data (defined inline) and outputs a formatted markdown table
with column alignment. Useful for generating tables like those in TASKS.md.

```lumen
cell max_int(a: Int, b: Int) -> Int
  if a > b
    a
  else
    b
  end
end

cell pad_right(s: String, width: Int) -> String
  let result = s
  while len(result) < width
    result = result + " "
  end
  result
end

cell compute_widths(headers: list[String], rows: list[list[String]]) -> list[Int]
  let num_cols = len(headers)

  # Start with header widths
  let widths = []
  let i = 0
  while i < num_cols
    widths = append(widths, len(headers[i]))
    i = i + 1
  end

  # Update widths from each row
  let r = 0
  while r < len(rows)
    let row = rows[r]
    let new_widths = []
    let c = 0
    while c < num_cols
      let cur = widths[c]
      if c < len(row)
        new_widths = append(new_widths, max_int(cur, len(row[c])))
      else
        new_widths = append(new_widths, cur)
      end
      c = c + 1
    end
    widths = new_widths
    r = r + 1
  end

  widths
end

cell generate_table(headers: list[String], rows: list[list[String]]) -> String
  let num_cols = len(headers)
  let widths = compute_widths(headers, rows)

  # Build header row
  let header_cells = []
  let i = 0
  while i < num_cols
    header_cells = append(header_cells, pad_right(headers[i], widths[i]))
    i = i + 1
  end
  let header_line = "| " + join(header_cells, " | ") + " |"

  # Build separator row
  let sep_cells = []
  i = 0
  while i < num_cols
    let dashes = ""
    let d = 0
    while d < widths[i]
      dashes = dashes + "-"
      d = d + 1
    end
    sep_cells = append(sep_cells, dashes)
    i = i + 1
  end
  let sep_line = "| " + join(sep_cells, " | ") + " |"

  # Build data rows
  let lines = [header_line, sep_line]
  let r = 0
  while r < len(rows)
    let row = rows[r]
    let cells = []
    let c = 0
    while c < num_cols
      let val = ""
      if c < len(row)
        val = row[c]
      end
      cells = append(cells, pad_right(val, widths[c]))
      c = c + 1
    end
    lines = append(lines, "| " + join(cells, " | ") + " |")
    r = r + 1
  end

  join(lines, "\n")
end

cell main() -> Null
  print("=== Markdown Table Generator ===")
  print("")

  # Example 1: Task tracking table
  let headers = ["ID", "Task", "Status", "Owner"]
  let rows = [
    ["T423", "TOML reader tool", "Done", "dogfood"],
    ["T424", "JSON pretty-printer", "Done", "dogfood"],
    ["T425", "Table generator", "Done", "dogfood"],
    ["T426", "Diff tool", "Done", "dogfood"],
    ["T427", "Version bumper", "Done", "dogfood"],
    ["T428", "Bench runner", "Done", "dogfood"]
  ]

  let table1 = generate_table(headers, rows)
  print(table1)
  print("")

  # Example 2: Benchmark results table
  let headers2 = ["Benchmark", "Mean (ms)", "Min (ms)", "Max (ms)"]
  let rows2 = [
    ["compile_hello", "12.5", "11.2", "14.1"],
    ["compile_large", "145.3", "140.0", "152.7"],
    ["typecheck_all", "89.2", "85.6", "93.8"],
    ["vm_fibonacci", "0.8", "0.7", "1.1"]
  ]

  let table2 = generate_table(headers2, rows2)
  print(table2)

  print("")
  print("=== Done ===")
  null
end
```
