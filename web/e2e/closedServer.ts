// A server whose registration is closed, with two accounts that already exist: how a real server gets to the state
// the invite page is for (design/ux.md, Invite page). The suite's main server (playwright.config.ts) has
// registration open, and a golden path that must prove an invite link is the only way in needs one that doesn't.
import { test as base } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { PASSWORD, uniqueUser } from "./fixtures";

/** An account the server was seeded with. The password is `PASSWORD`. */
export type SeededUser = { display_name: string; email: string; username: string };

export type ClosedServer = {
  /** Where the server listens, such as `http://127.0.0.1:41234`. */
  baseURL: string;
  /** Ana Ruiz, who is to make the team and its link. */
  admin: SeededUser;
  /** Sam Ortiz, who has an account and no team of Ana's. */
  member: SeededUser;
};

/** The server binary `npm run e2e`'s web server has built by the time a worker starts (cargo's target directory). */
const REPOSITORY_ROOT = resolve("..");
const BINARY = resolve(REPOSITORY_ROOT, process.env.CARGO_TARGET_DIR ?? "target", "debug", "upheap-server");

/** A port nothing listens on, asked of the OS, so a server left over from an earlier run is never mistaken for ours. */
function findFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address ? address.port : 0;
      probe.close(() => resolve(port));
    });
  });
}

function start(baseURL: string, databaseUrl: string, allowRegistration: boolean): ChildProcess {
  const url = new URL(baseURL);
  return spawn(BINARY, [], {
    stdio: ["ignore", "ignore", "inherit"],
    env: {
      ...process.env,
      ALLOW_REGISTRATION: String(allowRegistration),
      DATABASE_URL: databaseUrl,
      BIND_ADDR: `${url.hostname}:${url.port}`,
      BASE_URL: baseURL,
      LOG_FORMAT: "pretty",
      RUST_LOG: "warn",
    },
  });
}

async function waitUntilReady(baseURL: string, server: ChildProcess) {
  const exited = new Promise<never>((_, reject) =>
    server.once("exit", (code) => reject(new Error(`the server exited with ${code} before it was ready`))),
  );
  const ready = (async () => {
    for (let attempt = 0; attempt < 300; attempt++) {
      try {
        if ((await fetch(`${baseURL}/readyz`)).ok) return;
      } catch {
        // Not listening yet.
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error(`the server at ${baseURL} wasn't ready after 30 s`);
  })();
  await Promise.race([ready, exited]);
}

async function stop(server: ChildProcess) {
  if (server.exitCode !== null || server.signalCode !== null) return;
  const exited = new Promise((resolve) => server.once("exit", resolve));
  server.kill("SIGTERM");
  await exited;
}

async function register(baseURL: string, display_name: string): Promise<SeededUser> {
  const user = { ...uniqueUser(), display_name };
  const response = await fetch(`${baseURL}/api/v1/auth/register`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-csrf-protection": "1" },
    body: JSON.stringify({ ...user, password: PASSWORD }),
  });
  if (response.status !== 201) throw new Error(`seeding ${display_name} answered ${response.status}`);
  return user;
}

/**
 * `closedServer`, one per worker, started the first time a test asks for it: the real binary on a free port the
 * OS picked and a SQLite file in the OS temp directory, deleted at the end. It starts with
 * registration open, registers Ana Ruiz and Sam Ortiz, stops, and starts again on the same file with registration
 * closed, as a production server got there.
 *
 * The binary is the one the suite's web server built (`npm run e2e` builds the web app and embeds it, then builds the
 * server, before any worker starts); without it the fixture fails and says so. Tests on it go to absolute URLs, with a
 * browser context of their own per person (`baseURL` in `browser.newContext`).
 */
export const test = base.extend<object, { closedServer: ClosedServer }>({
  closedServer: [
    // Playwright reads a fixture's needs from its destructured first argument, and this one needs none.
    // eslint-disable-next-line no-empty-pattern
    async ({}, use) => {
      if (!existsSync(BINARY)) {
        throw new Error(`${BINARY} is missing. The suite's web server builds it: run the tests with \`npm run e2e\`.`);
      }
      const baseURL = `http://127.0.0.1:${await findFreePort()}`;
      const directory = await mkdtemp(join(tmpdir(), "upheap-e2e-"));
      const databaseUrl = `sqlite://${join(directory, "heap.db")}?mode=rwc`;
      let server: ChildProcess | undefined;
      try {
        server = start(baseURL, databaseUrl, true);
        await waitUntilReady(baseURL, server);
        const admin = await register(baseURL, "Ana Ruiz");
        const member = await register(baseURL, "Sam Ortiz");
        await stop(server);

        server = start(baseURL, databaseUrl, false);
        await waitUntilReady(baseURL, server);
        await use({ baseURL, admin, member });
      } finally {
        if (server) await stop(server);
        await rm(directory, { recursive: true, force: true });
      }
    },
    { scope: "worker", timeout: 60_000 },
  ],
});
