# Lumen Web REPL Design

Research and design document for building a world-class web-based REPL and playground for Lumen.

## Current State

The Lumen playground (`docs/.vitepress/theme/components/WasmPlayground.vue`) runs the full compiler and VM in the browser via WebAssembly. It supports:

- Single-file editing with CodeMirror 6 and Lumen syntax highlighting
- Example dropdown with pre-loaded programs
- Run and type-check via `lumen-wasm` bindings
- Output display with success/error states

## Reference Platforms

### Replit

- Full IDE experience: file tree, terminal, package management, deployment
- WebSocket-based connection to a Linux container per session
- Collaborative editing via OT/CRDT
- Heavy infrastructure cost per user (dedicated VM)
- **Applicable to Lumen**: Multi-file workspace model, but container approach is overkill for a compile-to-WASM language

### CodeSandbox / StackBlitz

- StackBlitz uses WebContainers: a full Node.js runtime in the browser using Service Workers
- Near-instant boot, no server, fully client-side
- CodeSandbox uses micro-VMs (Firecracker) for heavier workloads
- **Applicable to Lumen**: The Lumen VM already runs in WASM. No need for WebContainers or server VMs. The entire compile-run cycle happens client-side.

### CodePen / JSFiddle

- Lightweight: HTML/CSS/JS panels, instant preview
- No file system, single-file oriented
- Share via URL (encoded or backend-stored)
- **Applicable to Lumen**: Good UX model for quick experimentation. The current playground already follows this pattern.

### Jupyter Notebooks

- Cell-based execution: each cell is independently runnable
- Markdown cells interspersed with code cells
- Rich output (plots, tables, HTML)
- **Applicable to Lumen**: Lumen's markdown-native source format (`.lm.md`) is a natural fit for notebook-style editing. Each fenced code block could be an executable cell.

### Rust Playground

- Single-file, compiles via server-side `rustc`
- Dropdown for edition, optimization level, output mode (AST, MIR, LLVM IR, assembly)
- Share via GitHub Gist
- **Applicable to Lumen**: Lumen can do better -- full client-side compilation eliminates the need for a server.

## Proposed Architecture

### Phase 1: Enhanced Single-File Playground (Current + Improvements)

Already implemented:
- CodeMirror 6 with Lumen language mode
- Dark theme matching the pink brand
- Line numbers, bracket matching, search

Improvements to add:
- **Keyboard shortcuts**: Ctrl/Cmd+Enter to run, Ctrl/Cmd+Shift+Enter to check
- **Auto-save to localStorage**: Persist user code across page loads
- **URL sharing**: Encode source in URL fragment (`#code=base64(deflate(source))`) for gist-free sharing
- **LIR inspector**: Toggle to view compiled bytecode (already available via `compile()` API)
- **Multiple output modes**: Run, Check, Emit LIR, AST dump

### Phase 2: Multi-File Support

Key challenge: The Lumen module system uses `import module.path: Symbol` with file-based resolution.

**Virtual filesystem approach**:
```typescript
interface VirtualFS {
  files: Map<string, string>;        // path -> content
  activeFile: string;                 // currently editing
}
```

- Tab bar showing open files (like VS Code)
- Add/rename/delete files in the virtual FS
- Extend `lumen-wasm` to accept a file map: `run(mainSource, imports: Record<string, string>)`
- The WASM `compile_with_imports` function already supports multi-file compilation

**UI layout**:
```
+------------------+-------------------+
| File Explorer    | Editor (tabbed)   |
| - main.lm.md    |                   |
| - models.lm.md  |                   |
| - utils.lm.md   |                   |
| [+ New File]     |                   |
+------------------+-------------------+
| Output / Console                     |
+--------------------------------------+
```

### Phase 3: Notebook Mode

Leverage Lumen's markdown-native format for a Jupyter-like experience:

- Parse `.lm.md` into alternating markdown and code cells
- Render markdown cells as formatted text
- Code cells get CodeMirror editors with run buttons
- Each cell executes in sequence, building up scope
- Output appears inline below each cell

This is uniquely suited to Lumen because `.lm.md` files ARE notebooks. The format already interleaves prose and code.

### Phase 4: Persistent Projects and Sharing

