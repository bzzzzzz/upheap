// Team tags (design/ux.md, Tags): made on the Team page, picked and filtered in every heap of the team, renamed in one
// place, and listed on a heap's Tags tab.
import type { Page } from "@playwright/test";

import { addHeaps, addItem, closeItem, expect, fieldButton, goTo, itemPane, openItem, row, test } from "./fixtures";

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

  // Renaming it on the Team page says where it reaches. The page is reached from Beta's Tags tab, in the app, so
  // what Beta's list already holds is kept, and only the rename's cache refresh can bring the new name to it.
  await goTo(page, "Tags");
  await page.getByRole("link", { name: "Team page" }).first().click();
  await expect(page).toHaveURL(new RegExp(`/teams/${team}#team-tags$`));
  await tags.getByRole("button", { name: "Edit security" }).click();
  await expect(tags.getByText(/^Changes in every heap of /)).toBeVisible();
  await tags.getByLabel("Name of tag security").fill("security-review");
  await tags.getByRole("button", { name: "Save" }).click();
  await expect(tags.getByRole("button", { name: "Edit security-review" })).toBeVisible();

  // Back in Beta through the Team page's link (its name differs on a phone, where the whole row is the link).
  await page.getByRole("link", { name: /^(Open )?Beta\b/ }).click();
  await expect(page).toHaveURL(new RegExp(`${heapB}$`));
  await expect(row(page, "Beta work").getByRole("button", { name: "Filter by tag security-review" })).toBeVisible();

  // The heap's Tags tab lists it in the team's card.
  await goTo(page, "Tags");
  await expect(page.getByRole("region", { name: /^1 tag of the team / })).toContainText("security-review");

  // Alpha, opened afresh, shows it too.
  await page.goto(heapA);
  await expect(row(page, "Alpha work").getByRole("button", { name: "Filter by tag security-review" })).toBeVisible();
});

/** In the heap at `path`, makes the heap tag `name` through the item's Tags field and puts it on a new item `title`. */
async function heapTagOnNewItem(page: Page, path: string, title: string, name: string) {
  await page.goto(path);
  await addItem(page, title);
  await openItem(page, title);
  await fieldButton(page, "Tags").click();
  await itemPane(page).getByRole("combobox", { name: "Add a tag" }).fill(name);
  await page.getByRole("option", { name: `Create heap tag “${name}”` }).click();
  await expect(itemPane(page).getByRole("button", { name: `Remove tag ${name}` })).toBeVisible();
  await closeItem(page);
}

test("a heap tag is made a team tag after the merge is confirmed, and both heaps' items carry it", async ({
  page,
  heapPath,
}) => {
  const heapA = heapPath;
  const team = /\/teams\/([^/]+)\//.exec(heapA)?.[1] ?? "";
  await addHeaps(page, ["Beta"]);
  const heapB = `/teams/${team}/heaps/beta`;
  await heapTagOnNewItem(page, heapA, "Alpha invoice", "billing");
  await heapTagOnNewItem(page, heapB, "Beta invoice", "billing");

  // The ⋯ on the heap tag opens the confirm, which lists both heaps' tags and marks this heap.
  await page.goto(heapA);
  await goTo(page, "Tags");
  await page.getByRole("button", { name: "More for billing" }).click();
  await page.getByRole("menuitem", { name: "Make a team tag" }).click();
  const confirm = page.getByRole("alertdialog", { name: "Make “billing” a team tag?" });
  const rows = confirm.getByRole("listitem");
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: "this heap" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "1 item" })).toHaveCount(2);
  await expect(confirm).toContainText("2 items in 2 heaps will carry the team tag.");

  // The toast exists only once the server has merged them.
  await confirm.getByRole("button", { name: "Make team tag" }).click();
  await expect(page.getByRole("status").filter({ hasText: "is a team tag now" })).toContainText(
    "“billing” is a team tag now: 2 items in 2 heaps carry it.",
  );
  await expect(page.getByRole("region", { name: /^1 tag of the team / })).toContainText("billing");
  await expect(page.getByText("No tags of this heap yet.")).toBeVisible();

  // Beta's item carries the team tag now, and Beta's Tags tab lists it in the team's card.
  await page.goto(heapB);
  await openItem(page, "Beta invoice");
  await fieldButton(page, "Tags").click();
  await expect(itemPane(page).getByRole("button", { name: "Remove tag billing" })).toBeVisible();
  await closeItem(page);
  await goTo(page, "Tags");
  await expect(page.getByRole("region", { name: /^1 tag of the team / })).toContainText("billing");
  await expect(page.getByText("No tags of this heap yet.")).toBeVisible();
});
