import tailwindcss from "@tailwindcss/vite";
import { defineConfig, lazyPlugins } from "vite-plus";
import { sveltekit } from "@sveltejs/kit/vite";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import path from "path";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const host = process.env.TAURI_DEV_HOST;
const port = Number(process.env.CARDBE_DEV_PORT ?? "1420");
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error("CARDBE_DEV_PORT must be a port between 1 and 65535");
}
const isCustomPort = port !== 1420;
const ownGeneratedDir = `/.svelte-kit-dev-${port}/`;
const projectRoot = path.resolve(".").replaceAll("\\", "/");

// Keep the editor's base config available when only a custom-port instance runs.
if (isCustomPort && !existsSync(".svelte-kit/tsconfig.json")) {
  execFileSync(
    process.execPath,
    [path.resolve("node_modules/@sveltejs/kit/svelte-kit.js"), "sync"],
    { stdio: "inherit", env: { ...process.env, CARDBE_DEV_PORT: "1420" } },
  );
}

// https://vite.dev/config/
export default defineConfig({
  lint: {
    categories: { correctness: "error" },
    rules: { curly: "error", "typescript/no-this-alias": "off" },
    ignorePatterns: ["src-tauri/plugin-api/**"],
  },
  fmt: {
    printWidth: 80,
    sortPackageJson: false,
    svelte: {},
    ignorePatterns: [
      "pnpm-lock.yaml",
      "src/lib/paraglide/**",
      "src-tauri/plugin-api/**",
    ],
  },
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
  },
  plugins: lazyPlugins(() => [
    tailwindcss(),
    paraglideVitePlugin({
      project: "./project.inlang",
      outdir: "./src/lib/paraglide",
      outputStructure: "locale-modules",
      emitTsDeclarations: true,
      strategy: ["custom-cardbe", "baseLocale"],
    }),
    sveltekit(),
  ]),
  optimizeDeps: {
    // Keep late-discovered imports from rebuilding shared chunks and reloading
    // all desktop windows. Prebundle the CommonJS PDF dependencies explicitly.
    noDiscovery: true,
    include: [
      "@tauri-apps/api/dpi",
      "@tauri-apps/api/window",
      "pdf-lib",
      "@pdf-lib/fontkit",
    ],
  },
  // Custom ports use their own generated config and dependency cache.
  ...(isCustomPort
    ? {
        cacheDir: `node_modules/.vite-cardbe-${port}`,
        tsconfig: path.resolve(`.svelte-kit-dev-${port}/tsconfig.json`),
      }
    : {}),
  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || "127.0.0.1",
    // Let the HMR client use the origin serving /@vite/client, including the
    // LAN share proxy. HTTP and WebSocket use the same upstream port.
    hmr: { timeout: 30000 },
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: /** @param {string} filePath */ (filePath) => {
        const normalizedPath = filePath.replaceAll("\\", "/");
        return (
          normalizedPath.includes("/src-tauri/") ||
          normalizedPath.startsWith(`${projectRoot}/.cardbe-debug/`) ||
          normalizedPath.startsWith(`${projectRoot}/build/`) ||
          (isCustomPort && normalizedPath.includes("/.svelte-kit/")) ||
          (normalizedPath.includes("/.svelte-kit-dev-") &&
            !normalizedPath.includes(ownGeneratedDir))
        );
      },
    },
    warmup: {
      clientFiles: ["./src/routes/+layout.svelte", "./src/routes/+page.svelte"],
    },
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
});
