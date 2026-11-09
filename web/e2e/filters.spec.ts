// The list's one Filters panel (design/app-ux.md, The list §7), at each width: what the list shows and the filters
// together, with chips for both, and on a phone the sort inside it.
import { addItem, expect, isPhone, row, test } from "./fixtures";
import type { Page } from "@playwright/test";

/** Opens the panel: Filters on a tablet or wide screen, Filters & sort on a phone. */
async function openFilters(page: Page) {
  await page.getByRole("button", { name: /^Filters/ }).click();
  const panel = page.getByRole("dialog", { name: isPhone(page) ? "Filters & sort" : "Filters" });
  await expect(panel).toBeVisible();
  return panel;
}

test("one panel sets what the list shows and filters it, with a chip for each change", async ({ page }) => {
  await addItem(page, "Fix the outage", { need: "Critical" });
  await addItem(page, "Try a dark theme", { need: "Idea" });

  const panel = await openFilters(page);
  await panel.getByRole("radiogroup", { name: "Blocked" }).getByText("Shown", { exact: true }).click();
  await panel.getByRole("group", { name: "Need" }).getByRole("button", { name: "Critical", exact: true }).click();
  if (isPhone(page)) {
    await panel.getByRole("button", { name: "Show 1 of 2 items" }).click();
  } else {
    await expect(panel).toContainText("1 of 2 items");
    await panel.getByRole("button", { name: "Done" }).click();
  }
  await expect(panel).toHaveCount(0);
  await expect(row(page, "Try a dark theme")).toHaveCount(0);

  // The button counts both, and each has its chip.
  await expect(page.getByRole("button", { name: /^Filters.*, 2 on$/ })).toBeVisible();
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on).toContainText("1 of 2");
  await on.getByRole("button", { name: "Remove view setting Blocked shown" }).click();
  await expect(on.getByRole("button", { name: /^Remove view setting/ })).toHaveCount(0);
  // Reset all takes the filter off too.
  await on.getByRole("button", { name: "Reset all" }).click();
  await expect(on).toHaveCount(0);
  await expect(row(page, "Try a dark theme")).toBeVisible();

  // A view setting alone shows its chip, without a count, and Reset all in the panel puts it back.
  const again = await openFilters(page);
  await again.getByRole("radiogroup", { name: "In progress" }).getByText("Hidden", { exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(again).toHaveCount(0);
  await expect(on.getByRole("button", { name: "Remove view setting In progress hidden" })).toBeVisible();
  await expect(on).not.toContainText(" of ");
  await (await openFilters(page)).getByRole("button", { name: "Reset all" }).click();
  await page.keyboard.press("Escape");
  await expect(on).toHaveCount(0);
});

test("the need filter comes from the link, as the server applies it", async ({ page, heapPath }) => {
  await addItem(page, "Fix the outage", { need: "Critical" });
  await addItem(page, "Answer the customer", { need: "Expected" });
  await addItem(page, "Try a dark theme", { need: "Idea" });

  await page.goto(`${heapPath}?need=critical,expected,urgent`);
  await expect(row(page, "Fix the outage")).toBeVisible();
  await expect(row(page, "Answer the customer")).toBeVisible();
  await expect(row(page, "Try a dark theme")).toHaveCount(0);
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on.getByRole("button", { name: "Remove filter Need: Expected" })).toBeVisible();
  await on.getByRole("button", { name: "Remove filter Need: Critical" }).click();
  await expect(row(page, "Fix the outage")).toHaveCount(0);
  await expect(on).toContainText("1 of 3");
});

test("the sort sits in the toolbar, and on a phone at the top of the panel", async ({ page }) => {
  await addItem(page, "Slog", { effort: "XL" });
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });

  const panel = await openFilters(page);
  if (isPhone(page)) {
    await expect(page.getByLabel("Sort by", { exact: true }).and(page.locator("select"))).toBeHidden();
    await expect(panel.getByRole("heading", { level: 3 }).first()).toHaveText("Sort by");
    await panel.getByRole("radiogroup", { name: "Sort by" }).getByText("Largest impact", { exact: true }).click();
    await panel.getByRole("button", { name: /^Show 2 items$/ }).click();
  } else {
    await expect(panel.getByRole("radiogroup", { name: "Sort by" })).toHaveCount(0);
    await panel.getByRole("button", { name: "Close" }).click();
    await page.getByLabel("Sort by").selectOption({ label: "Largest impact" });
  }
  await expect(page).toHaveURL(/sort=impact/);
  await expect(page.getByRole("group", { name: "Filters on" })).toHaveCount(0);
});
