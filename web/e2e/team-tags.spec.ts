// Team tags (design/ux.md, Tags): made on the Team page, picked and filtered in every heap of the team, renamed in one
// place, and listed on a heap's Tags tab.
import type { Page } from "@playwright/test";

import {
  addHeaps,
  addItem,
  closeItem,
  expect,
  fieldButton,
  goTo,
  itemPane,
  moreEntry,
  openItem,
  openMore,
  row,
  test,
} from "./fixtures";

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

test("a team tag's History reads its rename on the Team page and on a heap's Tags tab", async ({ page, heapPath }) => {
  const team = /\/teams\/([^/]+)\//.exec(heapPath)?.[1] ?? "";

  // Made and renamed on the Team page. The renamed row only exists once the server has answered.
  await page.goto(`/teams/${team}`);
  const tags = page.getByRole("region", { name: "Tags", exact: true });
  await tags.getByLabel("New team tag name").fill("security");
  await tags.getByRole("button", { name: "Add tag" }).click();
  await tags.getByRole("button", { name: "Edit security" }).click();
  await tags.getByLabel("Name of tag security").fill("security-review");
  await tags.getByRole("button", { name: "Save" }).click();
  await expect(tags.getByRole("button", { name: "Edit security-review" })).toBeVisible();

  // An editor finds History in the row's form.
  await tags.getByRole("button", { name: "Edit security-review" }).click();
  await tags.getByText("History", { exact: true }).click();
  await expect(tags.getByText("You edited tag security-review")).toBeVisible();
  await expect(tags.getByText("Name: security → security-review")).toBeVisible();

  // The heap's Tags tab shows the same rename in the team card.
  await page.goto(heapPath);
  await goTo(page, "Tags");
  const card = page.getByRole("region", { name: /^1 tag of the team / });
  await card.getByRole("button", { name: "Edit security-review" }).click();
  await card.getByText("History", { exact: true }).click();
  await expect(card.getByText("You edited tag security-review")).toBeVisible();
  await expect(card.getByText("Name: security → security-review")).toBeVisible();
});

/** Makes the heap tag `name` in the heap at `path` through the API. */
async function addHeapTag(page: Page, path: string, name: string) {
  const made = await page.request.post(`/api/v1${path}/tags`, {
    headers: { "x-csrf-protection": "1" },
    data: { name },
  });
  expect(made.status()).toBe(201);
}

