// Connecting the CLI (design/architecture.md, Authentication and authorization §2): the consent page, as the browser
// half of the OAuth flow. The CLI's loopback listener is stood in for by routing its address, so no port has to be
// opened.
import { createHash, randomBytes } from "node:crypto";

import { expect, test, type APIRequestContext, type Page, type Request } from "@playwright/test";

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

/**
 * Signs the CLI in as the account the page is signed in to, naming its machine as `upheap auth login` does, and returns
 * its refresh token. `request` carries no cookie, as the CLI doesn't.
 */
async function signInCli(page: Page, request: APIRequestContext, deviceName: string) {
  const { verifier, challenge, state } = pkce();
  const approved = await page.request.post("/api/v1/oauth/authorize", {
    headers: { "x-csrf-protection": "1" },
    data: {
      response_type: "code",
      client_id: "upheap-cli",
      redirect_uri: REDIRECT_URI,
      code_challenge: challenge,
      code_challenge_method: "S256",
      state,
    },
  });
  const { redirect_to } = (await approved.json()) as { redirect_to: string };
  const tokens = await request.post("/api/v1/oauth/token", {
    form: {
      grant_type: "authorization_code",
      code: new URL(redirect_to).searchParams.get("code") ?? "",
      redirect_uri: REDIRECT_URI,
      client_id: "upheap-cli",
      code_verifier: verifier,
      device_name: deviceName,
    },
  });
  expect(tokens.status()).toBe(200);
  return ((await tokens.json()) as { refresh_token: string }).refresh_token;
}

test("Account lists where the CLI is signed in and signs one out after a confirm", async ({ page, request }) => {
  await registerThroughApi(page);
  const refreshToken = await signInCli(page, request, "ada-laptop");

  await page.goto("account");
  const card = page.getByRole("region", { name: "Command line" });
  await expect(card).toContainText("ada-laptop");
  await expect(card).toContainText(/Signed in \d{1,2} \w{3} · last used today/);

  // Cancel closes the confirm and puts focus back on the row's button.
  const signOut = card.getByRole("button", { name: "Sign out ada-laptop" });
  await signOut.click();
  const confirm = card.getByRole("group", { name: "Confirm sign-out" });
  await expect(confirm).toContainText("Sign out ada-laptop? The CLI there stops working until someone runs");
  await expect(confirm.getByRole("button", { name: "Sign out" })).toBeFocused();
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toHaveCount(0);
  await expect(signOut).toBeFocused();

  await signOut.click();
  await confirm.getByRole("button", { name: "Sign out" }).click();
  await expect(card.getByRole("status")).toHaveText("Signed out ada-laptop.");
  await expect(card.getByRole("heading", { name: "Command line" })).toBeFocused();
  await expect(card.getByRole("list")).toHaveCount(0);
  await expect(card).toContainText(
    "The CLI isn't signed in anywhere. To use it, install upheap and run upheap auth login --server http",
  );

  // The server ended it: the refresh token is refused.
  const refreshed = await request.post("/api/v1/oauth/token", {
    form: { grant_type: "refresh_token", refresh_token: refreshToken, client_id: "upheap-cli" },
  });
  expect(refreshed.status()).toBe(400);
  expect(((await refreshed.json()) as { error: string }).error).toBe("invalid_grant");
});

test("only one sign-in asks to be confirmed at a time, and an account with none says how to start", async ({
  page,
  request,
}) => {
  await registerThroughApi(page);
  await page.goto("account");
  await expect(page.getByRole("region", { name: "Command line" })).toContainText("The CLI isn't signed in anywhere.");
  await signInCli(page, request, "ada-laptop");
  await signInCli(page, request, "build-box");
  await page.reload();
  const card = page.getByRole("region", { name: "Command line" });
  await expect(card.getByRole("listitem")).toHaveCount(2);

  await card.getByRole("button", { name: "Sign out ada-laptop" }).click();
  await card.getByRole("button", { name: "Sign out build-box" }).click();

  await expect(card.getByRole("group", { name: "Confirm sign-out" })).toHaveCount(1);
  await expect(card.getByRole("group", { name: "Confirm sign-out" })).toContainText("Sign out build-box?");
  await expect(card.getByRole("button", { name: "Sign out ada-laptop" })).toBeVisible();
});
