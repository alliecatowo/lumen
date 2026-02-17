# Differential Test Harness

Framework to compile the same Lumen source with both the Rust compiler
and the self-hosted Lumen compiler, then compare the LIR output for
byte-level equality.

## Test Runner

```lumen
import self_host.main: compile, CompileOptions
import self_host.serialize: LirModule, write_module

record DiffTestCase(
  name: String,
  source_path: String,
  source: String
)

record DiffResult(
  name: String,
  passed: Bool,
  rust_cells: Int,
  lumen_cells: Int,
  rust_types: Int,
  lumen_types: Int,
  mismatches: list[String]
)

cell run_diff_test(test_case: DiffTestCase) -> DiffResult
  let opts = CompileOptions(
    filename: test_case.source_path,
    source: test_case.source,
    emit_lir: false,
    trace: false
  )

  # Compile with self-hosted compiler
  let lumen_result = compile(opts)

  # Compile with Rust compiler (via CLI subprocess)
  let rust_lir_json = run_rust_compiler(test_case.source_path)

  let mismatches = []

  match lumen_result
    case Err(err) ->
      mismatches = append(mismatches, "lumen compiler failed: {err}")
      return DiffResult(
        name: test_case.name,
        passed: false,
        rust_cells: 0,
        lumen_cells: 0,
        rust_types: 0,
        lumen_types: 0,
        mismatches: mismatches
      )
    case Ok(lumen_module) ->
      match rust_lir_json
        case Err(err) ->
          mismatches = append(mismatches, "rust compiler failed: {err}")
          return DiffResult(
            name: test_case.name,
            passed: false,
            rust_cells: 0,
            lumen_cells: length(lumen_module.cells),
            rust_types: 0,
            lumen_types: length(lumen_module.types),
            mismatches: mismatches
          )
        case Ok(rust_json) ->
          let rust_module = parse_rust_lir(rust_json)
          mismatches = compare_modules(lumen_module, rust_module)
          DiffResult(
            name: test_case.name,
            passed: length(mismatches) == 0,
            rust_cells: length(rust_module.cells),
            lumen_cells: length(lumen_module.cells),
            rust_types: length(rust_module.types),
            lumen_types: length(lumen_module.types),
            mismatches: mismatches
          )
      end
  end
end
```

## Module Comparison

Deep comparison of two LIR modules, reporting all differences.

```lumen
cell compare_modules(a: LirModule, b: LirModule) -> list[String]
  let diffs = []

  # Compare string tables
  if length(a.strings) != length(b.strings) then
    diffs = append(diffs, "string table size: lumen={length(a.strings)} rust={length(b.strings)}")
  else
    let i = 0
    while i < length(a.strings)
      if a.strings[i] != b.strings[i] then
        diffs = append(diffs, "string[{i}]: lumen='{a.strings[i]}' rust='{b.strings[i]}'")
      end
      i = i + 1
    end
  end

  # Compare type counts
  if length(a.types) != length(b.types) then
    diffs = append(diffs, "type count: lumen={length(a.types)} rust={length(b.types)}")
  end

  # Compare cell counts
  if length(a.cells) != length(b.cells) then
    diffs = append(diffs, "cell count: lumen={length(a.cells)} rust={length(b.cells)}")
  else
    let i = 0
    while i < length(a.cells)
      let ac = a.cells[i]
      let bc = b.cells[i]
      if ac.name != bc.name then
        diffs = append(diffs, "cell[{i}].name: lumen='{ac.name}' rust='{bc.name}'")
      end
      if length(ac.instructions) != length(bc.instructions) then
        diffs = append(diffs, "cell '{ac.name}' instruction count: lumen={length(ac.instructions)} rust={length(bc.instructions)}")
      else
        let j = 0
        while j < length(ac.instructions)
          if ac.instructions[j].encoded != bc.instructions[j].encoded then
            diffs = append(diffs, "cell '{ac.name}' instr[{j}]: lumen=0x{ac.instructions[j].encoded} rust=0x{bc.instructions[j].encoded}")
          end
          j = j + 1
        end
      end
      if length(ac.constants) != length(bc.constants) then
        diffs = append(diffs, "cell '{ac.name}' constant count: lumen={length(ac.constants)} rust={length(bc.constants)}")
      end
      i = i + 1
    end
  end

  diffs
end
```

## Rust Compiler Subprocess

Invoke the Rust compiler via CLI and capture LIR JSON output.

```lumen
cell run_rust_compiler(source_path: String) -> result[String, String]
  # Shell out to `lumen emit` to get LIR JSON from the Rust compiler.
  # This is a placeholder — actual implementation needs process spawning.
  Err("rust compiler subprocess not yet implemented")
end

cell parse_rust_lir(json: String) -> LirModule
  # Parse LIR JSON output from the Rust compiler into our LirModule type.
  # Placeholder — needs JSON parsing implementation.
  LirModule(
    version: "1.0.0",
    doc_hash: "",
    strings: [],
    types: [],
    cells: [],
    tools: [],
    effects: []
  )
end
```

## Batch Test Runner

Run all tests in a corpus directory and report results.

```lumen
cell run_corpus(dir: String) -> list[DiffResult]
  let results = []
  let files = read_dir(dir)
  for file in files
    if ends_with(file, ".lm") then
      let source = match read_file("{dir}/{file}")
        case Ok(s) -> s
        case Err(_) ->
          results = append(results, DiffResult(
            name: file,
            passed: false,
            rust_cells: 0,
            lumen_cells: 0,
            rust_types: 0,
            lumen_types: 0,
            mismatches: ["could not read file"]
          ))
          continue
      end
      let test_case = DiffTestCase(
        name: file,
        source_path: "{dir}/{file}",
        source: source
      )
      let result = run_diff_test(test_case)
      results = append(results, result)
    end
  end
  results
end

cell print_results(results: list[DiffResult]) -> Int
  let passed = 0
  let failed = 0
  for r in results
    if r.passed then
      print("  PASS  {r.name}")
      passed = passed + 1
    else
      print("  FAIL  {r.name}")
      for m in r.mismatches
        print("        {m}")
      end
      failed = failed + 1
    end
  end
  print("")
  print("{passed} passed, {failed} failed, {passed + failed} total")
  if failed > 0 then 1 else 0 end
end

cell main() -> Int
  let dirs = [
    "self-host/tests/corpus/trivial",
    "self-host/tests/corpus/expressions",
    "self-host/tests/corpus/statements",
    "self-host/tests/corpus/patterns",
    "self-host/tests/corpus/items",
    "self-host/tests/corpus/complex"
  ]
  let exit_code = 0
  for dir in dirs
    print("--- {dir} ---")
    let results = run_corpus(dir)
    let code = print_results(results)
    if code != 0 then
      exit_code = 1
    end
  end
  exit_code
end
```
