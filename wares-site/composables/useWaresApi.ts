export const useWaresApi = () => {
  const config = useRuntimeConfig()
  const baseUrl = config.public.apiBase || 'https://wares.lumen-lang.com/v1'

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

  return {
    fetchIndex,
    searchWares,
    getWare
  }
}
