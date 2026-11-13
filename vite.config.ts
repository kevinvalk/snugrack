import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import feedback from './feedback/vite.ts';

// The e2e builds its own core into SNUGRACK_WASM, away from ui/wasm, which a running dev server serves.
const wasm = process.env.SNUGRACK_WASM;

// Every commit to main is a release: its version is when it was made, Amsterdam time, as YYYY.MM.DD.HHmm.
const made = new Date(Number(process.env.SOURCE_DATE_EPOCH ?? execFileSync('git', ['log', '-1', '--pretty=%ct'])) * 1000);
const at = Object.fromEntries(
  new Intl.DateTimeFormat('en', { timeZone: 'Europe/Amsterdam', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })
    .formatToParts(made)
    .map(({ type, value }) => [type, value]),
);

export default defineConfig({
  base: './',
  define: { 'import.meta.env.VERSION': JSON.stringify(`${at.year}.${at.month}.${at.day}.${at.hour}${at.minute}`) },
  resolve: { alias: wasm ? [{ find: /^(\.\.?\/)+wasm\//, replacement: `${resolve(wasm)}/` }] : [] },
  plugins: [tailwindcss(), svelte(), feedback()],
  // manifold-3d locates its .wasm via import.meta.url; pre-bundling would break that.
  optimizeDeps: { exclude: ['manifold-3d'] },
  worker: { format: 'es' },
  // The web app shares its root with the Rust workspace; cargo's output is none of its business.
  server: { watch: { ignored: ['**/target/**'] } },
});
