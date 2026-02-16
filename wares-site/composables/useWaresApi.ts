export const useWaresApi = () => {
  const config = useRuntimeConfig()
  const baseUrl = config.public.apiBase || 'https://wares.lumen-lang.com/v1'

  const token = useState<string | null>('wares-token', () => typeof window !== 'undefined' ? localStorage.getItem('wares-token') : null)
  const user = useState<any | null>('wares-user', () => null)

  const fetchWithAuth = async (url: string, options: any = {}) => {
    const headers = { ...options.headers }
    if (token.value) {
      headers['Authorization'] = `Bearer ${token.value}`
    }
    return fetch(url, { ...options, headers })
  }

  const fetchIndex = async () => {
    try {
      const res = await fetch(`${baseUrl}/index`)
      if (!res.ok) throw new Error('Failed to fetch index')
      return await res.json()
    } catch (e) {
      console.error('API Error:', e)
      return null
    }
  }

  const searchWares = async (query: string) => {
    try {
      const res = await fetch(`${baseUrl}/search?q=${encodeURIComponent(query)}`)
      if (!res.ok) throw new Error('Search failed')
      return await res.json()
    } catch (e) {
      console.error('API Error:', e)
      return { packages: [], total: 0 }
    }
  }

  const getProof = async (name: string, version?: string) => {
    try {
      const url = version
        ? `${baseUrl}/wares/${name}/${version}/resolve-proof`
        : `${baseUrl}/wares/${name}/resolve-proof`
      const res = await fetch(url)
      if (!res.ok) throw new Error('Failed to fetch resolution proof')
      return await res.json()
    } catch (e) {
      console.error('API Error:', e)
      return null
    }
  }

  const getWare = async (name: string) => {
    try {
      const res = await fetch(`${baseUrl}/wares/${name}`)
      if (!res.ok) {
        if (res.status === 404) return null
        throw new Error('Failed to fetch ware')
      }
      return await res.json()
    } catch (e) {
      console.error('API Error:', e)
      return null
    }
  }

  const getAudit = async (name: string) => {
    try {
      const res = await fetch(`${baseUrl}/wares/${name}/audit`)
      if (!res.ok) throw new Error('Failed to fetch audit log')
      return await res.json()
    } catch (e) {
      console.error('API Error:', e)
      return { entries: [] }
    }
  }

  const fetchUser = async () => {
    if (!token.value) return null
    try {
      const res = await fetchWithAuth(`${baseUrl}/auth/user`)
      if (res.ok) {
        user.value = await res.json()
        return user.value
      } else {
        token.value = null
        if (typeof window !== 'undefined') localStorage.removeItem('wares-token')
        return null
      }
    } catch (e) {
      return null
    }
  }

  const login = () => {
    if (typeof window !== 'undefined') {
      // Create a login request to the registry
      const loginUrl = `${baseUrl}/auth/oidc/login`
      fetch(loginUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider: 'github', redirect_uri: window.location.origin + '/login' })
      }).then(res => res.json()).then(data => {
        if (data.auth_url) {
          window.location.href = data.auth_url
        }
      })
    }
  }

  const logout = () => {
    token.value = null
    user.value = null
    if (typeof window !== 'undefined') localStorage.removeItem('wares-token')
  }

  return {
    fetchIndex,
    searchWares,
    getWare,
    getAudit,
    getProof,
    fetchUser,
    login,
    logout,
    token,
    user
  }
}
