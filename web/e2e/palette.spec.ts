// The Find palette (design/ux.md, Keyboard): ⌘K finds items in every heap, goes to a heap, adds an item, and acts on one.
import type { Page } from "@playwright/test";

import { addDialog, addItem, expect, isPhone, seed, test } from "./fixtures";

/** Makes a heap named `name` in the account's team from the team's page, and returns to the first heap's list. */
async function addHeap(page: Page, heapPath: string, name: string) {
  await page.goto(heapPath.replace(/\/heaps\/[^/]+$/, ""));
  await page.getByRole("button", { name: "+ New heap" }).click();
  await page.getByLabel("New heap name").fill(name);
  await page.getByRole("button", { name: "Add heap" }).click();
  await expect(page.getByRole("region", { name: "Heaps" }).getByText(name, { exact: true })).toBeVisible();
}

/** Opens the palette by its key and returns it. */
async function openFind(page: Page) {
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Find", exact: true });
  await expect(palette.getByRole("searchbox", { name: "Find" })).toBeFocused();
  return palette;
}

/** Makes `title` in a second heap, named Garden, and goes back to the first heap's list. */
async function itemInGarden(page: Page, heapPath: string, title: string) {
  await addHeap(page, heapPath, "Garden");
  await page
    .getByRole("region", { name: "Heaps" })
    .getByRole("link", { name: /Garden/ })
    .click();
  await addItem(page, title);
  await page.goto(heapPath);
  await expect(page.getByRole("heading", { level: 1 })).toBeAttached();
}

test("finds an item in another heap by title or key and opens it", async ({ page, heapPath }) => {
  await itemInGarden(page, heapPath, "Prune the roses");
  let palette = await openFind(page);
  await palette.getByRole("searchbox").fill("roses");
  const found = palette.getByRole("menuitem", { name: /Prune the roses/ });
  await expect(found).toContainText("Garden");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/heaps\/garden\/items\/[^/]+$/);
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue("Prune the roses");

  // By its key.
  const key = new URL(page.url()).pathname.split("/").pop() ?? "";
  palette = await openFind(page);
  await palette.getByRole("searchbox").fill(key);
  await expect(palette.getByRole("menuitem", { name: /Prune the roses/ })).toBeVisible();
});

