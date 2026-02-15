<template>
  <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
    <div v-for="stat in stats" :key="stat.label" class="card text-center">
      <div class="text-2xl font-bold text-lumen-accent">{{ stat.value }}</div>
      <div class="text-sm text-lumen-textMuted mt-1">{{ stat.label }}</div>
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
