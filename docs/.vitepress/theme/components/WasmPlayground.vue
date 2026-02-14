<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch, nextTick } from "vue";
import { withBase } from "vitepress";

// CodeMirror imports
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightSpecialChars } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { syntaxHighlighting, indentOnInput, bracketMatching } from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { lumenLanguage, lumenHighlightStyle } from "../codemirror-lumen";

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
  source: string;
};

const examples: Record<string, Example> = {
  hello: {
    label: "Hello World",
    cell: "main",
    source: `cell main() -> String
  return "Hello, World!"
end`,
  },
  factorial: {
    label: "Factorial",
    cell: "main",
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
  fibonacci: {
    label: "Fibonacci",
    cell: "main",
    source: `cell fib(n: Int) -> Int
  if n <= 1
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

cell main() -> Int
  return fib(15)
end`,
  },
  pattern: {
    label: "Pattern Matching",
    cell: "main",
    source: `cell classify(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    2 -> return "two"
    _ -> return "many"
  end
end

cell main() -> String
  let results = [classify(0), classify(1), classify(5)]
  return join(results, ", ")
end`,
  },
  records: {
    label: "Records",
    cell: "main",
    source: `record Point
  x: Float
  y: Float
end

cell distance(p1: Point, p2: Point) -> Float
  let dx = p2.x - p1.x
  let dy = p2.y - p1.y
  return (dx * dx + dy * dy) ** 0.5
end

cell main() -> Float
  let a = Point(x: 0.0, y: 0.0)
  let b = Point(x: 3.0, y: 4.0)
  return distance(a, b)
end`,
  },
  error: {
    label: "Error Handling",
    cell: "main",
    source: `cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end

cell main() -> String
  let results = []
  for i in 0..=3
    match divide(10, i)
      ok(v) -> results = push(results, "10/{i} = {v}")
      err(e) -> results = push(results, "10/{i}: {e}")
    end
  end
  return join(results, "\\n")
end`,
  },
};

const selectedKey = ref<keyof typeof examples>("hello");
const sourceCode = ref(examples[selectedKey.value].source);
const api = ref<WasmApi | null>(null);
const status = ref("Loading WebAssembly runtime...");
const busy = ref(false);
const output = ref("Click Run to execute the code.");
const outputKind = ref<"neutral" | "ok" | "error">("neutral");

// CodeMirror refs
const editorContainer = ref<HTMLDivElement | null>(null);
let editorView: EditorView | null = null;
const languageCompartment = new Compartment();

const selected = computed(() => examples[selectedKey.value]);

// Dark theme matching the pink/dark design
const lumenEditorTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--vp-c-bg)",
    color: "#e0e0e0",
    fontSize: "13px",
    height: "100%",
  },
  ".cm-content": {
    fontFamily: "var(--vp-font-family-mono)",
    lineHeight: "1.6",
    padding: "12px 0",
    caretColor: "#FF4FA3",
  },
  "&.cm-focused .cm-cursor": {
    borderLeftColor: "#FF4FA3",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "rgba(255, 79, 163, 0.2) !important",
  },
  ".cm-activeLine": {
    backgroundColor: "rgba(255, 79, 163, 0.05)",
  },
  ".cm-gutters": {
    backgroundColor: "var(--vp-c-bg-alt)",
    color: "var(--vp-c-text-3)",
    border: "none",
    borderRight: "1px solid var(--vp-c-divider)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "rgba(255, 79, 163, 0.08)",
    color: "#FF4FA3",
  },
  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 8px 0 12px",
    minWidth: "32px",
    fontSize: "12px",
  },
  ".cm-matchingBracket": {
    backgroundColor: "rgba(255, 79, 163, 0.25)",
    outline: "1px solid rgba(255, 79, 163, 0.4)",
  },
  ".cm-searchMatch": {
    backgroundColor: "rgba(255, 79, 163, 0.2)",
    outline: "1px solid rgba(255, 79, 163, 0.4)",
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "rgba(255, 79, 163, 0.35)",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-scroller": {
    overflow: "auto",
  },
});

function createEditorState(doc: string): EditorState {
  return EditorState.create({
    doc,
    extensions: [
      lineNumbers(),
      highlightActiveLine(),
      highlightSpecialChars(),
      history(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      highlightSelectionMatches(),
      languageCompartment.of(lumenLanguage),
      syntaxHighlighting(lumenHighlightStyle),
      lumenEditorTheme,
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          sourceCode.value = update.state.doc.toString();
        }
      }),
    ],
  });
}

function setEditorContent(content: string) {
  if (!editorView) return;
  editorView.dispatch({
    changes: {
      from: 0,
      to: editorView.state.doc.length,
      insert: content,
    },
  });
}

