<template>
  <NuxtLink 
    :to="`/ware/${ware.name}`" 
    :class="[
      'group relative flex flex-col h-full transition-all duration-300',
      variant === 'compact' ? 'card p-4 hover:border-lumen-accent/30' : 'card p-6 hover:translate-y-[-2px] hover:shadow-xl hover:shadow-lumen-accent/5'
    ]"
  >
    <!-- Background Gradient Glow -->
    <div class="absolute inset-0 bg-gradient-to-br from-lumen-accent/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity rounded-xl"></div>

    <div class="relative flex-1">
      <div class="flex items-start justify-between gap-4 mb-2">
        <h3 :class="[
          'font-bold text-lumen-text group-hover:text-lumen-accent transition-colors truncate',
          variant === 'compact' ? 'text-base' : 'text-xl'
        ]">
          {{ ware.name }}
        </h3>
        <div class="flex items-center gap-2 shrink-0">
          <div v-if="ware.isVerified" class="text-green-500" title="Verified Author">
            <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
              <path fill-rule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.607.27 1.154.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
            </svg>
          </div>
          <span v-if="ware.version" class="text-[10px] font-bold tracking-wider uppercase px-2 py-0.5 rounded bg-lumen-accent/10 text-lumen-accent border border-lumen-accent/20">
            {{ ware.version.startsWith('v') ? ware.version : `v${ware.version}` }}
          </span>
        </div>
      </div>

      <p :class="[
        'text-lumen-textMuted leading-relaxed',
        variant === 'compact' ? 'text-xs line-clamp-2' : 'text-sm line-clamp-3 mb-4'
      ]">
        {{ ware.description || 'A masterpiece of Lumen engineering waiting for a description.' }}
      </p>
    </div>

    <!-- Metadata Footer -->
    <div :class="[
      'relative pt-4 flex items-center justify-between border-t border-lumen-border/50 group-hover:border-lumen-accent/20 transition-colors',
      variant === 'compact' ? 'mt-2' : 'mt-4'
    ]">
      <div class="flex items-center gap-3 min-w-0">
        <div v-if="ware.author" class="flex items-center gap-1.5 min-w-0">
          <div class="w-5 h-5 rounded-full bg-gradient-to-tr from-lumen-accent to-purple-500 flex items-center justify-center text-[10px] text-white font-bold shrink-0">
            {{ (ware.author || 'A').charAt(0).toUpperCase() }}
          </div>
          <span class="text-xs text-lumen-textMuted group-hover:text-lumen-text transition-colors truncate">
            {{ ware.author || 'Anonymous' }}
          </span>
        </div>
      </div>

      <div class="flex items-center gap-3 shrink-0 text-[10px] font-medium text-lumen-textMuted opacity-60 group-hover:opacity-100 transition-opacity">
        <span class="flex items-center gap-1">
          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          {{ formatDownloads(ware.downloads || 0) }}
        </span>
      </div>
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
  isVerified?: boolean
  owner?: string
}

const props = withDefaults(defineProps<{
  ware: Ware
  variant?: 'default' | 'compact'
}>(), {
  variant: 'default'
})

function formatDownloads(n: number): string {
  if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M'
  if (n >= 1000) return (n / 1000).toFixed(1) + 'K'
  return n.toLocaleString()
}
</script>
