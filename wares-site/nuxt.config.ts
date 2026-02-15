export default defineNuxtConfig({
  devtools: { enabled: false },
  compatibilityDate: '2024-11-01',
  ssr: true,

  app: {
    head: {
      title: 'Lumen Wares',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
        { name: 'description', content: 'The Lumen package registry - discover and share Lumen wares' },
        { name: 'theme-color', content: '#111111' }
      ],
      link: [
        { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }
      ]
    }
  },

  modules: ['@nuxtjs/tailwindcss'],

  tailwindcss: {
    cssPath: '~/assets/css/main.css',
    configPath: 'tailwind.config.ts'
  },

  runtimeConfig: {
    public: {
      apiBase: 'https://wares-registry.alliecatowo.workers.dev/v1'
    }
  },

  nitro: {
    preset: 'static'
  }
})
