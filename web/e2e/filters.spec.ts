// The list's one Filters panel (design/ux.md, The list §7), at each width: what the list shows and the filters
// together, with chips for both, and on a phone the sort inside it.
import { addItem, expect, expectOrder, isPhone, row, test } from "./fixtures";
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

test("the blocked layout goes with shown blocked items", async ({ page, heapPath }) => {
  await page.goto(`${heapPath}?blocked=shown&blocked_layout=mixed`);
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on.getByRole("button", { name: "Remove view setting Blocked mixed in" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Filters.*, 2 on$/ })).toBeVisible();

  await on.getByRole("button", { name: "Remove view setting Blocked shown" }).click();
  await expect(on).toHaveCount(0);
  await expect(page).not.toHaveURL(/blocked_layout/);

  // A layout without shown blocked items does nothing, so a link's is dropped.
  await page.goto(`${heapPath}?blocked_layout=mixed`);
  await expect(page.getByRole("button", { name: /^Filters/ })).toBeVisible();
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
  // On a phone the sort is out of sight in the panel, so the button counts it; wider, the select shows it.
  const button = page.getByRole("button", { name: /^Filters/ });
  if (isPhone(page)) {
    await expect(button).toHaveAccessibleName("Filters and sort, 1 on");
  } else {
    await expect(button).toHaveAccessibleName("Filters");
  }
});

test("on a phone, a sort from the link shows on the Filters button", async ({ page, heapPath }) => {
  test.skip(!isPhone(page), "Wider screens show the sort in the toolbar.");
  await page.goto(`${heapPath}?sort=due`);
  const button = page.getByRole("button", { name: /^Filters/ });
  await expect(button).toHaveAccessibleName("Filters and sort, 1 on");
  await expect(button).toContainText("1");
  // Reset all keeps the sort, so it isn't offered for the sort alone.
  const panel = await openFilters(page);
  await expect(panel.getByRole("button", { name: "Reset all" })).toHaveCount(0);
});

test("a tag in a row filters the list, a second tag adds to it, and the same tag again takes it off", async ({
  page,
  heapPath,
}) => {
  const headers = { "x-csrf-protection": "1" };
  const tag = async (name: string) => {
    const created = await page.request.post(`/api/v1${heapPath}/tags`, { headers, data: { name } });
    expect(created.status()).toBe(201);
    return (await created.json()).id as string;
  };
  const item = async (title: string, tag_ids: string[]) => {
    const created = await page.request.post(`/api/v1${heapPath}/items`, {
      headers,
      data: { title, need: "expected", tag_ids },
    });
    expect(created.status()).toBe(201);
  };
  const infra = await tag("infra");
  const docs = await tag("docs");
  await item("Move the database", [infra, docs]);
  await item("Write the guide", [docs]);
  await item("Plain thing", []);
  await page.goto(heapPath);
  await expect(row(page, "Plain thing")).toBeVisible();
  const database = row(page, "Move the database");
  const on = page.getByRole("group", { name: "Filters on" });

  await database.getByRole("button", { name: "Filter by tag infra" }).click();
  await expect(on.getByRole("button", { name: "Remove filter infra" })).toBeVisible();
  await expect(on).toContainText("1 of 3");
  await expect(row(page, "Write the guide")).toHaveCount(0);
  await expect(row(page, "Plain thing")).toHaveCount(0);
  // Pressing a tag never opens the item.
  await expect(page).not.toHaveURL(/\/items\//);
  await expect(database.getByRole("button", { name: "Remove filter tag infra" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  // The filter keeps items with any of its tags, so a second tag adds its items.
  await database.getByRole("button", { name: "Filter by tag docs" }).click();
  await expect(row(page, "Write the guide")).toBeVisible();
  await expect(row(page, "Plain thing")).toHaveCount(0);
  await expect(on).toContainText("2 of 3");

  // The same tag again takes it off.
  await database.getByRole("button", { name: "Remove filter tag infra" }).click();
  await expect(database.getByRole("button", { name: "Filter by tag infra" })).toHaveAttribute("aria-pressed", "false");
  await database.getByRole("button", { name: "Remove filter tag docs" }).click();
  await expect(row(page, "Plain thing")).toBeVisible();
  await expect(on).toHaveCount(0);
});

test("a parameter chip in a row filters the list by its option, and again takes it off", async ({ page, heapPath }) => {
  const headers = { "x-csrf-protection": "1" };
  const created = await page.request.post(`/api/v1${heapPath}/parameters`, {
    headers,
    data: { name: "Energy", options: ["High", "Low"] },
  });
  expect(created.status()).toBe(201);
  const energy = (await created.json()) as { id: string; options: { id: string; label: string }[] };
  const option = (label: string) => energy.options.find((o) => o.label === label)?.id ?? "";
  const item = async (title: string, parameter_values: Record<string, string>) => {
    const made = await page.request.post(`/api/v1${heapPath}/items`, {
      headers,
      data: { title, need: "expected", parameter_values },
    });
    expect(made.status()).toBe(201);
  };
  await item("Deep work", { [energy.id]: option("High") });
  await item("Inbox zero", { [energy.id]: option("Low") });
  await item("No energy set", {});
  await page.goto(heapPath);
  await expect(row(page, "No energy set")).toBeVisible();
  const deep = row(page, "Deep work");
  const on = page.getByRole("group", { name: "Filters on" });

  await deep.getByRole("button", { name: "Filter by Energy: High" }).click();
  await expect(on).toContainText("1 of 3");
  await expect(row(page, "Inbox zero")).toHaveCount(0);
  await expect(row(page, "No energy set")).toHaveCount(0);
  // Pressing a chip never opens the item.
  await expect(page).not.toHaveURL(/\/items\//);
  await expect(deep.getByRole("button", { name: "Remove filter Energy: High" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await deep.getByRole("button", { name: "Remove filter Energy: High" }).click();
  await expect(row(page, "No energy set")).toBeVisible();
  await expect(on).toHaveCount(0);
});

test("the list sorts by need, most needed first", async ({ page }) => {
  await addItem(page, "An idea", { need: "Idea" });
  await addItem(page, "Nice extra", { need: "Nice to have" });
  await addItem(page, "Must happen", { need: "Critical" });
  await addItem(page, "Someone waits", { need: "Expected" });

  const panel = await openFilters(page);
  if (isPhone(page)) {
    await panel.getByRole("radiogroup", { name: "Sort by" }).getByText("Most needed", { exact: true }).click();
    await panel.getByRole("button", { name: /^Show 4 items$/ }).click();
  } else {
    await panel.getByRole("button", { name: "Close" }).click();
    await page.getByLabel("Sort by").selectOption({ label: "Most needed" });
  }
  await expect(page).toHaveURL(/sort=need/);
  await expectOrder(page, ["Must happen", "Someone waits", "Nice extra", "An idea"]);
});
