# Lumen Standard Library

This directory contains Lumen's standard library packages.

## Structure

- **std/math.lm.md** — Mathematical constants and functions (floor, ceil, round, sqrt, log, pow, etc.)
- **std/text.lm.md** — String manipulation utilities (pad, truncate, repeat, contains, starts_with, ends_with, etc.)
- **std/collections.lm.md** — List/collection utilities (chunk, zip, flatten, unique, take, drop, etc.)
- **std/json.lm.md** — JSON parsing and manipulation (requires json tool provider at runtime)
- **std/crypto.lm.md** — Cryptographic functions (requires crypto tool provider at runtime)
- **std/http.lm.md** — HTTP client (requires http tool provider at runtime)
- **std/testing.lm.md** — Simple testing framework

## Usage

The standard library is designed to be imported into Lumen programs. Once module/import support is fully implemented, you'll be able to use:

```lumen
import std.math
import std.text  
import std.collections

cell main()
  let pi = math.PI()
  let rounded = math.round(3.7)
  let padded = text.pad_left("hello", 10, " ")
  let chunks = collections.chunk([1, 2, 3, 4, 5], 2)
  print(string(rounded))
end
```

## Implementation Status

- ✅ **math** — Fully implemented, compiles successfully
- ✅ **text** — Fully implemented, compiles successfully  
- ⚠️  **collections** — Implemented but requires type annotations for polymorphic functions
- ⚠️  **json** — Implemented but requires json tool provider and proper grant scoping
- ⚠️  **crypto** — Implemented but requires crypto tool provider
- ⚠️  **http** — Implemented but requires http tool provider and proper grant scoping
- ⚠️  **testing** — Implemented but requires type annotations for polymorphic assertions

## Notes

Some stdlib modules depend on runtime tool providers that must be configured in `lumen.toml`. The effect system and grant mechanisms ensure safe, policy-constrained access to these capabilities.

Functions expecting polymorphic types (currently using untyped parameters) may need explicit type parameters once Lumen adds support for generic functions.
