import { addItem, closeItem, expect, itemPane, openItem, row, test } from "./fixtures";

// One step ahead in the list and the item (design/ux-overview.md, What "one step ahead" means), with a mouse: the
// rows' hover actions don't show on a touch screen, which swipes instead.
test.beforeEach(({ isMobile }) => {
  test.skip(isMobile, "Hover actions and the pane's ↑ ↓ are for a wide screen with a mouse.");
});

test("a hovered row starts its item, and Done offers to start the next one", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });

  await row(page, "Quick win").hover();
  await row(page, "Quick win").getByRole("button", { name: "Start Quick win" }).click();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  // Once it has moved up into In progress.
  await expect(row(page, "Quick win")).toHaveCount(1);

  // Nothing else is in progress, so the toast offers the top open item.
  await row(page, "Quick win").getByRole("button", { name: "Mark Quick win done" }).click();
  await page.getByRole("button", { name: "Start next: Slog" }).click();
  await expect(page.getByRole("status").filter({ hasText: "“Slog” is in progress." })).toBeVisible();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();

  // With Slog under way, finishing something else offers nothing more.
  await addItem(page, "Water the plants");
  await row(page, "Water the plants").getByRole("button", { name: "Mark Water the plants done" }).click();
  await expect(page.getByRole("status").filter({ hasText: "“Water the plants” is done." })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Start next/ })).toHaveCount(0);

  // ⋯ has the other states.
  await row(page, "Slog").hover();
  await row(page, "Slog").getByRole("button", { name: "More for Slog" }).click();
  await page.getByRole("menuitem", { name: "Won't do" }).click();
  await expect(row(page, "Slog")).toHaveCount(0);
});

test("the item pane walks the list, and its key copies the link", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  const pane = itemPane(page);

  await openItem(page, "Quick win");
  await expect(pane.getByRole("link", { name: "Previous item" })).toHaveAttribute("aria-disabled", "true");
  await pane.getByRole("link", { name: "Next item" }).click();
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Slog");
  await expect(pane.getByRole("link", { name: "Next item" })).toHaveAttribute("aria-disabled", "true");

  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  await expect(pane.getByRole("button", { name: key, exact: true })).toBeVisible();

  // Walking replaces the item rather than adding to the history, so it still closes in one step.
  await pane.getByRole("link", { name: "Previous item" }).click();
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Quick win");
  await closeItem(page);
});

test("filters show as chips with what they leave out, and a changed view can be undone", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });

  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await filters.getByRole("group", { name: "Effort" }).getByRole("button", { name: "S", exact: true }).click();
  await filters.getByRole("button", { name: "Close" }).click();

  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on).toContainText("1 of 2");
  await expect(row(page, "Slog")).toHaveCount(0);

  // The changed view's chip offers to undo the change.
  await page.getByRole("button", { name: "Default (changed)" }).click();
  await page.getByRole("menuitem", { name: "Undo changes" }).click();
  await expect(on).toHaveCount(0);
  await expect(row(page, "Slog")).toBeVisible();

  // A chip takes its own filter off.
  await page.getByRole("button", { name: "Filters" }).click();
  await filters.getByRole("group", { name: "Effort" }).getByRole("button", { name: "S", exact: true }).click();
  await filters.getByRole("button", { name: "Close" }).click();
  await on.getByRole("button", { name: "Remove filter Effort S" }).click();
  await expect(on).toHaveCount(0);
  await expect(row(page, "Slog")).toBeVisible();
});
