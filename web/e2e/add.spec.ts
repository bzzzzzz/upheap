// Adding an item (design/ux-overview.md, Adding): where it landed, Add another remembered, the heap chosen in the
// title, the list's filters as the starting point, None for due, and on a phone the chips and More fields.
import {
  addDialog,
  addItem,
  changeState,
  closeItem,
  expect,
  expectSize,
  goTo,
  isPhone,
  itemPane,
  openItem,
  pickSize,
  row,
  test,
  type Need,
} from "./fixtures";
import type { Page } from "@playwright/test";

/** Opens the Add dialog from the list's button, or on a phone from the tabs' +. */
async function openAdd(page: Page) {
  await page.getByRole("button", { name: "Add an item" }).click();
  await expect(addDialog(page)).toBeVisible();
}

/** Fills in the title and the need of the item in the Add dialog. */
async function fillIn(page: Page, title: string, need: Need = "Expected") {
  const dialog = addDialog(page);
  await dialog.getByLabel("Title").fill(title);
  await dialog.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: need, exact: true }).click();
}

/** Creates the tag `name` and puts it on the item in the Add dialog, on a phone from the sheet + Tag opens. */
async function addNewTag(page: Page, name: string) {
  const phone = isPhone(page);
  if (phone) await addDialog(page).getByRole("button", { name: "+ Tag" }).click();
  const scope = phone ? page.getByRole("dialog", { name: "Tags", exact: true }) : addDialog(page);
  await scope.getByLabel("Add a tag").fill(name);
  await scope.getByRole("option", { name: `Create tag “${name}”` }).click();
  if (phone) await scope.getByRole("button", { name: "Close", exact: true }).click();
  await expect(addDialog(page).getByRole("button", { name: `Remove tag ${name}` })).toBeVisible();
}

/** Chooses the heap `name` from the Add dialog's title. */
async function chooseHeap(page: Page, name: string) {
  await addDialog(page)
    .getByRole("button", { name: /^Heap: / })
    .click();
  const heaps = page.getByRole("dialog", { name: "Add to which heap?" });
  await heaps.getByRole("radio", { name, exact: true }).click();
  await expect(heaps).toHaveCount(0);
}

/** Filters the list to `option` of the Filters panel's `group`. */
async function filterBy(page: Page, group: string, option: string) {
  await page.getByRole("button", { name: "Filters" }).click();
  const filters = page.getByRole("dialog", { name: "Filters" });
  await filters.getByRole("group", { name: group }).getByRole("button", { name: option, exact: true }).click();
  await filters.getByRole("button", { name: "Close" }).click();
  await expect(filters).toHaveCount(0);
}

test("Add says where the item landed, in a toast or in the dialog, and Add another is remembered", async ({ page }) => {
  // Without Add another the dialog closes, and the toast says where the item went, with a way to open it.
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  const toast = page.getByRole("status").filter({ hasText: "Added “Slog” at #1." });
  await expect(toast).toBeVisible();
  await toast.getByRole("button", { name: "Open it" }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Slog");
  await itemPane(page).getByRole("button", { name: "Close", exact: true }).click();
  await expect(itemPane(page)).toHaveCount(0);

  // With it, the dialog says so above the title, with the position from the ranked list.
  await openAdd(page);
  const dialog = addDialog(page);
  await expect(dialog.getByLabel("Add another")).not.toBeChecked();
  await dialog.getByLabel("Add another").check();
  await fillIn(page, "Quick win");
  await pickSize(page, "Effort", "S");
  await pickSize(page, "Impact", "XL");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog.getByRole("status")).toContainText(/“Quick win”.* at #1/);
  await fillIn(page, "Middling");
  await pickSize(page, "Effort", "M");
  await pickSize(page, "Impact", "M");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog.getByRole("status")).toContainText(/“Middling”.* at #2/);
  await expect(row(page, "Middling")).toBeVisible();

  // Add another stays ticked on this device, also after a reload.
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await page.reload();
  await expect(row(page, "Middling")).toBeVisible();
  await openAdd(page);
  await expect(dialog.getByLabel("Add another")).toBeChecked();
  await fillIn(page, "Last one");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog.getByRole("status")).toContainText("“Last one”");

  // Open it, with nothing typed, closes the dialog and opens the item.
  await dialog.getByRole("link", { name: "Open it" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Last one");
});

test("the next item can be typed and added while the list catches up", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build answers in the page, out of reach of request routing.");
  await openAdd(page);
  const dialog = addDialog(page);
  await dialog.getByLabel("Add another").check();

  // The list takes its time to come back after each add.
  await page.route(/\/api\/v1\/teams\/[^/]+\/heaps\/[^/]+\/items(\?|$)/, async (route) => {
    if (route.request().method() === "GET") await new Promise((done) => setTimeout(done, 1500));
    await route.continue();
  });
  await fillIn(page, "Buy milk");
  await dialog.getByLabel("Title").press("Enter");
  // Cleared as soon as the server has the item, before the list says where it went.
  await expect(dialog.getByLabel("Title")).toHaveValue("");
  await fillIn(page, "Call Sam");
  await expect(dialog.getByRole("status")).toContainText("“Buy milk”");
  await expect(dialog.getByLabel("Title")).toHaveValue("Call Sam");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog.getByLabel("Title")).toHaveValue("");
  await expect(dialog.getByRole("status")).toContainText("“Call Sam”");
  await page.unroute(/\/api\/v1\/teams\/[^/]+\/heaps\/[^/]+\/items(\?|$)/);
  await page.keyboard.press("Escape");
  await expect(row(page, "Buy milk")).toBeVisible();
  await expect(row(page, "Call Sam")).toBeVisible();
});

