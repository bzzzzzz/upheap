// What the golden-path tests share, on the server and on the local build: a fresh heap per test, and ways to find
// and add items as a user would.
import { expect, test as base, type Locator, type Page } from "@playwright/test";

type Size = "S" | "M" | "L" | "XL";

/** The option the local build's config sets. */
export type LocalBuildOptions = { localBuild: boolean };

export const test = base.extend<{ heapId: string } & LocalBuildOptions>({
  /** Whether the tests run against the local build, which has one user and nothing to sign in to. */
  localBuild: [false, { option: true }],
  /**
   * Opens a heap of the test's own: on the server, a new account's personal heap, which registering through the API
   * signs the page in to (the sign-up form has its own test); in the local build, the local user's first heap, as
   * every test's browser context starts with empty storage. So tests can run in parallel. Automatic, so a test that
   * doesn't need the heap's ID gets its heap too.
   */
  heapId: [
    async ({ page, localBuild }, use) => {
      if (!localBuild) {
        const registered = await page.request.post("/api/v1/auth/register", {
          headers: { "x-csrf-protection": "1" },
          data: { email: uniqueEmail(), display_name: "Tester", password: "a long test password" },
        });
        expect(registered.status()).toBe(201);
      }
      // Relative, so the local build's base path stays.
      await page.goto("./");
      await expect(page).toHaveURL(/\/heaps\/[^/]+$/);
      const heapId = new URL(page.url()).pathname.split("/").at(-1) ?? "";
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
