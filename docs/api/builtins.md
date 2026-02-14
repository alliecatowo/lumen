# API Reference: Builtins

Lumen provides built-in functions that are always available.

## Type Checking

### type_of

Get the type of a value:

```lumen
type_of(42)        # "Int"
type_of("hello")   # "String"
type_of([1, 2])    # "list[Int]"
```

## Collections

### length

Get the size of a collection:

```lumen
length([1, 2, 3])        # 3
length("hello")          # 5
length({"a": 1, "b": 2}) # 2
length({1, 2, 3})        # 3
```

### push / append

Add to a list:

```lumen
let items = [1, 2]
push(items, 3)      # [1, 2, 3]
append(items, 3)    # Same as push
```

### contains

Check if a value is in a collection:

```lumen
contains([1, 2, 3], 2)       # true
contains("hello", "ell")     # true
contains({1, 2, 3}, 2)       # true
```

### keys / values

Get map keys or values:

```lumen
let m = {"a": 1, "b": 2}
keys(m)      # ["a", "b"]
values(m)    # [1, 2]
```

### enumerate

Get index-value pairs:

```lumen
enumerate(["a", "b", "c"])
# [(0, "a"), (1, "b"), (2, "c")]
```

### filter

Filter a list:

```lumen
filter([1, 2, 3, 4, 5], fn(x) => x > 2)
# [3, 4, 5]
```

### map

Transform a list:

```lumen
map([1, 2, 3], fn(x) => x * 2)
# [2, 4, 6]
```

### reduce

Reduce a list to a single value:

```lumen
reduce([1, 2, 3, 4], fn(acc, x) => acc + x, 0)
# 10
```

### flatten

Flatten nested lists:

```lumen
flatten([[1, 2], [3, 4]])
# [1, 2, 3, 4]
```

### reverse

Reverse a list or string:

```lumen
reverse([1, 2, 3])    # [3, 2, 1]
reverse("hello")      # "olleh"
```

### sort

Sort a list:

```lumen
sort([3, 1, 2])       # [1, 2, 3]
sort(["c", "a", "b"]) # ["a", "b", "c"]
```

### min / max

Find minimum or maximum:

```lumen
min([3, 1, 2])   # 1
max([3, 1, 2])   # 3
```

### sum

Sum a list of numbers:

```lumen
sum([1, 2, 3, 4])   # 10
```

## Strings

### substring

Extract a substring:

```lumen
substring("hello", 1, 4)    # "ell"
substring("hello", 0, 2)    # "he"
```

### split

Split a string:

```lumen
split("a,b,c", ",")    # ["a", "b", "c"]
split("hello", "")     # ["h", "e", "l", "l", "o"]
```

### join

Join strings:

```lumen
join(["a", "b", "c"], "-")   # "a-b-c"
```

### trim

Remove whitespace:

```lumen
trim("  hello  ")    # "hello"
trim_start("  hello")  # "hello"
trim_end("hello  ")    # "hello"
```

### lower / upper

Change case:

```lumen
lower("HELLO")    # "hello"
upper("hello")    # "HELLO"
```

### replace

Replace substrings:

```lumen
replace("hello world", "world", "lumen")   # "hello lumen"
```

### starts_with / ends_with

Check string prefixes/suffixes:

```lumen
starts_with("hello", "he")    # true
ends_with("hello", "lo")      # true
```

### to_string

Convert to string:

```lumen
to_string(42)      # "42"
to_string(3.14)    # "3.14"
to_string(true)    # "true"
```

### parse_int / parse_float

Parse numbers from strings:

```lumen
parse_int("42")      # 42
parse_float("3.14")  # 3.14
```

## Math

### abs

Absolute value:

```lumen
abs(-5)     # 5
abs(3.14)   # 3.14
```

### floor / ceil / round

Rounding:

```lumen
floor(3.7)    # 3
ceil(3.2)     # 4
round(3.5)    # 4
```

### sqrt / pow

Power functions:

```lumen
sqrt(16)      # 4.0
pow(2, 10)    # 1024.0
```

