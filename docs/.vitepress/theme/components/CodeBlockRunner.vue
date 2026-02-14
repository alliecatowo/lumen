<script setup lang="ts">
import { onMounted, ref, nextTick } from 'vue';
import { withBase } from 'vitepress';

type LumenResult = {
  to_json: () => string;
};

type WasmApi = {
  run: (source: string, cellName?: string) => LumenResult;
  version: () => string;
};

const api = ref<WasmApi | null>(null);
const wasmReady = ref(false);
const wasmLoading = ref(true);

function parseResult(result: LumenResult): { ok?: string; error?: string } {
  try {
    return JSON.parse(result.to_json());
  } catch {
    return { error: "Failed to parse WASM response" };
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
      run: wasmModule.run,
      version: wasmModule.version,
    };
    wasmReady.value = true;
    wasmLoading.value = false;
  } catch (error) {
    console.warn("WASM not available:", error);
    wasmLoading.value = false;
  }
}

function createOutputElement(container: HTMLElement, content: string, isError: boolean): HTMLElement {
  const existing = container.querySelector('.lumen-inline-output');
  if (existing) existing.remove();
  
  const output = document.createElement('div');
  output.className = `lumen-inline-output ${isError ? 'error' : 'success'}`;
  output.textContent = content;
  container.appendChild(output);
  return output;
}

async function runCode(button: HTMLButtonElement, code: string, container: HTMLElement) {
  if (!api.value) {
    if (wasmLoading.value) {
      button.textContent = 'Loading...';
      return;
    }
    createOutputElement(container, 'WASM runtime not available', true);
    return;
  }
  
  button.className = 'lumen-play-button running';
  button.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <circle cx="12" cy="12" r="10" stroke-opacity="0.25"/>
    <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round">
      <animateTransform attributeName="transform" type="rotate" from="0 12 12" to="360 12 12" dur="1s" repeatCount="indefinite"/>
    </path>
  </svg> Running`;
  
  try {
    const result = parseResult(api.value.run(toCompilerSource(code)));
    
    if (result.error) {
      button.className = 'lumen-play-button error';
      button.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/>
        <line x1="15" y1="9" x2="9" y2="15"/>
        <line x1="9" y1="9" x2="15" y2="15"/>
      </svg> Error`;
      createOutputElement(container, result.error, true);
    } else {
      button.className = 'lumen-play-button success';
      button.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M20 6L9 17l-5-5"/>
      </svg> ${result.ok ?? 'OK'}`;
      createOutputElement(container, `Result: ${result.ok}`, false);
    }
  } catch (e) {
    button.className = 'lumen-play-button error';
    button.innerHTML = `Error`;
    createOutputElement(container, String(e), true);
  }
  
  setTimeout(() => {
    button.className = 'lumen-play-button';
    button.innerHTML = `<svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M8 5v14l11-7z"/>
    </svg> Run`;
  }, 3000);
}

function addPlayButtons() {
  const codeBlocks = document.querySelectorAll('div[class*="language-lumen"]');
  
  codeBlocks.forEach((block) => {
    if (block.querySelector('.lumen-play-button')) return;
    
    const pre = block.querySelector('pre');
    const code = pre?.querySelector('code')?.textContent;
    if (!code) return;
    
    // Make block position relative for absolute button
    (block as HTMLElement).style.position = 'relative';
    
    const button = document.createElement('button');
    button.className = 'lumen-play-button';
    button.innerHTML = `<svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M8 5v14l11-7z"/>
    </svg> Run`;
    
    button.addEventListener('click', () => {
      runCode(button, code, block as HTMLElement);
    });
    
    block.insertBefore(button, block.firstChild);
  });
}

onMounted(async () => {
  await initWasm();
  
  // Initial scan
  await nextTick();
  addPlayButtons();
  
  // Watch for content changes (SPA navigation)
  const observer = new MutationObserver(() => {
    addPlayButtons();
  });
  
  observer.observe(document.body, {
    childList: true,
    subtree: true,
  });
});
</script>

<template>
  <slot />
</template>