watch(selectedKey, (key) => {
  const src = examples[key].source;
  sourceCode.value = src;
  setEditorContent(src);
  output.value = `Ready to run "${examples[key].label}"`;
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
    status.value = `Ready (lumen-wasm v${api.value.version()})`;
  } catch (error) {
    status.value = "WASM not available. Build lumen-wasm first: `cd rust/lumen-wasm && wasm-pack build --target web`";
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
    output.value = `Type-check OK`;
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
    output.value = parsed.ok ?? "OK";
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  void initWasm();

  await nextTick();
  if (editorContainer.value) {
    editorView = new EditorView({
      state: createEditorState(sourceCode.value),
      parent: editorContainer.value,
    });
  }
});

onUnmounted(() => {
  if (editorView) {
    editorView.destroy();
    editorView = null;
  }
});
</script>

<template>
  <section class="wasm-playground">
    <header class="playground-header">
      <div class="status-row">
        <span class="status">{{ status }}</span>
        <div class="controls">
          <label for="example">Example:</label>
          <select id="example" v-model="selectedKey">
            <option v-for="(example, key) in examples" :key="key" :value="key">
              {{ example.label }}
            </option>
          </select>
        </div>
      </div>
    </header>

    <div class="playground-grid">
      <div class="editor-pane">
        <div class="editor-header">
          <span class="filename">main.lm.md</span>
          <div class="actions">
            <button :disabled="busy || !api" @click="runCheck" class="btn-secondary">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                <path d="M9 12l2 2 4-4"/>
                <circle cx="12" cy="12" r="10"/>
              </svg>
              Check
            </button>
            <button :disabled="busy || !api" @click="runProgram" class="btn-primary">
              <svg viewBox="0 0 24 24" fill="currentColor" width="14" height="14">
                <path d="M8 5v14l11-7z"/>
              </svg>
              Run
            </button>
          </div>
        </div>
        <div ref="editorContainer" class="code-editor" />
      </div>
      <div class="output-pane">
        <div class="output-header">
          <span>Output</span>
          <span :class="['status-badge', outputKind]">{{ outputKind }}</span>
        </div>
        <pre :class="['output', outputKind]">{{ output }}</pre>
      </div>
    </div>
  </section>
</template>

<style scoped>
.wasm-playground {
  border: 1px solid rgba(255, 79, 163, 0.25);
  border-radius: 12px;
  overflow: hidden;
  margin: 20px 0;
  background: var(--vp-c-bg-soft);
}

.playground-header {
  background: linear-gradient(135deg, rgba(255, 79, 163, 0.1), rgba(255, 141, 196, 0.05));
  border-bottom: 1px solid rgba(255, 79, 163, 0.15);
  padding: 12px 16px;
}

.status-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}

.status {
  font-size: 13px;
  color: var(--vp-c-text-2);
}

.controls {
  display: flex;
  align-items: center;
  gap: 8px;
}

.controls label {
  font-size: 13px;
  color: var(--vp-c-text-2);
}

.controls select {
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  padding: 6px 10px;
  min-width: 160px;
  background: var(--vp-c-bg);
  font-size: 13px;
}

.playground-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0;
}

.editor-pane,
.output-pane {
  display: flex;
  flex-direction: column;
}

.editor-header,
.output-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: var(--vp-c-bg-alt);
  border-bottom: 1px solid var(--vp-c-divider);
  font-size: 12px;
  color: var(--vp-c-text-2);
}

.output-header {
  border-left: 1px solid var(--vp-c-divider);
}

.filename {
  font-weight: 600;
  color: var(--vp-c-brand-1);
}

.status-badge {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
}

.status-badge.neutral {
  background: var(--vp-c-divider);
  color: var(--vp-c-text-2);
}

.status-badge.ok {
  background: rgba(16, 185, 129, 0.2);
  color: #10B981;
}

.status-badge.error {
  background: rgba(239, 68, 68, 0.2);
  color: #EF4444;
}

.actions {
  display: flex;
  gap: 8px;
}

.btn-primary,
.btn-secondary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-primary {
  background: var(--vp-c-brand-1);
  color: white;
  border: none;
}

.btn-primary:hover:not(:disabled) {
  background: var(--vp-c-brand-2);
}

.btn-secondary {
  background: transparent;
  border: 1px solid var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

.btn-secondary:hover:not(:disabled) {
  background: rgba(255, 79, 163, 0.1);
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.code-editor {
  min-height: 350px;
  background: var(--vp-c-bg);
}

.code-editor :deep(.cm-editor) {
  height: 100%;
  min-height: 350px;
}

.output {
  flex: 1;
  min-height: 350px;
  margin: 0;
  padding: 12px;
  border: none;
  border-left: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow: auto;
}

.output.ok {
  background: rgba(16, 185, 129, 0.04);
  border-left-color: rgba(16, 185, 129, 0.3);
}

.output.error {
  background: rgba(239, 68, 68, 0.04);
  border-left-color: rgba(239, 68, 68, 0.3);
  color: #EF4444;
}

@media (max-width: 768px) {
  .playground-grid {
    grid-template-columns: 1fr;
  }

  .output-header {
    border-left: none;
    border-top: 1px solid var(--vp-c-divider);
  }

  .output {
    border-left: none;
    min-height: 200px;
  }

  .status-row {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
