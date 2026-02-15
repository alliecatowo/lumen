import type { Config } from 'tailwindcss'

export default <Config>{
  content: [
    './components/**/*.{vue,js,ts}',
    './layouts/**/*.vue',
    './pages/**/*.vue',
    './app.vue'
  ],
  theme: {
    extend: {
      colors: {
        lumen: {
          bg: '#111111',
          bgSecondary: '#1a1a1a',
          surface: '#242424',
          border: '#333333',
          text: '#f5f5f5',
          textMuted: '#a0a0a0',
          accent: '#FF4FA3',
          accentHover: '#ff6fb8',
          accentSubtle: 'rgba(255, 79, 163, 0.15)'
        }
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Fira Code', 'monospace']
      }
    }
  },
  plugins: []
}
