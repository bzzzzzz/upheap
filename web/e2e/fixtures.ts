// What the golden-path tests share: a fresh account per test, and ways to find and add items as a user would.
import { expect, test as base, type Locator, type Page } from "@playwright/test";

type Size = "S" | "M" | "L" | "XL";

export const test = base.extend<{ heapId: string }>({
  /**
   * Registers a new account through the API (the sign-up form has its own test), which signs the page in, and opens
   * the account's personal heap. Every test gets its own account, so tests can run in parallel. Automatic, so a test
   * that doesn't need the heap's ID is signed in too.
   */
  heapId: [
    async ({ page }, use) => {
      const email = uniqueEmail();
      const registered = await page.request.post("/api/v1/auth/register", {
        headers: { "x-csrf-protection": "1" },
        data: { email, display_name: "Tester", password: "a long test password" },
      });
      expect(registered.status()).toBe(201);
      await page.goto("/");
      await expect(page).toHaveURL(/\/heaps\/[^/]+$/);
      const heapId = new URL(page.url()).pathname.split("/")[2] ?? "";
      await use(heapId);
    },
    { auto: true },
  ],
});

export { expect };

/** An address no other test uses, also across repeats and parallel workers. */
export const uniqueEmail = () => `e2e-${crypto.randomUUID()}@example.test`;

/** The list row of the item titled `title`. */
export const row = (page: Page, title: string): Locator =>
  page.getByRole("listitem").filter({ has: page.getByRole("button", { name: title, exact: true }) });

/** Expects the rows of the open list to show these titles, in this order. */
export async function expectOrder(page: Page, titles: string[]) {
  for (const [index, title] of titles.entries()) {
    await expect(row(page, title).getByLabel(`Position ${index + 1}`, { exact: true })).toBeVisible();
  }
}

/** Adds an item from the Add item form of the open list. */
export async function addItem(page: Page, title: string, sizes: { effort?: Size; impact?: Size } = {}) {
  const form = page.locator("form").filter({ has: page.getByLabel("New item title") });
  await form.getByLabel("New item title").fill(title);
  if (sizes.effort || sizes.impact) {
    const more = form.getByRole("button", { name: "More" });
    if (await more.isVisible()) await more.click();
    if (sizes.effort) await form.getByLabel("Effort").selectOption(sizes.effort);
    if (sizes.impact) await form.getByLabel("Impact").selectOption(sizes.impact);
  }
  await form.getByLabel("New item title").press("Enter");
  await expect(row(page, title)).toBeVisible();
  await expect(form.getByLabel("New item title")).toHaveValue("");
}

/** The calendar day `days` from today, in the browser's timezone (UTC, as the config sets it). */
export const dayFromToday = (days: number) => new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10);
