import { expect, test, type Route } from "@playwright/test";

import {
  addDialog,
  addItem,
  closeItem,
  HEAP_URL,
  itemPane,
  openItem,
  PASSWORD as password,
  registerThroughApi,
  row,
  signOut,
  uniqueUser,
} from "./fixtures";

test("a new user registers, signs out and signs back in", async ({ page }) => {
  const { email, username } = uniqueUser();

  await page.goto("/");
  // A plain visit has nowhere to return to, so sign-in's address carries no `redirect`.
  await expect(page).toHaveURL(/\/login$/);
  await page.getByRole("tab", { name: "Create account" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Ada");
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  // Registering creates the user's personal team, named by the username, with a first heap, and opens it.
  await expect(page).toHaveURL(new RegExp(`/teams/${username}/heaps/personal$`));
  await expect(page.getByRole("heading", { name: "Personal" })).toBeVisible();
  await expect(page.getByText("Nothing here yet. Add the first thing on your plate.")).toBeVisible();

  await signOut(page);
  await expect(page).toHaveURL(/\/login$/);
  // Sent empty, as before a password manager fills it: the server says which input, and counts no attempt.
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByLabel("Email")).toHaveAccessibleDescription("Enter your email address.");
  await expect(page.getByLabel("Email")).toBeFocused();
  await page.getByLabel("Password").fill("not the password");
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Sign in" }).click();
  // A wrong password isn't about one input, so it shows above the button.
  await expect(page.getByRole("alert")).toHaveText("That email and password don't match an account.");

  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(HEAP_URL);
  await expect(page.getByRole("heading", { name: "Personal" })).toBeVisible();
});

test("a taken email and a short password show their errors under the field", async ({ page }) => {
  const { email } = await registerThroughApi(page);
  await page.context().clearCookies();

  await page.goto("/login");
  await page.getByRole("tab", { name: "Create account" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Ada");
  await page.getByLabel("Username").fill(uniqueUser().username);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  const emailInput = page.getByLabel("Email");
  await expect(emailInput).toHaveAttribute("aria-invalid", "true");
  await expect(emailInput).toHaveAccessibleDescription("There's already an account with this email. Sign in instead.");
  await expect(emailInput).toBeFocused();
  // An alert, so it's announced also when focus was already in the input.
  await expect(page.getByRole("alert")).toHaveText("There's already an account with this email. Sign in instead.");

  // Editing the email clears its error.
  await emailInput.fill(uniqueUser().email);
  await expect(emailInput).not.toHaveAttribute("aria-invalid");

  const passwordInput = page.getByLabel("Password");
  await expect(passwordInput).toHaveAccessibleDescription("At least 10 characters");
  await passwordInput.fill("short");
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(passwordInput).toHaveAttribute("aria-invalid", "true");
  await expect(passwordInput).toHaveAccessibleDescription("Use at least 10 characters.");
  await expect(passwordInput).toBeFocused();
});

test("an item link opened while signed out signs in and opens the item", async ({ page }) => {
  const { email } = await registerThroughApi(page);
  await page.goto("/");
  await expect(page).toHaveURL(HEAP_URL);
  await addItem(page, "Fix the gate");
  await openItem(page, "Fix the gate");
  const itemUrl = new URL(page.url());
  const key = itemUrl.pathname.split("/").at(-1) ?? "";
  await closeItem(page);
  await signOut(page);
  await expect(page).toHaveURL(/\/login$/);

  await page.goto(itemUrl.pathname + itemUrl.search);
  await expect(page).toHaveURL(/\/login\?redirect=/);
  await expect(page.getByText(`Sign in to open the link to ${key}.`)).toBeVisible();
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page).toHaveURL(itemUrl.href);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Fix the gate");
});

test("a session that ends mid-use returns to the page, with nothing left of the last user", async ({ page }) => {
  // B first, so the page ends up signed in as A.
  const userB = await registerThroughApi(page);
  await page.context().clearCookies();
  await registerThroughApi(page);
  await page.goto("/");
  await expect(page).toHaveURL(HEAP_URL);
  const titles = ["A's private plan", "A's other secret"];
  for (const title of titles) await addItem(page, title);

  // The session ends; the next request finds out.
  await page.context().clearCookies();
  await row(page, titles[0] ?? "")
    .getByRole("link")
    .first()
    .click();
  await expect(page).toHaveURL(/\/login\?redirect=/);
  const back = new URL(page.url()).searchParams.get("redirect") ?? "";
  expect(back).toMatch(/\/items\/[^/]+$/);

  // B's requests for heap data wait, so anything shown meanwhile could only come from the old cache.
  const held: Route[] = [];
  await page.route("**/api/v1/teams/**", (route) => void held.push(route));
  await page.getByLabel("Email").fill(userB.email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL((url) => url.pathname + url.search + url.hash === back);
  const showsA = () =>
    page.evaluate(
      (titles) =>
        titles.some(
          (title) =>
            document.body.innerText.includes(title) ||
            [...document.querySelectorAll("input, textarea")].some(
              (field) => (field as HTMLInputElement).value === title,
            ),
        ),
      titles,
    );
  expect(await showsA()).toBe(false);
  await expect.poll(() => held.length).toBeGreaterThan(0);
  expect(await showsA()).toBe(false);

  // Requests from now on go straight through.
  await page.route("**/api/v1/teams/**", (route) => route.continue());
  for (const route of held) await route.continue();
  await page.waitForLoadState("networkidle");
  // B isn't in A's team, so once the answers land the page still has nothing of A's to show.
  expect(await showsA()).toBe(false);
});

test("a write after the session ends goes to sign-in and back", async ({ page }) => {
  const { email } = await registerThroughApi(page);
  await page.goto("/");
  await expect(page).toHaveURL(HEAP_URL);
  const heapPath = new URL(page.url()).pathname;

  await page.getByRole("button", { name: "Add an item" }).click();
  await addDialog(page).getByLabel("Title").fill("Written too late");
  await addDialog(page).getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: "Expected" }).click();
  // Reads wait meanwhile, so the write is what finds the session gone, not a refetch.
  await page.route("**/api/v1/**", (route) => (route.request().method() === "GET" ? undefined : route.fallback()));
  await page.context().clearCookies();
  await addDialog(page).getByLabel("Title").press("Enter");
  await expect(page).toHaveURL(`/login?redirect=${encodeURIComponent(heapPath)}`);
  await page.unrouteAll({ behavior: "ignoreErrors" });

  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(heapPath);
});

test("a link's notice names an item only by a key's shape", async ({ page }) => {
  await page.goto("/login?redirect=/teams/a/heaps/b/items/Your%20account%20is%20locked");
  await expect(page.getByText("Sign in to open that page.")).toBeVisible();
  await expect(page.getByText(/account is locked/)).toHaveCount(0);
});

test("a redirect that isn't a page of the app is ignored", async ({ page, baseURL }) => {
  const { email } = await registerThroughApi(page);
  const origin = new URL(baseURL ?? "").origin;
  const payloads = [
    "//evil.example/steal",
    "https://evil.example",
    "/\\evil.example",
    // A raw tab, which the URL parser drops, leaving `//evil.example`.
    "/\t/evil.example",
    "/.//evil.example",
    "javascript:alert(1)",
  ];

  for (const payload of payloads) {
    await page.context().clearCookies();
    await page.goto(`/login?redirect=${encodeURIComponent(payload)}`);
    await expect(page.getByLabel("Email")).toBeVisible();
    await expect(page.getByText(/^Sign in to open/), payload).toHaveCount(0);
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();

    await expect(page, payload).toHaveURL(HEAP_URL);
    expect(new URL(page.url()).origin, payload).toBe(origin);
  }
});
