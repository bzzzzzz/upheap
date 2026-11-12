import type { Page } from "@playwright/test";

import {
  addDialog,
  addItem,
  changeState,
  closeField,
  closeItem,
  dayFromToday,
  editContext,
  expect,
  expectOrder,
  field,
  fieldButton,
  goTo,
  isPhone,
  itemPane,
  makeTask,
  markDoneFromList,
  moreEntry,
  openHistory,
  openItem,
  openMore,
  pick,
  row,
  saveContext,
  test,
} from "./fixtures";

test("added items are ranked, and an edit moves an item", async ({ page }) => {
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });

  // More value for less effort ranks higher, whatever the order they were added in.
  await expectOrder(page, ["Quick win", "Slog"]);

  // An exact deadline that has passed puts the slog first.
  await openItem(page, "Slog");
  const due = await pick(page, "Due", "Exact date");
  await due.getByLabel("Exact due date").fill(dayFromToday(-1));
  await closeField(page);
  await expect(itemPane(page).getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  await closeItem(page);
  await expect(row(page, "Slog").getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  await expectOrder(page, ["Slog", "Quick win"]);
});

test("the item opens over the list, saves each change, and Back closes it", async ({ page, heapPath }) => {
  await addItem(page, "Write the rota");
  await openItem(page, "Write the rota");
  const pane = itemPane(page);

  // Each change saves as it's made, one after the other, and the value reads the new choice once the server has it.
  await pick(page, "Effort", "S");
  await expect(fieldButton(page, "Effort")).toHaveAccessibleName("Effort: S");
  await pick(page, "Impact", "XL");
  await expect(fieldButton(page, "Impact")).toHaveAccessibleName("Impact: XL");
  await pane.getByLabel("Title", { exact: true }).fill("Write the team rota");
  await pane.getByLabel("Title", { exact: true }).press("Enter");
  await (await editContext(page)).fill("For the next quarter");
  await saveContext(page, "For the next quarter");
  // The likeliest next state is the main button: Start, for an open item, then Done once it's started.
  const state = pane.getByRole("group", { name: "State", exact: true });
  await changeState(page, "Start", "main");
  await expect(state.getByRole("button").first()).toHaveText("Done");
  await expect(row(page, "Write the team rota")).toBeVisible();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  // History starts collapsed, with who added the item beside it, and opens with a click.
  const historyRegion = pane.getByRole("region", { name: "History" });
  await expect(historyRegion).toContainText("Added by you today");
  await expect(historyRegion).not.toContainText("You added item");
  const history = await openHistory(page);
  await expect(history).toContainText("In progress");
  // The user's own changes read as theirs.
  await expect(history).toContainText("You added item");

  // Done leaves the item open in the pane, which offers to reopen it.
  await changeState(page, "Done", "main");
  await expect(row(page, "Write the team rota")).toHaveCount(0);
  await changeState(page, "Reopen", "main");
  await expect(state.getByRole("button").first()).toHaveText("Start");
  await expect(row(page, "Write the team rota")).toBeVisible();

  // Back closes the item and leaves the list where it was.
  await page.goBack();
  await expect(pane).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));

  // Opened from a link, it closes to its list.
  await openItem(page, "Write the team rota");
  await page.reload();
  await expect(pane.getByRole("region", { name: "Context", exact: true })).toContainText("For the next quarter");
  await closeItem(page);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  await expect(row(page, "Write the team rota")).toBeVisible();
});

test("the fields read first, and a value opens its editor on a click", async ({ page }) => {
  await addItem(page, "Plan the trip");
  await openItem(page, "Plan the trip");
  const pane = itemPane(page);

  // Labelled values, with nothing open to edit.
  for (const name of ["Need: Expected", "Effort: Set effort", "Impact: Set impact", "Due: Set due", "Tags: None"]) {
    await expect(pane.getByRole("button", { name, exact: true })).toBeVisible();
  }
  await expect(pane.getByRole("radiogroup")).toHaveCount(0);

  // A click opens the editor, a pick saves and closes it, and focus goes back to the value.
  const editor = await pick(page, "Effort", "M");
  await expect(fieldButton(page, "Effort")).toHaveAccessibleName("Effort: M");
  await expect(editor).toHaveCount(0);
  await expect(fieldButton(page, "Effort")).toBeFocused();

  // Picking the picked one again clears it, where the choice is made on buttons.
  if (!isPhone(page)) {
    await pick(page, "Effort", "M");
    await expect(fieldButton(page, "Effort")).toHaveAccessibleName("Effort: Set effort");
  }
});

