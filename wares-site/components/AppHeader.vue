<template>
  <header class="sticky top-0 z-50 bg-lumen-bg/70 backdrop-blur-xl border-b border-lumen-border/50">
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div class="flex items-center justify-between h-20">
        <NuxtLink to="/" class="flex items-center gap-3 group">
          <div class="w-12 h-12 rounded-xl bg-gradient-to-tr from-lumen-accent/20 to-purple-500/20 border border-lumen-accent/30 flex items-center justify-center transition-all duration-300 group-hover:scale-105 group-hover:shadow-lg group-hover:shadow-lumen-accent/10">
            <svg class="w-7 h-7 text-lumen-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
            </svg>
          </div>
          <div class="flex flex-col">
            <span class="text-xl font-bold text-lumen-text leading-tight tracking-tight group-hover:text-lumen-accent transition-colors">Lumen</span>
            <span class="text-xs font-semibold text-lumen-textMuted uppercase tracking-widest opacity-60">Warehouse</span>
          </div>
        </NuxtLink>

        <nav class="hidden md:flex items-center gap-8">
          <NuxtLink to="/" class="text-sm font-medium text-lumen-textMuted hover:text-lumen-text transition-colors">
            Home
          </NuxtLink>
          <NuxtLink to="/search" class="text-sm font-medium text-lumen-textMuted hover:text-lumen-text transition-colors">
            Registry
          </NuxtLink>
          <a href="https://lumen-lang.com" target="_blank" class="text-sm font-medium text-lumen-textMuted hover:text-lumen-text transition-colors">
            Site
          </a>
        </nav>

        <div class="flex items-center gap-4">
          <template v-if="user">
            <NuxtLink to="/profile" class="flex items-center gap-2 group">
              <div v-if="user.avatar" class="w-8 h-8 rounded-full border border-lumen-border overflow-hidden">
                <img :src="user.avatar" class="w-full h-full object-cover" :alt="user.name" />
              </div>
              <div v-else class="w-8 h-8 rounded-full bg-lumen-accent/20 flex items-center justify-center text-xs font-bold text-lumen-accent">
                {{ user.name?.charAt(0) || 'U' }}
              </div>
              <span class="hidden sm:block text-sm font-medium text-lumen-textMuted group-hover:text-lumen-text transition-colors">
                {{ user.name }}
              </span>
            </NuxtLink>
            <button @click="logout" class="text-lumen-textMuted hover:text-red-400 p-2 transition-colors">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
            </button>
          </template>
          <button v-else @click="login" class="btn-primary text-sm px-6">
            Login
          </button>
        </div>
      </div>
    </div>
  </header>
</template>

<script setup lang="ts">
const { user, login, logout, fetchUser } = useWaresApi()

onMounted(() => {
  fetchUser()
})
</script>
