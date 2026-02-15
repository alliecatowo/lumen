<template>
  <div :class="['animate-pulse', className]">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineProps<{
  className?: string
}>()
</script>
