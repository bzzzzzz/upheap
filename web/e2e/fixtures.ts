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
      if (!localBuild) await registerThroughApi(page);
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

/** The password every account the tests register has. */
export const PASSWORD = "a long test password";

/**
 * Registers a new account through the API, which signs the page in to it (server only). Returns its email and
 * username; the password is `PASSWORD`.
 */
export async function registerThroughApi(page: Page) {
  const user = uniqueUser();
  const registered = await page.request.post("/api/v1/auth/register", {
    headers: { "x-csrf-protection": "1" },
    data: { ...user, display_name: "Tester", password: PASSWORD },
  });
  expect(registered.status()).toBe(201);
  return user;
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

/**
 * Where the open item's field `name` is changed: on a phone its row opens a sheet, which this opens and returns; on a
 * tablet or wide screen the field is in the pane itself.
 */
export async function field(page: Page, name: string): Promise<Locator> {
  // The fields show once the item has loaded.
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toBeVisible();
  const opener = itemPane(page).getByRole("button", { name: new RegExp(`^${name}( |$)`) });
  if ((await opener.count()) > 0 && (await opener.isVisible())) {
    await opener.click();
    const sheet = page.getByRole("dialog", { name, exact: true });
    await expect(sheet).toBeVisible();
    return sheet;
  }
  return itemPane(page);
}

/** Closes the sheet a field opened on a phone, if one is still open. */
export async function closeField(page: Page) {
  const sheet = page.getByRole("dialog").filter({ has: page.getByRole("heading") });
  if ((await sheet.count()) > 0) {
    await sheet.getByRole("button", { name: "Close", exact: true }).click();
    await expect(sheet).toHaveCount(0);
  }
}

/**
 * Picks `option` in the item's radio group `group`: a state, a size, a parameter's option, a due choice. Returns
 * where it was picked, the pane or a phone's sheet, which closes itself once a choice is the whole job.
 */
export async function pick(page: Page, group: string, option: string): Promise<Locator> {
  const scope = await field(page, group);
  await scope
    .getByRole("radiogroup", { name: group, exact: true })
    .getByRole("radio", { name: option, exact: true })
    .click();
  return scope;
}

/** Presses the item pane's state button `label`: Start, Block, Done, Unblock, Back to open or Reopen. */
export async function changeState(page: Page, label: string) {
  await itemPane(page)
    .getByRole("group", { name: "State", exact: true })
    .getByRole("button", { name: label, exact: true })
    .click();
}

/** Expects the rows of the open list to show these titles, in this order. */
export async function expectOrder(page: Page, titles: string[]) {
  for (const [index, title] of titles.entries()) {
    await expect(row(page, title).getByLabel(`Position ${index + 1}`, { exact: true })).toBeVisible();
  }
}

/** A need as the Add dialog and the item pane name it. */
export type Need = "Idea" | "Nice to have" | "Expected" | "Critical";

/** The Add dialog, open over the page. Its name goes on with the heap the item goes into. */
export const addDialog = (page: Page): Locator => page.getByRole("dialog", { name: /^Add an item to / });

/** Whether the page is as wide as a phone, where the Add dialog is a sheet of chips. */
export const isPhone = (page: Page) => (page.viewportSize()?.width ?? 0) < 640;

/**
 * Picks `size` for the Add dialog's `field`: among its buttons, or on a phone from the sheet its chip opens. Picking
 * the size it has clears it.
 */
export async function pickSize(page: Page, field: "Effort" | "Impact", size: Size) {
  const dialog = addDialog(page);
  if (isPhone(page)) {
    await dialog.getByRole("button", { name: new RegExp(`^${field} `) }).click();
    const sheet = page.getByRole("dialog", { name: field, exact: true });
    await sheet.getByRole("radio", { name: size, exact: true }).click();
    await expect(sheet).toHaveCount(0);
  } else {
    await dialog.getByRole("radiogroup", { name: field }).getByRole("radio", { name: size, exact: true }).click();
  }
}

/** Expects the Add dialog's `field` to have `size`, or with `null` no size. */
export async function expectSize(page: Page, field: "Effort" | "Impact", size: Size | null) {
  const dialog = addDialog(page);
  if (isPhone(page)) {
    await expect(dialog.getByRole("button", { name: `${field} ${size ?? "not set"}`, exact: true })).toBeVisible();
  } else if (size) {
    await expect(
      dialog.getByRole("radiogroup", { name: field }).getByRole("radio", { name: size, exact: true }),
    ).toBeChecked();
  } else {
    await expect(dialog.getByRole("radiogroup", { name: field }).getByRole("radio", { checked: true })).toHaveCount(0);
  }
}

/**
 * Adds an item through the Add dialog, as expected unless `need` says otherwise. With `listed`, the default, the page
 * is the open list, and the new row shows there. Add another must be off, as it is unless a test ticks it.
 */
export async function addItem(
  page: Page,
  title: string,
  fields: { need?: Need; effort?: Size; impact?: Size } = {},
  { listed = true }: { listed?: boolean } = {},
) {
  await page.getByRole("button", { name: "Add item", exact: true }).click();
  const dialog = addDialog(page);
  await dialog.getByLabel("Title").fill(title);
  await dialog
    .getByRole("radiogroup", { name: "Need" })
    .getByRole("radio", { name: fields.need ?? "Expected", exact: true })
    .click();
  if (fields.effort) await pickSize(page, "Effort", fields.effort);
  if (fields.impact) await pickSize(page, "Impact", fields.impact);
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog).toHaveCount(0);
  if (listed) await expect(row(page, title)).toBeVisible();
}

/** A page the app's navigation leads to. Tags and Parameters are tabs of the heap's Settings. */
export type AppPage = "Open" | "Closed" | "Activity" | "Settings" | "Tags" | "Parameters" | "Account";

/**
 * Goes to one of the app's pages through its navigation, wherever the layout puts it: the sidebar on a wide screen,
 * the rail on a tablet, and on a phone the bottom tabs or their More menu. Tags and Parameters go through Settings.
 */
export async function goTo(page: Page, name: AppPage) {
  if (name === "Tags" || name === "Parameters") {
    const tab = page.getByRole("navigation", { name: "Settings" }).getByRole("link", { name, exact: true });
    if (!(await tab.isVisible())) await goTo(page, "Settings");
    await tab.click();
    return;
  }
  const link = page.getByRole("link", { name, exact: true });
  if (await link.isVisible()) {
    await link.click();
  } else {
    await openMore(page);
    // A row may carry a note after its name, such as the account's username.
    await page
      .getByRole("navigation", { name: "More" })
      .getByRole("link", { name: new RegExp(`^${name}( |$)`) })
      .click();
  }
}

/** Opens More from the phone's tabs. */
async function openMore(page: Page) {
  await page.getByRole("navigation", { name: "Heap views" }).getByRole("button", { name: "More" }).click();
}

/** Signs out from the Account page, reached through the navigation at any width. */
export async function signOut(page: Page) {
  await goTo(page, "Account");
  await page.getByRole("main").getByRole("button", { name: "Sign out", exact: true }).click();
}

/** The calendar day `days` from today, in the browser's timezone (UTC, as the config sets it). */
export const dayFromToday = (days: number) => new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10);
