import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import path from "path";

const host = process.env.TAURI_DEV_HOST;
const port = Number(process.env.CARDBE_DEV_PORT ?? "1420");
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error("CARDBE_DEV_PORT must be a port between 1 and 65535");
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [tailwindcss(), sveltekit()],
  ...(port !== 1420 ? { cacheDir: `node_modules/.vite-cardbe-${port}` } : {}),
  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    // Let the HMR client use the origin serving /@vite/client, including the
    // LAN share proxy. HTTP and WebSocket use the same upstream port.
    hmr: { timeout: 30000 },
    watch: { // 3. tell Vite to ignore watching `src-tauri`
    ignored: ["**/src-tauri/**"] }
  },
  resolve: {
    alias: {
      $lib: path.resolve("./src/lib"),
    },
  },
  build: {
    // Some on-demand Shiki grammars and PDF fontkit are single modules above
    // Vite's default 500 kB advisory threshold.
    chunkSizeWarningLimit: 800,
  },
}));