test("an unset effort, impact and due date that hold the rank back are marked", async ({ page }) => {
  await addItem(page, "Renew the certificate", { need: "Critical", effort: "S", impact: "XL" });
  await addItem(page, "Dark mode", { need: "Idea" });
  await openItem(page, "Dark mode");

  // The server says what keeps it from ranking higher; the fields that can fix that carry the mark.
  await expect(fieldButton(page, "Effort")).toHaveAttribute("data-gap", "true");
  await expect(fieldButton(page, "Impact")).toHaveAttribute("data-gap", "true");
  await expect(fieldButton(page, "Due")).toHaveAttribute("data-gap", "true");
  // A low need is set on purpose, so it isn't marked.
  await expect(fieldButton(page, "Need")).not.toHaveAttribute("data-gap");

  await pick(page, "Effort", "M");
  await expect(fieldButton(page, "Effort")).toHaveAccessibleName("Effort: M");
  await expect(fieldButton(page, "Effort")).not.toHaveAttribute("data-gap");
  await expect(fieldButton(page, "Impact")).toHaveAttribute("data-gap", "true");
});

test("the rank chip says why the item ranks where it does", async ({ page }) => {
  await addItem(page, "Renew the certificate", { need: "Critical", effort: "S", impact: "XL" });
  await addItem(page, "Dark mode", { need: "Idea" });
  await openItem(page, "Dark mode");

  const chip = itemPane(page).getByRole("button", { name: "Why it's #2", exact: true });
  await expect(chip).toHaveText("#2");
  await chip.click();
  const why = page.getByRole("dialog", { name: "Why it's #2", exact: true });
  // What holds it back is text: the fields themselves are where to fix it.
  await expect(why.getByRole("list", { name: "Held back by" }).getByRole("listitem")).toContainText([
    "Need: Idea",
    "No due date",
    "Effort and impact not set",
  ]);
  await expect(why.getByRole("button")).toHaveCount(1);
  await why.getByRole("button", { name: "Close", exact: true }).click();
  await expect(why).toHaveCount(0);
  await expect(chip).toBeFocused();
});

test("the item that ranks first says nothing holds it back", async ({ page }) => {
  await addItem(page, "Dark mode", { need: "Idea", effort: "S", impact: "XL" });
  await openItem(page, "Dark mode");

  await itemPane(page).getByRole("button", { name: "Why it's #1", exact: true }).click();
  const why = page.getByRole("dialog", { name: "Why it's #1", exact: true });
  await expect(why.getByRole("listitem").first()).toBeVisible();
  await expect(why.getByText("Held back by")).toHaveCount(0);
});

test("the context reads as Markdown, never runs what is written in it, and Edit changes the source", async ({
  page,
}) => {
  await addItem(page, "Write the plan");
  await openItem(page, "Write the plan");
  const pane = itemPane(page);
  const context = pane.getByRole("region", { name: "Context", exact: true });
  await expect(context).toContainText("No context yet.");

  const source = "**bold** and a [link](https://example.com)\n\n- one\n- two\n\n<script>window.pwned = true</script>";
  await (await editContext(page)).fill(source);
  await saveContext(page, "bold and a link");
  await expect(context.locator("strong")).toHaveText("bold");
  const link = context.getByRole("link", { name: "link", exact: true });
  await expect(link).toHaveAttribute("target", "_blank");
  await expect(link).toHaveAttribute("rel", "noopener noreferrer");
  await expect(context.getByRole("listitem")).toHaveText(["one", "two"]);
  // A script-scheme link and a remote image are never live: the link reads as plain text, the image as a link.
  await (await editContext(page)).fill("[bad](javascript:alert(1)) and ![x](https://example.com/t.png)");
  await saveContext(page, "bad and");
  await expect(context.locator("a[href^='javascript:' i]")).toHaveCount(0);
  await expect(context.getByRole("link", { name: "bad", exact: true })).toHaveCount(0);
  await expect(context.locator("img")).toHaveCount(0);
  await expect(context.getByRole("link", { name: "x", exact: true })).toHaveAttribute(
    "href",
    "https://example.com/t.png",
  );
  await (await editContext(page)).fill(source);
  await saveContext(page, "bold and a link");
  // Raw HTML is dropped, not run.
  await expect(pane.locator("script")).toHaveCount(0);
  expect(await page.evaluate(() => "pwned" in window)).toBe(false);

  // Save and Cancel are the ways out of the form; the focus goes back to Edit. The rest is checked on a wider screen.
  test.skip(isPhone(page), "the phone's sheet is checked in its own test");
  const area = await editContext(page);
  await expect(area).toHaveValue(source);
  await area.fill("Just this");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(context).toContainText("Just this");
  await expect(pane.getByRole("button", { name: "Edit context" })).toBeFocused();

  await (await editContext(page)).fill("Never mind");
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Just this");
  await expect(context).not.toContainText("Never mind");
  await expect(pane.getByRole("button", { name: "Edit context" })).toBeFocused();

  // Escape in the textarea does nothing: the draft stays and the item stays open.
  const kept = await editContext(page);
  await kept.fill("Not lost to a stray key");
  await page.keyboard.press("Escape");
  await expect(kept).toHaveValue("Not lost to a stray key");
  await expect(pane).toBeVisible();
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Just this");

  // Closing the item with a changed draft saves it, so nothing typed is lost.
  await (await editContext(page)).fill("Kept on close");
  await closeItem(page);
  await openItem(page, "Write the plan");
  await expect(context).toContainText("Kept on close");
});

