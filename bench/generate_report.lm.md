# Benchmark Report Generator

Reads CSV benchmark results and generates a Markdown report with summary tables,
relative performance analysis, and per-benchmark detail breakdowns.

Replaces `bench/generate_report.py` — Task T420 (pre-bootstrap maturity).

## Statistics Helpers

```lumen
cell stat_min(values: list[Float]) -> Float
  let n = len(values)
  if n == 0
    return 0.0
  end
  let result = values[0]
  let i = 1
  while i < n
    if values[i] < result
      result = values[i]
    end
    i = i + 1
  end
  result
end

cell stat_max(values: list[Float]) -> Float
  let n = len(values)
  if n == 0
    return 0.0
  end
  let result = values[0]
  let i = 1
  while i < n
    if values[i] > result
      result = values[i]
    end
    i = i + 1
  end
  result
end

cell stat_mean(values: list[Float]) -> Float
  let n = len(values)
  if n == 0
    return 0.0
  end
  let total = 0.0
  for v in values
    total = total + v
  end
  total / to_float(n)
end

cell stat_median(values: list[Float]) -> Float
  let sorted = sort(values)
  let n = len(sorted)
  if n == 0
    return 0.0
  end
  if n % 2 == 0
    (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
  else
    sorted[n / 2]
  end
end

cell stat_stdev(values: list[Float]) -> Float
  let n = len(values)
  if n <= 1
    return 0.0
  end
  let m = stat_mean(values)
  let sum_sq = 0.0
  for v in values
    let diff = v - m
    sum_sq = sum_sq + diff * diff
  end
  sqrt(sum_sq / to_float(n - 1))
end
```

## CSV Loading and Grouping

The benchmark CSV has columns: `benchmark`, `language`, `time_ms`, plus optional extras.
Rows where `time_ms` is `"ERROR"` are skipped.

We store each valid measurement as a `Measurement`, then compute stats per unique
benchmark/language pair.

```lumen
record Measurement
  benchmark: String
  language: String
  time_ms: Float
end

record BenchStats
  benchmark: String
  language: String
  median: Float
  mean: Float
  min_val: Float
  max_val: Float
  stdev: Float
  runs: Int
end

cell load_measurements(csv_path: String) -> list[Measurement]
  let text = read_file(csv_path)
  let rows = csv_parse(text)

  if len(rows) < 2
    return []
  end

  # Find column indices from header row
  let header = rows[0]
  let bench_col = find_col(header, "benchmark")
  let lang_col = find_col(header, "language")
  let time_col = find_col(header, "time_ms")

  if bench_col < 0 or lang_col < 0 or time_col < 0
    print("Error: CSV missing required columns (benchmark, language, time_ms)")
    return []
  end

  let measurements = []
  let ri = 1
  while ri < len(rows)
    let row = rows[ri]
    if len(row) > time_col
      let time_str = trim(row[time_col])
      if time_str != "ERROR" and time_str != ""
        let m = Measurement(
          benchmark: trim(row[bench_col]),
          language: trim(row[lang_col]),
          time_ms: to_float(time_str)
        )
        measurements = append(measurements, m)
      end
    end
    ri = ri + 1
  end
  measurements
end

cell collect_times(measurements: list[Measurement], bench: String, lang: String) -> list[Float]
  let times = []
  for m in measurements
    if m.benchmark == bench and m.language == lang
      times = append(times, m.time_ms)
    end
  end
  times
end

cell aggregate(measurements: list[Measurement]) -> list[BenchStats]
  # Find unique benchmark|language pairs
  let keys = []
  for m in measurements
    let key = m.benchmark + "|" + m.language
    if not list_contains(keys, key)
      keys = append(keys, key)
    end
  end

  let stats = []
  for key in keys
    let parts = split(key, "|")
    let bench = parts[0]
    let lang = parts[1]
    let times = collect_times(measurements, bench, lang)

    let s = BenchStats(
      benchmark: bench,
      language: lang,
      median: stat_median(times),
      mean: stat_mean(times),
      min_val: stat_min(times),
      max_val: stat_max(times),
      stdev: stat_stdev(times),
      runs: len(times)
    )
    stats = append(stats, s)
  end
  stats
end

cell find_col(header: list[String], name: String) -> Int
  let i = 0
  while i < len(header)
    if trim(header[i]) == name
      return i
    end
    i = i + 1
  end
  -1
end
```

