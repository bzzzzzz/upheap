import { expect, test, type Page } from "@playwright/test";

import { addItem, closeItem, HEAP_URL, itemPane, openItem, signOut, uniqueUser } from "./fixtures";

const password = "a long test password";

/** Registers a new account through the API, which signs the page in to it, and returns its email. */
async function registerThroughApi(page: Page): Promise<string> {
  const user = uniqueUser();
  const registered = await page.request.post("/api/v1/auth/register", {
    headers: { "x-csrf-protection": "1" },
    data: { ...user, display_name: "Tester", password },
  });
  expect(registered.status()).toBe(201);
  return user.email;
}

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
  const email = await registerThroughApi(page);
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
  await expect(page.getByRole("alert")).toHaveCount(0);

  // Editing the email clears its error.
  await emailInput.fill(uniqueUser().email);
  await expect(emailInput).not.toHaveAttribute("aria-invalid");

  const passwordInput = page.getByLabel("Password");
  await expect(passwordInput).toHaveAccessibleDescription("At least 10 characters");
  await passwordInput.fill("short");
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(passwordInput).toHaveAttribute("aria-invalid", "true");
  await expect(passwordInput).toHaveAccessibleDescription("Password must be at least 10 characters.");
  await expect(passwordInput).toBeFocused();
});

test("an item link opened while signed out signs in and opens the item", async ({ page }) => {
  const email = await registerThroughApi(page);
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

test("a redirect to another site is ignored", async ({ page, baseURL }) => {
  const email = await registerThroughApi(page);
  await page.context().clearCookies();

  await page.goto("/login?redirect=//evil.example/steal");
  await expect(page.getByText(/^Sign in to open/)).toHaveCount(0);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page).toHaveURL(HEAP_URL);
  expect(new URL(page.url()).origin).toBe(new URL(baseURL ?? "").origin);
});
