<template>
  <div class="min-h-screen py-12">
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
      <div v-if="loading" class="space-y-8">
        <div class="flex items-center gap-6">
          <Skeleton className="w-24 h-24 rounded-full bg-lumen-bgSecondary" />
          <div class="space-y-2 flex-1">
            <Skeleton className="h-8 w-1/4 bg-lumen-bgSecondary rounded" />
            <Skeleton className="h-4 w-1/3 bg-lumen-bgSecondary rounded" />
          </div>
        </div>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          <WareCardSkeleton v-for="i in 3" :key="i" />
        </div>
      </div>

      <div v-else-if="user" class="space-y-12">
        <!-- Profile Header -->
        <div class="flex flex-col md:flex-row items-center md:items-start gap-8">
          <div class="relative group">
            <div class="absolute -inset-1 bg-gradient-to-tr from-lumen-accent to-purple-600 rounded-full blur opacity-25 group-hover:opacity-50 transition duration-1000 group-hover:duration-200"></div>
            <div class="relative w-32 h-32 rounded-full border-2 border-lumen-border overflow-hidden bg-lumen-bgSecondary">
              <img v-if="user.avatar" :src="user.avatar" class="w-full h-full object-cover" :alt="user.name" />
              <div v-else class="w-full h-full flex items-center justify-center text-4xl font-bold text-lumen-accent">
                {{ user.name?.charAt(0) || 'U' }}
              </div>
            </div>
          </div>
          
          <div class="flex-1 text-center md:text-left space-y-4">
            <div>
              <h1 class="text-4xl font-bold text-lumen-text leading-tight">{{ user.name }}</h1>
              <p class="text-lg text-lumen-textMuted flex items-center justify-center md:justify-start gap-2">
                <span class="opacity-70">Authenticated via</span>
                <span class="font-mono text-lumen-accent">{{ user.identity }}</span>
              </p>
            </div>
            
            <div class="flex flex-wrap justify-center md:justify-start gap-4">
              <div class="px-4 py-2 rounded-lg bg-lumen-surface border border-lumen-border">
                <div class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mb-1">Total Packages</div>
                <div class="text-2xl font-bold text-lumen-text">{{ user.packages?.length || 0 }}</div>
              </div>
              <div class="px-4 py-2 rounded-lg bg-lumen-surface border border-lumen-border">
                <div class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mb-1">Status</div>
                <div class="text-2xl font-bold text-green-500 flex items-center gap-2">
                  Verified
                  <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
                    <path fill-rule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.607.27 1.154.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
                  </svg>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Packages Section -->
        <div class="space-y-6">
          <div class="flex items-center justify-between">
            <h2 class="text-2xl font-bold text-lumen-text tracking-tight">Your Packages</h2>
            <NuxtLink to="/search" class="text-sm text-lumen-accent hover:underline">Explore all packages</NuxtLink>
          </div>
          
          <div v-if="user.packages?.length" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            <WareCard v-for="ware in user.packages" :key="ware.name" :ware="ware" />
          </div>
          <div v-else class="text-center py-20 card bg-lumen-surface/30">
            <div class="w-16 h-16 bg-lumen-bgSecondary rounded-full flex items-center justify-center mx-auto mb-4">
              <svg class="w-8 h-8 text-lumen-textMuted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
              </svg>
            </div>
            <h3 class="text-xl font-semibold text-lumen-text mb-2">No packages yet</h3>
            <p class="text-lumen-textMuted mb-8">Publish your first Lumen package using the CLI.</p>
            <div class="max-w-xs mx-auto text-left">
              <code class="block bg-lumen-bg p-4 rounded-lg text-sm text-lumen-accent">
                wares publish
              </code>
            </div>
          </div>
        </div>
      </div>

      <!-- Unauthorized -->
      <div v-else class="text-center py-20">
        <h2 class="text-2xl font-bold text-lumen-text mb-2">Unauthorized</h2>
        <p class="text-lumen-textMuted mb-6">You need to sign in to view your profile.</p>
        <button @click="login" class="btn-primary">Sign in with GitHub</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const { user, login, fetchUser } = useWaresApi()
const loading = ref(true)

onMounted(async () => {
  await fetchUser()
  loading.value = false
})

useHead({
  title: 'Profile - Lumen Warehouse'
})
</script>
