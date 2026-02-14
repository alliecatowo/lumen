# Standard Library: Testing

Simple testing framework for Lumen programs.

```lumen
# Test result record
record TestResult
  name: string
  passed: bool
  message: string
end

# Global test state (simulated with list)
cell create_test_suite() -> list[TestResult]
  return []
end

# Assert that two values are equal
cell assert_eq(actual, expected, message: string) -> TestResult
  let passed = actual == expected
  let result_message = message
  if not passed
    result_message = message + " (expected: " + string(expected) + ", got: " + string(actual) + ")"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that two values are not equal
cell assert_ne(actual, expected, message: string) -> TestResult
  let passed = actual != expected
  let result_message = message
  if not passed
    result_message = message + " (both values were: " + string(actual) + ")"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a condition is true
cell assert_true(condition: bool, message: string) -> TestResult
  return TestResult{
    name: message,
    passed: condition,
    message: message
  }
end

# Assert that a condition is false
cell assert_false(condition: bool, message: string) -> TestResult
  return TestResult{
    name: message,
    passed: not condition,
    message: message
  }
end

# Assert that a value is null
cell assert_null(value, message: string) -> TestResult
  let passed = value == null
  let result_message = message
  if not passed
    result_message = message + " (expected null, got: " + string(value) + ")"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a value is not null
cell assert_not_null(value, message: string) -> TestResult
  let passed = value != null
  let result_message = message
  if not passed
    result_message = message + " (value was null)"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a list contains a value
cell assert_contains(lst, value, message: string) -> TestResult
  let passed = contains(lst, value)
  let result_message = message
  if not passed
    result_message = message + " (list does not contain: " + string(value) + ")"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a list has expected length
cell assert_length(lst, expected_len: int, message: string) -> TestResult
  let actual_len = len(lst)
  let passed = actual_len == expected_len
  let result_message = message
  if not passed
    result_message = message + " (expected length: " + string(expected_len) + ", got: " + string(actual_len) + ")"
  end
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a string starts with a prefix
cell assert_starts_with(str_val: string, prefix: string, message: string) -> TestResult
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
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Assert that a string ends with a suffix
cell assert_ends_with(str_val: string, suffix: string, message: string) -> TestResult
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
  return TestResult{
    name: message,
    passed: passed,
    message: result_message
  }
end

# Run a test suite and print results
cell run_tests(tests: list[TestResult]) -> null
  let total = len(tests)
  let passed_count = 0
  let failed_count = 0

  print("=== Test Results ===")
  print("")

  for test in tests
    if test.passed
      print("[PASS] " + test.name)
      passed_count = passed_count + 1
    else
      print("[FAIL] " + test.message)
      failed_count = failed_count + 1
    end
  end

  print("")
  print("Total: " + string(total) + " tests")
  print("Passed: " + string(passed_count))
  print("Failed: " + string(failed_count))

  return null
end

# Add a test result to a test suite
cell add_test(suite: list[TestResult], test: TestResult) -> list[TestResult]
  return append(suite, test)
end
```
