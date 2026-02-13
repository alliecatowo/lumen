# Lumen Showcase - Task Management System

A comprehensive demonstration of all Lumen language features through a practical task management system.

## Overview

This project exercises every major feature of the Lumen programming language in a realistic, non-trivial application. It serves as both a language showcase and a test suite for real-world usage patterns.

## Features Demonstrated

### Type System
- **Type Aliases**: `TaskId = string` for semantic type names
- **Records**: `Task` and `TaskStats` with typed fields
- **Field Constraints**: `created_at: int where created_at >= 0`
- **Enums**: `Priority` and `TaskStatus` with multiple variants
- **Result Types**: `result[int, string]` for error handling

### Pattern Matching
- **Exhaustive Enum Matching**: All `Priority` and `TaskStatus` variants
- **If-Let Patterns**: `if let Ok(timestamp) = task.completed_at`
- **Match Expressions**: Nested matches with error handling
- **Variant Patterns**: `Priority::High`, `TaskStatus::Pending`

### Functions & Control Flow
- **Closures/Lambdas**: `fn(t: Task) -> bool => ...` as filter predicates
- **While Loops**: Iteration with mutation and early exit
- **Return Statements**: Early returns in search functions
- **Boolean Logic**: Complex conditionals with `and`

### Data Structures & Operations
- **List Operations**:
  - Manual `filter` implementation with closures
  - `map` pattern (extracting titles)
  - `reduce` pattern (counting, aggregation)
  - Bubble sort implementation with nested loops
- **String Operations**:
  - Concatenation with `++`
  - Conversion with `string(...)`
  - Multi-line string building

### Cell Architecture
- **Multiple Cells**: 15+ cells with clear separation of concerns
- **Cell Composition**: Cells calling other cells (`analyze_tasks` calls `compute_stats`, `filter_by_priority`)
- **Helper Functions**: Reusable utilities (`priority_to_string`, `priority_score`)
- **Test Cells**: Unit tests for core functionality

## Files

- `main.lm.md` - Complete task management system (346 lines)
- `minimal.lm.md` - Simplified version for quick testing
- `README.md` - This file
- `lumen.toml` - Package configuration

## Usage

### Type Check
```bash
lumen check examples/showcase/main.lm.md
```

### Run
```bash
lumen run examples/showcase/main.lm.md
```

Expected output: Formatted task analysis with statistics, filtering, sorting, and test results.

## Known Issues & Discoveries

### Runtime Issues (Discovered During Testing)
1. **Undefined Cell Error**: Despite successful compilation, the VM reports "undefined cell: main" at runtime. This suggests a bug in the cell lookup or LIR emission.
2. **Integer Display**: Integer return values display as "null" instead of the actual value.
3. **String Returns**: String return values from main cell don't print properly.

### Workarounds Applied
- Removed `for ... in ... do` syntax in favor of while loops (for loops don't support `do` keyword)
- Removed complex `len()` constraints (only simple comparisons supported in `where` clauses)
- Avoided `if let` assignments to variables due to scoping issues
- Used incremental string building instead of interpolation for complex concatenation

### Language Gaps Found
- No `timestamp()` or time functions available
- No native `sort`, `filter`, `map` intrinsics (implemented manually)
- Process runtimes (memory, machine) not testable without proper grants in scope
- Effect system difficult to test without real tool providers

## What This Proves

This showcase demonstrates that Lumen's core language features are solid and composable. A real-world application can be built using:
- Rich type system (records, enums, type aliases, constraints)
- Functional patterns (closures, higher-order functions)
- Imperative constructs (while loops, mutation, early returns)
- Pattern matching (exhaustive, if-let, match expressions)
- Error handling (result types, explicit error propagation)

The fact that 346 lines of non-trivial code type-check successfully indicates the compiler pipeline is robust. The runtime issues discovered are valuable feedback for VM development.

## Future Enhancements

When multi-file imports work:
- Split into `models.lm.md` (types), `utils.lm.md` (helpers), `main.lm.md` (orchestration)
- Add process runtimes with real storage effects
- Integrate tool providers for persistence
- Add more comprehensive test suite