## Unique Value Extraction

```lumen
cell unique_benchmarks(stats: list[BenchStats]) -> list[String]
  let result = []
  for s in stats
    if not list_contains(result, s.benchmark)
      result = append(result, s.benchmark)
    end
  end
  sort(result)
end

cell unique_languages(stats: list[BenchStats]) -> list[String]
  let result = []
  for s in stats
    if not list_contains(result, s.language)
      result = append(result, s.language)
    end
  end
  sort(result)
end

cell list_contains(items: list[String], target: String) -> Bool
  for item in items
    if item == target
      return true
    end
  end
  false
end

cell find_stat(stats: list[BenchStats], bench: String, lang: String) -> BenchStats?
  for s in stats
    if s.benchmark == bench and s.language == lang
      return s
    end
  end
  null
end

cell has_stat(stats: list[BenchStats], bench: String, lang: String) -> Bool
  for s in stats
    if s.benchmark == bench and s.language == lang
      return true
    end
  end
  false
end

cell get_stat(stats: list[BenchStats], bench: String, lang: String) -> BenchStats
  for s in stats
    if s.benchmark == bench and s.language == lang
      return s
    end
  end
  # Should never reach here — caller must check has_stat first
  BenchStats(benchmark: "", language: "", median: 0.0, mean: 0.0, min_val: 0.0, max_val: 0.0, stdev: 0.0, runs: 0)
end

cell fastest_language(stats: list[BenchStats], bench: String) -> String
  let best_lang = ""
  let best_time = 999999999.0
  for s in stats
    if s.benchmark == bench
      if s.median < best_time
        best_time = s.median
        best_lang = s.language
      end
    end
  end
  best_lang
end
```

## Number Formatting

```lumen
cell fmt_f0(val: Float) -> String
  # Format float with 0 decimal places
  to_string(to_int(val + 0.5))
end

cell fmt_f1(val: Float) -> String
  # Format float with 1 decimal place
  let whole = to_int(val)
  let frac = to_int((val - to_float(whole)) * 10.0 + 0.5)
  if frac >= 10
    to_string(whole + 1) + ".0"
  else
    to_string(whole) + "." + to_string(frac)
  end
end

cell fmt_ratio(val: Float) -> String
  fmt_f1(val) + "x"
end
```

## Report Generation — Summary Table

```lumen
cell summary_row(stats: list[BenchStats], bench: String, languages: list[String]) -> String
  let fastest = fastest_language(stats, bench)
  let row = "| " + bench + " |"
  for lang in languages
    if has_stat(stats, bench, lang)
      let s = get_stat(stats, bench, lang)
      if lang == fastest
        row = row + " **" + fmt_f0(s.median) + "** |"
      else
        row = row + " " + fmt_f0(s.median) + " |"
      end
    else
      row = row + " - |"
    end
  end
  row = row + " " + fastest + " |"
  row
end

cell gen_summary(stats: list[BenchStats], benchmarks: list[String], languages: list[String]) -> list[String]
  let lines = []
  lines = append(lines, "## Summary (median time in ms)")
  lines = append(lines, "")

  let header = "| Benchmark |"
  for lang in languages
    header = header + " " + lang + " |"
  end
  header = header + " Fastest |"
  lines = append(lines, header)

  let sep = "|-----------|"
  for lang in languages
    sep = sep + "------:|"
  end
  sep = sep + "---------|"
  lines = append(lines, sep)

  for bench in benchmarks
    lines = append(lines, summary_row(stats, bench, languages))
  end
  lines = append(lines, "")
  lines
end
```

