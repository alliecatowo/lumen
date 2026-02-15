<template>
  <div class="min-h-screen">
    <!-- Header -->
    <div class="bg-lumen-bgSecondary border-b border-lumen-border">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <h1 class="text-3xl font-bold text-lumen-text mb-6">Browse Wares</h1>
        
        <div class="flex flex-col md:flex-row gap-4">
          <div class="flex-1">
            <SearchBar v-model="searchQuery" @search="doSearch" />
          </div>
          
          <div class="flex gap-2 flex-wrap">
            <button
              v-for="cat in categories"
              :key="cat"
              @click="toggleCategory(cat)"
              :class="[
                'px-3 py-2 rounded-lg text-sm font-medium transition-colors',
                selectedCategories.includes(cat)
                  ? 'bg-lumen-accent text-white'
                  : 'bg-lumen-surface border border-lumen-border text-lumen-textMuted hover:text-lumen-text'
              ]"
            >
              {{ cat }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Results -->
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
      <div class="flex items-center justify-between mb-6">
        <p class="text-lumen-textMuted">
          <template v-if="loading">
            Searching...
          </template>
          <template v-else-if="searchQuery">
            {{ results.length }} result{{ results.length !== 1 ? 's' : '' }} for "{{ searchQuery }}"
          </template>
          <template v-else>
            {{ results.length }} package{{ results.length !== 1 ? 's' : '' }} available
          </template>
        </p>
      </div>

      <!-- Loading -->
      <div v-if="loading" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        <WareCardSkeleton v-for="i in 6" :key="i" />
      </div>

      <!-- Results Grid -->
      <div v-else-if="results.length" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        <WareCard v-for="ware in results" :key="ware.name" :ware="ware" />
      </div>

      <!-- Empty State -->
      <div v-else class="text-center py-20">
        <svg class="w-20 h-20 mx-auto text-lumen-textMuted mb-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <h3 class="text-xl font-semibold text-lumen-text mb-2">No wares found</h3>
        <p class="text-lumen-textMuted mb-6">
          Try a different search term or browse all packages
        </p>
        <NuxtLink to="/search" class="btn-primary">
          View All
        </NuxtLink>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const route = useRoute()
const router = useRouter()
const { searchWares, fetchIndex } = useWaresApi()

const searchQuery = ref((route.query.q as string) || '')
const results = ref<any[]>([])
const loading = ref(true)
const selectedCategories = ref<string[]>([])

const categories = ['HTTP', 'AI', 'Database', 'Utils', 'Testing', 'CLI']

onMounted(async () => {
  if (searchQuery.value) {
    await doSearch(searchQuery.value)
  } else {
    await loadAll()
  }
})

watch(() => route.query.q, async (newQuery) => {
  searchQuery.value = (newQuery as string) || ''
  if (searchQuery.value) {
    await doSearch(searchQuery.value)
  } else {
    await loadAll()
  }
})

async function loadAll() {
  loading.value = true
  const index = await fetchIndex()
  if (index?.packages) {
    results.value = index.packages
  }
  loading.value = false
}

async function doSearch(query: string) {
  loading.value = true
  const res = await searchWares(query)
  results.value = res?.results || []
  loading.value = false
}

function toggleCategory(cat: string) {
  const idx = selectedCategories.value.indexOf(cat)
  if (idx > -1) {
    selectedCategories.value.splice(idx, 1)
  } else {
    selectedCategories.value.push(cat)
  }
}

useHead({
  title: computed(() => searchQuery.value ? `Search: ${searchQuery.value} - Lumen Warehouse` : 'Browse - Lumen Warehouse')
})
</script>
