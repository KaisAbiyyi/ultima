import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [sveltekit()],

  // Optimize dependencies to prevent memory leaks during build
  optimizeDeps: {
    include: ["flowbite-svelte", "flowbite-svelte-icons"],
  },

  // Build optimizations to reduce memory usage
  build: {
    // Disable source maps in production to save memory
    sourcemap: false,
    // Enable minification
    minify: "terser",
    // Reduce chunk size warning threshold
    chunkSizeWarningLimit: 1000,
    // Rollup options
    rollupOptions: {
      output: {
        // Manual chunks to separate vendor code
        manualChunks(id) {
          if (id.includes("@tauri-apps")) {
            return "vendor";
          }
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
});
