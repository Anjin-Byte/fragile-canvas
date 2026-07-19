import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'

export default defineConfig({
  plugins: [svelte(), wasm()],
  base: process.env.GITHUB_ACTIONS ? '/fragile-canvas/' : '/',
  server: {
    fs: {
      allow: [
        '.',
        '../wasm/pkg',
        '../ui',
        '../phi',
      ],
    },
  },
  // Treat the linked ui and phi packages as source (not pre-bundled)
  // so the Svelte plugin processes their .svelte/.svelte.ts files
  optimizeDeps: {
    exclude: ['@fragile-canvas/ui', '@gestalt/phi'],
  },
  ssr: {
    noExternal: ['@fragile-canvas/ui', '@gestalt/phi'],
  },
})
