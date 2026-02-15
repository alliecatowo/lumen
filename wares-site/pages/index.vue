<template>
  <div>
    <!-- Hero Section -->
    <section class="relative py-20 overflow-hidden">
      <div class="absolute inset-0 bg-gradient-to-b from-lumen-bgSecondary via-lumen-bg to-lumen-bg"></div>
      <div class="absolute inset-0 opacity-30">
        <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-lumen-accent/20 rounded-full blur-3xl"></div>
        <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-purple-500/20 rounded-full blur-3xl"></div>
      </div>
      
      <div class="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div class="text-center">
          <div class="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-lumen-accentSubtle text-lumen-accent text-sm font-medium mb-6">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
            </svg>
            Package Registry
          </div>
          
          <h1 class="text-5xl md:text-6xl font-bold text-lumen-text mb-6">
            Lumen <span class="text-lumen-accent">Warehouse</span>
          </h1>
          
          <p class="text-xl text-lumen-textMuted max-w-2xl mx-auto mb-10">
            Discover and share Lumen wares (packages). Build faster with community-contributed libraries, tools, and agents.
          </p>

          <div class="max-w-xl mx-auto mb-12">
            <SearchBar />
          </div>

          <div class="flex items-center justify-center gap-4">
            <NuxtLink to="/search" class="btn-primary">
              Browse All
            </NuxtLink>
            <a 
              href="https://lumen-lang.com/docs" 
              target="_blank"
              class="btn-secondary"
            >
              Documentation
            </a>
          </div>
        </div>
      </div>
    </section>

    <!-- Stats Section -->
    <section class="py-12 border-y border-lumen-border bg-lumen-bgSecondary">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <StatsDisplay :stats="stats" />
      </div>
    </section>

    <!-- Featured Wares -->
    <section class="py-16">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div class="flex items-center justify-between mb-8">
          <h2 class="text-2xl font-bold text-lumen-text">Featured Wares</h2>
          <NuxtLink to="/search" class="text-lumen-accent hover:text-lumen-accentHover transition-colors flex items-center gap-1">
            View all
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </NuxtLink>
        </div>

        <div v-if="loading" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          <WareCardSkeleton v-for="i in 3" :key="i" />
        </div>

        <div v-else-if="featuredWares.length" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          <WareCard v-for="ware in featuredWares" :key="ware.name" :ware="ware" />
        </div>

        <div v-else class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-lumen-textMuted mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
          </svg>
          <p class="text-lumen-textMuted">No wares found. Be the first to publish!</p>
          <NuxtLink to="/search" class="btn-primary mt-4 inline-block">
            Browse Registry
          </NuxtLink>
        </div>
      </div>
    </section>

    <!-- Latest Wares -->
    <section v-if="latestWares.length" class="py-16 bg-lumen-bgSecondary/30">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <h2 class="text-2xl font-bold text-lumen-text mb-8">Recently Updated</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <WareCard v-for="ware in latestWares" :key="`latest-${ware.name}`" :ware="ware" variant="compact" />
        </div>
      </div>
    </section>

    <!-- Quick Install Section -->
    <section class="py-16 bg-lumen-bgSecondary border-t border-lumen-border">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div class="max-w-2xl mx-auto text-center">
          <h2 class="text-2xl font-bold text-lumen-text mb-4">Quick Install</h2>
          <p class="text-lumen-textMuted mb-6">
            Install packages using the Lumen CLI
          </p>
          <div class="bg-lumen-bg rounded-xl border border-lumen-border p-6 text-left relative overflow-hidden group">
            <div class="absolute inset-0 bg-gradient-to-r from-lumen-accent/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
            <div class="flex items-center gap-2 text-lumen-textMuted text-sm mb-2">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
              Terminal
            </div>
            <code class="block font-mono text-lg text-lumen-accent">
              wares install &lt;package-name&gt;
            </code>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
const { fetchIndex, searchWares } = useWaresApi()

const loading = ref(true)
const featuredWares = ref<any[]>([])
const latestWares = ref<any[]>([])
const stats = ref([
  { label: 'Total Wares', value: '—' },
  { label: 'Total Downloads', value: '—' },
  { label: 'Categories', value: '—' },
  { label: 'Contributors', value: '—' }
])

onMounted(async () => {
  const index = await fetchIndex()
  if (index) {
    stats.value = [
      { label: 'Total Wares', value: index.totalPackages?.toLocaleString() || '0' },
      { label: 'Total Downloads', value: formatNumber(index.totalDownloads || 0) },
      { label: 'Categories', value: index.categories?.length || '0' },
      { label: 'Contributors', value: index.contributors?.toLocaleString() || '0' }
    ]
    
    if (index.packages?.length) {
      // For now, let's treat the first 3 as "featured" and the rest as "latest"
      // In a real system, we'd have a 'featured' flag or better sorting.
      featuredWares.value = index.packages.slice(0, 3)
      latestWares.value = [...index.packages].sort((a, b) => 
        new Date(b.updatedAt || 0).getTime() - new Date(a.updatedAt || 0).getTime()
      ).slice(0, 8)
    }
  }
  loading.value = false
})

function formatNumber(n: number): string {
  if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M'
  if (n >= 1000) return (n / 1000).toFixed(1) + 'K'
  return n.toString()
}

useHead({
  title: 'Lumen Warehouse - Package Registry'
})
</script>
