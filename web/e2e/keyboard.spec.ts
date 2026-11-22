import { addItem, expect, isPhone, itemPane, row, test } from "./fixtures";

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
