import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// In development the API server runs separately; the dev server forwards API calls to it.
const apiTarget = process.env.API_PROXY_TARGET ?? "http://localhost:8137";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": apiTarget,
      "/healthz": apiTarget,
      "/readyz": apiTarget,
    },
  },
});
