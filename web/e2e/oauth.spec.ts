// Connecting the CLI (design/cli.md, Signing in): the consent page, as the browser half of the OAuth flow. The CLI's
// loopback listener is stood in for by routing its address, so no port has to be opened.
import { createHash, randomBytes } from "node:crypto";

import { expect, test, type Page, type Request } from "@playwright/test";

import { PASSWORD as password, registerThroughApi } from "./fixtures";

const REDIRECT_URI = "http://127.0.0.1:53682/callback";

/** A fresh PKCE verifier, its S256 challenge and a state, as the CLI makes them. */
function pkce() {
  const verifier = randomBytes(32).toString("base64url");
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  return { verifier, challenge, state: randomBytes(16).toString("base64url") };
}

function authorizePath(challenge: string, state: string, overrides: Record<string, string> = {}) {
  const query = new URLSearchParams({
    response_type: "code",
    client_id: "upheap-cli",
    redirect_uri: REDIRECT_URI,
    code_challenge: challenge,
    code_challenge_method: "S256",
    state,
    ...overrides,
  });
  return `/oauth/authorize?${query.toString()}`;
}

/**
 * Answers the browser's visit to the CLI's redirect URI with `status`, and returns the visit's query once it comes.
 * A `204` leaves the browser on the consent page, as when the CLI has stopped listening.
 */
function catchRedirect(page: Page, status = 200): Promise<URLSearchParams> {
  return new Promise((resolve) => {
    void page.route(`${REDIRECT_URI}**`, async (route) => {
      const request: Request = route.request();
      resolve(new URL(request.url()).searchParams);
      await route.fulfill({ status, contentType: "text/plain", body: status === 204 ? undefined : "Signed in." });
    });
  });
}

test("the CLI is connected after signing in, and its token reads the account", async ({ page, request }) => {
  const { email, username } = await registerThroughApi(page);
  await page.context().clearCookies();
  const { verifier, challenge, state } = pkce();
  const redirected = catchRedirect(page);

  await page.goto(authorizePath(challenge, state));
  await expect(page).toHaveURL(/\/login\?redirect=/);
  await expect(page.getByText("Sign in to connect Upheap CLI.")).toBeVisible();
  await page.getByLabel("Email or username").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page.getByRole("heading", { name: "Allow Upheap CLI to use your account?" })).toBeVisible();
  await expect(page.getByText(`Signed in as Tester · @${username}`)).toBeVisible();
  await page.getByRole("button", { name: "Allow" }).click();
  const query = await redirected;
  expect(query.get("state")).toBe(state);
  const code = query.get("code") ?? "";
  expect(code).not.toBe("");

  // The CLI's half: the code and verifier get a token, which reads the account with nothing but the header.
  const tokens = await request.post("/api/v1/oauth/token", {
    form: {
      grant_type: "authorization_code",
      code,
      redirect_uri: REDIRECT_URI,
      client_id: "upheap-cli",
      code_verifier: verifier,
    },
  });
  expect(tokens.status()).toBe(200);
  const { access_token: accessToken } = (await tokens.json()) as { access_token: string };
  const me = await request.get("/api/v1/me", { headers: { authorization: `Bearer ${accessToken}` } });
  expect(me.status()).toBe(200);
  expect(((await me.json()) as { user: { username: string } }).user.username).toBe(username);
});

test("Cancel tells the CLI the user said no", async ({ page }) => {
  await registerThroughApi(page);
  const { challenge, state } = pkce();
  // The CLI has stopped listening, so the browser stays on the page.
  const redirected = catchRedirect(page, 204);

  await page.goto(authorizePath(challenge, state));
  await page.getByRole("button", { name: "Cancel" }).click();

  const query = await redirected;
  expect(query.get("error")).toBe("access_denied");
  expect(query.get("state")).toBe(state);
  expect(query.has("code")).toBe(false);
  await expect(page.getByText("You can close this tab and go back to the terminal.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Allow" })).toHaveCount(0);
});

test("a request the server refuses shows why and offers nothing", async ({ page }) => {
  await registerThroughApi(page);
  const { challenge, state } = pkce();

  await page.goto(authorizePath(challenge, state, { redirect_uri: "https://evil.example/callback" }));

  await expect(page.getByRole("alert")).toHaveText(
    "This app asked to send your sign-in somewhere other than this computer, so it can't be connected.",
  );
  await expect(page.getByRole("button")).toHaveCount(0);
});

test("a state that reads as JSON comes back exactly as sent, through sign-in too", async ({ page }) => {
  const { email } = await registerThroughApi(page);
  await page.context().clearCookies();
  const { challenge } = pkce();

  // Signed out: the request goes through sign-in and back.
  let redirected = catchRedirect(page);
  await page.goto(authorizePath(challenge, "123456"));
  await expect(page).toHaveURL(/\/login\?redirect=/);
  await page.getByLabel("Email or username").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.getByRole("button", { name: "Allow" }).click();
  expect((await redirected).get("state")).toBe("123456");

  // Signed in, values the router would rewrite: 1e3 as 1000, "abc" without its quotes.
  for (const state of ["1e3", '"abc"']) {
    await page.unrouteAll();
    redirected = catchRedirect(page);
    await page.goto(authorizePath(challenge, state));
    await page.getByRole("button", { name: "Allow" }).click();
    expect((await redirected).get("state")).toBe(state);
  }
});

test("Use another account signs out and comes back as the other user", async ({ page }) => {
  const other = await registerThroughApi(page);
  await page.context().clearCookies();
  await registerThroughApi(page);
  const { challenge, state } = pkce();

  await page.goto(authorizePath(challenge, state));
  await expect(page.getByText(`@${other.username}`)).toHaveCount(0);
  await page.getByRole("button", { name: "Use another account" }).click();
  await expect(page).toHaveURL(/\/login\?redirect=/);
  await expect(page.getByText("Sign in to connect Upheap CLI.")).toBeVisible();
  await page.getByLabel("Email or username").fill(other.email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page.getByText(`Signed in as Tester · @${other.username}`)).toBeVisible();
  await expect(page.getByRole("button", { name: "Allow" })).toBeVisible();
});
