<template>
  <header class="sticky top-0 z-50 bg-lumen-bg/80 backdrop-blur-lg border-b border-lumen-border">
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div class="flex items-center justify-between h-16">
        <NuxtLink to="/" class="flex items-center gap-3 group">
          <div class="w-10 h-10 rounded-lg bg-lumen-surface border border-lumen-border flex items-center justify-center">
            <svg class="w-6 h-6 text-lumen-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
            </svg>
          </div>
          <div>
            <span class="text-lg font-semibold text-lumen-text group-hover:text-lumen-accent transition-colors">Lumen</span>
            <span class="text-lg font-light text-lumen-textMuted">Warehouse</span>
          </div>
        </NuxtLink>

        <nav class="hidden md:flex items-center gap-6">
          <NuxtLink to="/" class="text-lumen-textMuted hover:text-lumen-text transition-colors">
            Home
          </NuxtLink>
          <NuxtLink to="/search" class="text-lumen-textMuted hover:text-lumen-text transition-colors">
            Browse
          </NuxtLink>
        </nav>

        <div class="flex items-center gap-3">
          <NuxtLink to="/search" class="btn-primary text-sm">
            Search
          </NuxtLink>
        </div>
      </div>
    </div>
  </header>
</template>
