import { ref } from 'vue';

export const playgroundSource = ref<string | null>(null);

export function loadIntoPlayground(source: string) {
    playgroundSource.value = source;
    // Scroll to playground if it exists
    const el = document.querySelector('.wasm-playground');
    if (el) {
        el.scrollIntoView({ behavior: 'smooth' });
    }
}
