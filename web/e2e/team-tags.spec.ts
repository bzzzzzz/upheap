// Team tags (design/ux.md, Tags): made on the Team page, picked and filtered in every heap of the team, renamed in one
// place, and listed on a heap's Tags tab.
import type { Page } from "@playwright/test";

import { addHeaps, addItem, closeItem, expect, fieldButton, itemPane, openItem, row, test } from "./fixtures";

/** Puts the team tag `name` on the item `title` through the item's Tags field, picking it from what `typed` finds. */
async function tagWith(page: Page, title: string, typed: string, name: string) {
  await openItem(page, title);
  await fieldButton(page, "Tags").click();
  const box = itemPane(page).getByRole("combobox", { name: "Add a tag" });
  await box.fill(typed);
  await page.getByRole("option", { name: new RegExp(`^${name}\\s?, team tag`) }).click();
  await expect(itemPane(page).getByRole("button", { name: `Remove tag ${name}` })).toBeVisible();
}

/** Filters the list by the team tag `name`, and expects `shown` on it and `hidden` not. */
async function filterByTeamTag(page: Page, name: string, shown: string, hidden: string) {
  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await filters
    .getByRole("group", { name: "Tags (any)" })
    // Chrome puts a space before the hidden ", team tag" it reads after the name.
    .getByRole("button", { name: new RegExp(`^${name}\\s?, team tag$`) })
    .click();
  await filters.getByRole("button", { name: "Close" }).click();
  await expect(filters).toHaveCount(0);
  await expect(row(page, shown)).toBeVisible();
  await expect(row(page, hidden)).toHaveCount(0);
}

test("a team tag is made on the Team page, used and filtered in two heaps, and renamed for both", async ({
  page,
  heapPath,
}) => {
  const heapA = heapPath;
  const team = /\/teams\/([^/]+)\//.exec(heapA)?.[1] ?? "";
  await addHeaps(page, ["Beta"]);
  const heapB = `/teams/${team}/heaps/beta`;

  // The team tag is made on the Team page. The row only exists once the server has answered.
  await page.goto(`/teams/${team}`);
  const tags = page.getByRole("region", { name: "Tags", exact: true });
  await tags.getByLabel("New team tag name").fill("security");
  await tags.getByLabel("New team tag description").fill("Anything about access");
  await tags.getByRole("button", { name: "Add tag" }).click();
  await expect(tags.getByRole("button", { name: "Edit security" })).toBeVisible();
  await expect(tags).toContainText("Anything about access");

  // Each heap puts it on an item and filters by it.
  for (const [path, tagged, plain] of [
    [heapA, "Alpha work", "Alpha plain"],
    [heapB, "Beta work", "Beta plain"],
  ] as const) {
    await page.goto(path);
    await addItem(page, tagged);
    await addItem(page, plain);
    await tagWith(page, tagged, "sec", "security");
    await closeItem(page);
    await filterByTeamTag(page, "security", tagged, plain);
  }

  // Renaming it on the Team page says where it reaches.
  await page.goto(`/teams/${team}`);
  await tags.getByRole("button", { name: "Edit security" }).click();
  await expect(tags.getByText(/^Changes in every heap of /)).toBeVisible();
  await tags.getByLabel("Name of tag security").fill("security-review");
  await tags.getByRole("button", { name: "Save" }).click();
  await expect(tags.getByRole("button", { name: "Edit security-review" })).toBeVisible();

  for (const [path, tagged] of [
    [heapA, "Alpha work"],
    [heapB, "Beta work"],
  ] as const) {
    await page.goto(path);
    await expect(row(page, tagged).getByRole("button", { name: "Filter by tag security-review" })).toBeVisible();
  }

  // The heap's Tags tab lists it in the team's card.
  await page.goto(`${heapB}/settings/tags`);
  await expect(page.getByRole("region", { name: /^1 tag of the team / })).toContainText("security-review");
});
