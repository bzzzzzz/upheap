// The Password card on Account (design/ux.md, Password): changing the password, which ends the user's other sign-ins.
import { expect, test, type Page } from "@playwright/test";

import { goTo, HEAP_URL, PASSWORD, registerThroughApi } from "./fixtures";

const NEW_PASSWORD = "another long password";

/** A new account's heap, signed in. */
async function startSignedIn(page: Page) {
  const user = await registerThroughApi(page);
  await page.goto("/");
  await expect(page).toHaveURL(HEAP_URL);
  return user;
}

/** The Password card, opened from the Account page. */
async function openCard(page: Page) {
  await goTo(page, "Account");
  const card = page.getByRole("region", { name: "Password", exact: true });
  await card.getByRole("button", { name: "Change password" }).click();
  return card;
}

test("a user changes their password and signs in with the new one", async ({ page }) => {
  const { email } = await startSignedIn(page);
  await goTo(page, "Account");
  const card = page.getByRole("region", { name: "Password", exact: true });
  // Closed, the card is its heading and the button.
  await expect(card.getByLabel("Current password")).toHaveCount(0);

  await card.getByRole("button", { name: "Change password" }).click();
  await expect(card.getByLabel("Current password")).toBeFocused();
  await expect(card.getByLabel("New password")).toHaveAccessibleDescription("At least 10 characters.");
  await expect(card).toContainText("This signs you out of your other browsers and the command line.");
  await card.getByLabel("Current password").fill(PASSWORD);
  await card.getByLabel("New password").fill(NEW_PASSWORD);
  // Enter in either input submits.
  await card.getByLabel("New password").press("Enter");

  await expect(card.getByRole("status")).toHaveText("Password changed. No other sign-ins were open.");
  await expect(card.getByLabel("Current password")).toHaveCount(0);
  await expect(card.getByRole("button", { name: "Change password" })).toBeFocused();

  await page.getByRole("main").getByRole("button", { name: "Sign out", exact: true }).click();
  await expect(page).toHaveURL(/\/login$/);
  await page.getByLabel("Email or username").fill(email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText("That email or username and password don't match an account.");
  await page.getByLabel("Password").fill(NEW_PASSWORD);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(HEAP_URL);
});

test("changing the password signs the same user out of their other browser", async ({ page, browser }, testInfo) => {
  const { email } = await startSignedIn(page);
  const other = await browser.newContext(testInfo.project.use);
  try {
    const signedIn = await other.request.post("/api/v1/auth/login", {
      headers: { "x-csrf-protection": "1" },
      data: { login: email, password: PASSWORD },
    });
    expect(signedIn.status()).toBe(200);
    const otherPage = await other.newPage();
    await otherPage.goto("/");
    await expect(otherPage).toHaveURL(HEAP_URL);

    const card = await openCard(page);
    await card.getByLabel("Current password").fill(PASSWORD);
    await card.getByLabel("New password").fill(NEW_PASSWORD);
    await card.getByRole("button", { name: "Change password" }).click();
    await expect(card.getByRole("status")).toHaveText("Password changed. 1 other sign-in ended.");

    // The other browser's next request is refused, and it lands on sign-in.
    await otherPage.goto("/");
    await expect(otherPage).toHaveURL(/\/login/);
    // This one stays signed in.
    await page.reload();
    await expect(page.getByRole("region", { name: "Password", exact: true })).toBeVisible();
  } finally {
    await other.close();
  }
});

test("a wrong current password and a short new one show their errors under their inputs", async ({ page }) => {
  await startSignedIn(page);
  const card = await openCard(page);
  const current = card.getByLabel("Current password");
  const next = card.getByLabel("New password");

  await current.fill("not the password");
  await next.fill(NEW_PASSWORD);
  await card.getByRole("button", { name: "Change password" }).click();
  await expect(current).toHaveAccessibleDescription("That isn't your current password.");
  await expect(current).toHaveAttribute("aria-invalid", "true");
  // The wrong one is cleared, ready for a refill, and has focus. What was typed in the other stays.
  await expect(current).toHaveValue("");
  await expect(current).toBeFocused();
  await expect(next).toHaveValue(NEW_PASSWORD);

  await current.fill(PASSWORD);
  await next.fill("nine char");
  await card.getByRole("button", { name: "Change password" }).click();
  await expect(next).toHaveAccessibleDescription("Use at least 10 characters.");
  await expect(next).toHaveAttribute("aria-invalid", "true");
  await expect(next).toBeFocused();
  await expect(current).toHaveValue(PASSWORD);

  // Sent empty, as the server asks for: it says which input.
  await next.fill("");
  await card.getByRole("button", { name: "Change password" }).click();
  await expect(next).toHaveAccessibleDescription("Enter a new password.");
});

test("Cancel and Escape close the form, clear it, and give focus back", async ({ page }) => {
  await startSignedIn(page);
  const card = await openCard(page);
  const opener = card.getByRole("button", { name: "Change password" });

  await card.getByLabel("Current password").fill("something");
  await card.getByLabel("New password").fill("something else");
  await card.getByRole("button", { name: "Cancel" }).click();
  await expect(card.getByLabel("Current password")).toHaveCount(0);
  await expect(opener).toBeFocused();

  await opener.click();
  await expect(card.getByLabel("Current password")).toHaveValue("");
  await expect(card.getByLabel("New password")).toHaveValue("");
  await card.getByLabel("New password").fill("typed again");
  await card.getByLabel("New password").press("Escape");
  await expect(card.getByLabel("Current password")).toHaveCount(0);
  await expect(opener).toBeFocused();
});
