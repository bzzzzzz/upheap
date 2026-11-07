// End-to-end tests of the golden paths (design/technical-design.md, Testing §5), in Chromium against the real
// server binary with the built web app embedded. The server runs on a fresh in-memory SQLite database, and each
// test registers its own account, so tests share nothing and run in parallel.
import { defineConfig, devices } from "@playwright/test";

const port = 8139;
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "e2e",
  // The local build's own tests (playwright.local.config.ts).
  testIgnore: ["local/**"],
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL,
    // Due dates are calendar days in the browser's timezone; the tests compute them in UTC too.
    timezoneId: "UTC",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    // The web app is embedded at compile time, so it's built first.
    command: "npm run build && cd .. && cargo run --locked -q -p server",
    url: `${baseURL}/readyz`,
    timeout: 10 * 60 * 1000,
    reuseExistingServer: false,
    env: {
      DATABASE_URL: "sqlite::memory:",
      BASE_URL: baseURL,
      BIND_ADDR: `127.0.0.1:${port}`,
      LOG_FORMAT: "pretty",
      RUST_LOG: "warn",
    },
  },
});
