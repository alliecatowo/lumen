# Data Pipeline

> A data transformation pipeline demonstrating map, filter, and reduce patterns.
> Shows functional-style data processing using Lumen's for loops and records.

```lumen
record DataPoint
  label: String
  value: Float
  category: String
end

record Summary
  category: String
  count: Int
  total: Float
  average: Float
end

cell make_point(label: String, value: Float, category: String) -> DataPoint
  return DataPoint(label: label, value: value, category: category)
end

cell generate_data() -> list[DataPoint]
  let data = []
  data = append(data, make_point("alpha", 42.5, "A"))
  data = append(data, make_point("beta", 17.3, "B"))
  data = append(data, make_point("gamma", 88.1, "A"))
  data = append(data, make_point("delta", 5.7, "C"))
  data = append(data, make_point("epsilon", 63.2, "B"))
  data = append(data, make_point("zeta", 91.4, "A"))
  data = append(data, make_point("eta", 28.9, "C"))
  data = append(data, make_point("theta", 54.6, "B"))
  data = append(data, make_point("iota", 12.1, "A"))
  data = append(data, make_point("kappa", 76.8, "C"))
  return data
end

cell filter_by_category(data: list[DataPoint], cat: String) -> list[DataPoint]
  let items = []
  for dp in data
    if dp.category == cat
      items = append(items, dp)
    end
  end
  return items
end

cell filter_above(data: list[DataPoint], threshold: Float) -> list[DataPoint]
  let items = []
  for dp in data
    if dp.value > threshold
      items = append(items, dp)
    end
  end
  return items
end

cell sum_values(data: list[DataPoint]) -> Float
  let total = 0.0
  for dp in data
    total = total + dp.value
  end
  return total
end

cell find_min(data: list[DataPoint]) -> Float
  let m = 999999.0
  for dp in data
    if dp.value < m
      m = dp.value
    end
  end
  return m
end

cell find_max(data: list[DataPoint]) -> Float
  let m = 0.0
  for dp in data
    if dp.value > m
      m = dp.value
    end
  end
  return m
end

cell summarize(data: list[DataPoint], cat: String) -> Summary
  let filtered = filter_by_category(data, cat)
  let count = len(filtered)
  let total = sum_values(filtered)
  let avg = total / to_float(count)
  return Summary(category: cat, count: count, total: total, average: avg)
end

cell extract_labels(data: list[DataPoint]) -> list[String]
  let labels = []
  for dp in data
    labels = append(labels, dp.label)
  end
  return labels
end

cell main() -> Null
  print("═══════════════════════════════════")
  print("  📊 Lumen Data Pipeline")
  print("═══════════════════════════════════")
  print("")

  let data = generate_data()
  print("Raw data (" + to_string(len(data)) + " points):")
  for dp in data
    print("  " + dp.label + ": " + to_string(dp.value) + " [" + dp.category + "]")
  end

  print("")
  print("─── Filter: value > 50.0 ───")
  let high = filter_above(data, 50.0)
  for dp in high
    print("  " + dp.label + ": " + to_string(dp.value))
  end

  print("")
  print("─── Category Summaries ───")
  let categories = ["A", "B", "C"]
  for cat in categories
    let s = summarize(data, cat)
    print("  Category " + s.category + ":")
    print("    Count:   " + to_string(s.count))
    print("    Total:   " + to_string(s.total))
    print("    Average: " + to_string(s.average))
  end

  print("")
  let grand_total = sum_values(data)
  print("Grand total: " + to_string(grand_total))
  print("Pipeline complete.")
  return null
end
```
