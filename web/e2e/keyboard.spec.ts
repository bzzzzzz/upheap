import { type Page } from "@playwright/test";
import { addHeaps, addItem, expect, isPhone, itemPane, openItem, row, test } from "./fixtures";

test("b blocks the focused row and moves to the next", async ({ page }) => {
  await addItem(page, "First", { effort: "S", impact: "XL" });
  await addItem(page, "Second", { effort: "XL", impact: "S" });
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "First").getByRole("link", { name: "First", exact: true })).toBeFocused();
  await page.keyboard.press("b");
  await expect(page.getByRole("status").filter({ hasText: "“First” is blocked." })).toBeVisible();
  await expect(row(page, "First")).toHaveCount(0);
  await expect(row(page, "Second").getByRole("link", { name: "Second", exact: true })).toBeFocused();
});

test("? lists every key and Escape closes it", async ({ page }) => {
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("?");
  const dialog = page.getByRole("dialog", { name: "Keyboard shortcuts" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Block", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Find an item, or go to a heap")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("a wide screen shows its keys", async ({ page }, info) => {
  await addItem(page, "Hinted");
  const bar = page.getByRole("region", { name: "Keys on this screen" });
  if (info.project.name !== "desktop") {
    await expect(bar).toBeHidden();
    return;
  }
  await expect(bar).toContainText("All keys");
  await expect(page.getByRole("button", { name: "Find" })).toContainText(/⌘K|Ctrl K/);
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "Hinted").getByTitle("Block")).toBeVisible();
});

/** How far the hint bar's bottom edge is from the window's, in whole pixels. */
async function barGap(page: Page) {
  const box = await page.getByRole("region", { name: "Keys on this screen" }).boundingBox();
  const height = page.viewportSize()?.height ?? 0;
  return Math.round(height - ((box?.y ?? 0) + (box?.height ?? 0)));
}

test("the keys sit at the window's bottom, for a short heap as for a long one", async ({ page, heapPath }, info) => {
  test.skip(info.project.name !== "desktop", "the hint bar shows only on a wide screen with a mouse");
  // Another heap, so select mode is offered. Empty.
  await addHeaps(page, ["Elsewhere"]);
  await expect(page.getByText("Nothing here yet.")).toBeVisible();
  await expect.poll(() => barGap(page)).toBe(0);
  // Short, with an item open and in select mode.
  await addItem(page, "Short");
  await expect.poll(() => barGap(page)).toBe(0);
  await openItem(page, "Short");
  await expect.poll(() => barGap(page)).toBe(0);
  await page.keyboard.press("Escape");
  await expect(itemPane(page)).toBeHidden();
  await page.getByRole("button", { name: "Select", exact: true }).click();
  await expect(page.getByRole("region", { name: "Keys on this screen" })).toContainText("Done selecting");
  await expect.poll(() => barGap(page)).toBe(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("region", { name: "Keys on this screen" })).toContainText("Open");
  // Long: at the window's bottom while scrolled, and right under the last row at the end.
  for (let n = 1; n <= 30; n++) {
    const created = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data: { title: `Long ${n}`, need: "expected" },
    });
    expect(created.status()).toBe(201);
  }
  await page.reload();
  await expect(row(page, "Long 30")).toBeVisible();
  await expect.poll(() => barGap(page)).toBe(0);
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
  await expect.poll(() => barGap(page)).toBe(0);
  // The last row ends above the bar, not under it.
  const lastRowGap = async () => {
    const last = await page.locator("main li").last().boundingBox();
    const bar = await page.getByRole("region", { name: "Keys on this screen" }).boundingBox();
    return Math.round((bar?.y ?? 0) - ((last?.y ?? 0) + (last?.height ?? 0)));
  };
  await expect.poll(lastRowGap).toBeGreaterThanOrEqual(0);
});

test("↓ in the item pane walks to the next item", async ({ page }) => {
  test.skip(isPhone(page), "a phone's sheet has no walk links");
  await addItem(page, "First", { effort: "S", impact: "XL" });
  await addItem(page, "Second", { effort: "XL", impact: "S" });
  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  const title = itemPane(page).getByLabel("Title", { exact: true });
  await expect(title).toHaveValue("First");
  await page.keyboard.press("ArrowDown");
  await expect(title).toHaveValue("Second");
});
