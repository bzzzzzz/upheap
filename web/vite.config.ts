import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defaultClientConditions, defineConfig } from "vite";

// In development the API server runs separately; the dev server forwards API calls to it.
const apiTarget = process.env.API_PROXY_TARGET ?? "http://localhost:8137";

// `--mode local-build` builds the local build (design/local-build.md): the backend runs in a worker in the page, so the
// `local-build` condition points `#api-fetch` at it (package.json `imports`). It needs the backend built into
// src/local/backend first (`npm run build:backend`). GitHub Pages serves it under the repository's name, which
// UPHEAP_BASE gives, such as `/upheap/`.
export default defineConfig(({ mode }) => {
  const local = mode === "local-build";
  return {
    base: local ? (process.env.UPHEAP_BASE ?? "/") : "/",
    plugins: [react(), tailwindcss()],
    resolve: { conditions: local ? ["local-build", ...defaultClientConditions] : [...defaultClientConditions] },
    worker: { format: "es" },
    // SQLite-wasm finds its .wasm next to its own module, which pre-bundling would move.
    optimizeDeps: { exclude: ["@sqlite.org/sqlite-wasm"] },
    server: local
      ? {}
      : {
          proxy: {
            "/api": apiTarget,
            "/healthz": apiTarget,
            "/readyz": apiTarget,
          },
        },
  };
});