test("an add on its way is no draft to discard, and what's changed meanwhile stays", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build answers in the page, out of reach of request routing.");
  const items = /\/api\/v1\/teams\/[^/]+\/heaps\/[^/]+\/items(\?|$)/;
  // The server takes its time to take each item.
  await page.route(items, async (route) => {
    if (route.request().method() === "POST") await new Promise((done) => setTimeout(done, 1500));
    await route.continue();
  });
  await openAdd(page);
  const dialog = addDialog(page);
  await dialog.getByLabel("Add another").check();
  const need = dialog.getByRole("radiogroup", { name: "Need" });

  // Changed while the first add is on its way, the title and the need are the next item's.
  await fillIn(page, "Pay rent");
  await dialog.getByLabel("Title").press("Enter");
  await dialog.getByLabel("Title").fill("Call Sam");
  await need.getByRole("radio", { name: "Critical", exact: true }).click();
  await expect(dialog.getByRole("status")).toContainText("“Pay rent”");
  await expect(dialog.getByLabel("Title")).toHaveValue("Call Sam");
  await expect(need.getByRole("radio", { name: "Critical", exact: true })).toBeChecked();

  // Sent and untouched, there's nothing to discard: Escape closes, and the toast says where it went.
  await dialog.getByLabel("Title").press("Enter");
  await dialog.getByLabel("Title").press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "Added “Call Sam”" })).toBeVisible();
  await page.unroute(items);
  await expect(row(page, "Call Sam")).toBeVisible();
  await expect(row(page, "Pay rent")).toBeVisible();
});

test("an add that lands after its dialog closed leaves the next dialog open", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build answers in the page, out of reach of request routing.");
  const items = /\/api\/v1\/teams\/[^/]+\/heaps\/[^/]+\/items(\?|$)/;
  await page.route(items, async (route) => {
    if (route.request().method() === "POST") await new Promise((done) => setTimeout(done, 1500));
    await route.continue();
  });
  await openAdd(page);
  const dialog = addDialog(page);
  await fillIn(page, "First thing");
  await dialog.getByLabel("Title").press("Enter");
  await dialog.getByLabel("Title").press("Escape");
  await expect(dialog).toHaveCount(0);

  // The first add's answer closes nothing: the next draft stays where it is.
  await openAdd(page);
  await dialog.getByLabel("Title").fill("Second thing");
  await expect(page.getByRole("status").filter({ hasText: "Added “First thing”" })).toBeVisible();
  await expect(dialog.getByLabel("Title")).toHaveValue("Second thing");
  await page.unroute(items);
});

test("an add that fails after the dialog closed says so in a toast", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build answers in the page, out of reach of request routing.");
  const items = /\/api\/v1\/teams\/[^/]+\/heaps\/[^/]+\/items(\?|$)/;
  await page.route(items, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    await new Promise((done) => setTimeout(done, 1000));
    await route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ code: "internal", message: "Something went wrong.", status: 500 }),
    });
  });
  await openAdd(page);
  const dialog = addDialog(page);
  await fillIn(page, "Pay rent");
  await dialog.getByLabel("Title").press("Enter");
  // Sent, so Escape closes without asking; the failure comes after.
  await dialog.getByLabel("Title").press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("alert").filter({ hasText: "“Pay rent” wasn't added:" })).toBeVisible();
  await expect(row(page, "Pay rent")).toHaveCount(0);
});

test("an item the list's search hides is added without a position", async ({ page }) => {
  await addItem(page, "Water the plants");
  await page.getByLabel("Search items").fill("plants");
  await addItem(page, "Pay the rent", {}, { listed: false });
  const toast = page.getByRole("status").filter({ hasText: "Added “Pay the rent” to " });
  await expect(toast).toBeVisible();
  await expect(toast).not.toContainText("#");
  await page.getByLabel("Search items").fill("");
  await expect(row(page, "Pay the rent")).toBeVisible();
});