test("the item has one main state button, and ⋯ holds the rest", async ({ page }) => {
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const state = itemPane(page).getByRole("group", { name: "State", exact: true });

  // Start and ⋯, and nothing else.
  await expect(state.getByRole("button")).toHaveCount(2);
  await expect(state.getByRole("button").first()).toHaveText("Start");
  await expect(state.getByRole("button", { name: "More", exact: true })).toBeVisible();

  const more = await openMore(page);
  for (const name of ["Block", "Done", "Won't do", "Copy link"]) {
    await expect(moreEntry(page, more, name)).toBeVisible();
  }
  // Done from ⋯ closes the item; the pane offers to reopen it.
  await moreEntry(page, more, "Done").click();
  await expect(state.getByRole("button", { name: "Reopen", exact: true })).toBeVisible();
});

test("a reader's item has values to read, no Edit, and ⋯ with only Copy link", async ({ page, heapPath }) => {
  // An archived heap is read-only, as it is for a reader: the server offers neither edit nor change_state.
  await addItem(page, "Rotate the certs", { effort: "S" });
  await openItem(page, "Rotate the certs");
  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  const archived = await page.request.post(`/api/v1${heapPath}/archive`, { headers: { "x-csrf-protection": "1" } });
  expect(archived.ok()).toBe(true);
  await page.goto(`${heapPath}/items/${key}`);
  const pane = itemPane(page);
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");

  await expect(pane.getByText("Expected", { exact: true })).toBeVisible();
  await expect(pane.getByText("Not set").first()).toBeVisible();
  await expect(pane.getByRole("button", { name: /^(Need|Effort|Impact|Due|Tags): / })).toHaveCount(0);
  await expect(pane.getByRole("button", { name: /^(Edit|Add) context$/ })).toHaveCount(0);
  await expect(pane.getByRole("group", { name: "State", exact: true }).getByRole("button")).toHaveCount(1);

  const more = await openMore(page);
  await expect(moreEntry(page, more, "Copy link")).toBeVisible();
  await expect(more.getByRole(isPhone(page) ? "button" : "menuitem")).toHaveCount(isPhone(page) ? 2 : 1);
});

test("History opened on one item starts collapsed on the next", async ({ page }) => {
  // Phones have no Next item in the pane: the sheet closes, and each item opens fresh.
  test.skip(isPhone(page), "the pane walks the list only on wider screens");
  await addItem(page, "Water the plants", { effort: "S", impact: "XL" });
  await addItem(page, "Fix the gate", { effort: "XL", impact: "S" });
  await openItem(page, "Water the plants");
  await openHistory(page);
  await itemPane(page).getByRole("link", { name: "Next item", exact: true }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Fix the gate");
  await expect(itemPane(page).getByRole("region", { name: "History" }).locator("details")).not.toHaveAttribute("open");
});
test("an item is closed from the list, undone, and reopened from Closed", async ({ page }) => {
  await addItem(page, "Write release notes");

  await markDoneFromList(page, "Write release notes");
  await expect(row(page, "Write release notes")).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "“Write release notes” is done." });
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(row(page, "Write release notes")).toBeVisible();

  await markDoneFromList(page, "Write release notes");
  await expect(row(page, "Write release notes")).toHaveCount(0);
  await goTo(page, "Closed");
  const today = page.getByRole("region", { name: "Today" });
  const closed = today.getByRole("listitem").filter({ hasText: "Write release notes" });
  await expect(closed).toContainText("Done · today by you");
  await closed.getByRole("button", { name: "Reopen" }).click();
  await expect(page.getByText("Nothing closed yet.")).toBeVisible();

  await goTo(page, "Open");
  await expect(row(page, "Write release notes")).toBeVisible();
});

test("the keyboard moves through the list, opens an item and marks one done", async ({ page }) => {
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  const link = (title: string) => row(page, title).getByRole("link", { name: title, exact: true });

  await page.locator("body").click({ position: { x: 1, y: 1 } });
  await page.keyboard.press("ArrowDown");
  await expect(link("Quick win")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(link("Slog")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Slog");

  // Escape closes the item and puts the focus back on its row.
  await page.keyboard.press("Escape");
  await expect(itemPane(page)).toHaveCount(0);
  await expect(link("Slog")).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("x");
  await expect(row(page, "Quick win")).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "“Quick win” is done." })).toBeVisible();
});

