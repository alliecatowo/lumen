<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { withBase } from "vitepress";

type LumenResult = {
  to_json: () => string;
};

type WasmApi = {
  check: (source: string) => LumenResult;
  compile: (source: string) => LumenResult;
  run: (source: string, cellName?: string) => LumenResult;
  version: () => string;
};

type Example = {
  label: string;
  cell: string;
  expected: string;
  source: string;
};

const examples: Record<string, Example> = {
  factorial: {
    label: "Factorial",
    cell: "main",
    expected: "720",
    source: `cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell main() -> Int
  return factorial(6)
end`,
  },
  risk: {
    label: "Risk Classifier",
    cell: "main",
    expected: "high",
    source: `cell risk_label(score: Int) -> String
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
end`,
  },
  latency: {
    label: "Latency Average",
    cell: "main",
    expected: "108",
    source: `cell average_ms(xs: list[Int]) -> Int
  let total = 0
  for x in xs
    total += x
  end
  return total / length(xs)
end

cell main() -> Int
  let latencies = [98, 110, 105, 120]
  return average_ms(latencies)
end`,
  },
};

const selectedKey = ref<keyof typeof examples>("factorial");
const sourceCode = ref(examples[selectedKey.value].source);
const api = ref<WasmApi | null>(null);
const status = ref("Loading WebAssembly runtime...");
const busy = ref(false);
const output = ref("Press Check / Compile / Run after the runtime is ready.");
const outputKind = ref<"neutral" | "ok" | "error">("neutral");

const selected = computed(() => examples[selectedKey.value]);

watch(selectedKey, (key) => {
  sourceCode.value = examples[key].source;
  output.value = `Loaded "${examples[key].label}". Expected output: ${examples[key].expected}`;
  outputKind.value = "neutral";
});

function parseResult(result: LumenResult): { ok?: string; error?: string } {
  try {
    return JSON.parse(result.to_json());
  } catch {
    return { error: "Failed to parse WASM response JSON." };
  }
}

function toCompilerSource(source: string): string {
  const trimmed = source.trim();
  if (trimmed.includes("```lumen")) {
    return source;
  }
  return `\`\`\`lumen\n${source}\n\`\`\``;
}

async function initWasm() {
  try {
    const moduleUrl = withBase("/wasm/lumen_wasm.js");
    const wasmModule = await import(/* @vite-ignore */ moduleUrl);
    await wasmModule.default();

    api.value = {
      check: wasmModule.check,
      compile: wasmModule.compile,
      run: wasmModule.run,
      version: wasmModule.version,
    };
    status.value = `Runtime ready (lumen-wasm v${api.value.version()})`;
  } catch (error) {
    status.value =
      "WASM runtime not available. Ensure the docs deployment built rust/lumen-wasm.";
    outputKind.value = "error";
    output.value = String(error);
  }
}

async function runCheck() {
  if (!api.value) return;
  busy.value = true;
  try {
    const parsed = parseResult(api.value.check(toCompilerSource(sourceCode.value)));
    if (parsed.error) {
      outputKind.value = "error";
      output.value = parsed.error;
      return;
    }
    outputKind.value = "ok";
    output.value = `Type-check OK\n\n${parsed.ok ?? ""}`;
  } finally {
    busy.value = false;
  }
}

async function runCompile() {
  if (!api.value) return;
  busy.value = true;
  try {
    const parsed = parseResult(api.value.compile(toCompilerSource(sourceCode.value)));
    if (parsed.error) {
      outputKind.value = "error";
      output.value = parsed.error;
      return;
    }

    try {
      const lir = JSON.parse(parsed.ok ?? "{}");
      outputKind.value = "ok";
      output.value = `Compile OK\nFunctions: ${lir.functions?.length ?? 0}\nConstants: ${lir.constants?.length ?? 0}\n\n${JSON.stringify(lir, null, 2)}`;
    } catch {
      outputKind.value = "ok";
      output.value = parsed.ok ?? "Compile OK";
    }
  } finally {
    busy.value = false;
  }
}

async function runProgram() {
  if (!api.value) return;
  busy.value = true;
  try {
    const parsed = parseResult(
      api.value.run(toCompilerSource(sourceCode.value), selected.value.cell),
    );
    if (parsed.error) {
      outputKind.value = "error";
      output.value = parsed.error;
      return;
    }
    outputKind.value = "ok";
    output.value = `Run OK\nResult: ${parsed.ok}\nExpected: ${selected.value.expected}`;
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  void initWasm();
});
</script>

<template>
  <section class="wasm-playground">
    <header class="playground-header">
      <h2>Interactive Browser Runner</h2>
      <p>{{ status }}</p>
    </header>

    <div class="controls">
      <label for="example">Example</label>
      <select id="example" v-model="selectedKey">
        <option v-for="(example, key) in examples" :key="key" :value="key">
          {{ example.label }}
        </option>
      </select>
    </div>

    <div class="playground-grid">
      <div>
        <textarea v-model="sourceCode" spellcheck="false" />
        <div class="actions">
          <button :disabled="busy || !api" @click="runCheck">Check</button>
          <button :disabled="busy || !api" @click="runCompile">Compile</button>
          <button :disabled="busy || !api" @click="runProgram">Run</button>
        </div>
      </div>
      <pre :class="['output', outputKind]">{{ output }}</pre>
    </div>
  </section>
</template>

<style scoped>
.wasm-playground {
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  padding: 16px;
  margin: 20px 0;
  background: color-mix(in srgb, var(--vp-c-bg-soft) 80%, white 20%);
}

.playground-header h2 {
  margin: 0 0 6px;
}

.playground-header p {
  margin: 0 0 14px;
  font-size: 13px;
  color: var(--vp-c-text-2);
}

.controls {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.controls select {
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  padding: 6px 10px;
  min-width: 220px;
  background: var(--vp-c-bg);
}

.playground-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}

textarea {
  width: 100%;
  min-height: 300px;
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  line-height: 1.45;
  border: 1px solid var(--vp-c-divider);
  border-radius: 10px;
  padding: 10px;
  background: var(--vp-c-bg);
}

.actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}

button {
  border: 1px solid var(--vp-c-brand-2);
  border-radius: 8px;
  padding: 6px 12px;
  background: var(--vp-c-brand-1);
  color: white;
  font-weight: 600;
  cursor: pointer;
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.output {
  border: 1px solid var(--vp-c-divider);
  border-radius: 10px;
  min-height: 348px;
  margin: 0;
  padding: 10px;
  overflow: auto;
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  line-height: 1.45;
  white-space: pre-wrap;
  background: var(--vp-c-bg);
}

.output.ok {
  border-color: rgba(16, 185, 129, 0.45);
}

.output.error {
  border-color: rgba(239, 68, 68, 0.45);
}

@media (max-width: 900px) {
  .playground-grid {
    grid-template-columns: 1fr;
  }
}
</style>
