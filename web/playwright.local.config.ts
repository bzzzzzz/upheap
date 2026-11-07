// The golden paths that apply to the local build (design/local-build.md), plus its own tests in e2e/local, at
// each width against the static build served as GitHub Pages serves it (scripts/pages-server.ts), under a base path.
// Each test's browser context starts with empty storage, so each gets a fresh local user and database. The backend
// must be built first (`npm run build:backend`).
import { defineConfig } from "@playwright/test";

import type { LocalBuildOptions } from "./e2e/fixtures";
import { projects } from "./e2e/projects";

const port = 8140;
const base = "/upheap/";
const baseURL = `http://127.0.0.1:${port}${base}`;

export default defineConfig<LocalBuildOptions>({
  testDir: "e2e",
  // Signing up and in is the server's.
  testIgnore: ["accounts.spec.ts"],
  outputDir: "test-results/local",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI
    ? [["github"], ["html", { open: "never", outputFolder: "playwright-report/local" }]]
    : "list",
  use: {
    baseURL,
    localBuild: true,
    timezoneId: "UTC",
    trace: "retain-on-failure",
  },
  projects,
  webServer: {
    command: "npm run build:local && node scripts/pages-server.ts",
    url: baseURL,
    timeout: 2 * 60 * 1000,
    reuseExistingServer: false,
    env: { UPHEAP_BASE: base, PORT: String(port) },
  },
});