test("the team tags path: made once, used in two heaps, renamed, moved, and heap tags promoted for a third", async ({
  page,
  heapPath,
}) => {
  // Arrange: heaps A (the personal one), B and C of one team. B has a heap tag infra, as A will.
  const heapA = heapPath;
  const team = /\/teams\/([^/]+)\//.exec(heapA)?.[1] ?? "";
  await addHeaps(page, ["Beta", "Gamma"]);
  const heapB = `/teams/${team}/heaps/beta`;
  const heapC = `/teams/${team}/heaps/gamma`;
  await addHeapTag(page, heapB, "infra");

  // Step 1: the team tag is made on the Team page.
  await page.goto(`/teams/${team}`);
  const tags = page.getByRole("region", { name: "Tags", exact: true });
  await tags.getByLabel("New team tag name").fill("security");
  await tags.getByLabel("New team tag description").fill("Anything about access");
  await tags.getByRole("button", { name: "Add tag" }).click();
  await expect(tags.getByRole("button", { name: "Edit security" })).toBeVisible();

  // Step 2: in A, an item takes the team tag from what "sec" finds, and a heap tag infra made on it.
  await page.goto(heapA);
  await addItem(page, "Rotate the certs");
  await addItem(page, "Alpha plain");
  await tagWith(page, "Rotate the certs", "sec", "security");
  await itemPane(page).getByRole("combobox", { name: "Add a tag" }).fill("infra");
  await page.getByRole("option", { name: "Create heap tag “infra”" }).click();
  await expect(itemPane(page).getByRole("button", { name: "Remove tag infra" })).toBeVisible();
  await closeItem(page);

  // Step 3: in B, which never made a tag of that name, another item takes it.
  await page.goto(heapB);
  await addItem(page, "Review the VPN");
  await addItem(page, "Beta plain");
  await tagWith(page, "Review the VPN", "sec", "security");
  await closeItem(page);

  // Step 4: B's list and A's, filtered by it, each show their own item.
  await filterByTeamTag(page, "security", "Review the VPN", "Beta plain");
  await page.goto(heapA);
  await filterByTeamTag(page, "security", "Rotate the certs", "Alpha plain");

  // Step 5: renamed once on the Team page, it shows the new name in both heaps.
  await page.goto(`/teams/${team}`);
  await tags.getByRole("button", { name: "Edit security" }).click();
  await tags.getByLabel("Name of tag security").fill("security-review");
  await tags.getByRole("button", { name: "Save" }).click();
  await expect(tags.getByRole("button", { name: "Edit security-review" })).toBeVisible();
  for (const [path, title] of [
    [heapA, "Rotate the certs"],
    [heapB, "Review the VPN"],
  ] as const) {
    await page.goto(path);
    await expect(row(page, title).getByRole("button", { name: "Filter by tag security-review" })).toBeVisible();
  }

  // Step 6: A's item, moved to B, keeps the team tag, and its heap tag infra lands on B's infra.
  await page.goto(heapA);
  await openItem(page, "Rotate the certs");
  await moreEntry(page, await openMore(page), "Move to another heap…").click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await picker.getByRole("menuitem", { name: "Beta" }).click();
  await picker.getByRole("button", { name: "Move", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Moved to Beta." })).toBeVisible();
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: infra, security-review");
  await closeItem(page);
  await filterByTeamTag(page, "security-review", "Rotate the certs", "Beta plain");
  await expect(row(page, "Review the VPN")).toBeVisible();

  // Step 7: A and B each have a heap tag billing on an item. Promoted from A's Tags tab once the merge is confirmed,
  // it's the one team tag on both items, and C, which had none, can use it.
  await heapTagOnNewItem(page, heapA, "Send the invoices", "billing");
  await heapTagOnNewItem(page, heapB, "Chase the payments", "billing");
  await page.goto(heapA);
  await goTo(page, "Tags");
  await page.getByRole("button", { name: "More for billing" }).click();
  await page.getByRole("menuitem", { name: "Make a team tag" }).click();
  const confirm = page.getByRole("alertdialog", { name: "Make “billing” a team tag?" });
  await expect(confirm).toContainText("2 items in 2 heaps will carry the team tag.");
  await confirm.getByRole("button", { name: "Make team tag" }).click();
  await expect(page.getByRole("status").filter({ hasText: "is a team tag now" })).toBeVisible();
  for (const [path, tagged, plain] of [
    [heapA, "Send the invoices", "Alpha plain"],
    [heapB, "Chase the payments", "Beta plain"],
  ] as const) {
    await page.goto(path);
    await filterByTeamTag(page, "billing", tagged, plain);
  }
  await page.goto(heapC);
  await addItem(page, "Reconcile the accounts");
  await tagWith(page, "Reconcile the accounts", "bill", "billing");
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

  // Each heap's item carries the team tag now: filtering by the team tag finds it. Beta's Tags tab lists it in the
  // team's card.
  for (const [path, tagged, plain] of [
    [heapA, "Alpha invoice", "Alpha plain"],
    [heapB, "Beta invoice", "Beta plain"],
  ] as const) {
    await page.goto(path);
    await addItem(page, plain);
    await filterByTeamTag(page, "billing", tagged, plain);
  }
  await goTo(page, "Tags");
  await expect(page.getByRole("region", { name: /^1 tag of the team / })).toContainText("billing");
  await expect(page.getByText("No tags of this heap yet.")).toBeVisible();
});
