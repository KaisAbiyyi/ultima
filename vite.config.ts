import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  optimizeDeps: { entries: ['index.html'] },
  server: {
    watch: { ignored: ['**/src-tauri/target/**', '**/crates/**/target/**', '**/tools/**/target/**', '**/artifacts/**'] }
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node'
  }
});
