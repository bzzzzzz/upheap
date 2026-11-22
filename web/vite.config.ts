import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type ProxyOptions } from "vite";

// In development the API server runs separately; the dev server forwards API calls to it. `API_PROXY_TARGET` may name
// a deployed server (`https://…`), to try local web changes against its data.
const apiTarget = process.env.API_PROXY_TARGET ?? "http://localhost:8137";

const api: ProxyOptions = {
  target: apiTarget,
  // A deployed server sits behind a proxy that routes by host name, so the request names the target's host.
  changeOrigin: true,
  // An https server marks its session cookie `Secure`, which not every browser keeps for http://localhost.
  configure: (proxy) => {
    proxy.on("proxyRes", (response) => {
      const cookies = response.headers["set-cookie"];
      if (cookies) response.headers["set-cookie"] = cookies.map((cookie) => cookie.replace(/; Secure/i, ""));
    });
  },
};

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: { "/api": api, "/healthz": api, "/readyz": api },
  },
});
