# Builtins Reference

Lumen provides built-in functions that are always available. They are organized by category below.

## String Operations

| Function | Description |
|----------|-------------|
| `len`, `length` | Return length of a collection or string |
| `upper` | Convert a string to uppercase |
| `lower` | Convert a string to lowercase |
| `trim` | Remove leading and trailing whitespace |
| `split` | Split a string by a separator |
| `join` | Join a list of strings with a separator |
| `replace` | Replace occurrences in a string |
| `starts_with` | True if a string starts with a prefix |
| `ends_with` | True if a string ends with a suffix |
| `index_of` | Find index of substring, or -1 |
| `pad_left` | Pad string on the left to a given width |
| `pad_right` | Pad string on the right to a given width |
| `chars` | Return list of characters in a string |
| `matches` | Test if a string matches a pattern |
| `capitalize` | First character uppercase, rest lowercase |
| `title_case` | Capitalize each word |
| `snake_case` | Convert to snake_case |
| `camel_case` | Convert to camelCase |

## Collection Operations

| Function | Description |
|----------|-------------|
| `append` | Append an element to a list |
| `sort` | Sort a list |
| `reverse` | Reverse a list or string |
| `filter` | Keep elements matching a predicate |
| `map` | Apply a function to each element |
| `reduce` | Fold a list with an accumulator |
| `flat_map` | Map then flatten |
| `zip` | Zip two lists into list of tuples |
| `enumerate` | Get index-value pairs |
| `any` | True if any element matches predicate |
| `all` | True if all elements match predicate |
| `find` | First element matching predicate, or null |
| `contains` | Test if a collection contains a value |
| `unique` | Remove duplicates from a list |
| `flatten` | Flatten nested lists |
| `chunk` | Split list into chunks of given size |
| `window` | Sliding window over list |
| `take` | Take first n elements |
| `drop` | Drop first n elements |
| `first` | First element of list or tuple |
| `last` | Last element of list or tuple |
| `group_by` | Group elements by key from closure |
| `is_empty` | True if collection or string is empty |
| `keys` | Return keys of a map or record |
| `values` | Return values of a map or record |
| `entries` | Key-value pairs (where applicable) |
| `has_key` | Test if map has key |
| `merge` | Merge maps |
| `size` | Alias for length |
| `add` | Add element to set |
| `remove` | Remove element from set |
| `count` | Count elements matching a predicate |
| `position` | Index of first element matching predicate, or -1 |
| `slice` | Extract a sub-range from list or string |

## Conversion

| Function | Description |
|----------|-------------|
| `to_string`, `str` | Convert a value to String |
| `to_int`, `int` | Convert a value to Int |
| `to_float`, `float` | Convert a value to Float |
| `type_of` | Return the runtime type name of a value |
| `parse_json` | Parse a JSON string into a value |
| `to_json` | Serialize a value to a JSON string |
| `to_set` | Convert list to set |

## Math

| Function | Description |
|----------|-------------|
| `abs` | Absolute value of a number |
| `min` | Return the minimum of two values |
| `max` | Return the maximum of two values |
| `round` | Round to nearest integer |
| `ceil` | Round up |
| `floor` | Round down |
| `sqrt` | Square root |
| `pow` | Power (base, exponent) |
| `log` | Natural logarithm |
| `sin`, `cos` | Trigonometry |
| `clamp` | Clamp value between min and max |

## I/O

| Function | Description |
|----------|-------------|
| `print` | Print a value to stdout |
| `read_file` | Read a file's contents as a string |
| `write_file` | Write a string to a file |
| `get_env` | Read an environment variable |
| `timestamp` | Current Unix timestamp (Float) |
| `random` | Generate a random float in [0, 1) |

## Utility

| Function | Description |
|----------|-------------|
| `range` | Generate a list of integers in a range |
| `hash`, `sha256` | Compute hash of a value |
| `debug` | Print debug representation to stderr |
| `sizeof` | Size of value in bytes (internal) |
| `clone` | Deep clone a value |
| `count` | Count elements matching predicate |
| `matches` | Test if string matches pattern |
| `position` | Index of first matching element |
| `slice` | Extract sub-range |

## Result Handling

| Function | Description |
|----------|-------------|
| `ok` | Create ok result value |
| `err` | Create err result value |
| `is_ok` | Check if result is ok |
| `is_err` | Check if result is err |
| `unwrap` | Extract ok value (panics on err) |
| `unwrap_or` | Extract ok value or default |

## Async & Orchestration

| Function | Description |
|----------|-------------|
| `spawn` | Create a future |
| `parallel` | Run futures in parallel, collect results |
| `race` | Return first completed future |
| `vote` | Return majority result from futures |
| `select` | Return first non-null from futures |
| `timeout` | Add timeout to async operation |

## Encoding

| Function | Description |
|----------|-------------|
| `base64_encode` | Base64 encode |
| `base64_decode` | Base64 decode |
| `hex_encode` | Hex encode |
| `hex_decode` | Hex decode |
| `url_encode` | URL encode |
| `url_decode` | URL decode |
