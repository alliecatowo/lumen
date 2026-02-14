# Browser WASM Guide

Run Lumen directly in the browser via `rust/lumen-wasm`.

## 1) Build the WASM Package

```bash
cd rust/lumen-wasm
wasm-pack build --target web --release
```

Generated artifacts:

- `rust/lumen-wasm/pkg/lumen_wasm.js`
- `rust/lumen-wasm/pkg/lumen_wasm_bg.wasm`

If you want a full UI demo, see `examples/wasm_browser.html` in the repo.

## 2) Minimal Browser Runner (`check` + `compile` + `run`)

```html
<script type="module">
import init, { check, compile, run, version } from "../rust/lumen-wasm/pkg/lumen_wasm.js";

const parse = (result) => JSON.parse(result.to_json());

function execute(source, cell = "main") {
  const checked = parse(check(source));
  if (checked.error) return { stage: "check", ...checked };

  const compiled = parse(compile(source));
  if (compiled.error) return { stage: "compile", ...compiled };

  const executed = parse(run(source, cell));
  if (executed.error) return { stage: "run", ...executed };

  return {
    check: checked.ok,
    lir: JSON.parse(compiled.ok),
    output: executed.ok,
  };
}

await init();
console.log("Lumen WASM version:", version());
</script>
```

## 3) Runnable Lumen Examples

All three examples below are verified with the current compiler/VM using `check`, `emit`, and `run`.

### Example A: Factorial Service Logic

```lumen
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell main() -> Int
  return factorial(6)
end
```

Expected `run(..., "main")` output: `720`

### Example B: Risk Label Classification

```lumen
cell risk_label(score: Int) -> String
  if score < 0
    return "invalid"
  end

  match score
    0 -> return "none"
    1 -> return "low"
    2 -> return "medium"
    _ -> return "high"
  end
end

cell main() -> String
  return risk_label(3)
end
```

Expected output: `high`

### Example C: Latency Aggregation

```lumen
cell average_ms(xs: list[Int]) -> Int
  let total = 0
  for x in xs
    total = total + x
  end
  return total / length(xs)
end

cell main() -> Int
  let latencies = [98, 110, 105, 120]
  return average_ms(latencies)
end
```

Expected output: `108`

## 4) Execute the Examples in Browser JS

```js
const factorialSource = `
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell main() -> Int
  return factorial(6)
end
`;

const riskSource = `
cell risk_label(score: Int) -> String
  if score < 0
    return "invalid"
  end

  match score
    0 -> return "none"
    1 -> return "low"
    2 -> return "medium"
    _ -> return "high"
  end
end

cell main() -> String
  return risk_label(3)
end
`;

const latencySource = `
cell average_ms(xs: list[Int]) -> Int
  let total = 0
  for x in xs
    total = total + x
  end
  return total / length(xs)
end

cell main() -> Int
  let latencies = [98, 110, 105, 120]
  return average_ms(latencies)
end
`;

console.log("factorial", execute(factorialSource));
console.log("risk", execute(riskSource));
console.log("latency", execute(latencySource));
```

## 5) Troubleshooting

- If `init()` fails, verify the `.wasm` file is served by an HTTP server (not `file://`).
- If `compile` succeeds but `run` fails, inspect `stage` and `error` from `execute(...)`.
- For broader WASM planning/context: `docs/WASM_STRATEGY.md`.
