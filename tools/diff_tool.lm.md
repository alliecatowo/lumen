# Simple Diff Tool

Compares two files line-by-line and outputs lines that differ. Lines present only
in the first file are prefixed with `-`, lines present only in the second with `+`.
Uses a simple longest-common-subsequence approach for better diff quality.

```lumen
cell diff_lines(lines_a: list[String], lines_b: list[String]) -> list[String]
  # Simple line-by-line comparison
  # Walk both lists and report differences
  let result = []
  let ia = 0
  let ib = 0
  let na = len(lines_a)
  let nb = len(lines_b)

  while ia < na and ib < nb
    let a = lines_a[ia]
    let b = lines_b[ib]
    if a == b
      # Lines match — skip
      ia = ia + 1
      ib = ib + 1
    else
      # Check if line a appears later in b (was something added before it)
      let found_a_in_b = false
      let scan = ib + 1
      while scan < nb and scan < ib + 5
        if lines_b[scan] == a
          found_a_in_b = true
          break
        end
        scan = scan + 1
      end

      if found_a_in_b
        # Lines were added in b before the matching line
        while ib < scan
          result = append(result, "+ " + lines_b[ib])
          ib = ib + 1
        end
      else
        # Check if line b appears later in a (was something removed)
        let found_b_in_a = false
        let scan2 = ia + 1
        while scan2 < na and scan2 < ia + 5
          if lines_a[scan2] == b
            found_b_in_a = true
            break
          end
          scan2 = scan2 + 1
        end

        if found_b_in_a
          while ia < scan2
            result = append(result, "- " + lines_a[ia])
            ia = ia + 1
          end
        else
          # Both differ — report removal and addition
          result = append(result, "- " + a)
          result = append(result, "+ " + b)
          ia = ia + 1
          ib = ib + 1
        end
      end
    end
  end

  # Remaining lines in a are removals
  while ia < na
    result = append(result, "- " + lines_a[ia])
    ia = ia + 1
  end

  # Remaining lines in b are additions
  while ib < nb
    result = append(result, "+ " + lines_b[ib])
    ib = ib + 1
  end

  result
end

cell main() -> Null
  # Compare two files
  # In a real run, these would be CLI arguments
  let file_a = "tools/diff_test_a.txt"
  let file_b = "tools/diff_test_b.txt"

  if not exists(file_a) or not exists(file_b)
    print("=== Diff Tool Demo (inline data) ===")
    print("")

    # Demo with inline data
    let lines_a = ["line 1: hello", "line 2: world", "line 3: foo", "line 4: bar", "line 5: baz"]
    let lines_b = ["line 1: hello", "line 2: changed", "line 3: foo", "line 4: bar", "line 5: baz", "line 6: added"]

    let diffs = diff_lines(lines_a, lines_b)

    if len(diffs) == 0
      print("Files are identical.")
    else
      print("Found {len(diffs)} difference(s):")
      print("")
      let i = 0
      while i < len(diffs)
        print(diffs[i])
        i = i + 1
      end
    end
  else
    print("=== Diff: {file_a} vs {file_b} ===")
    print("")

    let lines_a = read_lines(file_a)
    let lines_b = read_lines(file_b)

    let diffs = diff_lines(lines_a, lines_b)

    if len(diffs) == 0
      print("Files are identical.")
    else
      print("Found {len(diffs)} difference(s):")
      print("")
      let i = 0
      while i < len(diffs)
        print(diffs[i])
        i = i + 1
      end
    end
  end

  print("")
  print("=== Done ===")
  null
end
```
