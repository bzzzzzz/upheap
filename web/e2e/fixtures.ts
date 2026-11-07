// What the golden-path tests share, on the server and on the local build: a fresh heap per test, and ways to find
// and add items as a user would.
import { expect, test as base, type Locator, type Page } from "@playwright/test";

type Size = "S" | "M" | "L" | "XL";

/** The option the local build's config sets. */
export type LocalBuildOptions = { localBuild: boolean };

export const test = base.extend<{ heapPath: string } & LocalBuildOptions>({
  /** Whether the tests run against the local build, which has one user and nothing to sign in to. */
  localBuild: [false, { option: true }],
  /**
   * Opens a heap of the test's own: on the server, a new account's personal heap, which registering through the API
   * signs the page in to (the sign-up form has its own test); in the local build, the local user's first heap, as
   * every test's browser context starts with empty storage. So tests can run in parallel. Automatic, so a test that
   * doesn't need the heap's path gets its heap too.
   */
  heapPath: [
    async ({ page, localBuild }, use) => {
      if (!localBuild) {
        const registered = await page.request.post("/api/v1/auth/register", {
          headers: { "x-csrf-protection": "1" },
          data: { ...uniqueUser(), display_name: "Tester", password: "a long test password" },
        });
        expect(registered.status()).toBe(201);
      }
      // Relative, so the local build's base path stays.
      await page.goto("./");
      await expect(page).toHaveURL(HEAP_URL);
      await use(new URL(page.url()).pathname);
    },
    { auto: true },
  ],
});

export { expect };

/** A heap's address: `/teams/{team}/heaps/{heap}`, under the local build's base path too. */
export const HEAP_URL = /\/teams\/[^/]+\/heaps\/[^/]+$/;

/** An email and username no other test uses, also across repeats and parallel workers. */
export function uniqueUser() {
  const id = crypto.randomUUID().replaceAll("-", "");
  return { email: `e2e-${id}@example.test`, username: `e2e-${id.slice(0, 20)}` };
}

/** The list row of the item titled `title`. */
export const row = (page: Page, title: string): Locator =>
  page.getByRole("listitem").filter({ has: page.getByRole("link", { name: title, exact: true }) });

/** The open item: a pane beside the list, a panel over it or a sheet, by the width. */
export const itemPane = (page: Page): Locator => page.getByRole("region", { name: "Item", exact: true });

/** Opens the item titled `title` from its row. */
export async function openItem(page: Page, title: string) {
  await row(page, title).getByRole("link", { name: title, exact: true }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(title);
}

/** Closes the open item with its ✕. */
export async function closeItem(page: Page) {
  await itemPane(page).getByRole("button", { name: "Close", exact: true }).click();
  await expect(itemPane(page)).toHaveCount(0);
}

/** Picks `option` in the item pane's radio group `group`: a state, a size, a parameter's option. */
export async function pick(page: Page, group: string, option: string) {
  await itemPane(page)
    .getByRole("radiogroup", { name: group, exact: true })
    .getByRole("radio", { name: option, exact: true })
    .click();
}

/** Expects the rows of the open list to show these titles, in this order. */
export async function expectOrder(page: Page, titles: string[]) {
  for (const [index, title] of titles.entries()) {
    await expect(row(page, title).getByLabel(`Position ${index + 1}`, { exact: true })).toBeVisible();
  }
}

/** Adds an item through the Add dialog of the open list. */
export async function addItem(page: Page, title: string, sizes: { effort?: Size; impact?: Size } = {}) {
  await page.getByRole("button", { name: "Add an item" }).click();
  const dialog = page.getByRole("dialog", { name: "Add an item" });
  await dialog.getByLabel("Title").fill(title);
  for (const [field, size] of [
    ["Effort", sizes.effort],
    ["Impact", sizes.impact],
  ] as const) {
    if (size)
      await dialog.getByRole("radiogroup", { name: field }).getByRole("radio", { name: size, exact: true }).click();
  }
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect(row(page, title)).toBeVisible();
}

/** A page the app's navigation leads to. */
export type AppPage = "Open" | "Closed" | "Activity" | "Tags" | "Parameters" | "Account";

/**
 * Goes to one of the app's pages through its navigation, wherever the layout puts it: the sidebar on a wide screen,
 * the rail on a tablet, and on a phone the bottom tabs or their More menu.
 */
export async function goTo(page: Page, name: AppPage) {
  const link = page.getByRole("link", { name, exact: true });
  if (await link.isVisible()) {
    await link.click();
  } else {
    await page.getByRole("navigation", { name: "Heap views" }).getByRole("button", { name: "More" }).click();
    await page.getByRole("menuitem", { name, exact: true }).click();
  }
}

/** Signs out, from the sidebar or rail, or on a phone from More. */
export async function signOut(page: Page) {
  const button = page.getByRole("button", { name: "Sign out", exact: true });
  if (await button.isVisible()) {
    await button.click();
  } else {
    await page.getByRole("navigation", { name: "Heap views" }).getByRole("button", { name: "More" }).click();
    await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
  }
}

/** The calendar day `days` from today, in the browser's timezone (UTC, as the config sets it). */
export const dayFromToday = (days: number) => new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10);