## Report Generation — Relative Performance

```lumen
cell relative_row(stats: list[BenchStats], bench: String, languages: list[String]) -> String
  let c_median = 0.0
  if has_stat(stats, bench, "c")
    let c_stat = get_stat(stats, bench, "c")
    c_median = c_stat.median
  end

  let row = "| " + bench + " |"
  for lang in languages
    if has_stat(stats, bench, lang)
      let s = get_stat(stats, bench, lang)
      if c_median > 0.0
        let ratio = s.median / c_median
        row = row + " " + fmt_ratio(ratio) + " |"
      else
        row = row + " " + fmt_f0(s.median) + "ms |"
      end
    else
      row = row + " - |"
    end
  end
  row
end

cell gen_relative(stats: list[BenchStats], benchmarks: list[String], languages: list[String]) -> list[String]
  let lines = []
  lines = append(lines, "## Relative Performance (vs C baseline)")
  lines = append(lines, "")
  lines = append(lines, "Values show how many times slower than C (1.0x = same speed).")
  lines = append(lines, "")

  let header = "| Benchmark |"
  for lang in languages
    header = header + " " + lang + " |"
  end
  lines = append(lines, header)

  let sep = "|-----------|"
  for lang in languages
    sep = sep + "------:|"
  end
  lines = append(lines, sep)

  for bench in benchmarks
    lines = append(lines, relative_row(stats, bench, languages))
  end
  lines = append(lines, "")
  lines
end
```

## Report Generation — Detailed Results

```lumen
cell detail_row(s: BenchStats) -> String
  "| " + s.language
      + " | " + fmt_f1(s.median)
      + " | " + fmt_f1(s.mean)
      + " | " + fmt_f1(s.min_val)
      + " | " + fmt_f1(s.max_val)
      + " | " + fmt_f1(s.stdev)
      + " | " + to_string(s.runs)
      + " |"
end

cell gen_details(stats: list[BenchStats], benchmarks: list[String], languages: list[String]) -> list[String]
  let lines = []
  lines = append(lines, "## Detailed Results")
  lines = append(lines, "")

  for bench in benchmarks
    lines = append(lines, "### " + bench)
    lines = append(lines, "")
    lines = append(lines, "| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |")
    lines = append(lines, "|----------|----------:|--------:|-------:|-------:|------:|-----:|")

    for lang in languages
      if has_stat(stats, bench, lang)
        let s = get_stat(stats, bench, lang)
        lines = append(lines, detail_row(s))
      end
    end
    lines = append(lines, "")
  end
  lines
end
```

## Report Generation — Lumen Analysis

