// The reset page and the forgot-password line on sign-in (design/ux.md, Reset page and Sign in and create account):
// the golden path of a person who forgot their password, on a server whose operator makes the link with the server's
// own command, and a small test for each state of the page. Passwords are the server's, so the local build skips
// this file.
import type { Browser, TestInfo } from "@playwright/test";

import { test as closed, type ClosedServer } from "./closedServer";
import { expect, expectFitsWidth, HEAP_URL, PASSWORD, signOut, test } from "./fixtures";

const NEW_PASSWORD = "another long password";

/** A browser context of its own on the closed server, signed out. */
function visitor(browser: Browser, testInfo: TestInfo, { baseURL }: ClosedServer) {
  return browser.newContext({ ...testInfo.project.use, baseURL });
}

/** The part of a link after `/reset/`: the token. */
const tokenOf = (link: string) => link.split("/").pop() ?? "";

/**
 * Sets a password with a link through the API, as a person at the page does, and returns the status. Used to put the
 * shared account back to `PASSWORD`, and to use up a link.
 */
async function setPasswordThrough(link: string, password: string): Promise<number> {
  const origin = new URL(link).origin;
  const response = await fetch(`${origin}/api/v1/password-resets/${tokenOf(link)}`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-csrf-protection": "1" },
    body: JSON.stringify({ password }),
  });
  return response.status;
}

test("the sign-in page says how a forgotten password is reset", async ({ page }) => {
  await page.goto("/login");
  const forgot = page.getByRole("button", { name: "Forgot your password?" });
  const line = page.getByText("Whoever runs this Upheap server can send you a reset link.");
  await expect(forgot).toHaveAttribute("aria-expanded", "false");
  await expect(line).toHaveCount(0);

  await forgot.click();
  await expect(forgot).toHaveAttribute("aria-expanded", "true");
  await expect(line).toBeVisible();
  await expectFitsWidth(page, "Sign in with the line open");

  await forgot.click();
  await expect(line).toHaveCount(0);

  // Create account has no such line.
  await page.getByRole("tab", { name: "Create account" }).click();
  await expect(forgot).toHaveCount(0);
});

closed(
  "someone who forgot their password sets a new one with the operator's link and is signed in",
  async ({ browser, closedServer }, testInfo) => {
    const { forgetful, resetLink } = closedServer;
    const context = await visitor(browser, testInfo, closedServer);
    try {
      // The operator's command prints the link; the person opens it.
      const link = await resetLink(forgetful.username);
      expect(link).toMatch(new RegExp(`^${closedServer.baseURL}/reset/[A-Za-z0-9_-]{43}$`));
      const page = await context.newPage();
      await page.goto(link);
      await expect(page.getByRole("heading", { name: "Set a new password" })).toBeVisible();
      await expect(page.getByText(`for Mia Chen (@${forgetful.username})`)).toBeVisible();
      const input = page.getByLabel("New password");
      await expect(input).toBeFocused();
      await expect(input).toHaveAccessibleDescription("At least 10 characters.");
      await expect(page.getByText("This signs you out everywhere else.")).toBeVisible();
      await expectFitsWidth(page, "The reset page");

      // Enter submits; they land in the app with a line saying so.
      await input.fill(NEW_PASSWORD);
      await input.press("Enter");
      await expect(page).toHaveURL(HEAP_URL);
      await expect(page.getByText("Password changed. You're signed in.")).toBeVisible();
      await page.reload();
      await expect(page.getByText("Password changed. You're signed in.")).toHaveCount(0);

      // The old password is gone, and the new one signs in.
      await signOut(page);
      await expect(page).toHaveURL(/\/login$/);
      await page.getByLabel("Email or username").fill(forgetful.email);
      await page.getByLabel("Password").fill(PASSWORD);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await expect(page.getByRole("alert")).toHaveText("That email or username and password don't match an account.");
      await page.getByLabel("Password").fill(NEW_PASSWORD);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await expect(page).toHaveURL(HEAP_URL);

      // The link is used up.
      await page.goto(link);
      await expect(page.getByRole("heading", { name: "This reset link was already used." })).toBeVisible();
      await expect(page.getByText("Ask whoever runs this server for a new one.")).toBeVisible();
    } finally {
      await context.close();
      // The account goes back to the password the other tests know.
      expect(await setPasswordThrough(await resetLink(forgetful.username), PASSWORD)).toBe(200);
    }
  },
);

closed("a newer link cancels the earlier one", async ({ browser, closedServer }, testInfo) => {
  const { forgetful, resetLink } = closedServer;
  const context = await visitor(browser, testInfo, closedServer);
  try {
    const first = await resetLink(forgetful.username);
    const second = await resetLink(forgetful.email);
    expect(second).not.toBe(first);

    const page = await context.newPage();
    await page.goto(first);
    await expect(page.getByRole("heading", { name: "This reset link no longer works." })).toBeVisible();
    await expect(page.getByText("A newer one was made, or the password was changed since.")).toBeVisible();
    await expect(page.getByText("Ask whoever runs this server for a new one.")).toBeVisible();
    await expect(page.getByRole("link", { name: "Open Upheap" })).toHaveAttribute("href", "/");
    await expectFitsWidth(page, "The cancelled reset page");

    await page.goto(second);
    await expect(page.getByRole("heading", { name: "Set a new password" })).toBeVisible();
  } finally {
    await context.close();
  }
});

closed("a link that was used says so", async ({ browser, closedServer }, testInfo) => {
  const { forgetful, resetLink } = closedServer;
  const context = await visitor(browser, testInfo, closedServer);
  try {
    // Used through the API with the password the account already has, so it is as it was.
    const link = await resetLink(forgetful.username);
    expect(await setPasswordThrough(link, PASSWORD)).toBe(200);

    const page = await context.newPage();
    await page.goto(link);
    await expect(page.getByRole("heading", { name: "This reset link was already used." })).toBeVisible();
    await expect(page.getByRole("link", { name: "Open Upheap" })).toBeVisible();
  } finally {
    await context.close();
  }
});

test("a link that isn't one says so, and Open Upheap leads on", async ({ page }) => {
  await page.goto("/reset/not-a-token");
  await expect(page.getByRole("heading", { name: "This isn't a reset link." })).toBeVisible();
  await expect(
    page.getByText("Check that the whole link was copied, or ask whoever runs this server for a new one."),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Open Upheap" })).toHaveAttribute("href", "/");
});

closed(
  "a password that is too short is refused under the input, and the link keeps working",
  async ({ browser, closedServer }, testInfo) => {
    const { forgetful, resetLink } = closedServer;
    const context = await visitor(browser, testInfo, closedServer);
    try {
      const page = await context.newPage();
      await page.goto(await resetLink(forgetful.username));
      const input = page.getByLabel("New password");
      await input.fill("too short");
      await page.getByRole("button", { name: "Set password and sign in" }).click();

      await expect(page.getByText("Use at least 10 characters.")).toBeVisible();
      await expect(input).toHaveAttribute("aria-invalid", "true");
      await expect(input).toBeFocused();
      await expect(input).toHaveValue("too short");

      // Editing clears the problem; an empty password has its own words.
      await input.fill("");
      await page.getByRole("button", { name: "Set password and sign in" }).click();
      await expect(page.getByText("Enter a new password.")).toBeVisible();
      await expect(page.getByRole("heading", { name: "Set a new password" })).toBeVisible();
    } finally {
      await context.close();
    }
  },
);
