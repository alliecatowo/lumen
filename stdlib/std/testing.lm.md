# Standard Library: Testing

Simple, deterministic testing helpers for Lumen programs.

```lumen
# Test result record
record TestResult
  name: String
  passed: Bool
  message: String
end

# Summary for a suite run
record TestSummary
  total: Int
  passed: Int
  failed: Int
  all_passed: Bool
end

# Create a suite (just a list of test results)
cell create_test_suite() -> list[TestResult]
  return []
end

# Add a test result to a test suite
cell add_test(suite: list[TestResult], test: TestResult) -> list[TestResult]
  return append(suite, test)
end

# Assert that two values are equal
cell assert_eq[T](actual: T, expected: T, message: String) -> TestResult
  let passed = actual == expected
  let result_message = message
  if not passed
    result_message = message + " (expected: " + string(expected) + ", got: " + string(actual) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Alias: same as assert_eq
cell assert_equal[T](actual: T, expected: T, message: String) -> TestResult
  return assert_eq(actual, expected, message)
end

# Assert that two values are not equal
cell assert_ne[T](actual: T, expected: T, message: String) -> TestResult
  let passed = actual != expected
  let result_message = message
  if not passed
    result_message = message + " (both values were: " + string(actual) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Alias: same as assert_ne
cell assert_not_equal[T](actual: T, expected: T, message: String) -> TestResult
  return assert_ne(actual, expected, message)
end

# Assert that a condition is true
cell assert_true(condition: Bool, message: String) -> TestResult
  let result_message = message
  if not condition
    result_message = message + " (expected true, got false)"
  end
  return TestResult(
    name: message,
    passed: condition,
    message: result_message
  )
end

# Assert that a condition is false
cell assert_false(condition: Bool, message: String) -> TestResult
  let passed = not condition
  let result_message = message
  if not passed
    result_message = message + " (expected false, got true)"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a value is null
cell assert_null[T](value: T | Null, message: String) -> TestResult
  let passed = value == null
  let result_message = message
  if not passed
    result_message = message + " (expected null, got: " + string(value) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a value is not null
cell assert_not_null[T](value: T | Null, message: String) -> TestResult
  let passed = value != null
  let result_message = message
  if not passed
    result_message = message + " (value was null)"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a list contains a value
cell assert_contains[T](collection: list[T], value: T, message: String) -> TestResult
  let passed = contains(collection, value)
  let result_message = message
  if not passed
    result_message = message + " (missing: " + string(value) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a list does not contain a value
cell assert_not_contains[T](collection: list[T], value: T, message: String) -> TestResult
  let passed = not contains(collection, value)
  let result_message = message
  if not passed
    result_message = message + " (unexpected value present: " + string(value) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a list has expected length
cell assert_length[T](lst: list[T], expected_len: Int, message: String) -> TestResult
  let actual_len = len(lst)
  let passed = actual_len == expected_len
  let result_message = message
  if not passed
    result_message = message + " (expected length: " + string(expected_len) + ", got: " + string(actual_len) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a list is empty
cell assert_empty[T](value: list[T], message: String) -> TestResult
  let actual_len = len(value)
  let passed = actual_len == 0
  let result_message = message
  if not passed
    result_message = message + " (expected empty, got length: " + string(actual_len) + ")"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a list is not empty
cell assert_not_empty[T](value: list[T], message: String) -> TestResult
  let actual_len = len(value)
  let passed = actual_len > 0
  let result_message = message
  if not passed
    result_message = message + " (expected non-empty)"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a string starts with a prefix
cell assert_starts_with(str_val: String, prefix: String, message: String) -> TestResult
  let actual_len = len(str_val)
  let prefix_len = len(prefix)
  let passed = false
  if prefix_len <= actual_len
    let start = slice(str_val, 0, prefix_len)
    passed = start == prefix
  end
  let result_message = message
  if not passed
    result_message = message + " ('" + str_val + "' does not start with '" + prefix + "')"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Assert that a string ends with a suffix
cell assert_ends_with(str_val: String, suffix: String, message: String) -> TestResult
  let str_len = len(str_val)
  let suffix_len = len(suffix)
  let passed = false
  if suffix_len <= str_len
    let start_pos = str_len - suffix_len
    let end_part = slice(str_val, start_pos, str_len)
    passed = end_part == suffix
  end
  let result_message = message
  if not passed
    result_message = message + " ('" + str_val + "' does not end with '" + suffix + "')"
  end
  return TestResult(
    name: message,
    passed: passed,
    message: result_message
  )
end

# Return true when every test passed
cell all_passed(tests: list[TestResult]) -> Bool
  for test in tests
    if not test.passed
      return false
    end
  end
  return true
end

# Build a deterministic summary without printing
cell summarize_tests(tests: list[TestResult]) -> TestSummary
  let total = len(tests)
  let passed_count = 0
  for test in tests
    if test.passed
      passed_count = passed_count + 1
    end
  end
  let failed_count = total - passed_count
  return TestSummary(
    total: total,
    passed: passed_count,
    failed: failed_count,
    all_passed: failed_count == 0
  )
end

# Run a test suite, print results, and return summary
cell run_tests(tests: list[TestResult]) -> TestSummary
  let summary = summarize_tests(tests)

  print("=== Test Results ===")
  print("")

  for test in tests
    if test.passed
      print("[PASS] " + test.name)
    else
      print("[FAIL] " + test.message)
    end
  end

  print("")
  print("Total: " + string(summary.total) + " tests")
  print("Passed: " + string(summary.passed))
  print("Failed: " + string(summary.failed))

  return summary
end
```
