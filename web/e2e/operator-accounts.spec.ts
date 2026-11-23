// Accounts (ux.md, Accounts): the golden path of an operator of the server, who finds an account, makes a reset
// link for it, turns it off and back on, and the page that is not found for anyone else. On the closed server, whose
// seeded accounts include an operator, made with the server's own command.
import type { Page } from "@playwright/test";

import { test as closed } from "./closedServer";
import { expect, expectFitsWidth, goTo, HEAP_URL, isPhone, PASSWORD } from "./fixtures";

const CSRF = { "x-csrf-protection": "1" };

/** Opens the actions of the account `name`: a menu on a tablet or wide screen, a sheet on a phone. */
async function chooseAction(page: Page, name: string, action: string) {
  await page.getByRole("button", { name: `Actions for ${name}` }).click();
  const choice = isPhone(page)
    ? page.getByRole("dialog", { name }).getByRole("button", { name: action, exact: true })
    : page.getByRole("menuitem", { name: action, exact: true });
  await choice.click();
}

closed(
  "an operator makes a reset link, turns an account off and on again",
  async ({ browser, closedServer }, testInfo) => {
    const { baseURL, operator } = closedServer;
    const context = await browser.newContext({ ...testInfo.project.use, baseURL });
    try {
      const signedIn = await context.request.post("/api/v1/auth/login", {
        headers: CSRF,
        data: { login: operator.username, password: PASSWORD },
      });
      expect(signedIn.status()).toBe(200);
      const page = await context.newPage();
      await page.goto("./");
      await expect(page).toHaveURL(HEAP_URL);

      // Accounts is in the navigation, and lists every account.
      await goTo(page, "Accounts");
      await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toBeVisible();
      await expect(page.getByText("Every account on this server.")).toBeVisible();
      const accounts = page.getByRole("main").getByRole("list");
      await expect(accounts.getByRole("listitem").filter({ hasText: "Ivy Stone" })).toContainText("Operator");
      await expectFitsWidth(page, "Accounts");

      // The search filters the list as it is typed.
      await page.getByLabel("Search accounts").fill("ola ber");
      const row = accounts.getByRole("listitem").filter({ hasText: "Ola Berg" });
      await expect(row).toHaveCount(1);
      await expect(accounts.getByRole("listitem")).toHaveCount(1);
      await page.getByLabel("Search accounts").fill("nobody-has-this-name");
      await expect(page.getByText("No accounts match.")).toBeVisible();
      await page.getByLabel("Search accounts").fill("ola ber");
      await expect(row).toBeVisible();

      // A reset link: shown in a read-only field with Copy, and gone with Done.
      await chooseAction(page, "Ola Berg", "Make reset link");
      const scope = isPhone(page) ? page.getByRole("dialog", { name: "Ola Berg" }) : row;
      await expect(scope.getByLabel("Reset link")).toHaveValue(new RegExp(`^${baseURL}/reset/[A-Za-z0-9_-]{43}$`));
      await expect(scope.getByRole("button", { name: "Copy" })).toBeVisible();
      await scope.getByRole("button", { name: "Done" }).click();
      await expect(page.getByLabel("Reset link")).toHaveCount(0);
      if (!isPhone(page)) await expect(page.getByRole("button", { name: "Actions for Ola Berg" })).toBeFocused();

      // Turning an account off asks first, saying what ends.
      await chooseAction(page, "Ola Berg", "Deactivate…");
      const question = "Turn off Ola Berg's account? 1 sign-in ends. Their items and teams stay.";
      const confirm = isPhone(page)
        ? page.getByRole("alertdialog", { name: "Ola Berg" })
        : page.getByRole("alertdialog", { name: question });
      await expect(confirm).toContainText(question);
      await confirm.getByRole("button", { name: "Cancel" }).click();
      await expect(page.getByRole("alertdialog")).toHaveCount(0);
      if (!isPhone(page)) await expect(page.getByRole("button", { name: "Actions for Ola Berg" })).toBeFocused();

      await chooseAction(page, "Ola Berg", "Deactivate…");
      await confirm.getByRole("button", { name: "Deactivate", exact: true }).click();
      await expect(row).toContainText("Deactivated");
      await expect(row.locator(".opacity-60")).toHaveCount(1);

      // Turning it back on acts at once, and offers Undo.
      await chooseAction(page, "Ola Berg", "Reactivate");
      const toast = page.getByRole("status").filter({ hasText: "Ola Berg's account is back on." });
      await expect(toast).toBeVisible();
      await expect(row).not.toContainText("Deactivated");
      await expect(row.locator(".opacity-60")).toHaveCount(0);
    } finally {
      await context.close();
    }
  },
);

closed(
  "someone who isn't an operator has no Accounts entry, and the page is not found",
  async ({ browser, closedServer }, testInfo) => {
    const { baseURL, member } = closedServer;
    const context = await browser.newContext({ ...testInfo.project.use, baseURL });
    try {
      const signedIn = await context.request.post("/api/v1/auth/login", {
        headers: CSRF,
        data: { login: member.username, password: PASSWORD },
      });
      expect(signedIn.status()).toBe(200);
      const page = await context.newPage();
      await page.goto("./");
      await expect(page).toHaveURL(HEAP_URL);
      await expect(page.getByRole("link", { name: "Accounts", exact: true })).toHaveCount(0);

      await page.goto("/accounts");
      await expect(page.getByText("Not found", { exact: true })).toBeVisible();
      await expect(page.getByRole("heading", { name: "Page not found" })).toBeVisible();
      await expect(page.getByRole("heading", { name: "Accounts", exact: true })).toHaveCount(0);
    } finally {
      await context.close();
    }
  },
);