test("an item is blocked by another and unblocked", async ({ page }) => {
  await addItem(page, "Deploy", { effort: "S", impact: "XL" });
  await addItem(page, "Get approval");
  const status = () => itemPane(page).getByRole("group", { name: "Status" });
  /** Opens Change blocker… from ⋯ on the open, blocked item. */
  const changeBlocker = async () => {
    await moreEntry(page, await openMore(page), "Change blocker…").click();
    const dialog = page.getByRole("dialog", { name: "Change blocker", exact: true });
    await expect(dialog).toBeVisible();
    return dialog;
  };

  // Blocked items are hidden by default.
  await openItem(page, "Deploy");
  await changeState(page, "Block", "more");
  await expect(row(page, "Deploy")).toHaveCount(0);
  await expect(status()).toContainText("Blocked");
  await closeItem(page);
  // The note under the list says what's hidden, and brings it back.
  await expect(page.getByText("1 blocked item is hidden.")).toBeVisible();
  await page.getByRole("button", { name: "Show it" }).click();
  await expect(row(page, "Deploy")).toBeVisible();

  // Blocking saved the state at once; the blocker is named after, from ⋯, and the status line links to it.
  await openItem(page, "Deploy");
  await (await changeBlocker()).getByLabel("Waiting for").selectOption({ label: "Get approval" });
  await expect(status()).toContainText("Blocked by");
  await expect(status().getByRole("link", { name: /^[0-9a-z]{5} Get approval$/ })).toBeVisible();
  await closeItem(page);
  // The blocker takes on the priority of what waits for it, and says so.
  await expect(row(page, "Get approval")).toContainText("Blocks “Deploy”");

  // The blocker isn't offered the item it blocks to wait for, which would be a cycle.
  await openItem(page, "Get approval");
  await changeState(page, "Block", "more");
  const blocker = await changeBlocker();
  await expect(blocker.getByLabel("Waiting for")).toBeVisible();
  await expect(blocker.getByLabel("Waiting for").getByRole("option")).toHaveText(["Nothing picked"]);
  await closeField(page);
  await changeState(page, "Unblock", "main");
  await expect(status()).not.toContainText("Blocked");
  await closeItem(page);

  await openItem(page, "Deploy");
  await changeState(page, "Unblock", "main");
  await expect(status()).not.toContainText("Blocked by");
  await closeItem(page);
  await expect(row(page, "Get approval")).not.toContainText("Blocks “Deploy”");
});

test("the Add dialog adds items in a row, with rough or exact due times", async ({ page }) => {
  // `N` works once the list is there. (On a phone the tabs' + is there sooner.)
  await expect(page.getByRole("heading", { level: 1 })).toBeAttached();
  await page.keyboard.press("n");
  const dialog = addDialog(page);
  const due = dialog.getByRole("radiogroup", { name: "Due" });
  const need = dialog.getByRole("radiogroup", { name: "Need" });
  const add = dialog.getByRole("button", { name: "Add", exact: true });

  // Every item needs a need: Add stays off until one is picked, and the pick explains itself.
  await dialog.getByLabel("Title").fill("Book the dentist");
  await expect(add).toBeDisabled();
  await need.getByRole("radio", { name: "Nice to have", exact: true }).click();
  await expect(dialog.getByText("Wanted, but nobody is waiting for it.")).toBeVisible();
  await expect(add).toBeEnabled();

  // A rough choice reads back with the label it was picked by, which comes from the server.
  await due.getByRole("radio", { name: "next week" }).click();
  await dialog.getByLabel("Add another").check();
  await dialog.getByLabel("Title").press("Enter");
  await expect(row(page, "Book the dentist").getByText("next week", { exact: true })).toBeVisible();
  // It says where the item landed, in the dialog.
  await expect(dialog.getByRole("status")).toContainText(/“Book the dentist”.* at #1/);

  // With Add another, the dialog stays open and clears, the need too.
  await expect(dialog.getByLabel("Title")).toHaveValue("");
  await expect(due.getByRole("radio", { name: "next week" })).not.toBeChecked();
  await expect(need.getByRole("radio", { name: "Nice to have", exact: true })).not.toBeChecked();
  await dialog.getByLabel("Title").fill("Renew the passport");
  await need.getByRole("radio", { name: "Critical", exact: true }).click();
  await due.getByRole("radio", { name: "Exact date" }).click();
  await dialog.getByLabel("Exact due date").fill(dayFromToday(-1));
  await dialog.getByLabel("Title").press("ControlOrMeta+Enter");
  await expect(row(page, "Renew the passport").getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  // Overdue and critical, it went in at the top.
  await expect(dialog.getByRole("status")).toContainText(/“Renew the passport”.* at #1/);

  // Closing with a title typed asks first.
  await dialog.getByLabel("Title").fill("A stray thought");
  await dialog.getByLabel("Title").press("Escape");
  await dialog.getByRole("button", { name: "Discard" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "A stray thought")).toHaveCount(0);
});

test("the Undo toast stays while items open and close over the list", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) < 640, "On a phone the item sheet covers the list.");
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  await addItem(page, "Middling", { effort: "M", impact: "M" });

  // Done from a row while another item is open, then another item opened and the pane closed.
  await openItem(page, "Quick win");
  await markDoneFromList(page, "Slog");
  await expect(row(page, "Slog")).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "“Slog” is done." });
  await expect(toast).toBeVisible();
  await openItem(page, "Middling");
  await page.keyboard.press("Escape");
  await expect(itemPane(page)).toHaveCount(0);
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(row(page, "Slog")).toBeVisible();
});

