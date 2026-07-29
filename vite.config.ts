import { defineConfig } from "vite";

// ponytail: @kvnwolf/dobby/vite preset inlined (resolve.tsconfigPaths +
// server.allowedHosts) — the file:-linked dobby resolves 'vite' from its REAL
// path at config-load time, where vite deliberately isn't installed. Restore
// `mergeConfig(dobbyVite, …)` once dobby installs from a registry.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  // prevent Vite from obscuring rust errors
  clearScreen: false,
  // vite@8 resolves tsconfig path aliases natively
  resolve: { tsconfigPaths: true },
  server: {
    // portless serves the app through per-worktree custom hostnames
    allowedHosts: true,
    hmr: host ? { host, port: 1421, protocol: "ws" } : undefined,
    host,
    // tauri expects a fixed port, fail if that port is not available
    port: 1420,
    strictPort: true,
    // tell Vite to ignore watching `src-tauri`
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