**Client-side storage**:
- IndexedDB for project persistence (larger than localStorage)
- Each project = a named collection of virtual files
- Project list in sidebar

**URL sharing**:
- Short codes: For small programs, encode in URL fragment using `base64(deflate(source))`
- For multi-file projects, use a backend service or GitHub Gists:
  - `POST /api/share` -> returns short ID
  - `GET /playground?share=abc123` -> loads shared project
  - Alternatively, export/import as `.zip` bundles

**Embed mode**:
- `<iframe>` embeddable playground for blog posts and documentation
- URL parameter `?embed=true` hides chrome, shows only editor + output
- Configurable: `?theme=dark&readonly=true&autorun=true`

## Technical Considerations

### WASM API Extensions Needed

The current `lumen-wasm` API exposes `check`, `compile`, `run`, and `version`. For the full REPL experience, additional bindings would be needed:

1. `compile_with_imports(main: string, imports: Record<string, string>)` -- multi-file support
2. `format(source: string)` -- code formatting
3. `hover_info(source: string, line: number, col: number)` -- type info at cursor
4. `completions(source: string, line: number, col: number)` -- autocomplete suggestions
5. `diagnostics(source: string)` -- inline error/warning markers

### Performance

- Compilation is fast (< 50ms for typical programs) since the entire pipeline runs in WASM
- For larger programs, use a Web Worker to avoid blocking the UI thread
- Debounce type-checking on keystroke (300ms delay)

### Code Compression for URL Sharing

Using `pako` (zlib for JavaScript):
```typescript
import pako from 'pako';

function encodeSource(source: string): string {
  const compressed = pako.deflate(new TextEncoder().encode(source));
  return btoa(String.fromCharCode(...compressed));
}

function decodeSource(encoded: string): string {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return new TextDecoder().decode(pako.inflate(bytes));
}
```

This keeps URLs under ~2000 characters for programs up to ~1KB of source.

### Inline Diagnostics with CodeMirror

CodeMirror 6 supports lint extensions that can show inline errors:

```typescript
import { linter, Diagnostic } from "@codemirror/lint";

const lumenLinter = linter((view) => {
  const diagnostics: Diagnostic[] = [];
  const result = api.check(view.state.doc.toString());
  if (result.error) {
    diagnostics.push({
      from: result.offset ?? 0,
      to: result.offset ?? 0,
      severity: "error",
      message: result.error,
    });
  }
  return diagnostics;
});
```

### Monaco vs. CodeMirror

CodeMirror 6 was chosen over Monaco for several reasons:
- Smaller bundle (~150KB vs ~2MB for Monaco)
- Better mobile support
- Simpler API for custom language modes
- VitePress SSR compatibility (Monaco requires complex async loading)
- CodeMirror's streaming tokenizer is sufficient for syntax highlighting; a full parser (Lezer grammar) can be added later for richer features

## Implementation Priority

| Priority | Feature | Effort | Impact |
|----------|---------|--------|--------|
| Done | CodeMirror 6 integration | -- | High |
| P1 | Keyboard shortcuts (Ctrl+Enter) | Small | Medium |
| P1 | localStorage auto-save | Small | Medium |
| P1 | URL fragment sharing | Medium | High |
| P2 | LIR bytecode inspector | Small | Medium |
| P2 | Inline error diagnostics | Medium | High |
| P2 | Multi-file tabs + virtual FS | Large | High |
| P3 | Notebook mode | Large | High |
| P3 | Web Worker for compilation | Medium | Medium |
| P4 | Persistent projects (IndexedDB) | Medium | Medium |
| P4 | Backend sharing service | Large | Medium |
| P4 | Embeddable playground | Medium | Medium |

## Conclusion

Lumen is in a unique position compared to most languages because the entire compiler and VM run client-side in WASM. This eliminates the need for server infrastructure (like Rust Playground, Replit, or Go Playground) and enables instant compilation feedback. The combination of WASM-powered execution and Lumen's markdown-native source format makes it possible to build a notebook-style playground that no other language can match.

The recommended path is to incrementally enhance the current playground: add keyboard shortcuts and URL sharing first (high impact, low effort), then multi-file support, then evolve toward a full notebook experience.