test("a short link opens the item it names", async ({ page, heapPath }) => {
  await addItem(page, "Share the plan");
  await openItem(page, "Share the plan");
  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  expect(key).toMatch(/^[0-9a-z]{5}$/);
  await closeItem(page);

  // `/h/{team}/{heap}/{item}` expands to the item's full address.
  const short = heapPath.replace(/teams\/([^/]+)\/heaps\/([^/]+)$/, `h/$1/$2/${key}`);
  await page.goto(short);
  await expect(page).toHaveURL(new RegExp(`${heapPath}/items/${key}$`));
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Share the plan");
});

test("on a phone, need, effort and impact open sheets, and the bar holds the main button and ⋯", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) >= 640, "Only a phone has the sheets and the bar.");
  await addItem(page, "Plan the offsite");
  await openItem(page, "Plan the offsite");
  const pane = itemPane(page);

  // The toast from adding it stays under the sheet, which keeps its bar usable.
  await expect(page.getByRole("status").filter({ hasText: "Added “Plan the offsite”" })).toBeVisible();
  // The bar holds the main button and ⋯, and nothing else.
  const bar = pane.getByRole("group", { name: "State", exact: true });
  await expect(bar.getByRole("button")).toHaveCount(2);
  await expect(bar.getByRole("button").first()).toHaveText("Start");
  await bar.getByRole("button", { name: "More", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "More", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "More", exact: true })).toHaveCount(0);

  // The three cells of the first row each open a sheet.
  for (const name of ["Need", "Effort", "Impact"]) {
    const sheet = await field(page, name);
    await expect(sheet.getByRole("radiogroup", { name, exact: true })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(sheet).toHaveCount(0);
  }
  // A need is picked from rows with what it means under each.
  const need = await field(page, "Need");
  await expect(need.getByRole("radio")).toHaveCount(4);
  await need.getByRole("radio", { name: "Critical", exact: true }).click();
  await expect(fieldButton(page, "Need")).toHaveAccessibleName("Need: Critical");

  // Escape does nothing; a tap on the dimmed page saves a changed draft like Save does, and Save does too.
  const context = pane.getByRole("region", { name: "Context", exact: true });
  const area = await editContext(page);
  await area.fill("Venue first");
  await page.keyboard.press("Escape");
  await expect(area).toHaveValue("Venue first");
  await page.mouse.click(200, 20);
  await expect(context).toContainText("Venue first");
  await (await editContext(page)).fill("Venue, then dates");
  await saveContext(page, "Venue, then dates");

  // A new tag from the Tags sheet, its offer in view under the field.
  const tags = await field(page, "Tags");
  await tags.getByLabel("Add a tag").fill("travel");
  await tags.getByRole("option", { name: "Create tag “travel”" }).click();
  await closeField(page);
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: travel");

  // Won't do is under ⋯; a closed item shows all its fields, unchangeable.
  await changeState(page, "Won't do", "more");
  await expect(context.getByText("Venue, then dates", { exact: true })).toBeVisible();
  await expect(pane.getByRole("button", { name: /^(Edit|Add) context$/ })).toHaveCount(0);
  await expect(pane.getByRole("button", { name: /^(Need|Effort|Impact|Due|Tags): / })).toHaveCount(0);
  await expect(bar.getByRole("button")).toHaveCount(2);
  await expect(bar.getByRole("button").first()).toHaveText("Reopen");
});

test("a note is made a task and its stage is set from the item", async ({ page }) => {
  await addItem(page, "Rotate the certs");
  await openItem(page, "Rotate the certs");
  const pane = itemPane(page);
  const phone = isPhone(page);
  const status = pane.getByRole("group", { name: "Status" });
  await expect(status).toContainText("Note · As written");

  // Making a note a task asks first, naming it: a task can't go back.
  const more = pane.getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true });
  await moreEntry(page, await openMore(page), "Make it a task…").click();
  const confirm = page.getByRole("alertdialog", { name: "Make “Rotate the certs” a task?" });
  await expect(confirm).toContainText("A task can't go back to a note.");
  // Cancel keeps the note and gives focus back to the ⋯.
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toHaveCount(0);
  await expect(more).toBeFocused();
  await expect(status).toContainText("Note · As written");
  await moreEntry(page, await openMore(page), "Make it a task…").click();
  await confirm.getByRole("button", { name: "Make it a task" }).click();
  await expect(status).toContainText("Task · As written");

  // The stage saves at once. A task can be defined, and offers no second Make it a task.
  const menu = await openMore(page);
  await expect(menu.getByText("Make it a task…")).toHaveCount(0);
  if (phone) await menu.getByRole("radio", { name: "With context" }).click();
  else await menu.getByRole("menuitemradio", { name: "With context" }).click();
  await expect(status).toContainText("Task · With context");

  await closeItem(page);
  await expect(page.getByRole("link", { name: /^Rotate the certs\s?, task, with context$/ })).toBeVisible();
});

