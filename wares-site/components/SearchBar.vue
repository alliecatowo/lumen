<template>
  <form @submit.prevent="handleSearch" class="relative">
    <div class="relative">
      <svg class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-lumen-textMuted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        v-model="query"
        type="text"
        placeholder="Search wares..."
        class="input-field pl-12 pr-4"
        @input="$emit('update:modelValue', query)"
      />
      <button 
        v-if="query" 
        type="button"
        @click="clearSearch"
        class="absolute right-4 top-1/2 -translate-y-1/2 text-lumen-textMuted hover:text-lumen-text"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  </form>
</template>

<script setup lang="ts">
const query = defineModel<string>({ default: '' })

const emit = defineEmits<{
  search: [query: string]
}>()

function handleSearch() {
  if (query.value.trim()) {
    emit('search', query.value.trim())
    navigateTo(`/search?q=${encodeURIComponent(query.value.trim())}`)
  }
}

function clearSearch() {
  query.value = ''
}
</script>