test("the item goes into the heap chosen in the dialog's title", async ({ page, heapPath }) => {
  // A second heap to choose, from the team's page.
  await page.goto(heapPath.replace(/\/heaps\/[^/]+$/, ""));
  await page.getByLabel("New heap name").fill("Garden");
  await page.getByRole("button", { name: "Add heap" }).click();
  await expect(page.getByLabel("Name of heap Garden")).toBeVisible();
  await page.goto(heapPath);
  await expect(page.getByRole("button", { name: "Refresh order" })).toBeVisible();

  // A tag of this heap, which the list is filtered to.
  await openAdd(page);
  const dialog = addDialog(page);
  await fillIn(page, "Sweep the yard");
  await addNewTag(page, "chores");
  await dialog.getByLabel("Title").press("Enter");
  await expect(row(page, "Sweep the yard")).toBeVisible();
  await filterBy(page, "Tags (any)", "chores");

  await openAdd(page);
  await expect(dialog.getByRole("button", { name: "Remove tag chores" })).toBeVisible();
  const label = (await dialog.getByRole("button", { name: /^Heap: / }).getAttribute("aria-label")) ?? "";
  const here = label.replace(/^Heap: /, "");
  await fillIn(page, "Plant the bulbs", "Nice to have");
  await pickSize(page, "Effort", "M");

  // Choosing another heap keeps what's typed and picked, apart from the tags, which belong to a heap.
  await chooseHeap(page, "Garden");
  await expect(page.getByRole("dialog", { name: "Add an item to Garden" })).toBeVisible();
  await expect(dialog.getByLabel("Title")).toHaveValue("Plant the bulbs");
  await expect(
    dialog.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: "Nice to have", exact: true }),
  ).toBeChecked();
  await expectSize(page, "Effort", "M");
  await expect(dialog.getByRole("button", { name: "Remove tag chores" })).toHaveCount(0);

  // Back in the heap on screen, the filter's tag is there again.
  await chooseHeap(page, here);
  await expect(page.getByRole("dialog", { name: `Add an item to ${here}` })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Remove tag chores" })).toBeVisible();
  await expect(dialog.getByLabel("Title")).toHaveValue("Plant the bulbs");
  await chooseHeap(page, "Garden");

  // It isn't in the list on screen, so the toast names the heap instead of a position.
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "Plant the bulbs")).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "Added “Plant the bulbs” to Garden." });
  await toast.getByRole("button", { name: "Open it" }).click();
  await expect(page).toHaveURL(/\/heaps\/garden\/items\/[0-9a-z]+$/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Plant the bulbs");
});

test("the dialog starts from the list's filters, also after each item added", async ({ page }) => {
  await openAdd(page);
  await fillIn(page, "Rotate the keys");
  await pickSize(page, "Effort", "S");
  await addNewTag(page, "ops");
  await addDialog(page).getByLabel("Title").press("Enter");
  await expect(row(page, "Rotate the keys")).toBeVisible();
  await addItem(page, "Water the plants", { effort: "L" });

  // Filter the list to the tag and small items.
  await filterBy(page, "Tags (any)", "ops");
  await filterBy(page, "Effort", "S");
  await expect(row(page, "Water the plants")).toHaveCount(0);

  // The new item starts with them, so it shows in the list on screen.
  await openAdd(page);
  const dialog = addDialog(page);
  await expect(dialog.getByRole("button", { name: "Remove tag ops" })).toBeVisible();
  await expectSize(page, "Effort", "S");
  await expectSize(page, "Impact", null);
  await dialog.getByLabel("Add another").check();
  await fillIn(page, "Patch the servers");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog.getByRole("status")).toContainText(/“Patch the servers”.* at #[12]/);
  await expect(row(page, "Patch the servers")).toBeVisible();

  // Cleared for the next item, it starts from the filters again.
  await expect(dialog.getByLabel("Title")).toHaveValue("");
  await expect(dialog.getByRole("button", { name: "Remove tag ops" })).toBeVisible();
  await expectSize(page, "Effort", "S");
  await dialog.getByRole("button", { name: "Remove tag ops" }).click();
  await expect(dialog.getByRole("button", { name: "Remove tag ops" })).toHaveCount(0);

  // An item the filters hide is added all the same, and the message says where without a position.
  await dialog.getByLabel("Add another").uncheck();
  await fillIn(page, "Repaint the shed");
  await pickSize(page, "Effort", "L");
  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "Repaint the shed")).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "Added “Repaint the shed” to " });
  await expect(toast).toBeVisible();
  await expect(toast).not.toContainText("#");
});

