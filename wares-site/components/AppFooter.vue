<template>
  <footer class="border-t border-lumen-border bg-lumen-bgSecondary">
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
      <div class="flex flex-col md:flex-row items-center justify-between gap-4">
        <div class="flex items-center gap-2">
          <svg class="w-5 h-5 text-lumen-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
          </svg>
          <span class="text-lumen-textMuted">Lumen Warehouse</span>
        </div>
        
        <div class="flex items-center gap-6 text-sm text-lumen-textMuted">
          <a href="https://lumen-lang.com" class="hover:text-lumen-text transition-colors">Lumen</a>
          <a href="https://github.com/alliecatowo/lumen" class="hover:text-lumen-text transition-colors">GitHub</a>
          <span>Package Registry</span>
        </div>

        <p class="text-sm text-lumen-textMuted">
          &copy; {{ new Date().getFullYear() }} Lumen
        </p>
      </div>
    </div>
  </footer>
</template>
