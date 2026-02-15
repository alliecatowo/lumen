<template>
  <div class="min-h-[50vh] flex items-center justify-center">
    <div class="text-center">
      <div v-if="error" class="space-y-4">
        <div class="w-16 h-16 bg-red-500/10 text-red-500 rounded-full flex items-center justify-center mx-auto mb-4">
          <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </div>
        <h2 class="text-2xl font-bold text-lumen-text">Authentication Failed</h2>
        <p class="text-lumen-textMuted">{{ error }}</p>
        <button @click="login" class="btn-primary">Try Again</button>
      </div>
      
      <div v-else class="space-y-4">
        <div class="w-16 h-16 border-4 border-lumen-accent border-t-transparent rounded-full animate-spin mx-auto mb-4"></div>
        <h2 class="text-2xl font-bold text-lumen-text">Authenticating...</h2>
        <p class="text-lumen-textMuted">Completing your connection to the Lumen Warehouse.</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const route = useRoute()
const router = useRouter()
const { token, login, fetchUser } = useWaresApi()
const error = ref<string | null>(null)

const config = useRuntimeConfig()
const baseUrl = config.public.apiBase || 'https://wares.lumen-lang.com/v1'

onMounted(async () => {
  const code = route.query.code as string
  const state = route.query.state as string
  const sessionId = state?.split(':')[0]

  if (!code || !sessionId) {
    error.value = 'Invalid callback parameters'
    return
  }

  try {
    // Exchange session_id for token
    const res = await fetch(`${baseUrl}/auth/oidc/token?session_id=${sessionId}`)
    const data = await res.json()

    if (res.ok && data.access_token) {
      token.value = data.access_token
      if (typeof window !== 'undefined') {
        localStorage.setItem('wares-token', data.access_token)
      }
      await fetchUser()
      router.push('/')
    } else {
      error.value = data.error || 'Failed to retrieve token'
    }
  } catch (e) {
    error.value = 'A network error occurred during authentication'
  }
})
</script>
