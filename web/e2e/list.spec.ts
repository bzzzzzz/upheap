import { addItem, expect, itemPane, openItem, row, test } from "./fixtures";

// One step ahead in the list and the item (design/ux.md, The list and The item), with a mouse: the
// rows' play and done buttons, shown on hover, don't show on a touch screen, which swipes instead.
test.beforeEach(({ isMobile }) => {
  test.skip(isMobile, "The done button and the pane's ↑ ↓ are for a wide screen with a mouse.");
});

test("the play button starts an item, and Done offers to start the next one", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });

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
  await row(page, "Water the plants").getByRole("button", { name: "Start Water the plants" }).hover();
  await row(page, "Water the plants").getByRole("button", { name: "Mark Water the plants done" }).click();
  await expect(page.getByRole("status").filter({ hasText: "“Water the plants” is done." })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Start next/ })).toHaveCount(0);

  // Slog counts even while a filter leaves it out of the list.
  await addItem(page, "Tidy up", { effort: "S" });
  await addItem(page, "Quick fix", { effort: "S" });
  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await filters.getByRole("group", { name: "Effort" }).getByRole("button", { name: "S", exact: true }).click();
  await filters.getByRole("button", { name: "Close" }).click();
  await expect(row(page, "Slog")).toHaveCount(0);
  await row(page, "Tidy up").getByRole("button", { name: "Start Tidy up" }).hover();
  await row(page, "Tidy up").getByRole("button", { name: "Mark Tidy up done" }).click();
  await expect(page.getByRole("status").filter({ hasText: "“Tidy up” is done." })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Start next/ })).toHaveCount(0);
  await page.getByRole("group", { name: "Filters on" }).getByRole("button", { name: "Remove filter Effort S" }).click();
});

test("hovering or focusing the play button shows the done button beside it, and s starts a focused row", async ({
  page,
}) => {
  await addItem(page, "Quick win");
  await addItem(page, "Slog");
  const quick = row(page, "Quick win");
  const done = quick.getByRole("button", { name: "Mark Quick win done" });
  const play = quick.getByRole("button", { name: "Start Quick win" });
  const slot = done.locator("..");

  // Hidden until the play button is hovered, but still in the layout.
  await expect(slot).toHaveCSS("opacity", "0");
  await play.hover();
  await expect(slot).toHaveCSS("opacity", "1");

  // Tab order: play, done, then the row link.
  await page.mouse.move(0, 0);
  await play.focus();
  await page.keyboard.press("Tab");
  await expect(done).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(quick.getByRole("link", { name: "Quick win", exact: true })).toBeFocused();

  // s starts the focused row, and the focus stays on it.
  await page.keyboard.press("s");
  await expect(page.getByRole("status").filter({ hasText: "“Quick win” is in progress." })).toBeVisible();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  // The row moves up into In progress, and the focus follows it.
  await expect(
    page.getByRole("region", { name: "In progress" }).getByRole("link", { name: "Quick win", exact: true }),
  ).toBeFocused();
});

test("the item pane walks the list, and the link sign by its title copies the link", async ({ page, heapPath }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  const pane = itemPane(page);

  await openItem(page, "Quick win");
  await expect(pane.getByRole("link", { name: "Previous item" })).toHaveAttribute("aria-disabled", "true");
  // From the keyboard, focus stays on the arrow, there for the next step, even once it's at the end.
  await pane.getByRole("link", { name: "Next item" }).focus();
  await page.keyboard.press("Enter");
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Slog");
  await expect(pane.getByRole("link", { name: "Next item" })).toHaveAttribute("aria-disabled", "true");
  await expect(pane.getByRole("link", { name: "Next item" })).toBeFocused();

  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  await expect(pane.getByText(key, { exact: true })).toBeVisible();
  await expect(pane.getByRole("button", { name: `Copy the link to ${key}` })).toBeVisible();

  // Walking replaces the item rather than adding to the history, so it still closes in one step.
  await pane.getByRole("link", { name: "Previous item" }).click();
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Quick win");
  await page.goBack();
  await expect(pane).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
});

test("filters show as chips with what they leave out, and each chip takes its filter off", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });

  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await filters.getByRole("group", { name: "Effort" }).getByRole("button", { name: "S", exact: true }).click();
  await filters.getByRole("button", { name: "Close" }).click();

  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on).toContainText("1 of 2");
  await expect(row(page, "Slog")).toHaveCount(0);

  // A search narrows the rows but not the count, which says what the filters leave out.
  await page.getByLabel("Search items").fill("no such item");
  await expect(row(page, "Quick win")).toHaveCount(0);
  await expect(on).toContainText("1 of 2");
  await page.getByLabel("Search items").fill("");

  // A chip takes its own filter off.
  await on.getByRole("button", { name: "Remove filter Effort S" }).click();
  await expect(on).toHaveCount(0);
  await expect(row(page, "Slog")).toBeVisible();
});