test("Tab shows the actions, and Mark done marks the item done", async ({ page }) => {
  await addItem(page, "Water the ferns");
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("ferns");
  await expect(palette.getByRole("menuitem", { name: /Water the ferns/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(palette.getByRole("menuitem", { name: "Start" })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(palette.getByRole("menuitem", { name: "Mark done" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("status").filter({ hasText: "“Water the ferns” is done." })).toBeVisible();
  await expect(palette).toHaveCount(0);
});

test("Copy link closes the palette with a toast", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await addItem(page, "Share the plan");
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("Share");
  await expect(palette.getByRole("menuitem", { name: /Share the plan/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await palette.getByRole("menuitem", { name: "Copy link" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Link copied." })).toBeVisible();
  await expect(palette).toHaveCount(0);
});

test("an item an agent has offers to take it back", async ({ page, heapPath }) => {
  await seed(page, heapPath, "Rotate the certs", true);
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("certs");
  await expect(palette.getByRole("menuitem", { name: /Rotate the certs/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(palette.getByRole("menuitem", { name: "Take back and start" })).toBeVisible();
  await expect(palette.getByRole("menuitem", { name: "Take back and mark done" })).toBeVisible();
});

test("Enter and Tab wait for the results of what is typed", async ({ page }) => {
  await addItem(page, "Water the ferns");
  const palette = await openFind(page);
  const url = page.url();
  // Hold the answer back, so "before the results" is a state the test controls, not a race with the server.
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/v1/me/search*", async (route) => {
    await held;
    await route.continue();
  });
  // Before the answer comes, Enter must not open the Add dialog or the item, and Tab must not show actions.
  await palette.getByRole("searchbox").fill("ferns");
  await expect(palette.getByText("Searching…")).toBeVisible();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await expect(addDialog(page)).toHaveCount(0);
  await expect(palette.getByRole("menuitem", { name: "Start" })).toHaveCount(0);
  expect(page.url()).toBe(url);
  // Once the item is there, Enter opens it.
  release();
  await expect(palette.getByRole("menuitem", { name: /Water the ferns/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/items\/[^/]+$/);
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue("Water the ferns");
  await expect(addDialog(page)).toHaveCount(0);
});

test("Due saves a rough date", async ({ page }) => {
  await addItem(page, "Renew the permit");
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("permit");
  await expect(palette.getByRole("menuitem", { name: /Renew the permit/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await palette.getByRole("menuitem", { name: "Due…" }).click();
  await palette.getByRole("radiogroup", { name: "Due" }).getByRole("radio").first().click();
  await expect(page.getByRole("status").filter({ hasText: "Due date saved for “Renew the permit”." })).toBeVisible();
});

test("the add row opens the Add dialog with the text as its title", async ({ page }) => {
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("Buy seeds");
  await palette.getByRole("menuitem", { name: /^Add “Buy seeds” as a new item/ }).click();
  await expect(addDialog(page).getByLabel("Title")).toHaveValue("Buy seeds");
});

test("with no text it goes to another heap", async ({ page, heapPath }) => {
  await addHeap(page, heapPath, "Garden");
  await page.goto(heapPath);
  const palette = await openFind(page);
  await palette.getByRole("menuitem", { name: /^Garden/ }).click();
  await expect(page).toHaveURL(/\/heaps\/garden$/);
});

test("Shift+Tab goes back to the results, and Escape closes", async ({ page }) => {
  await addItem(page, "Mow the lawn");
  const palette = await openFind(page);
  await palette.getByRole("searchbox").fill("lawn");
  await expect(palette.getByRole("menuitem", { name: /Mow the lawn/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(palette.getByRole("menuitem", { name: "Start" })).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(palette.getByRole("searchbox")).toHaveValue("lawn");
  await expect(palette.getByRole("menuitem", { name: /Mow the lawn/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(palette).toHaveCount(0);
});

test("the sidebar's Find button opens it", async ({ page }) => {
  test.skip(isPhone(page), "A phone has no sidebar");
  await page.getByRole("button", { name: "Find", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Find", exact: true })).toBeVisible();
});

test("on a phone the top bar's icon opens it, and a tap on a result opens the item", async ({ page, heapPath }) => {
  test.skip(!isPhone(page), "Only a phone has the top bar");
  await addItem(page, "Fix the gate");
  await page.getByRole("button", { name: "Find in all heaps" }).tap();
  const palette = page.getByRole("dialog", { name: "Find", exact: true });
  await expect(palette.getByRole("searchbox", { name: "Find" })).toBeFocused();
  // Above the on-screen keyboard, as the sheets are: the shell keeps its height in `--keyboard-inset`.
  await page.evaluate(() => document.documentElement.style.setProperty("--keyboard-inset", "300px"));
  const above = (page.viewportSize()?.height ?? 0) - 300;
  await expect
    .poll(async () => {
      const box = await palette.boundingBox();
      return box ? Math.round(box.y + box.height) : Infinity;
    })
    .toBeLessThanOrEqual(above);
  await palette.getByRole("searchbox").fill("gate");
  await palette.getByRole("menuitem", { name: /Fix the gate/ }).tap();
  await expect(page).toHaveURL(/\/items\/[^/]+$/);
  await expect(palette).toHaveCount(0);
  // The item took Find's place, so Back goes to the list, not to Find.
  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  await expect(palette).toHaveCount(0);
});

test("on a phone ✕ closes it once the field is empty", async ({ page, heapPath }) => {
  test.skip(!isPhone(page), "Only a phone has the top bar");
  await page.getByRole("button", { name: "Find in all heaps" }).tap();
  const palette = page.getByRole("dialog", { name: "Find", exact: true });
  await palette.getByRole("searchbox").fill("gate");
  await palette.getByRole("button", { name: "Clear" }).tap();
  await expect(palette.getByRole("searchbox")).toHaveValue("");
  await palette.getByRole("button", { name: "Close" }).tap();
  await expect(palette).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
});

test("Back closes it and stays on the page", async ({ page, heapPath }) => {
  const palette = await openFind(page);
  await page.goBack();
  await expect(palette).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  // Escape goes back once only, and the page stays.
  await openFind(page);
  await page.keyboard.press("Escape");
  await expect(palette).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  await expect(page.getByRole("heading", { level: 1 })).toBeAttached();
});