### sin / cos / tan

Trigonometry:

```lumen
sin(0)        # 0.0
cos(0)        # 1.0
```

### random

Generate random number:

```lumen
random()           # Float between 0 and 1
random_int(1, 10)  # Int between 1 and 10
```

## UUID and Time

### uuid / uuid_v4

Generate UUIDs:

```lumen
uuid()      # "550e8400-e29b-41d4-a716-446655440000"
uuid_v4()   # Random UUID v4
```

### timestamp

Get current timestamp:

```lumen
timestamp()         # Unix timestamp in seconds
timestamp_ms()      # Unix timestamp in milliseconds
```

## I/O

### print

Print to stdout:

```lumen
print("Hello, World!")
print("{name} is {age} years old")
```

### format

Format a string:

```lumen
format("Hello, {}!", "World")           # "Hello, World!"
format("{} + {} = {}", 1, 2, 3)         # "1 + 2 = 3"
```

## Result Handling

### ok / err

Create result values:

```lumen
ok(42)              # result[Int, E] with value 42
err("failed")       # result[T, String] with error
```

### is_ok / is_err

Check result status:

```lumen
is_ok(ok(42))       # true
is_ok(err("x"))     # false
is_err(ok(42))      # false
is_err(err("x"))    # true
```

### unwrap / unwrap_or

Extract result values:

```lumen
unwrap(ok(42))              # 42 (panics on err)
unwrap_or(err("x"), 0)      # 0
```

## Async

### spawn

Create a future:

```lumen
let future = spawn(long_running_task())
```

### await

Wait for a future:

```lumen
let result = await future
```

### parallel

Run futures in parallel:

```lumen
await parallel for item in items
  process(item)
end
```

### race

Return first completed:

```lumen
let result = await race
  fetch_from_a()
  fetch_from_b()
end
```

### timeout

Add timeout to async:

```lumen
let result = await timeout(5000, fetch_data())
```

## JSON

### json_parse

Parse JSON string:

```lumen
json_parse('{"name": "Alice"}')   # Json value
```

### json_stringify

Convert to JSON string:

```lumen
json_stringify({"name": "Alice"})   # '{"name":"Alice"}'
```

### json_get

Get value from JSON:

```lumen
let data = json_parse('{"name": "Alice"}')
json_get(data, "name")    # "Alice"
```

## Encoding

### base64_encode / base64_decode

Base64 encoding:

```lumen
base64_encode("hello")     # "aGVsbG8="
base64_decode("aGVsbG8=")  # "hello"
```

### hex_encode / hex_decode

Hex encoding:

```lumen
hex_encode(b"cafe")        # "cafe"
hex_decode("cafe")         # b"cafe"
```

## Hashing

### hash

Compute hash:

```lumen
hash("hello")              # Integer hash
sha256("hello")            # SHA-256 hex string
```

## Complete Builtin List

| Category | Functions |
|----------|-----------|
| Collections | `length`, `push`, `append`, `contains`, `keys`, `values`, `enumerate`, `filter`, `map`, `reduce`, `flatten`, `reverse`, `sort`, `min`, `max`, `sum` |
| Strings | `substring`, `split`, `join`, `trim`, `trim_start`, `trim_end`, `lower`, `upper`, `replace`, `starts_with`, `ends_with`, `to_string`, `parse_int`, `parse_float` |
| Math | `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `sin`, `cos`, `tan`, `random`, `random_int` |
| UUID/Time | `uuid`, `uuid_v4`, `timestamp`, `timestamp_ms` |
| I/O | `print`, `format` |
| Result | `ok`, `err`, `is_ok`, `is_err`, `unwrap`, `unwrap_or` |
| Async | `spawn`, `await`, `parallel`, `race`, `timeout`, `select`, `vote` |
| JSON | `json_parse`, `json_stringify`, `json_get` |
| Encoding | `base64_encode`, `base64_decode`, `hex_encode`, `hex_decode` |
| Hashing | `hash`, `sha256` |


