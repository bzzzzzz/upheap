import { addItem, drag, expect, itemPane, openItem, row, test } from "./fixtures";

// Gestures for a finger (ux.md, Phone details), on the phone and tablet projects, where the pointer is coarse.
test.beforeEach(({ isMobile }) => {
  test.skip(!isMobile, "Swipes and drags are for touch screens.");
});

test("a row swipes right to mark it done, and left for its other states", async ({ page }) => {
  await addItem(page, "Water the plants");
  await addItem(page, "Call the bank");

  await drag(page, row(page, "Water the plants"), 200, 0);
  await expect(row(page, "Water the plants")).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "“Water the plants” is done." })).toBeVisible();

  await drag(page, row(page, "Call the bank"), -150, 0);
  await row(page, "Call the bank").getByRole("button", { name: "In progress", exact: true }).click();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "“Call the bank” is in progress." })).toBeVisible();
});

test("a short swipe doesn't open the item, and the open item drags away", async ({ page, viewport }) => {
  await addItem(page, "Sort the post");

  // A swipe that doesn't go far enough springs back, without opening the item.
  await drag(page, row(page, "Sort the post"), 40, 0);
  await expect(itemPane(page)).toHaveCount(0);
  await expect(row(page, "Sort the post")).toBeVisible();

  // The phone's sheet drags down to close, the tablet's panel right.
  await openItem(page, "Sort the post");
  const phone = (viewport?.width ?? 0) < 640;
  await drag(page, itemPane(page).getByRole("heading", { name: "Item" }), phone ? 0 : 250, phone ? 250 : 0);
  await expect(itemPane(page)).toHaveCount(0);
  await expect(row(page, "Sort the post")).toBeVisible();
});

test("Filters open as a sheet on a phone and a popover on a tablet", async ({ page, viewport }) => {
  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await expect(filters).toBeVisible();
  const box = await filters.boundingBox();
  const height = viewport?.height ?? 0;
  const width = viewport?.width ?? 0;
  if (width < 640) {
    // A sheet reaches the bottom of the screen.
    expect(Math.round((box?.y ?? 0) + (box?.height ?? 0))).toBeGreaterThanOrEqual(height - 40);
  } else {
    // A popover is narrower than the screen and stops short of its edges; its height follows the filters it holds, up
    // to a limit, past which it scrolls. The dialog is the content, so the popover holding it is what is measured.
    const popover = await filters.locator("..").boundingBox();
    expect(Math.round(popover?.width ?? width)).toBeLessThan(width - 100);
    expect(Math.round((popover?.y ?? 0) + (popover?.height ?? height))).toBeLessThan(height);
  }
  await filters.getByRole("button", { name: "Close" }).click();
  await expect(filters).toHaveCount(0);
});