test("a task is worked through its steps from the item", async ({ page }) => {
  await addItem(page, "Rotate the certs");
  await openItem(page, "Rotate the certs");
  await makeTask(page, "Rotate the certs");
  const steps = itemPane(page).getByRole("region", { name: "Steps" });

  // Enter adds a step and keeps the field for the next; Escape closes it and puts the focus back on Add.
  await steps.getByRole("button", { name: "Add a step" }).click();
  const field = steps.getByRole("textbox", { name: "New step" });
  await field.fill("Inventory the certs");
  await field.press("Enter");
  await expect(steps.getByRole("textbox", { name: "Step title" })).toHaveValue("Inventory the certs");
  await expect(field).toBeFocused();
  await field.fill("Run in staging");
  await field.press("Enter");
  await expect(steps.getByRole("textbox", { name: "Step title" })).toHaveCount(2);
  await field.press("Escape");
  await expect(steps.getByRole("button", { name: "Add a step" })).toBeFocused();
  await expect(itemPane(page)).toBeVisible();
  await expect(steps.getByRole("heading", { name: "Steps 0/2" })).toBeVisible();

  // Ticking the first shows in the item and on the row.
  await steps.getByRole("checkbox", { name: "Inventory the certs" }).click({ force: true });
  await expect(steps.getByRole("heading", { name: "Steps 1/2" })).toBeVisible();
  // The row's link says it to a screen reader too, after the title.
  await expect(page.getByRole("link", { name: /^Rotate the certs\s?, task, 1 of 2 steps ticked$/ })).toBeVisible();
  await expect(page.getByRole("main").getByRole("listitem").filter({ hasText: "Rotate the certs" })).toContainText(
    "1/2",
  );

  // Dropping the second strikes it through and leaves the count; Restore brings it back.
  await steps.getByRole("button", { name: "Step actions" }).nth(1).click();
  await page.getByRole("menuitem", { name: "Drop" }).click();
  await expect(steps.getByRole("heading", { name: "Steps 1/1" })).toBeVisible();
  await expect(steps.getByRole("textbox", { name: "Step title" }).nth(1)).toHaveCSS(
    "text-decoration-line",
    "line-through",
  );
  await expect(page.getByRole("main").getByRole("listitem").filter({ hasText: "Rotate the certs" })).toContainText(
    "1/1",
  );

  // The same after a reload.
  await page.reload();
  const reloaded = itemPane(page).getByRole("region", { name: "Steps" });
  await expect(reloaded.getByRole("heading", { name: "Steps 1/1" })).toBeVisible();
  await expect(reloaded.getByRole("checkbox", { name: "Inventory the certs" })).toBeChecked();
  await expect(reloaded.getByRole("textbox", { name: "Step title" }).nth(1)).toHaveCSS(
    "text-decoration-line",
    "line-through",
  );
});

/** Changes the open item the way another member would, through the API, and returns once the server has it. */
async function changeElsewhere(page: Page, heapPath: string, patch: Record<string, unknown>) {
  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  const address = `/api/v1${heapPath}/items/${key}`;
  const current = await page.request.get(address);
  expect(current.ok()).toBe(true);
  const { version } = await current.json();
  const changed = await page.request.patch(address, {
    headers: { "x-csrf-protection": "1", "If-Match": `"${version}"` },
    data: patch,
  });
  expect(changed.ok()).toBe(true);
}

/** Makes the open pane fetch the item again, as coming back to the window does, and waits for the new title to show. */
async function refetchShowing(page: Page, title: string) {
  await page.evaluate(() => window.dispatchEvent(new Event("visibilitychange")));
  // The title is read from the same answer as the context, so once it shows, the pane has the other change.
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(title);
}

test("saving a context someone else changed meanwhile shows theirs above the draft, and Save again replaces it", async ({
  page,
  heapPath,
}) => {
  test.skip(isPhone(page), "the phone's sheet is checked in its own test");
  await addItem(page, "Write the plan");
  await openItem(page, "Write the plan");
  const pane = itemPane(page);
  const context = pane.getByRole("region", { name: "Context", exact: true });
  const saved = pane.getByRole("heading", { name: "Saved meanwhile", exact: true });

  // Cancel keeps theirs.
  await (await editContext(page)).fill("Mine");
  await changeElsewhere(page, heapPath, { description: "Theirs", title: "Renamed once" });
  await refetchShowing(page, "Renamed once");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(saved).toBeVisible();
  await expect(pane.getByText("Theirs", { exact: true })).toBeVisible();
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveValue("Mine");
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Theirs");
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveCount(0);

  // Save again replaces it with the draft.
  await (await editContext(page)).fill("Mine again");
  await changeElsewhere(page, heapPath, { description: "Theirs again", title: "Renamed twice" });
  await refetchShowing(page, "Renamed twice");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(saved).toBeVisible();
  await pane.getByRole("button", { name: "Save again", exact: true }).click();
  await expect(context).toContainText("Mine again");
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveCount(0);
});

