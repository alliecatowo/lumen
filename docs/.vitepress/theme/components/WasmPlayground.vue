<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch, nextTick } from "vue";
import { withBase } from "vitepress";
import { playgroundSource } from "../playground-state";

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
const consoleVisible = ref(true);

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

watch(playgroundSource, (src) => {
  if (src) {
    sourceCode.value = src;
    setEditorContent(src);
    output.value = "Code loaded from example.";
    outputKind.value = "neutral";
    // Clear it so we don't reload it again if something else triggers watch
    playgroundSource.value = null;
  }
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
  <div class="playground-wrapper">
    <section class="wasm-playground">
      <!-- Sidebar: Files & Examples -->
      <aside class="playground-sidebar">
        <div class="sidebar-section">
          <h3 class="sidebar-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/>
              <polyline points="13 2 13 9 20 9"/>
            </svg>
            Explorer
          </h3>
          <div class="file-list">
            <div class="file-item active">
              <span class="file-icon">M</span>
              main.lm.md
            </div>
          </div>
        </div>

        <div class="sidebar-section">
          <h3 class="sidebar-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
            </svg>
            Examples
          </h3>
          <div class="example-list">
            <button
              v-for="(example, key) in examples"
              :key="key"
              :class="['example-item', { active: selectedKey === key }]"
              @click="selectedKey = key"
            >
              {{ example.label }}
            </button>
          </div>
        </div>
      </aside>

      <!-- Main Content: Editor and Toolbar -->
      <main class="playground-main">
        <header class="playground-toolbar">
          <div class="toolbar-left">
            <div class="tabs">
              <div class="tab active">
                main.lm.md
                <span class="tab-close">×</span>
              </div>
            </div>
          </div>
          <div class="toolbar-right">
            <div class="status-indicator" :class="outputKind">
              <span class="pulse"></span>
              {{ status }}
            </div>
            <div class="actions">
              <button :disabled="busy || !api" @click="runCheck" class="btn btn-ghost" title="Type-check code">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
                  <path d="M9 12l2 2 4-4"/>
                  <circle cx="12" cy="12" r="10"/>
                </svg>
                Check
              </button>
              <button :disabled="busy || !api" @click="runProgram" class="btn btn-primary" title="Execute program">
                <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                  <path d="M8 5v14l11-7z"/>
                </svg>
                Run
              </button>
            </div>
          </div>
        </header>

        <div class="editor-container">
          <div ref="editorContainer" class="code-editor" />
        </div>

        <!-- Terminal Output -->
        <footer class="playground-console" :class="{ collapsed: !consoleVisible }">
          <div class="console-header" @click="consoleVisible = !consoleVisible">
            <div class="console-title">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                <polyline points="4 17 10 11 4 5"/>
                <line x1="12" y1="19" x2="20" y2="19"/>
              </svg>
              Terminal
            </div>
            <div class="console-controls">
              <span :class="['status-badge', outputKind]">{{ outputKind }}</span>
              <button class="btn-icon" @click.stop="output = ''">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                  <path d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                </svg>
              </button>
            </div>
          </div>
          <div class="console-body">
            <pre :class="['output', outputKind]">{{ output }}</pre>
          </div>
        </footer>
      </main>
    </section>
  </div>
</template>

<style scoped>
/* Full-screen experience layout */
.playground-wrapper {
  max-width: 1400px;
  margin: 3rem auto;
  padding: 0 2rem;
}

.wasm-playground {
  display: flex;
  height: 75vh;
  min-height: 600px;
  background: var(--vp-c-bg-soft);
  border: 1px solid rgba(255, 79, 163, 0.2);
  border-radius: 16px;
  overflow: hidden;
  box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(8px);
}

/* Sidebar Styling */
.playground-sidebar {
  width: 240px;
  background: var(--vp-c-bg-alt);
  border-right: 1px solid var(--vp-c-divider);
  display: flex;
  flex-direction: column;
  padding: 1rem 0;
  flex-shrink: 0;
}

.sidebar-section {
  margin-bottom: 1.5rem;
}

.sidebar-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--vp-c-text-3);
  padding: 0 1.25rem;
  margin-bottom: 0.75rem;
  display: flex;
  align-items: center;
  gap: 8px;
  letter-spacing: 0.05em;
}

.file-list, .example-list {
  display: flex;
  flex-direction: column;
}

.file-item, .example-item {
  padding: 0.5rem 1.25rem;
  font-size: 13px;
  text-align: left;
  border: none;
  background: transparent;
  color: var(--vp-c-text-2);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: all 0.2s;
}

.file-item:hover, .example-item:hover {
  background: rgba(255, 79, 163, 0.05);
  color: var(--vp-c-text-1);
}

.file-item.active, .example-item.active {
  background: rgba(255, 79, 163, 0.1);
  color: var(--vp-c-brand-1);
  border-left: 2px solid var(--vp-c-brand-1);
}

.file-icon {
  width: 16px;
  height: 16px;
  background: #FF4FA3;
  color: white;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  font-weight: 800;
}

/* Main Area */
.playground-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--vp-c-bg);
  min-width: 0;
}

.playground-toolbar {
  height: 48px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 1rem;
  background: var(--vp-c-bg-alt);
  border-bottom: 1px solid var(--vp-c-divider);
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 1.5rem;
}

.status-indicator {
  font-size: 12px;
  color: var(--vp-c-text-2);
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-indicator.ok { color: #10B981; }
.status-indicator.error { color: #EF4444; }

.pulse {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 0 rgba(255, 79, 163, 0.4);
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(255, 79, 163, 0.4); }
  70% { transform: scale(1); box-shadow: 0 0 0 6px rgba(255, 79, 163, 0); }
  100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(255, 79, 163, 0); }
}

.actions {
  display: flex;
  gap: 8px;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.btn-primary {
  background: var(--vp-c-brand-1);
  color: white;
  border: none;
  box-shadow: 0 4px 12px rgba(255, 79, 163, 0.3);
}

.btn-primary:hover {
  background: var(--vp-c-brand-2);
  transform: translateY(-1px);
  box-shadow: 0 6px 16px rgba(255, 79, 163, 0.4);
}

.btn-ghost {
  background: transparent;
  color: var(--vp-c-text-2);
  border: 1px solid var(--vp-c-divider);
}

.btn-ghost:hover {
  background: rgba(255, 79, 163, 0.05);
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

.editor-container {
  flex: 1;
  position: relative;
  overflow: hidden;
}

.code-editor {
  height: 100%;
}

.code-editor :deep(.cm-editor) {
  height: 100%;
}

/* Console Styling */
.playground-console {
  height: 200px;
  background: var(--vp-c-bg-alt);
  border-top: 1px solid var(--vp-c-divider);
  display: flex;
  flex-direction: column;
  transition: all 0.3s ease;
}

.playground-console.collapsed {
  height: 38px;
}

.console-header {
  height: 38px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 1rem;
  cursor: pointer;
}

.console-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--vp-c-text-3);
  display: flex;
  align-items: center;
  gap: 8px;
}

.console-body {
  flex: 1;
  background: #000;
  margin: 0 12px 12px;
  border-radius: 8px;
  overflow: auto;
}

.output {
  margin: 0;
  padding: 12px;
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.6;
  color: #e0e0e0;
  white-space: pre-wrap;
}

.output.error { color: #FF6B6B; }
.output.ok { color: #10B981; }

/* Responsive adjustments */
@media (max-width: 900px) {
  .playground-wrapper {
    margin: 1.5rem 0;
    padding: 0 1rem;
  }
  
  .playground-sidebar {
    display: none;
  }
}
</style>
