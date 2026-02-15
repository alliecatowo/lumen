<template>
  <NuxtLink :to="`/ware/${ware.name}`" class="card group block">
    <div class="flex items-start justify-between gap-4">
      <div class="flex-1 min-w-0">
        <h3 class="text-lg font-semibold text-lumen-text group-hover:text-lumen-accent transition-colors truncate">
          {{ ware.name }}
        </h3>
        <p class="mt-1 text-sm text-lumen-textMuted line-clamp-2">
          {{ ware.description || 'No description available' }}
        </p>
      </div>
      <span v-if="ware.version" class="badge shrink-0">
        v{{ ware.version }}
      </span>
    </div>

    <div class="mt-4 flex items-center gap-4 text-xs text-lumen-textMuted">
      <span v-if="ware.downloads !== undefined" class="flex items-center gap-1">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        {{ formatDownloads(ware.downloads) }}
      </span>
      <span v-if="ware.author" class="flex items-center gap-1">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
        </svg>
        {{ ware.author }}
      </span>
    </div>

    <div v-if="ware.keywords?.length" class="mt-3 flex flex-wrap gap-1.5">
      <span 
        v-for="keyword in ware.keywords.slice(0, 3)" 
        :key="keyword"
        class="text-xs px-2 py-0.5 rounded bg-lumen-bgSecondary text-lumen-textMuted"
      >
        {{ keyword }}
      </span>
    </div>
  </NuxtLink>
</template>

<script setup lang="ts">
interface Ware {
  name: string
  version?: string
  description?: string
  downloads?: number
  author?: string
  keywords?: string[]
}

defineProps<{
  ware: Ware
}>()

function formatDownloads(n: number): string {
  if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M'
  if (n >= 1000) return (n / 1000).toFixed(1) + 'K'
  return n.toString()
}
</script>