test("a conflict on Done keeps the item open, with theirs shown above the draft", async ({ page, heapPath }) => {
  test.skip(isPhone(page), "a phone edits the context in a sheet, which covers the pane's Done");
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const pane = itemPane(page);
  await (await editContext(page)).fill("Shipped");
  await changeElsewhere(page, heapPath, { description: "Held back", title: "Renamed" });
  await refetchShowing(page, "Renamed");

  await changeState(page, "Done", "more");
  await expect(pane.getByRole("heading", { name: "Saved meanwhile", exact: true })).toBeVisible();
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveValue("Shipped");
  const state = pane.getByRole("group", { name: "State", exact: true });
  await expect(state.getByRole("button", { name: "Start", exact: true })).toBeVisible();
  await expect(state.getByRole("button", { name: "Reopen", exact: true })).toHaveCount(0);
});

test("a draft stays readable when the item is closed elsewhere, and says it wasn't saved", async ({
  page,
  heapPath,
}) => {
  test.skip(isPhone(page), "the phone's sheet is checked in its own test");
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const pane = itemPane(page);
  await (await editContext(page)).fill("Original");
  await saveContext(page, "Original");
  await (await editContext(page)).fill("Wrap-up that never got saved");
  await changeElsewhere(page, heapPath, { state: "done" });
  await page.evaluate(() => window.dispatchEvent(new Event("visibilitychange")));

  await expect(pane.getByText("This item can't be edited any more; your text wasn't saved.")).toBeVisible();
  const draft = pane.getByRole("textbox", { name: "Context", exact: true });
  await expect(draft).toHaveValue("Wrap-up that never got saved");
  await expect(draft).toHaveJSProperty("readOnly", true);
  await pane
    .getByRole("region", { name: "Context", exact: true })
    .getByRole("button", { name: "Close", exact: true })
    .click();
  await expect(draft).toHaveCount(0);
  await expect(pane.getByRole("region", { name: "Context", exact: true })).toBeFocused();
});

test("on a phone the context sheet has Save and Cancel, and a conflict keeps it open", async ({ page, heapPath }) => {
  test.skip(!isPhone(page), "the sheet is the phone's form");
  await addItem(page, "Write the plan");
  await openItem(page, "Write the plan");
  const sheet = page.getByRole("dialog", { name: "Context", exact: true });
  await (await editContext(page)).fill("Mine");
  await expect(sheet.getByRole("button", { name: "Save", exact: true })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Cancel", exact: true })).toBeVisible();

  await changeElsewhere(page, heapPath, { description: "Theirs", title: "Renamed once" });
  await refetchShowing(page, "Renamed once");
  // Dismissing with a changed draft runs the same save, so the conflict keeps the sheet open.
  await sheet.getByRole("button", { name: "Close", exact: true }).click();
  await expect(sheet.getByRole("heading", { name: "Saved meanwhile", exact: true })).toBeVisible();
  await expect(sheet.getByRole("textbox", { name: "Context", exact: true })).toHaveValue("Mine");

  await sheet.getByRole("button", { name: "Save again", exact: true }).click();
  await expect(sheet).toHaveCount(0);
  await expect(itemPane(page).getByRole("region", { name: "Context", exact: true })).toContainText("Mine");
});

test("a context typed for the wrap-up is saved before Done closes the item", async ({ page }) => {
  test.skip(isPhone(page), "a phone edits the context in a sheet, which covers the pane's Done");
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const pane = itemPane(page);
  await (await editContext(page)).fill("Shipped, and the notes are in the wiki");

  await changeState(page, "Done", "more");
  await expect(
    pane.getByRole("group", { name: "State", exact: true }).getByRole("button", { name: "Reopen" }),
  ).toBeVisible();
  await expect(pane.getByRole("region", { name: "Context", exact: true })).toContainText(
    "Shipped, and the notes are in the wiki",
  );
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveCount(0);
});

test("leaving with a conflict shown doesn't save the draft over theirs", async ({ page, heapPath }) => {
  test.skip(isPhone(page), "the phone's sheet is checked in its own test");
  await addItem(page, "Write the plan");
  await openItem(page, "Write the plan");
  const pane = itemPane(page);
  await (await editContext(page)).fill("Mine");
  await changeElsewhere(page, heapPath, { description: "Theirs", title: "Renamed" });
  await refetchShowing(page, "Renamed");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(pane.getByRole("heading", { name: "Saved meanwhile", exact: true })).toBeVisible();

  await closeItem(page);
  await openItem(page, "Renamed");
  await expect(itemPane(page).getByRole("region", { name: "Context", exact: true })).toContainText("Theirs");
  await expect(itemPane(page).getByRole("region", { name: "Context", exact: true })).not.toContainText("Mine");
});

test("on a phone the sheet of an item closed elsewhere shows the draft and still dismisses", async ({
  page,
  heapPath,
}) => {
  test.skip(!isPhone(page), "the sheet is the phone's form");
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const sheet = page.getByRole("dialog", { name: "Context", exact: true });
  await (await editContext(page)).fill("Wrap-up that never got saved");
  await changeElsewhere(page, heapPath, { state: "done" });
  await page.evaluate(() => window.dispatchEvent(new Event("visibilitychange")));
  await expect(sheet.getByText("This item can't be edited any more; your text wasn't saved.")).toBeVisible();

  await sheet.getByRole("button", { name: "Close", exact: true }).first().click();
  await expect(sheet).toHaveCount(0);
});

/** Makes a task through the API with one step, which has this context, and opens it. */
async function openTaskWithStep(page: Page, heapPath: string, context?: string) {
  const headers = { "x-csrf-protection": "1" };
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: "Rotate the certs", need: "expected" },
  });
  expect(created.status()).toBe(201);
  const { key, version } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  const made = await page.request.patch(item, {
    headers: { ...headers, "If-Match": `"${version}"` },
    data: { kind: "task" },
  });
  expect(made.status()).toBe(200);
  const step = await page.request.post(`${item}/steps`, { headers, data: { title: "Inventory the certs", context } });
  expect(step.status()).toBe(201);
  await page.goto(`${heapPath}/items/${key}`);
  const steps = itemPane(page).getByRole("region", { name: "Steps" });
  await expect(steps.getByRole("textbox", { name: "Step title" })).toHaveValue("Inventory the certs");
  return steps;
}

