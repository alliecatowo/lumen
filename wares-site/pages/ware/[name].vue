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
      <div v-else class="space-y-12">
        <!-- Header -->
        <div class="card relative overflow-hidden group">
          <!-- Background Glow -->
          <div v-if="ware.isVerified" class="absolute inset-0 bg-gradient-to-r from-green-500/5 to-transparent opacity-50"></div>
          
          <div class="relative flex flex-col md:flex-row md:items-start md:justify-between gap-6">
            <div class="space-y-4">
              <div class="flex items-center gap-4">
                <h1 class="text-4xl font-bold text-lumen-text tracking-tight">{{ ware.name }}</h1>
                <div class="flex items-center gap-2">
                  <span v-if="ware.version" class="px-2 py-1 bg-lumen-bgSecondary text-lumen-accent rounded text-sm font-mono border border-lumen-border">
                    {{ ware.version.startsWith('v') ? ware.version : `v${ware.version}` }}
                  </span>
                  <div v-if="ware.isVerified" class="flex items-center gap-1 px-2 py-1 bg-green-500/10 text-green-500 rounded text-xs font-bold border border-green-500/20 shadow-sm animate-in fade-in slide-in-from-left-2 transition-all">
                    <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                      <path fill-rule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.607.27 1.154.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
                    </svg>
                    Verified
                  </div>
                </div>
              </div>
              <p class="text-xl text-lumen-textMuted leading-relaxed max-w-2xl">
                {{ ware.description || 'A masterpiece of Lumen engineering waiting for a description.' }}
              </p>
            </div>
            
            <div class="flex items-center gap-4">
              <button @click="copyInstall" class="btn-primary flex items-center gap-3 py-3 px-8 text-lg hover:scale-[1.02] transition-transform active:scale-[0.98]">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                </svg>
                {{ copied ? 'Copied!' : 'Install' }}
              </button>
            </div>
          </div>

          <!-- Quick Stats Bar -->
          <div class="flex flex-wrap gap-8 mt-8 pt-8 border-t border-lumen-border/50">
            <div class="flex flex-col">
              <span class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mb-1">Downloads</span>
              <div class="text-lg font-bold text-lumen-text flex items-center gap-2">
                <svg class="w-4 h-4 text-lumen-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                {{ (ware.downloads || 0).toLocaleString() }}
              </div>
            </div>
            <div class="flex flex-col">
              <span class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mb-1">Author / Owner</span>
              <div class="text-lg font-bold text-lumen-text flex items-center gap-2">
                <div class="w-6 h-6 rounded-full bg-gradient-to-tr from-lumen-accent to-purple-500 flex items-center justify-center text-[10px] text-white font-bold">
                  {{ (ware.author || 'A').charAt(0).toUpperCase() }}
                </div>
                {{ ware.author || 'Anonymous' }}
              </div>
            </div>
            <div v-if="ware.updatedAt" class="flex flex-col">
              <span class="text-xs font-semibold text-lumen-textMuted uppercase tracking-wider mb-1">Last Updated</span>
              <div class="text-lg font-bold text-lumen-text">
                {{ formatDate(ware.updatedAt) }}
              </div>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
          <!-- Main Content -->
          <div class="lg:col-span-2 space-y-12">
            <!-- Install Command -->
            <div class="bg-lumen-bgSecondary/30 rounded-2xl border border-lumen-border p-8 relative overflow-hidden group">
              <div class="absolute inset-0 bg-gradient-to-r from-lumen-accent/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
              <h3 class="text-lg font-bold text-lumen-text mb-4 flex items-center gap-2">
                <svg class="w-5 h-5 text-lumen-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
                Quick Installation
              </h3>
              <div class="flex items-center gap-3">
                <code class="flex-1 font-mono text-xl text-lumen-accent bg-lumen-bg px-6 py-4 rounded-xl border border-lumen-border/50 overflow-x-auto shadow-inner">
                  wares install {{ ware.name }}
                </code>
                <button @click="copyInstall" class="btn-secondary h-full py-4 px-6 rounded-xl hover:bg-lumen-surface transition-all">
                  <svg v-if="!copied" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                  </svg>
                  <svg v-else class="w-6 h-6 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
              </div>
            </div>

            <!-- Audit Trail (Transparency Log) -->
            <div class="space-y-6">
              <div class="flex items-center justify-between">
                <h3 class="text-2xl font-bold text-lumen-text tracking-tight flex items-center gap-3">
                  <svg class="w-6 h-6 text-lumen-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                  </svg>
                  Audit Trail
                </h3>
                <div v-if="logInfo" class="flex gap-4 text-[10px] font-mono text-lumen-textMuted bg-lumen-bg/50 px-3 py-1.5 rounded-lg border border-lumen-border/30">
                  <span title="Merkle Tree Size"><span class="text-lumen-accent lowercase opacity-70 italic">size:</span> {{ logInfo.tree_size }}</span>
                  <span :title="logInfo.root_hash"><span class="text-lumen-accent lowercase opacity-70 italic">root:</span> {{ (logInfo.root_hash || '').substring(0, 8) }}...</span>
                </div>
              </div>
              
              <div class="card p-0 overflow-hidden border-lumen-border/30">
                <div v-if="auditLoading" class="p-8 space-y-4">
                  <div v-for="i in 3" :key="i" class="h-16 bg-lumen-bg/50 animate-pulse rounded-xl border border-lumen-border/10"></div>
                </div>
                <div v-else-if="auditEntries.length" class="divide-y divide-lumen-border/30">
                  <div 
                    v-for="(entry, idx) in auditEntries" 
                    :key="entry.uuid"
                    class="group transition-colors hover:bg-white/[0.01]"
                  >
                    <div 
                      class="p-5 flex items-start justify-between cursor-pointer"
                      @click="toggleEntry(idx)"
                    >
                      <div class="space-y-1">
                        <div class="flex items-center gap-3">
                          <span class="text-sm font-bold text-lumen-text group-hover:text-lumen-accent transition-colors">Version {{ entry.version }}</span>
                          <span class="px-2 py-0.5 bg-green-500/10 text-green-400 text-[10px] font-bold uppercase rounded-full border border-green-500/20">
                            Verified Signed
                          </span>
                          <span class="text-[10px] font-mono text-lumen-textMuted opacity-50">#{{ entry.index }}</span>
                        </div>
                        <p class="text-sm text-lumen-textMuted">
                          By <span class="text-lumen-text font-medium">{{ entry.identity }}</span>
                        </p>
                      </div>
                      <div class="flex items-center gap-4">
                        <span class="text-[11px] font-mono text-lumen-textMuted opacity-60">
                          {{ formatDate(entry.integrated_at) }}
                        </span>
                        <svg 
                          class="w-4 h-4 text-lumen-textMuted transition-transform duration-300"
                          :class="{ 'rotate-180': expandedEntry === idx }"
                          fill="none" stroke="currentColor" viewBox="0 0 24 24"
                        >
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                        </svg>
                      </div>
                    </div>
                    
                    <!-- Expandable Details -->
                    <div v-if="expandedEntry === idx" class="px-5 pb-5 animate-in slide-in-from-top-2 duration-300">
                      <div class="bg-black/20 rounded-xl p-4 border border-lumen-border/20 space-y-4">
                        <div class="grid grid-cols-2 gap-4">
                          <div class="space-y-1">
                            <label class="text-[9px] font-bold text-lumen-textMuted uppercase tracking-widest opacity-50">Content Hash (SHA-256)</label>
                            <div class="font-mono text-[11px] text-lumen-text bg-lumen-bg/40 p-2 rounded border border-lumen-border/10 truncate">
                              {{ entry.content_hash }}
                            </div>
                          </div>
                          <div class="space-y-1">
                            <label class="text-[9px] font-bold text-lumen-textMuted uppercase tracking-widest opacity-50">Entry UUID</label>
                            <div class="font-mono text-[11px] text-lumen-text bg-lumen-bg/40 p-2 rounded border border-lumen-border/10 truncate">
                              {{ entry.uuid }}
                            </div>
                          </div>
                        </div>
                        
                        <div class="flex items-center justify-between pt-2">
                           <div class="flex items-center gap-2">
                             <div class="w-1.5 h-1.5 rounded-full bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.6)]"></div>
                             <span class="text-[10px] text-green-400 font-bold uppercase tracking-wider">Log Chain Verified</span>
                           </div>
                           <button class="px-3 py-1 bg-lumen-accent/10 hover:bg-lumen-accent/20 border border-lumen-accent/30 text-lumen-accent text-[9px] font-bold uppercase rounded-lg transition-all tracking-widest hover:scale-105 active:scale-95">
                             View Inclusion Proof
                           </button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
                <div v-else class="p-12 text-center text-lumen-textMuted italic">
                  <p>No transparency log entries found for this package.</p>
                </div>
              </div>
            </div>
          </div>

          <!-- Sidebar -->
          <div class="space-y-8">
            <!-- Dependencies -->
            <div class="card space-y-4">
              <h3 class="text-lg font-bold text-lumen-text tracking-tight uppercase tracking-wider opacity-60">Dependencies</h3>
              <div v-if="ware.dependencies && Object.keys(ware.dependencies).length" class="flex flex-wrap gap-2">
                <NuxtLink 
                  v-for="(version, name) in ware.dependencies" 
                  :key="name"
                  :to="`/ware/${name}`"
                  class="px-3 py-2 bg-lumen-bg rounded-xl border border-lumen-border text-sm text-lumen-textMuted hover:text-lumen-text hover:border-lumen-accent/50 transition-all hover:scale-105"
                >
                  {{ name }} <span class="opacity-50 ml-1 text-xs">{{ version }}</span>
                </NuxtLink>
              </div>
              <p v-else class="text-sm text-lumen-textMuted italic">No external dependencies.</p>
            </div>

            <!-- Version History -->
            <div class="card space-y-4">
              <h3 class="text-lg font-bold text-lumen-text tracking-tight uppercase tracking-wider opacity-60">Version History</h3>
              <div v-if="ware.versions?.length" class="space-y-3">
                <div 
                  v-for="v in ware.versions" 
                  :key="v"
                  class="flex items-center justify-between py-2 border-b border-lumen-border/50 last:border-0 group"
                >
                  <span class="font-mono text-sm text-lumen-accent group-hover:underline cursor-pointer">v{{ v }}</span>
                  <svg class="w-4 h-4 text-lumen-textMuted opacity-0 group-hover:opacity-100 transition-opacity" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16l-4-4m0 0l4-4m-4 4h18" />
                  </svg>
                </div>
              </div>
            </div>

            <!-- Metadata Links -->
            <div v-if="ware.repository" class="card space-y-4">
              <h3 class="text-lg font-bold text-lumen-text tracking-tight uppercase tracking-wider opacity-60">Links</h3>
              <a :href="ware.repository" target="_blank" class="flex items-center gap-3 text-lumen-textMuted hover:text-lumen-text transition-colors group">
                <svg class="w-5 h-5 opacity-70 group-hover:opacity-100" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
                </svg>
                GitHub Repository
              </a>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const route = useRoute()
const { getWare, getAudit } = useWaresApi()

const ware = ref<any>(null)
const auditEntries = ref<any[]>([])
const logInfo = ref<any>(null)
const loading = ref(true)
const auditLoading = ref(true)
const copied = ref(false)
const expandedEntry = ref<number | null>(null)

const name = computed(() => route.params.name as string)

async function loadData() {
  if (name.value) {
    loading.value = true
    auditLoading.value = true
    ware.value = await getWare(name.value)
    loading.value = false
    
    const auditData = await getAudit(name.value)
    auditEntries.value = auditData.entries || []
    logInfo.value = auditData.logInfo || null
    auditLoading.value = false
  }
}

onMounted(() => {
  loadData()
})

watch(name, () => {
  loadData()
})

function toggleEntry(index: number) {
  expandedEntry.value = expandedEntry.value === index ? null : index
}

function copyInstall() {
  if (ware.value) {
    navigator.clipboard.writeText(`wares install ${ware.value.name}`)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  }
}

function formatDate(dateStr: string): string {
  if (!dateStr) return 'N/A'
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
