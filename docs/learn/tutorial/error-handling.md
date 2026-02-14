# Tutorial: Error Handling

Learn how to handle errors gracefully with Lumen's result type.

## The Result Type

Lumen uses `result[Ok, Err]` for error handling:

```lumen
cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end
```

## Handling Results

### Pattern Matching

```lumen
cell safe_divide(a: Int, b: Int) -> String
  match divide(a, b)
    ok(value) -> return "Result: {value}"
    err(msg) -> return "Error: {msg}"
  end
end
```

### The `try` Operator

Use `try` to propagate errors:

```lumen
cell calculate(a: Int, b: Int) -> result[Int, String]
  let x = try divide(a, b)      # Returns err if divide fails
  let y = try divide(x, 2)      # Returns err if this fails
  return ok(y)
end
```

### The `??` Operator

Provide a default value:

```lumen
cell main() -> Int
  let result = divide(10, 0)
  let value = result ?? 0  # Use 0 if result is err
  return value
end
```

## Creating Results

### Ok Values

```lumen
cell find_user(id: Int) -> result[String, String]
  if id == 1
    return ok("Alice")
  end
  return err("User not found")
end
```

### Err Values

```lumen
cell validate_age(age: Int) -> result[Int, String]
  if age < 0
    return err("Age cannot be negative")
  end
  if age > 150
    return err("Age seems unrealistic")
  end
  return ok(age)
end
```

## Error Propagation

Chain operations that can fail:

```lumen
cell process_user(id: Int) -> result[String, String]
  let user = try find_user(id)
  let age = try parse_age(user)
  let validated = try validate_age(age)
  return ok("User {user} is {validated} years old")
end
```

If any `try` fails, the function returns the error immediately.

## Multiple Error Types

Different errors for different failures:

```lumen
enum DbError
  ConnectionFailed
  NotFound
  Timeout
end

enum ParseError
  InvalidFormat
  MissingField(name: String)
end

cell load_config() -> result[Config, ParseError]
  # ...
end

cell save_to_db(config: Config) -> result[Null, DbError]
  # ...
end
```

## Result Methods

### is_ok / is_err

Check result status:

```lumen
cell check(result: result[Int, String]) -> String
  if result.is_ok()
    return "Success"
  end
  return "Failed"
end
```

### unwrap

Get the value or panic. This is best reserved for tests and one-off scripts, not production paths:

```lumen
cell dangerous() -> Int
  let result = divide(10, 2)
  return result.unwrap()  # Panics on err
end
```

### unwrap_or

Get the value with a fallback:

```lumen
cell safe_value() -> Int
  let result = divide(10, 0)
  return result.unwrap_or(0)  # Returns 0 on error
end
```

## Practical Example: User Registration

```lumen
record User
  username: String where length(username) >= 3
  email: String where email.contains("@")
  age: Int where age >= 13
end

enum ValidationError
  UsernameTooShort
  InvalidEmail
  Underage
end

cell validate_username(username: String) -> result[String, ValidationError]
  if length(username) < 3
    return err(UsernameTooShort)
  end
  return ok(username)
end

cell validate_email(email: String) -> result[String, ValidationError]
  if not email.contains("@")
    return err(InvalidEmail)
  end
  return ok(email)
end

cell validate_age(age: Int) -> result[Int, ValidationError]
  if age < 13
    return err(Underage)
  end
  return ok(age)
end

cell create_user(username: String, email: String, age: Int) -> result[User, ValidationError]
  let valid_username = try validate_username(username)
  let valid_email = try validate_email(email)
  let valid_age = try validate_age(age)
  
  return ok(User(
    username: valid_username,
    email: valid_email,
    age: valid_age
  ))
end

cell main() -> String
  match create_user("alice", "alice@example.com", 25)
    ok(user) -> return "Created user: {user.username}"
    err(UsernameTooShort) -> return "Username must be at least 3 characters"
    err(InvalidEmail) -> return "Invalid email address"
    err(Underage) -> return "Must be at least 13 years old"
  end
end
```

## Result vs Null

Use `result` when:
- You need to explain why something failed
- There are multiple failure modes
- The caller should handle errors differently

Use `Null` in unions when:
- Failure is rare and uninteresting
- You just need "value or nothing"

```lumen
# Result for meaningful errors
cell parse_int(s: String) -> result[Int, String]
  # Returns error explaining why parsing failed
end

# Null for simple optionality
cell find_first(items: list[Int]) -> Int | Null
  # Returns null if empty, no explanation needed
end
```

## Best Practices

1. **Use descriptive error types** — `err("file not found")` vs `err(FileNotFound)`
2. **Propagate with try** — Don't match and re-return errors
3. **Handle at the right level** — Let errors bubble up to where they can be handled
4. **Don't over-use unwrap** — Handle errors explicitly in production code

## Runtime Diagnostics

The runtime now surfaces common failure modes as explicit errors instead of silently ignoring them:

- Invalid indexed writes like assigning with `[]` to a non-container value return a runtime type error.
- Tool output/contract mismatches return `OutputValidationFailed` with both expected schema and actual output details.

## Next Steps

- [AI-Native Features](/learn/ai-native/tools) — Tools and grants
- [Advanced Effects](/learn/advanced/effects) — Effect system deep dive