test("a step's context renders as Markdown, and Edit opens its source in a field that saves on leaving it", async ({
  page,
  heapPath,
}) => {
  const steps = await openTaskWithStep(page, heapPath);
  // With no context the field is there at once, so adding takes one step; leaving it saves and shows it rendered.
  await steps.getByRole("button", { name: "Show context" }).click();
  const field = steps.getByRole("textbox", { name: "Step context" });
  await field.fill("Use **bold**\nand <b>raw</b>");
  await field.press("Tab");
  await expect(steps.locator("strong")).toHaveText("bold");
  await expect(field).toHaveCount(0);
  await expect(steps.locator("b")).toHaveCount(0);
  const edit = steps.getByRole("button", { name: "Edit step context" });
  await expect(edit).toBeVisible();

  // Edit shows the source, focused with the caret at the end; leaving it saves the change.
  await edit.click();
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("Use **bold**\nand <b>raw</b>");
  await page.keyboard.type(" more");
  await field.press("Tab");
  await expect(steps.getByText("raw more")).toBeVisible();
  await expect(field).toHaveCount(0);

  // Unchanged text goes back to the rendered text without a save.
  await edit.click();
  await field.press("Tab");
  await expect(field).toHaveCount(0);
  await expect(edit).toBeVisible();
  await page.reload();
  await itemPane(page).getByRole("button", { name: "Show context" }).click();
  await expect(itemPane(page).locator("strong")).toHaveText("bold");
});

test("a reader sees a step's context rendered and no Edit", async ({ page, heapPath }) => {
  const steps = await openTaskWithStep(page, heapPath, "Check **all** hosts");
  const archived = await page.request.post(`/api/v1${heapPath}/archive`, { headers: { "x-csrf-protection": "1" } });
  expect(archived.ok()).toBe(true);
  await page.reload();
  await steps.getByRole("button", { name: "Show context" }).click();
  await expect(steps.locator("strong")).toHaveText("all");
  await expect(steps.getByRole("button", { name: "Edit step context" })).toHaveCount(0);
  await expect(steps.getByRole("textbox", { name: "Step context" })).toHaveCount(0);
});

test("a step's context edit that fails to save stays in the field, and Escape neither drops it nor closes the item", async ({
  page,
  heapPath,
}) => {
  const steps = await openTaskWithStep(page, heapPath, "Old text");
  await steps.getByRole("button", { name: "Show context" }).click();
  await steps.getByRole("button", { name: "Edit step context" }).click();
  const field = steps.getByRole("textbox", { name: "Step context" });
  await field.fill("New text");
  await field.press("Escape");
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("New text");
  await expect(itemPane(page)).toBeVisible();

  // The save is refused: the field stays, with the draft, and the reason shows.
  await page.route(/\/steps\/\d+$/, (route) =>
    route.request().method() === "PATCH"
      ? route.fulfill({
          status: 500,
          contentType: "application/problem+json",
          body: JSON.stringify({ type: "about:blank", title: "Internal error", status: 500, code: "internal" }),
        })
      : route.fallback(),
  );
  await field.press("Tab");
  await expect(steps.getByRole("alert")).toBeVisible();
  await expect(field).toHaveValue("New text");
  await expect(steps.locator("p", { hasText: "Old text" })).toHaveCount(0);

  // Once it can be saved, leaving the field again saves it and shows it rendered.
  await page.unroute(/\/steps\/\d+$/);
  await field.focus();
  await field.press("Tab");
  await expect(field).toHaveCount(0);
  await expect(steps.getByText("New text")).toBeVisible();
});