test("a parameter the filters pick one option of starts with it, under More fields on a phone", async ({ page }) => {
  await goTo(page, "Parameters");
  await page.getByLabel("New parameter name").fill("Customer");
  await page.getByLabel("Weight of the new parameter").fill("50");
  await page.getByLabel(/^Options of the new parameter/).fill("Enterprise\nFree");
  await page.getByRole("button", { name: "Add parameter" }).click();
  await expect(page.getByLabel("Name of parameter Customer")).toBeVisible();
  await goTo(page, "Open");
  await filterBy(page, "Customer (any)", "Enterprise");

  await openAdd(page);
  const dialog = addDialog(page);
  if (isPhone(page)) {
    await expect(dialog.getByRole("button", { name: "More fields" })).toHaveAttribute("aria-expanded", "true");
  }
  await expect(
    dialog.getByRole("radiogroup", { name: "Customer" }).getByRole("radio", { name: "Enterprise", exact: true }),
  ).toBeChecked();
  await fillIn(page, "Renew the contract");
  await dialog.getByLabel("Title").press("Enter");
  await expect(row(page, "Renew the contract")).toContainText("Customer: Enterprise");
  await expect(page.getByRole("status").filter({ hasText: "Added “Renew the contract” at #1." })).toBeVisible();
});

test("the message names the group the item went into when something is pinned", async ({ page }) => {
  await addItem(page, "Under way");
  await openItem(page, "Under way");
  await changeState(page, "Start");
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  await closeItem(page);

  await addItem(page, "Next thing");
  await expect(page.getByRole("status").filter({ hasText: "Added “Next thing” at #1 in Up next." })).toBeVisible();
});

test("due starts at None in the Add dialog", async ({ page }) => {
  test.skip(isPhone(page), "A phone's due chips have no None: tapping the picked one clears it.");
  await openAdd(page);
  const dialog = addDialog(page);
  const due = dialog.getByRole("radiogroup", { name: "Due" });
  await expect(due.getByRole("radio", { name: "None", exact: true })).toBeChecked();

  await due.getByRole("radio", { name: "next week" }).click();
  await expect(due.getByRole("radio", { name: "None", exact: true })).not.toBeChecked();
  await due.getByRole("radio", { name: "None", exact: true }).click();
  await expect(due.getByRole("radio", { name: "next week" })).not.toBeChecked();
  await expect(due.getByRole("radio", { name: "None", exact: true })).toBeChecked();

  await fillIn(page, "Someday");
  await dialog.getByLabel("Title").press("Enter");
  await expect(row(page, "Someday")).toBeVisible();
  await expect(row(page, "Someday")).not.toContainText("next week");
});

test("on a phone, Add is a sheet of one-tap chips, with More fields", async ({ page }) => {
  test.skip(!isPhone(page), "Only a phone has the chips.");
  await openAdd(page);
  const dialog = addDialog(page);
  await fillIn(page, "Call the plumber");

  // Effort opens a sheet of sizes that closes on a pick; picking the same size again clears it.
  await expectSize(page, "Effort", null);
  await pickSize(page, "Effort", "M");
  await expectSize(page, "Effort", "M");
  await pickSize(page, "Effort", "M");
  await expectSize(page, "Effort", null);
  await pickSize(page, "Impact", "L");

  // The due chips: a tap picks one, a second clears it.
  const due = dialog.getByRole("radiogroup", { name: "Due" });
  await due.getByRole("radio", { name: "next week" }).click();
  await expect(due.getByRole("radio", { name: "next week" })).toBeChecked();
  await due.getByRole("radio", { name: "next week" }).click();
  await expect(due.getByRole("radio", { name: "next week" })).not.toBeChecked();
  await due.getByRole("radio", { name: "next week" }).click();

  // + Tag opens the tag picker in a sheet; the tag's chip takes it off again.
  await addNewTag(page, "home");

  // The description waits under More fields.
  await expect(dialog.getByLabel("Description")).toHaveCount(0);
  await dialog.getByRole("button", { name: "More fields" }).click();
  await dialog.getByLabel("Description").fill("The kitchen tap");

  // Add sits above the keyboard, in the sheet's last row.
  const add = await dialog.getByRole("button", { name: "Add", exact: true }).boundingBox();
  const height = page.viewportSize()?.height ?? 0;
  expect((add?.y ?? 0) + (add?.height ?? 0)).toBeLessThanOrEqual(height);
  expect(add?.height).toBeGreaterThanOrEqual(44);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "Call the plumber")).toContainText("next week");
  await expect(row(page, "Call the plumber")).toContainText("home");
});