```lumen
cell lumen_analysis_row(stats: list[BenchStats], bench: String, lumen_stat: BenchStats) -> String
  let rank = 1
  let total_langs = 0
  let fastest_name = ""
  let fastest_median = 999999999.0

  for s in stats
    if s.benchmark == bench
      total_langs = total_langs + 1
      if s.median < lumen_stat.median
        rank = rank + 1
      end
      if s.median < fastest_median
        fastest_median = s.median
        fastest_name = s.language
      end
    end
  end

  let ratio = 0.0
  if fastest_median > 0.0
    ratio = lumen_stat.median / fastest_median
  end

  "| " + bench
      + " | " + to_string(rank) + "/" + to_string(total_langs)
      + " | " + fmt_ratio(ratio)
      + " | " + fastest_name
      + " |"
end

cell gen_lumen_analysis(stats: list[BenchStats], benchmarks: list[String]) -> list[String]
  let lines = []
  lines = append(lines, "## Lumen Performance Analysis")
  lines = append(lines, "")

  let lumen_rows = []
  let ratio_sum = 0.0
  let ratio_count = 0

  for bench in benchmarks
    if has_stat(stats, bench, "lumen")
      let lumen_stat = get_stat(stats, bench, "lumen")
      lumen_rows = append(lumen_rows, lumen_analysis_row(stats, bench, lumen_stat))

      let fastest_median = 999999999.0
      for s in stats
        if s.benchmark == bench and s.median < fastest_median
          fastest_median = s.median
        end
      end
      if fastest_median > 0.0
        ratio_sum = ratio_sum + lumen_stat.median / fastest_median
        ratio_count = ratio_count + 1
      end
    end
  end

  if len(lumen_rows) > 0
    lines = append(lines, "| Benchmark | Lumen Rank | vs Fastest | Fastest Language |")
    lines = append(lines, "|-----------|:----------:|:----------:|:----------------:|")
    for row in lumen_rows
      lines = append(lines, row)
    end
    lines = append(lines, "")

    if ratio_count > 0
      let avg = ratio_sum / to_float(ratio_count)
      lines = append(lines, "Average slowdown vs fastest: **" + fmt_ratio(avg) + "**")
      lines = append(lines, "")
    end
  else
    lines = append(lines, "No Lumen results found in the data.")
    lines = append(lines, "")
  end
  lines
end
```

## Report Generation — Main

```lumen
cell generate_report(stats: list[BenchStats], csv_path: String) -> String
  let benchmarks = unique_benchmarks(stats)
  let languages = unique_languages(stats)

  let lines = []
  lines = append(lines, "# Lumen Cross-Language Benchmark Report")
  lines = append(lines, "")
  lines = append(lines, "Source: `" + csv_path + "`")
  lines = append(lines, "")

  for line in gen_summary(stats, benchmarks, languages)
    lines = append(lines, line)
  end

  for line in gen_relative(stats, benchmarks, languages)
    lines = append(lines, line)
  end

  for line in gen_details(stats, benchmarks, languages)
    lines = append(lines, line)
  end

  for line in gen_lumen_analysis(stats, benchmarks)
    lines = append(lines, line)
  end

  lines = append(lines, "---")
  lines = append(lines, "*Generated by `bench/generate_report.lm.md`*")
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Entry Point

```lumen
cell find_csv_files(dir: String) -> list[String]
  let entries = read_dir(dir)
  let csv_files = []
  for entry in entries
    if ends_with(entry, ".csv")
      csv_files = append(csv_files, dir + "/" + entry)
    end
  end
  csv_files
end

cell main() -> Int
  # Check if a results directory exists
  if not exists("bench/results")
    print("No bench/results/ directory found.")
    print("Run benchmarks first with: bash bench/run_all.sh")
    print("")
    print("Expected CSV format:")
    print("  benchmark,language,time_ms")
    print("  fibonacci,c,12.3")
    print("  fibonacci,lumen,45.6")
    return 1
  end

  # Find CSV files in the results directory
  let files = find_csv_files("bench/results")
  if len(files) == 0
    print("No CSV files found in bench/results/")
    print("Run benchmarks first with: bash bench/run_all.sh")
    return 1
  end

  # Use first CSV file found, or results.csv if it exists
  let target = files[0]
  for f in files
    if contains(f, "results.csv")
      target = f
    end
  end

  print("Reading benchmark data from: " + target)

  let measurements = load_measurements(target)
  if len(measurements) == 0
    print("No valid benchmark results found in " + target)
    return 1
  end

  let stats = aggregate(measurements)
  if len(stats) == 0
    print("No valid benchmark results found in " + target)
    return 1
  end

  print("Found " + to_string(len(measurements)) + " measurements across " + to_string(len(stats)) + " benchmark/language combinations")

  let report = generate_report(stats, target)

  # Write report to file
  let output_path = "bench/results/report.md"
  write_file(output_path, report)
  print("Report written to " + output_path)

  # Also print to stdout
  print("")
  print(report)

  0
end
```
