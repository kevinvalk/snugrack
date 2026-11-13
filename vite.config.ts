import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import feedback from './feedback/vite.ts';

export default defineConfig({
  base: './',
  plugins: [tailwindcss(), svelte(), feedback()],
  // manifold-3d locates its .wasm via import.meta.url; pre-bundling would break that.
  optimizeDeps: { exclude: ['manifold-3d'] },
  worker: { format: 'es' },
  // The web app shares its root with the Rust workspace; cargo's output is none of its business.
  server: { watch: { ignored: ['**/target/**'] } },
});
