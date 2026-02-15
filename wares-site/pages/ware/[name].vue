<template>
  <div class="min-h-screen">
    <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
      <!-- Back Link -->
      <NuxtLink to="/search" class="inline-flex items-center gap-2 text-lumen-textMuted hover:text-lumen-text transition-colors mb-8">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        Back to Browse
      </NuxtLink>

      <!-- Loading -->
      <div v-if="loading" class="space-y-6">
        <div class="card">
          <Skeleton className="h-10 w-1/2 bg-lumen-bgSecondary rounded mb-4" />
          <Skeleton className="h-6 w-3/4 bg-lumen-bgSecondary rounded mb-2" />
          <Skeleton className="h-4 w-1/4 bg-lumen-bgSecondary rounded" />
        </div>
        <Skeleton className="h-32 w-full bg-lumen-bgSecondary rounded" />
      </div>

      <!-- Not Found -->
      <div v-else-if="!ware" class="text-center py-20">
        <svg class="w-20 h-20 mx-auto text-lumen-textMuted mb-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <h2 class="text-2xl font-bold text-lumen-text mb-2">Package Not Found</h2>
        <p class="text-lumen-textMuted mb-6">
          The package "{{ route.params.name }}" doesn't exist in the registry.
        </p>
        <NuxtLink to="/search" class="btn-primary">
          Browse All Packages
        </NuxtLink>
      </div>

      <!-- Package Details -->
      <div v-else class="space-y-8">
        <!-- Header -->
        <div class="card">
          <div class="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
            <div>
              <div class="flex items-center gap-3 mb-2">
                <h1 class="text-3xl font-bold text-lumen-text">{{ ware.name }}</h1>
                <span v-if="ware.version" class="badge text-sm">
                  v{{ ware.version }}
                </span>
              </div>
              <p class="text-lg text-lumen-textMuted">
                {{ ware.description || 'No description available' }}
              </p>
            </div>
            
            <div class="flex items-center gap-4">
              <button @click="copyInstall" class="btn-primary flex items-center gap-2">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                </svg>
                {{ copied ? 'Copied!' : 'Install' }}
              </button>
            </div>
          </div>

          <!-- Stats -->
          <div class="flex flex-wrap gap-6 mt-6 pt-6 border-t border-lumen-border">
            <div v-if="ware.downloads !== undefined" class="flex items-center gap-2 text-lumen-textMuted">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
              {{ ware.downloads.toLocaleString() }} downloads
            </div>
            <div v-if="ware.author" class="flex items-center gap-2 text-lumen-textMuted">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
              </svg>
              {{ ware.author }}
            </div>
            <div v-if="ware.license" class="flex items-center gap-2 text-lumen-textMuted">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ ware.license }}
            </div>
          </div>
        </div>

        <!-- Install Command -->
        <div class="bg-lumen-bgSecondary rounded-xl border border-lumen-border p-6">
          <h3 class="text-lg font-semibold text-lumen-text mb-3">Installation</h3>
          <div class="flex items-center gap-3">
            <code class="flex-1 font-mono text-lg text-lumen-accent bg-lumen-bg px-4 py-3 rounded-lg overflow-x-auto">
              wrhs install {{ ware.name }}
            </code>
            <button @click="copyInstall" class="btn-secondary">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Dependencies -->
        <div v-if="ware.dependencies && Object.keys(ware.dependencies).length" class="card">
          <h3 class="text-lg font-semibold text-lumen-text mb-4">Dependencies</h3>
          <div class="flex flex-wrap gap-2">
            <NuxtLink 
              v-for="(version, name) in ware.dependencies" 
              :key="name"
              :to="`/ware/${name}`"
              class="px-3 py-2 bg-lumen-bg rounded-lg border border-lumen-border text-lumen-textMuted hover:text-lumen-text hover:border-lumen-accent/50 transition-colors"
            >
              {{ name }}
              <span class="text-lumen-textMuted/60 ml-1">{{ version }}</span>
            </NuxtLink>
          </div>
        </div>

        <!-- Version History -->
        <div v-if="ware.versions?.length" class="card">
          <h3 class="text-lg font-semibold text-lumen-text mb-4">Version History</h3>
          <div class="space-y-3">
            <div 
              v-for="v in ware.versions" 
              :key="v.version"
              class="flex items-center justify-between py-2 border-b border-lumen-border last:border-0"
            >
              <div class="flex items-center gap-3">
                <span class="font-mono text-lumen-accent">v{{ v.version }}</span>
                <span v-if="v.published_at" class="text-sm text-lumen-textMuted">
                  {{ formatDate(v.published_at) }}
                </span>
              </div>
              <a 
                v-if="v.download_url"
                :href="v.download_url"
                class="text-lumen-textMuted hover:text-lumen-accent transition-colors"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
              </a>
            </div>
          </div>
        </div>

        <!-- Links -->
        <div v-if="ware.repository" class="flex gap-4">
          <a 
            :href="ware.repository" 
            target="_blank"
            rel="noopener"
            class="btn-secondary flex items-center gap-2"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
            </svg>
            View Repository
          </a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const route = useRoute()
const { getWare } = useWaresApi()

const ware = ref<any>(null)
const loading = ref(true)
const copied = ref(false)

const name = computed(() => route.params.name as string)

onMounted(async () => {
  if (name.value) {
    ware.value = await getWare(name.value)
  }
  loading.value = false
})

watch(name, async (newName) => {
  if (newName) {
    loading.value = true
    ware.value = await getWare(newName)
    loading.value = false
  }
})

function copyInstall() {
  if (ware.value) {
    navigator.clipboard.writeText(`wrhs install ${ware.value.name}`)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  }
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric'
  })
}

useHead({
  title: computed(() => ware.value ? `${ware.value.name} - Lumen Warehouse` : 'Package Not Found - Lumen Warehouse')
})
</script>
