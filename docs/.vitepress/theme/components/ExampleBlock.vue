import { ref } from 'vue';
import { loadIntoPlayground } from '../playground-state';

const props = defineProps<{
  title: string;
  initialOpen?: boolean;
}>();

const isOpen = ref(props.initialOpen ?? false);
const container = ref<HTMLElement | null>(null);

function tryItOut() {
  const code = container.value?.querySelector('code')?.textContent;
  if (code) {
    loadIntoPlayground(code);
  }
}
</script>

<template>
  <div ref="container" class="example-block" :class="{ 'is-open': isOpen }">
    <div class="example-header" @click="isOpen = !isOpen">
      <div class="example-title">
        <svg :class="{ 'is-rotated': isOpen }" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
          <polyline points="9 18 15 12 9 6"/>
        </svg>
        {{ title }}
      </div>
      <div class="example-actions">
        <button class="try-button" @click.stop="tryItOut">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
            <polygon points="5 3 19 12 5 21 5 3"/>
          </svg>
          Try it Out
        </button>
      </div>
    </div>
    <div v-show="isOpen" class="example-content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.example-block {
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  margin: 16px 0;
  overflow: hidden;
  background: var(--vp-c-bg-soft);
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.example-block.is-open {
  border-color: rgba(255, 79, 163, 0.3);
  box-shadow: 0 10px 30px -10px rgba(255, 79, 163, 0.15);
}

.example-header {
  padding: 12px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  cursor: pointer;
  user-select: none;
}

.example-header:hover {
  background: rgba(255, 79, 163, 0.05);
}

.example-title {
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--vp-c-text-1);
}

.example-title svg {
  transition: transform 0.2s ease;
  color: var(--vp-c-text-3);
}

.example-title svg.is-rotated {
  transform: rotate(90deg);
  color: var(--vp-c-brand-1);
}

.try-button {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: rgba(255, 79, 163, 0.1);
  color: var(--vp-c-brand-1);
  border: 1px solid rgba(255, 79, 163, 0.2);
  border-radius: 6px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.02em;
  cursor: pointer;
  transition: all 0.2s;
}

.try-button:hover {
  background: var(--vp-c-brand-1);
  color: white;
  transform: translateY(-1px);
}

.example-content {
  padding: 0 16px 16px;
  border-top: 1px solid var(--vp-c-divider);
}

.example-content :deep(div[class*='language-']) {
  margin: 8px 0 0;
}
</style>
