# Test Runner

Runs test cells and reports results.

```lumen
cell run_tests(dir: string) -> Null
  let files = glob(path_join(dir, "**/*.lm.md"))
  let passed = 0
  let failed = 0
  let total = 0

  let i = 0
  while i < len(files)
    let f = files[i]
    let content = read_file(f)
    if contains(content, "cell test_")
      let result = exec("target/release/lumen run " + f + " --cell main")
      total = total + 1
      if contains(string(result), "error")
        print("FAIL: {f}")
        failed = failed + 1
      else
        print("PASS: {f}")
        passed = passed + 1
      end
    end
    i = i + 1
  end

  print("")
  print("Results: {passed}/{total} passed, {failed} failed")
end

cell main() -> Null
  run_tests("tests")
end
```
