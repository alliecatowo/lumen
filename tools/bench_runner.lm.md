# Benchmark Results Aggregator

Reads CSV benchmark output files, computes mean/median/min/max for each benchmark
name, and outputs a summary table. Expects CSV with columns: name, duration_ms.

```lumen
cell float_min(a: Float, b: Float) -> Float
  if a < b
    a
  else
    b
  end
end

cell float_max(a: Float, b: Float) -> Float
  if a > b
    a
  else
    b
  end
end

cell compute_mean(vals: list[Float]) -> Float
  let n = len(vals)
  if n == 0
    return 0.0
  end
  let total = 0.0
  let i = 0
  while i < n
    total = total + vals[i]
    i = i + 1
  end
  total / to_float(n)
end

cell compute_median(vals: list[Float]) -> Float
  let n = len(vals)
  if n == 0
    return 0.0
  end
  # Manual insertion sort for floats
  let sorted = []
  let i = 0
  while i < n
    let v = vals[i]
    let inserted = false
    let new_sorted = []
    let j = 0
    while j < len(sorted)
      if not inserted and v <= sorted[j]
        new_sorted = append(new_sorted, v)
        inserted = true
      end
      new_sorted = append(new_sorted, sorted[j])
      j = j + 1
    end
    if not inserted
      new_sorted = append(new_sorted, v)
    end
    sorted = new_sorted
    i = i + 1
  end

  let mid = n / 2
  if n % 2 == 0
    (sorted[mid - 1] + sorted[mid]) / 2.0
  else
    sorted[mid]
  end
end

cell compute_min_val(vals: list[Float]) -> Float
  let n = len(vals)
  if n == 0
    return 0.0
  end
  let result = vals[0]
  let i = 1
  while i < n
    result = float_min(result, vals[i])
    i = i + 1
  end
  result
end

cell compute_max_val(vals: list[Float]) -> Float
  let n = len(vals)
  if n == 0
    return 0.0
  end
  let result = vals[0]
  let i = 1
  while i < n
    result = float_max(result, vals[i])
    i = i + 1
  end
  result
end

cell rpad(s: String, width: Int) -> String
  let result = s
  while len(result) < width
    result = result + " "
  end
  result
end

cell format_f(val: Float) -> String
  let s = to_string(val)
  let dot_pos = index_of(s, ".")
  if dot_pos < 0
    return s + ".00"
  end
  let decimals = len(s) - dot_pos - 1
  if decimals == 0
    return s + "00"
  end
  if decimals == 1
    return s + "0"
  end
  if decimals > 2
    # Truncate to 2 decimal places
    let result = ""
    let i = 0
    while i < dot_pos + 3
      result = result + s[i]
      i = i + 1
    end
    return result
  end
  s
end

cell collect_unique_names(rows: list[list[String]]) -> list[String]
  let names = []
  let r = 1
  while r < len(rows)
    let row = rows[r]
    if len(row) >= 2
      let name = trim(row[0])
      # Check if name is already in the list
      let found = false
      let i = 0
      while i < len(names)
        if names[i] == name
          found = true
          break
        end
        i = i + 1
      end
      if not found
        names = append(names, name)
      end
    end
    r = r + 1
  end
  names
end

cell collect_values_for(rows: list[list[String]], target_name: String) -> list[Float]
  let vals = []
  let r = 1
  while r < len(rows)
    let row = rows[r]
    if len(row) >= 2
      let name = trim(row[0])
      if name == target_name
        vals = append(vals, to_float(trim(row[1])))
      end
    end
    r = r + 1
  end
  vals
end

cell process_csv_data(csv_text: String) -> Null
  let rows = csv_parse(csv_text)

  if len(rows) < 2
    print("No benchmark data found.")
    return null
  end

  let names = collect_unique_names(rows)

  # Print summary table header
  print("| " + rpad("Benchmark", 20) + " | " + rpad("Mean", 10) + " | " + rpad("Median", 10) + " | " + rpad("Min", 10) + " | " + rpad("Max", 10) + " | " + rpad("Runs", 5) + " |")
  print("| " + rpad("--------------------", 20) + " | " + rpad("----------", 10) + " | " + rpad("----------", 10) + " | " + rpad("----------", 10) + " | " + rpad("----------", 10) + " | " + rpad("-----", 5) + " |")

  let i = 0
  while i < len(names)
    let name = names[i]
    let floats = collect_values_for(rows, name)

    let mean = format_f(compute_mean(floats))
    let median = format_f(compute_median(floats))
    let min_v = format_f(compute_min_val(floats))
    let max_v = format_f(compute_max_val(floats))
    let runs = to_string(len(floats))

    print("| " + rpad(name, 20) + " | " + rpad(mean, 10) + " | " + rpad(median, 10) + " | " + rpad(min_v, 10) + " | " + rpad(max_v, 10) + " | " + rpad(runs, 5) + " |")
    i = i + 1
  end
  null
end

cell main() -> Null
  print("=== Benchmark Results Aggregator ===")
  print("")

  # Look for benchmark CSV files
  let csv_files = glob("bench*.csv")
  if len(csv_files) > 0
    let i = 0
    while i < len(csv_files)
      print("Processing: {csv_files[i]}")
      print("")
      let content = read_file(csv_files[i])
      process_csv_data(content)
      print("")
      i = i + 1
    end
  else
    # Use sample data for demo
    print("No benchmark CSV files found — using sample data.")
    print("")

    let sample = "name,duration_ms\ncompile_hello,12.5\ncompile_hello,11.8\ncompile_hello,13.1\ncompile_hello,12.2\ncompile_hello,11.9\ncompile_large,145.3\ncompile_large,140.1\ncompile_large,148.7\ncompile_large,143.2\ntypecheck_suite,89.2\ntypecheck_suite,87.1\ntypecheck_suite,91.5\nvm_fibonacci,0.82\nvm_fibonacci,0.79\nvm_fibonacci,0.85\nvm_fibonacci,0.81"

    process_csv_data(sample)
  end

  print("")
  print("=== Done ===")
  null
end
```
