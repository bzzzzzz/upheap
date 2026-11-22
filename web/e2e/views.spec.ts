// Saved views in the Filters panel (ux.md, The list §6), at each width: save the list's settings, apply a
// view, and rename, update and archive it.
import { addItem, expect, isPhone, row, test } from "./fixtures";
import type { Page } from "@playwright/test";

/** Opens the panel: Filters on a tablet or wide screen, Filters & sort on a phone. */
async function openFilters(page: Page) {
  await page.getByRole("button", { name: /^Filters/ }).click();
  const panel = page.getByRole("dialog", { name: isPhone(page) ? "Filters & sort" : "Filters" });
  await expect(panel).toBeVisible();
  return panel;
}

/** Closes the panel, which keeps the page behind it out of reach, the toast too. */
async function closePanel(page: Page) {
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

test("a view is saved from the list's settings and applied again", async ({ page }) => {
  await addItem(page, "Fix the outage", { need: "Critical" });
  await addItem(page, "Try a dark theme", { need: "Idea" });

  const panel = await openFilters(page);
  await panel.getByRole("radiogroup", { name: "Blocked" }).getByText("Shown", { exact: true }).click();
  await panel.getByRole("group", { name: "Need" }).getByRole("button", { name: "Critical", exact: true }).click();
  await panel.getByRole("button", { name: "Save as view…" }).click();
  await panel.getByLabel("View name").fill("Outages");
  await panel.getByLabel("View name").press("Enter");
  await expect(panel.getByRole("button", { name: "Outages, current view", exact: true })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Save as view…" })).toBeFocused();

  await panel.getByRole("button", { name: "Reset all" }).click();
  await expect(panel.getByRole("button", { name: "Outages", exact: true })).toBeVisible();
  await expect(row(page, "Try a dark theme")).toBeVisible();

  await panel.getByRole("button", { name: "Outages", exact: true }).click();
  const on = page.getByRole("group", { name: "Filters on" });
  if (isPhone(page)) {
    await expect(panel).toHaveCount(0);
  } else {
    await expect(panel.getByRole("button", { name: "Outages, current view", exact: true })).toBeVisible();
  }
  await expect(on.getByRole("button", { name: "Remove filter Need: Critical" })).toBeVisible();
  await expect(on.getByRole("button", { name: "Remove view setting Blocked shown" })).toBeVisible();
  await expect(row(page, "Try a dark theme")).toHaveCount(0);
});

test("applying a view clears the agent filters and keeps the search", async ({ page, heapPath }) => {
  await addItem(page, "Fix the outage", { need: "Critical" });
  await page.goto(`${heapPath}?need=critical`);
  let panel = await openFilters(page);
  await panel.getByRole("button", { name: "Save as view…" }).click();
  await panel.getByLabel("View name").fill("Outages");
  await panel.getByLabel("View name").press("Enter");
  await expect(panel.getByRole("button", { name: "Outages, current view", exact: true })).toBeVisible();
  await closePanel(page);

  await page.goto(`${heapPath}?need=critical&agent=none`);
  await page.getByRole("searchbox", { name: "Search items" }).fill("outage");
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on.getByRole("button", { name: "Remove filter Agent: Not with an agent" })).toBeVisible();

  panel = await openFilters(page);
  await expect(panel.getByRole("button", { name: "Outages", exact: true })).toBeVisible();
  await panel.getByRole("button", { name: "Outages", exact: true }).click();
  await expect(on.getByRole("button", { name: /^Remove filter Agent/ })).toHaveCount(0);
  await expect(page.getByRole("searchbox", { name: "Search items" })).toHaveValue("outage");
});

test("a view is renamed and updated to the list's settings", async ({ page }) => {
  await addItem(page, "Fix the outage", { need: "Critical" });
  const panel = await openFilters(page);
  await panel.getByRole("button", { name: "Save as view…" }).click();
  await panel.getByLabel("View name").fill("Everything");
  await panel.getByLabel("View name").press("Enter");
  await expect(panel.getByRole("button", { name: "Everything, current view", exact: true })).toBeVisible();

  await panel.getByRole("button", { name: "More for Everything" }).click();
  await page.getByRole("menuitem", { name: "Rename…" }).click();
  await panel.getByLabel("View name").fill("Anything");
  await panel.getByLabel("View name").press("Enter");
  await expect(panel.getByRole("button", { name: "Anything, current view", exact: true })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Anything, current view" })).toBeFocused();

  await panel.getByRole("group", { name: "Need" }).getByRole("button", { name: "Critical", exact: true }).click();
  await expect(panel.getByRole("button", { name: "Anything", exact: true })).toBeVisible();
  await panel.getByRole("button", { name: "More for Anything" }).click();
  await page.getByRole("menuitem", { name: "Update to current settings" }).click();
  // Updating closes the panel, so its toast can be seen and heard.
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByText("Updated Anything.")).toBeVisible();
  const reopened = await openFilters(page);
  await expect(reopened.getByRole("button", { name: "Anything, current view", exact: true })).toBeVisible();
});

test("an archived view comes back with Undo", async ({ page }) => {
  let panel = await openFilters(page);
  await panel.getByRole("button", { name: "Save as view…" }).click();
  await panel.getByLabel("View name").fill("Old");
  await panel.getByLabel("View name").press("Enter");
  await expect(panel.getByRole("button", { name: "Old, current view", exact: true })).toBeVisible();

  await panel.getByRole("button", { name: "More for Old" }).click();
  await page.getByRole("menuitem", { name: "Archive" }).click();
  // Archiving closes the panel, so the toast's Undo is in reach.
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByText("Archived Old.")).toBeVisible();

  await page.getByRole("button", { name: "Undo" }).click();
  panel = await openFilters(page);
  await expect(panel.getByRole("button", { name: "Old, current view", exact: true })).toBeVisible();
});
