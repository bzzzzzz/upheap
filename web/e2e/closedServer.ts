// A server whose registration is closed, with six accounts that already exist: how a real server gets to the state
// the invite page is for (design/ux.md, Invite page). The third is for the reset page's spec, which changes its
// password; it can make reset links the way the operator does, with the server's own command (design/ux.md, Reset
// page). The fourth is for the email change spec, which changes its address. The fifth is an operator, made one with
// the server's own command, and the sixth is the account the Accounts spec turns off and on (design/ux.md,
// Accounts). The suite's main server (playwright.config.ts) has
// registration open, and a golden path that must prove an invite link is the only way in needs one that doesn't.
import { expect, test as base } from "@playwright/test";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readdir, readFile, rm, stat } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { PASSWORD, uniqueUser } from "./fixtures";

/** The contact line the closed server's operator set, which the pages that say to ask whoever runs the server show. */
export const OPERATOR_CONTACT = "Ask Sam in #it, or write to it@example.com";

/** An account the server was seeded with. The password is `PASSWORD`. */
export type SeededUser = { display_name: string; email: string; username: string };

export type ClosedServer = {
  /** Where the server listens, such as `http://127.0.0.1:41234`. */
  baseURL: string;
  /** Ana Ruiz, who is to make the team and its link. */
  admin: SeededUser;
  /** Sam Ortiz, who has an account and no team of Ana's. */
  member: SeededUser;
  /** Mia Chen, whose password the reset spec changes, and puts back to `PASSWORD` when it is done. */
  forgetful: SeededUser;
  /** Eli Park, whose email address the email change spec changes. It signs in by username, which stays. */
  mover: SeededUser;
  /** Ivy Stone, an operator of the server. */
  operator: SeededUser;
  /**
   * Makes a reset link for `login` (a username or an email address) by running the server's `user reset-link`
   * command against its database, and returns the link. Fails unless the command exits 0.
   */
  resetLink: (login: string) => Promise<string>;
  /**
   * Waits for the newest mail to `email` that holds a link under `path` (`reset`, `change-email` or `confirm`) in the server's
   * mail directory (its mail goes to files there), and returns the link. `after` is the time the person asked, so an
   * earlier mail isn't taken.
   */
  mailedLink: (email: string, after: Date, path: MailedLinkPath) => Promise<string>;
};

/** What comes before the token in a link the server mails. */
export type MailedLinkPath = "reset" | "change-email" | "confirm";

/** The server binary `npm run e2e`'s web server has built by the time a worker starts (cargo's target directory). */
const REPOSITORY_ROOT = fileURLToPath(new URL("../..", import.meta.url));
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

function serverEnv(baseURL: string, databaseUrl: string, allowRegistration: boolean, mailDirectory?: string) {
  const url = new URL(baseURL);
  return {
    ...process.env,
    ALLOW_REGISTRATION: String(allowRegistration),
    DATABASE_URL: databaseUrl,
    BIND_ADDR: `${url.hostname}:${url.port}`,
    BASE_URL: baseURL,
    LOG_FORMAT: "pretty",
    UPHEAP_OPERATOR_CONTACT: OPERATOR_CONTACT,
    RUST_LOG: "warn",
    ...(mailDirectory && { MAIL_URL: `file://${mailDirectory}`, MAIL_FROM: "Upheap <upheap@example.com>" }),
  };
}

function start(baseURL: string, databaseUrl: string, allowRegistration: boolean, mailDirectory?: string): ChildProcess {
  return spawn(BINARY, [], {
    stdio: ["ignore", "ignore", "inherit"],
    env: serverEnv(baseURL, databaseUrl, allowRegistration, mailDirectory),
  });
}

/** The operator's command: the link is all it prints on stdout, and a non-zero exit rejects. */
async function makeResetLink(baseURL: string, databaseUrl: string, login: string): Promise<string> {
  const { stdout } = await promisify(execFile)(BINARY, ["user", "reset-link", login], {
    env: serverEnv(baseURL, databaseUrl, false),
  });
  return stdout.trim();
}

/** The operator's command that makes `login` an operator, which the server's own database file lets run beside it. */
async function makeOperator(baseURL: string, databaseUrl: string, login: string): Promise<void> {
  await promisify(execFile)(BINARY, ["user", "make-operator", login], { env: serverEnv(baseURL, databaseUrl, false) });
}

/**
 * The link under `path` in the newest mail to `email` written after `after`, or `undefined` while there is none. The
 * file transport writes quoted-printable: soft line breaks and `=3D` are undone far enough to read a link.
 */
async function findMailedLink(
  mailDirectory: string,
  email: string,
  after: Date,
  path: MailedLinkPath,
): Promise<string | undefined> {
  const mails: { text: string; written: number }[] = [];
  for (const name of await readdir(mailDirectory)) {
    const path = join(mailDirectory, name);
    const written = (await stat(path)).mtimeMs;
    if (written < after.getTime()) continue;
    const text = (await readFile(path, "utf8")).replace(/=\r?\n/g, "").replace(/=3D/g, "=");
    if (text.includes(`To: ${email}`)) mails.push({ text, written });
  }
  const newest = mails.sort((a, b) => b.written - a.written)[0];
  return newest?.text.match(new RegExp(`https?://\\S+/${path}/[A-Za-z0-9_-]+`))?.[0];
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
 * OS picked and a SQLite file and a mail directory in the OS temp directory, deleted at the end. It starts with
 * registration open, registers Ana Ruiz, Sam Ortiz, Mia Chen, Eli Park, Ivy Stone and Ola Berg, stops, makes Ivy an operator, and starts again on the same file with
 * registration closed and its mail written to files, as a production server got there, with mail.
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
      // Mail goes to files here, and is removed with the database.
      const mailDirectory = join(directory, "mail");
      await mkdir(mailDirectory);
      const databaseUrl = `sqlite://${join(directory, "heap.db")}?mode=rwc`;
      let server: ChildProcess | undefined;
      try {
        server = start(baseURL, databaseUrl, true);
        await waitUntilReady(baseURL, server);
        const admin = await register(baseURL, "Ana Ruiz");
        const member = await register(baseURL, "Sam Ortiz");
        const forgetful = await register(baseURL, "Mia Chen");
        const mover = await register(baseURL, "Eli Park");
        const operator = await register(baseURL, "Ivy Stone");
        // Ola Berg, whom the Accounts spec turns off and on, and puts back on when it is done.
        await register(baseURL, "Ola Berg");
        await stop(server);
        await makeOperator(baseURL, databaseUrl, operator.username);

        server = start(baseURL, databaseUrl, false, mailDirectory);
        await waitUntilReady(baseURL, server);
        await use({
          baseURL,
          admin,
          member,
          forgetful,
          mover,
          operator,
          resetLink: (login) => makeResetLink(baseURL, databaseUrl, login),
          mailedLink: async (email, after, path) => {
            await expect.poll(() => findMailedLink(mailDirectory, email, after, path)).toBeDefined();
            return (await findMailedLink(mailDirectory, email, after, path)) ?? "";
          },
        });
      } finally {
        if (server) await stop(server);
        await rm(directory, { recursive: true, force: true });
      }
    },
    { scope: "worker", timeout: 60_000 },
  ],
});
