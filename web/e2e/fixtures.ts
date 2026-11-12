// What the golden-path tests share: a fresh heap per test, and ways to find and add items as a user would.
import { expect, test as base, type Locator, type Page } from "@playwright/test";

type Size = "S" | "M" | "L" | "XL";

export const test = base.extend<{ heapPath: string }>({
  /**
   * Opens a heap of the test's own: a new account's personal heap, which registering through the API signs the page
   * in to (the sign-up form has its own test). So tests can run in parallel. Automatic, so a test that doesn't need
   * the heap's path gets its heap too.
   */
  heapPath: [
    async ({ page }, use) => {
      await registerThroughApi(page);
      await page.goto("./");
      await expect(page).toHaveURL(HEAP_URL);
      await use(new URL(page.url()).pathname);
    },
    { auto: true },
  ],
});

export { expect };

/** A heap's address: `/teams/{team}/heaps/{heap}`. */
export const HEAP_URL = /\/teams\/[^/]+\/heaps\/[^/]+$/;

/** An email and username no other test uses, also across repeats and parallel workers. */
export function uniqueUser() {
  const id = crypto.randomUUID().replaceAll("-", "");
  return { email: `e2e-${id}@example.test`, username: `e2e-${id.slice(0, 20)}` };
}

/** The password every account the tests register has. */
export const PASSWORD = "a long test password";

/**
 * Registers a new account through the API, which signs the page in to it. Returns its email and
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

/**
 * The list row of the item titled `title`. The row's link is named by the title and then what a screen reader hears
 * after it (", task", ", blocked", ", with an agent"), so those may follow.
 */
export const row = (page: Page, title: string): Locator =>
  page.getByRole("listitem").filter({ has: page.getByRole("link", { name: rowLinkName(title) }) });

/** The name of a row's link: the title, then what a screen reader hears after it, which Chrome starts with a space. */
const rowLinkName = (title: string) => new RegExp(`^${escapeRegExp(title)}(\\s*, .+)?$`);

/** The open item: a pane beside the list, a panel over it or a sheet, by the width. */
export const itemPane = (page: Page): Locator => page.getByRole("region", { name: "Item", exact: true });

/** Opens the item titled `title` from its row. */
export async function openItem(page: Page, title: string) {
  await row(page, title)
    .getByRole("link", { name: rowLinkName(title) })
    .click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(title);
}

/** Makes the open item, a note, a task through its More menu, answering the question that asks. */
export async function makeTask(page: Page, title: string) {
  const more = itemPane(page).getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true });
  await more.click();
  if (isPhone(page))
    await page
      .getByRole("dialog", { name: "More", exact: true })
      .getByRole("button", { name: "Make it a task…" })
      .click();
  else await page.getByRole("menu", { name: "More" }).getByRole("menuitem", { name: "Make it a task…" }).click();
  await page
    .getByRole("alertdialog", { name: `Make “${title}” a task?` })
    .getByRole("button", { name: "Make it a task" })
    .click();
  await expect(itemPane(page).getByRole("region", { name: "Steps" })).toBeVisible();
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
  await expect(page.locator("main [data-row-link]")).toHaveText(
    titles.map((title) => new RegExp(`^${escapeRegExp(title)}(,|$)`)),
  );
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Drags from `at` (the middle by default) across the width of `target` by `dx`, `dy`, in steps, as a finger would. */
export async function drag(page: Page, target: Locator, dx: number, dy: number, at = 0.5) {
  // Waits until it's still, as the phone's sheet slides up as it opens.
  await target.click({ trial: true, position: { x: ((await target.boundingBox())?.width ?? 0) * at, y: 8 } });
  const box = await target.boundingBox();
  if (!box) throw new Error("Nothing to drag");
  const x = box.x + box.width * at;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx, y + dy, { steps: 12 });
  await page.mouse.up();
}

/**
 * Marks an open item done from its list row: a swipe right on a touch screen, the done button beside play with a
 * mouse (design/ux.md, The list).
 */
export async function markDoneFromList(page: Page, title: string, from: Locator = row(page, title)) {
  const touch = await page.evaluate(() => window.matchMedia("(pointer: coarse)").matches);
  if (touch) {
    // From the left of the row, clear of an item panel over its right. A row that was swiped and sprang back (the
    // question to take an item back from an agent was cancelled) isn't under the finger until it is still.
    await expect
      .poll(() =>
        from.locator('[class~="group/row"]').evaluate((el) => new DOMMatrix(getComputedStyle(el).transform).m41),
      )
      .toBeLessThan(1);
    await drag(page, from, 200, 0, 0.2);
  } else {
    await from.getByRole("button", { name: `Mark ${title} done` }).click();
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

/** Expects the page to be no wider than the screen: nothing on it makes it scroll sideways. */
export async function expectFitsWidth(page: Page, name: string) {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow, `${name} scrolls sideways`).toBeLessThanOrEqual(0);
}

/** A page the app's navigation leads to. Tags, Parameters and Ranking are tabs of the heap's Settings. */
export type AppPage = "Open" | "Closed" | "Activity" | "Settings" | "Tags" | "Parameters" | "Ranking" | "Account";

/**
 * Goes to one of the app's pages through its navigation, wherever the layout puts it: the sidebar on a wide screen,
 * the rail on a tablet, and on a phone the bottom tabs or their More menu. Tags, Parameters and Ranking go through
 * Settings.
 */
export async function goTo(page: Page, name: AppPage) {
  if (name === "Tags" || name === "Parameters" || name === "Ranking") {
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

/**
 * Adds a parameter on the Parameters tab, through its New parameter form, with `options` highest priority first, and
 * waits for its card.
 */
export async function addParameter(page: Page, name: string, weight: number, options: string[]) {
  await goTo(page, "Parameters");
  await page.getByRole("button", { name: "New parameter" }).click();
  await page.getByLabel("New parameter name").fill(name);
  await page.getByLabel("Weight of the new parameter").fill(String(weight));
  await page.getByLabel(/^Options of the new parameter/).fill(options.join("\n"));
  await page.getByRole("button", { name: "Add parameter" }).click();
  await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
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

/**
 * Adds heaps named `names` to the team of the heap on screen, through the API, and returns to that heap, reloaded so
 * the switcher lists them.
 */
export async function addHeaps(page: Page, names: string[]) {
  const start = new URL(page.url()).pathname;
  const team = /\/teams\/([^/]+)\//.exec(start)?.[1];
  if (!team) throw new Error(`not on a heap: ${start}`);
  for (const name of names) {
    const created = await page.request.post(`/api/v1/teams/${team}/heaps`, {
      headers: { "x-csrf-protection": "1" },
      data: { name },
    });
    expect(created.status()).toBe(201);
  }
  await page.goto(start);
  await expect(page).toHaveURL(HEAP_URL);
}

/** Creates a team from the Teams page and waits for its page. */
export async function createTeam(page: Page, name: string, linkName?: string) {
  await page.goto("/teams");
  await page.getByRole("button", { name: "+ Create team" }).click();
  await page.getByLabel("Name", { exact: true }).fill(name);
  if (linkName) await page.getByLabel("Link name").fill(linkName);
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
}
