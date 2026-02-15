<template>
  <div class="grid grid-cols-2 md:grid-cols-4 gap-4 md:gap-8">
    <div v-for="stat in stats" :key="stat.label" class="group px-4 py-2 border-l-2 border-lumen-border hover:border-lumen-accent transition-colors">
      <div class="text-3xl font-bold text-lumen-text group-hover:text-lumen-accent transition-colors tabular-nums">
        {{ stat.value }}
      </div>
      <div class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mt-1 opacity-70">
        {{ stat.label }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Stat {
  label: string
  value: string | number
}

defineProps<{
  stats: Stat[]
}>()
</script>
