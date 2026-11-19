import type { Locator, Page } from "@playwright/test";

import {
  addDialog,
  addHeaps,
  addItem,
  changeState,
  closeField,
  closeItem,
  createTeam,
  dayFromToday,
  editContext,
  expect,
  expectOrder,
  field,
  fieldButton,
  fillEditor,
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
  await fillEditor(await editContext(page), "For the next quarter");
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
  await fillEditor(await editContext(page), source);
  await saveContext(page, "bold and a link");
  await expect(context.locator("strong")).toHaveText("bold");
  const link = context.getByRole("link", { name: "link", exact: true });
  await expect(link).toHaveAttribute("target", "_blank");
  await expect(link).toHaveAttribute("rel", "noopener noreferrer");
  await expect(context.getByRole("listitem")).toHaveText(["one", "two"]);
  // A script-scheme link and a remote image are never live: the link reads as plain text, the image as a link.
  await fillEditor(await editContext(page), "[bad](javascript:alert(1)) and ![x](https://example.com/t.png)");
  await saveContext(page, "bad and");
  await expect(context.locator("a[href^='javascript:' i]")).toHaveCount(0);
  await expect(context.getByRole("link", { name: "bad", exact: true })).toHaveCount(0);
  await expect(context.locator("img")).toHaveCount(0);
  await expect(context.getByRole("link", { name: "x", exact: true })).toHaveAttribute(
    "href",
    "https://example.com/t.png",
  );
  await fillEditor(await editContext(page), source);
  await saveContext(page, "bold and a link");
  // Raw HTML is dropped, not run.
  await expect(pane.locator("script")).toHaveCount(0);
  expect(await page.evaluate(() => "pwned" in window)).toBe(false);

  // Save and Cancel are the ways out of the form; the focus goes back to Edit. The rest is checked on a wider screen.
  test.skip(isPhone(page), "the phone's sheet is checked in its own test");
  const area = await editContext(page);
  await expect(area).toHaveText(source, { useInnerText: true });
  await fillEditor(area, "Just this");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(context).toContainText("Just this");
  await expect(pane.getByRole("button", { name: "Edit context" })).toBeFocused();

  await fillEditor(await editContext(page), "Never mind");
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Just this");
  await expect(context).not.toContainText("Never mind");
  await expect(pane.getByRole("button", { name: "Edit context" })).toBeFocused();

  // Escape in the textarea does nothing: the draft stays and the item stays open.
  const kept = await editContext(page);
  await fillEditor(kept, "Not lost to a stray key");
  await page.keyboard.press("Escape");
  await expect(kept).toHaveText("Not lost to a stray key");
  await expect(pane).toBeVisible();
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Just this");

  // The editor styles the Markdown as it is typed, and what is saved is the plain text.
  const styled = await editContext(page);
  await fillEditor(styled, "# Title\n\n**bold**");
  await expect(styled.getByText("bold", { exact: true })).toHaveCSS("font-weight", "600");
  await expect(styled.getByText("Title", { exact: true })).toHaveCSS("font-weight", "600");
  // The heading is 1.07 times the text around it, whatever size that is on this screen.
  const sizes = await styled.evaluate((content) => {
    const size = (node: Element) => parseFloat(getComputedStyle(node).fontSize);
    const title = [...content.querySelectorAll("span")].find((span) => span.textContent?.includes("Title"));
    return { text: size(content), title: title ? size(title) : 0 };
  });
  expect(sizes.title).toBeCloseTo(sizes.text * 1.07, 1);
  // ⌘ or Ctrl with Enter saves: the text shows rendered and the form is gone.
  await styled.press("ControlOrMeta+Enter");
  await expect(context.getByRole("heading", { name: "Title" })).toBeVisible();
  await expect(styled).toBeHidden();

  // Closing the item with a changed draft saves it, so nothing typed is lost.
  await fillEditor(await editContext(page), "Kept on close");
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
  for (const name of ["Block", "Done", "Won't do"]) {
    await expect(moreEntry(page, more, name)).toBeVisible();
  }
  // The item's link is copied from its key in the header, not from ⋯.
  await expect(moreEntry(page, more, "Copy link")).toHaveCount(0);
  // Done from ⋯ closes the item; the pane offers to reopen it, and focus goes there, as ⋯ has gone.
  await moreEntry(page, more, "Done").click();
  const reopen = state.getByRole("button", { name: "Reopen", exact: true });
  await expect(reopen).toBeVisible();
  await expect(reopen).toBeFocused();
  await expect(state.getByRole("button", { name: "More", exact: true })).toHaveCount(0);
});

test("a reader's item has values to read, no Edit, no ⋯, and its link to copy", async ({ page, context, heapPath }) => {
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
  // Nothing to change and nothing in ⋯, so there is neither.
  await expect(pane.getByRole("group", { name: "State", exact: true })).toHaveCount(0);
  await expect(pane.getByRole("button", { name: "More", exact: true })).toHaveCount(0);

  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await pane.getByRole("button", { name: `Copy link to ${key}`, exact: true }).click();
  await expect(pane.getByRole("status").filter({ hasText: "Link copied" })).toHaveCount(1);
});

test("the key in the item's header copies its short link, and a check confirms it", async ({
  page,
  context,
  heapPath,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await addItem(page, "Share the plan");
  await openItem(page, "Share the plan");
  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  const pane = itemPane(page);
  const copy = pane.getByRole("button", { name: `Copy link to ${key}`, exact: true });
  await expect(copy).toHaveText(key);
  await expect(copy).toHaveAttribute("title", "Copy link");
  if (isPhone(page)) expect(Math.round((await copy.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
  await expect(copy.locator("svg.lucide-link")).toHaveCount(1);

  await copy.click();
  // The check shows only after the write worked, so the clipboard is read after it.
  await expect(copy.locator("svg.lucide-check")).toHaveCount(1);
  await expect(pane.getByRole("status").filter({ hasText: "Link copied" })).toHaveCount(1);
  const [, , team, , heap] = heapPath.split("/");
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toBe(`${new URL(page.url()).origin}/h/${team}/${heap}/${key}`);
  // After two seconds the link icon is back.
  await expect(copy.locator("svg.lucide-link")).toHaveCount(1);
  await expect(pane.getByRole("status").filter({ hasText: "Link copied" })).toHaveCount(0);

  // The copied link opens the item.
  await page.goto(copied);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Share the plan");
});

test("a copy of the item's link the browser refuses says so in a toast", async ({ page }) => {
  test.skip(page.viewportSize()?.width !== 1280, "One width is enough for the failure.");
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("denied")) },
      configurable: true,
    });
  });
  await page.reload();
  await addItem(page, "Share the plan");
  await openItem(page, "Share the plan");
  const pane = itemPane(page);
  const copy = pane.getByRole("button", { name: /^Copy link to / });
  await copy.click();
  await expect(page.getByText("Couldn't copy the link.", { exact: true })).toBeVisible();
  await expect(copy.locator("svg.lucide-link")).toHaveCount(1);
  await expect(pane.getByRole("status").filter({ hasText: "Link copied" })).toHaveCount(0);
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
test("History shows an edited context as a diff, its long unchanged start folded", async ({ page }) => {
  await addItem(page, "Plan the launch");
  await openItem(page, "Plan the launch");
  const start = Array.from({ length: 30 }, (_, i) => `step${i + 1}`).join(" ");
  await fillEditor(await editContext(page), `${start} then ship on Friday`);
  await saveContext(page, "ship on Friday");
  await fillEditor(await editContext(page), `${start} then ship on Monday`);
  await saveContext(page, "ship on Monday");

  const history = await openHistory(page);
  const edits = history.getByRole("listitem").filter({ hasText: "You edited item" });
  const edit = edits.first();
  await expect(edit.locator("del")).toHaveText("removed Friday");
  await expect(edit.locator("ins")).toHaveText("added Monday");
  // 33 unchanged words before the edit keep the last 8; the rest fold behind a button that shows them.
  await expect(edit).not.toContainText("step1 ");
  await edit.getByRole("button", { name: "… 25 more words" }).click();
  await expect(edit).toContainText("step1 step2");
  // The button goes, so focus moves to the stretch it unfolded.
  await expect(edit.locator("span[tabindex='-1']")).toBeFocused();
  // Setting the context from nothing has nothing to diff, so it shows in full.
  await expect(edits.nth(1)).toContainText(`${start} then ship on Friday`);
  await expect(edits.nth(1).locator("ins, del")).toHaveCount(0);
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

test("a blocked item shows why in words, in the pane and on the row's padlock", async ({ page, heapPath }) => {
  await addItem(page, "Renew the domain");
  await addItem(page, "Pay the registrar");
  const status = () => itemPane(page).getByRole("group", { name: "Status" });

  // The reason is set from the CLI or the API; the web shows it. The text is only there once the server has it.
  await openItem(page, "Renew the domain");
  await changeElsewhere(page, heapPath, { state: "blocked", blocked_reason: "Waiting for the registrar's invoice" });
  await page.reload();
  await expect(status()).toContainText("Blocked: Waiting for the registrar's invoice");
  await closeItem(page);
  await page.getByRole("button", { name: "Show it" }).click();
  await expect(row(page, "Renew the domain").getByRole("button", { name: "Unblock Renew the domain" })).toHaveAttribute(
    "title",
    "Unblock. Blocked: Waiting for the registrar's invoice",
  );
  await expect(row(page, "Renew the domain").getByRole("link")).toContainText(
    ", blocked: Waiting for the registrar's invoice",
  );

  // Unblocking from the row clears the reason; Undo puts it back with the state.
  const padlock = row(page, "Renew the domain").getByRole("button", { name: "Unblock Renew the domain" });
  await padlock.click();
  await expect(padlock).toHaveCount(0);
  await page
    .getByRole("status")
    .filter({ hasText: "“Renew the domain” is open." })
    .getByRole("button", { name: "Undo" })
    .click();
  await expect(padlock).toHaveAttribute("title", "Unblock. Blocked: Waiting for the registrar's invoice");
  // The open row leaves as the blocked one comes back: wait until only one is there.
  await expect(row(page, "Renew the domain")).toHaveCount(1);

  // With a blocker named too, both show: who blocks it, then why.
  await openItem(page, "Renew the domain");
  await moreEntry(page, await openMore(page), "Change blocker…").click();
  await page
    .getByRole("dialog", { name: "Change blocker", exact: true })
    .getByLabel("Waiting for")
    .selectOption({ label: "Pay the registrar" });
  await expect(status().getByRole("link", { name: /^[0-9a-z]{5} Pay the registrar$/ })).toBeVisible();
  await expect(status()).toContainText(": Waiting for the registrar's invoice");

  // Leaving blocked clears it.
  await changeState(page, "Unblock", "main");
  await expect(status()).not.toContainText("Blocked");
  await expect(status()).not.toContainText("invoice");
});

test("a blocked item's reason is set, changed and removed from ⋯", async ({ page, heapPath }) => {
  await addItem(page, "Renew the domain");
  await openItem(page, "Renew the domain");
  const status = () => itemPane(page).getByRole("group", { name: "Status" });
  const editor = () => page.getByRole("dialog", { name: "Why it's blocked", exact: true });
  const openEditor = async (entry: string) => {
    await moreEntry(page, await openMore(page), entry).click();
    await expect(editor()).toBeVisible();
    return editor();
  };
  const more = itemPane(page).getByRole("group", { name: "State", exact: true }).getByRole("button", { name: "More" });

  // Set. The status line has the text only once the server answered.
  await changeState(page, "Block", "more");
  await expect(status()).toContainText("Blocked");
  let dialog = await openEditor("Add reason…");
  await expect(dialog.getByLabel("Reason")).toBeFocused();
  await expect(dialog.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await dialog.getByLabel("Reason").fill("Waiting for the invoice");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(status()).toContainText("Blocked: Waiting for the invoice");
  await expect(editor()).toHaveCount(0);
  await expect(more).toBeFocused();

  // Change, with Enter.
  dialog = await openEditor("Change reason…");
  await expect(dialog.getByLabel("Reason")).toHaveValue("Waiting for the invoice");
  await dialog.getByLabel("Reason").fill("Waiting for the receipt");
  await dialog.getByLabel("Reason").press("Enter");
  await expect(status()).toContainText("Blocked: Waiting for the receipt");
  await expect(editor()).toHaveCount(0);

  // Too long: the counter and the error show, and Save is off.
  dialog = await openEditor("Change reason…");
  await dialog.getByLabel("Reason").fill("x".repeat(301));
  await expect(dialog.getByText("301 / 300")).toBeVisible();
  await expect(dialog.getByText("Keep it to 300 characters.")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(editor()).toHaveCount(0);
  await expect(status()).toContainText("Blocked: Waiting for the receipt");

  // Remove.
  dialog = await openEditor("Change reason…");
  await dialog.getByRole("button", { name: "Remove reason" }).click();
  await expect(status()).not.toContainText("receipt");
  await expect(status()).toContainText("Blocked");
  await expect(editor()).toHaveCount(0);
  await moreEntry(page, await openMore(page), "Add reason…").waitFor();
  await page.keyboard.press("Escape");

  // A refused save keeps the editor and the draft, and says so.
  dialog = await openEditor("Add reason…");
  await dialog.getByLabel("Reason").fill("Waiting for the bank");
  await changeElsewhere(page, heapPath, { state: "open" });
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("Couldn't save the reason.");
  await expect(dialog.getByLabel("Reason")).toHaveValue("Waiting for the bank");
  // The first answer is the conflict, which brings the open item in; trying again says it isn't blocked.
  await expect(status()).not.toContainText("Blocked");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText("Couldn't save the reason. This item isn't blocked any more.");
  await expect(dialog.getByLabel("Reason")).toHaveValue("Waiting for the bank");
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

test("tags are edited in place in the Tags row, which closes on leaving it, Escape or Enter", async ({ page }) => {
  await addItem(page, "Pack for the trip");
  await openItem(page, "Pack for the trip");
  const pane = itemPane(page);
  const title = pane.getByLabel("Title", { exact: true });
  await expect(title).toBeVisible();
  const box = pane.getByRole("combobox", { name: "Add a tag" });

  // A click turns the row into the strip, its box focused, with no popover or sheet.
  await fieldButton(page, "Tags").click();
  await expect(box).toBeFocused();
  await expect(fieldButton(page, "Tags")).toHaveCount(0);
  await expect(page.getByRole("dialog", { name: "Tags", exact: true })).toHaveCount(0);

  // Each new tag saves, and the strip stays for the next one. The chips come from the server's answer.
  await box.fill("travel");
  await page.getByRole("option", { name: "Create heap tag “travel”" }).click();
  await expect(pane.getByRole("button", { name: "Remove tag travel" })).toBeVisible();
  await box.fill("packing");
  await box.press("Enter");
  await expect(pane.getByRole("button", { name: "Remove tag packing" })).toBeVisible();
  await expect(box).toBeFocused();

  // Enter in the empty box closes it, and focus goes back to the value.
  await box.press("Enter");
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: packing, travel");
  await expect(fieldButton(page, "Tags")).toBeFocused();

  // Removing a tag keeps it open. Escape clears typed text first; in the empty box it closes the strip, not the item.
  await fieldButton(page, "Tags").click();
  await pane.getByRole("button", { name: "Remove tag travel" }).click();
  await expect(pane.getByRole("button", { name: "Remove tag travel" })).toHaveCount(0);
  await expect(box).toBeFocused();
  await box.fill("x");
  await box.press("Escape");
  await expect(box).toHaveValue("");
  await expect(box).toBeFocused();
  await box.press("Escape");
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: packing");
  await expect(fieldButton(page, "Tags")).toBeFocused();
  await expect(title).toBeVisible();

  // A Remove reached with the keyboard hands the focus back to the box, so Escape still closes only the strip.
  await fieldButton(page, "Tags").click();
  await box.fill("trip");
  await box.press("Enter");
  await expect(pane.getByRole("button", { name: "Remove tag trip" })).toBeVisible();
  await box.press("Shift+Tab");
  await expect(pane.getByRole("button", { name: "Remove tag trip" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(pane.getByRole("button", { name: "Remove tag trip" })).toHaveCount(0);
  await expect(box).toBeFocused();
  await box.press("Escape");
  await expect(fieldButton(page, "Tags")).toBeFocused();
  await expect(title).toBeVisible();

  // An existing tag is picked from the matches; leaving the strip closes it, and focus stays where it went.
  await fieldButton(page, "Tags").click();
  await box.fill("trav");
  await page.getByRole("option", { name: "travel", exact: true }).click();
  await expect(pane.getByRole("button", { name: "Remove tag travel" })).toBeVisible();
  await title.click();
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: packing, travel");
  await expect(title).toBeFocused();

  // A click on a control below the strip lands, though the row grows shorter as the strip closes.
  await fieldButton(page, "Tags").click();
  await expect(box).toBeFocused();
  await editContext(page);
  await expect(box).toHaveCount(0);
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
  await fillEditor(area, "Venue first");
  await page.keyboard.press("Escape");
  await expect(area).toHaveText("Venue first");
  await page.mouse.click(200, 20);
  await expect(context).toContainText("Venue first");
  await fillEditor(await editContext(page), "Venue, then dates");
  await saveContext(page, "Venue, then dates");

  // A new tag in the Tags row, in place in the item's sheet, its offer in view under the field.
  await fieldButton(page, "Tags").click();
  await pane.getByLabel("Add a tag").fill("travel");
  await pane.getByRole("option", { name: "Create heap tag “travel”" }).click();
  await expect(pane.getByRole("button", { name: "Remove tag travel" })).toBeVisible();
  await pane.getByLabel("Add a tag").press("Escape");
  await expect(fieldButton(page, "Tags")).toHaveAccessibleName("Tags: travel");

  // Won't do is under ⋯; a closed item shows all its fields, unchangeable.
  await changeState(page, "Won't do", "more");
  await expect(context.getByText("Venue, then dates", { exact: true })).toBeVisible();
  await expect(pane.getByRole("button", { name: /^(Edit|Add) context$/ })).toHaveCount(0);
  await expect(pane.getByRole("button", { name: /^(Need|Effort|Impact|Due|Tags): / })).toHaveCount(0);
  // Reopen is all a closed item can do, so the bar has no ⋯.
  await expect(bar.getByRole("button")).toHaveCount(1);
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

test("an item is moved to another heap of its team from ⋯, after a confirm step", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  await addItem(page, "Water the plants");
  await openItem(page, "Water the plants");

  await moreEntry(page, await openMore(page), "Move to another heap…").click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await picker.getByRole("menuitem", { name: "Garden" }).click();
  await expect(picker.getByRole("heading", { name: "Move to Garden?" })).toBeVisible();
  await expect(picker).toContainText("Tags it doesn't have there are dropped.");
  await picker.getByRole("button", { name: "Move", exact: true }).click();

  await expect(page).toHaveURL(/\/teams\/[^/]+\/heaps\/garden\/items\/[^/]+$/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");
  await expect(page.getByRole("status").filter({ hasText: "Moved to Garden." })).toBeVisible();
});

test("an item is moved to a heap of another team from ⋯", async ({ page, heapPath }) => {
  await createTeam(page, "Acme");
  const acme = new URL(page.url()).pathname.split("/")[2] ?? "";
  const made = await page.request.post(`/api/v1/teams/${acme}/heaps`, {
    headers: { "x-csrf-protection": "1" },
    data: { name: "Roadmap" },
  });
  expect(made.status()).toBe(201);
  await page.goto(heapPath);
  await addItem(page, "Water the plants");
  await openItem(page, "Water the plants");

  await moreEntry(page, await openMore(page), "Move to another heap…").click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await expect(picker.getByText("Acme", { exact: true })).toBeVisible();
  await picker.getByRole("menuitem", { name: "Roadmap" }).click();
  await expect(picker.getByRole("heading", { name: "Move to Roadmap in Acme?" })).toBeVisible();
  await expect(picker).toContainText("Tags and values Acme doesn't have are dropped.");
  await expect(picker).toContainText("Its history stays in");
  await expect(picker).not.toContainText("sent it to an agent");
  await picker.getByRole("button", { name: "Move", exact: true }).click();

  await expect(page).toHaveURL(new RegExp(`/teams/${acme}/heaps/roadmap/items/[^/]+$`));
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");
  await expect(page.getByRole("status").filter({ hasText: "Moved to Roadmap in Acme." })).toBeVisible();
});

test("a note heaps were made from moves only within its team", async ({ page, heapPath }) => {
  await createTeam(page, "Acme");
  const acme = new URL(page.url()).pathname.split("/")[2] ?? "";
  const headers = { "x-csrf-protection": "1" };
  const made = await page.request.post(`/api/v1/teams/${acme}/heaps`, { headers, data: { name: "Roadmap" } });
  expect(made.status()).toBe(201);
  await page.goto(heapPath);
  await addItem(page, "Plan the garden");
  await openItem(page, "Plan the garden");
  const key = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  const note = await page.request.get(`/api/v1${heapPath}/items/${key}`);
  const { id } = (await note.json()) as { id: string };
  const team = /\/teams\/([^/]+)\//.exec(heapPath)?.[1] ?? "";
  for (const name of ["Garden", "Beds"]) {
    const created = await page.request.post(`/api/v1/teams/${team}/heaps`, {
      headers,
      data: name === "Garden" ? { name, origin_id: id } : { name },
    });
    expect(created.status()).toBe(201);
  }
  await page.goto(`${heapPath}/items/${key}`);

  await moreEntry(page, await openMore(page), "Move to another heap…").click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await expect(picker.getByRole("menuitem", { name: "Beds" })).toBeVisible();
  await expect(picker.getByRole("menuitem", { name: "Roadmap" })).toHaveCount(0);
  await expect(picker).toContainText(/Heaps were made from this note, so it moves only within .+'s team\./);
});

test("several items are selected in the list and moved to another heap at once", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds", "Rotate the certs"]) await addItem(page, title);
  const select = page.getByRole("button", { name: "Select", exact: true });
  const picked = (title: string) => page.getByRole("checkbox", { name: new RegExp(`^${title}`) });

  // Back leaves select mode, and Select goes into it again with nothing selected.
  await select.click();
  await expect(picked("Water the plants")).toBeVisible();
  await page.goBack();
  await expect(select).toBeVisible();
  await select.click();
  await expect(page.getByRole("status").filter({ hasText: "0 selected" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Move to…" })).toBeDisabled();

  await picked("Water the plants").click();
  await picked("Pull the weeds").click();
  await expect(picked("Water the plants")).toBeChecked();
  await expect(picked("Rotate the certs")).not.toBeChecked();
  await expect(page.getByRole("status").filter({ hasText: "2 selected" })).toBeVisible();

  await page.getByRole("button", { name: "Move to…" }).click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await picker.getByRole("menuitem", { name: "Garden" }).click();
  await expect(picker.getByRole("heading", { name: "Move 2 items to Garden?" })).toBeVisible();
  await expect(picker).toContainText("Tags they don't have there are dropped.");
  await picker.getByRole("button", { name: "Move 2", exact: true }).click();

  // The moved rows are gone, the list is out of select mode, and the toast leads to the heap they went to.
  const toast = page.getByRole("status").filter({ hasText: "Moved 2 items to Garden." });
  await expect(toast).toBeVisible();
  await expect(page.getByRole("link", { name: /^Rotate the certs/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Water the plants/ })).toHaveCount(0);
  await expect(select).toBeVisible();
  await toast.getByRole("button", { name: "Open Garden" }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/heaps\/garden$/);
  await expect(page.getByRole("link", { name: /^Water the plants/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Pull the weeds/ })).toBeVisible();
});

/** Goes into select mode on a heap that has another heap, with the items titled `titles` selected. */
async function selectItems(page: Page, titles: string[]) {
  await page.getByRole("button", { name: "Select", exact: true }).click();
  for (const title of titles) await page.getByRole("checkbox", { name: new RegExp(`^${title}`) }).click();
  await expect(page.getByRole("status").filter({ hasText: `${titles.length} selected` })).toBeVisible();
}

/** Adds an item through the API and returns its key and version. */
async function seedItem(page: Page, heapPath: string, title: string) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers: { "x-csrf-protection": "1" },
    data: { title, need: "expected" },
  });
  expect(created.status()).toBe(201);
  return (await created.json()) as { key: string; version: number };
}

const bulkButton = (page: Page, name: string) => page.getByRole("button", { name, exact: true });
const toastWith = (page: Page, text: string) => page.getByRole("status").filter({ hasText: text });

test("selected items are marked done at once, after asking", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds", "Rotate the certs"]) await addItem(page, title);
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  await bulkButton(page, "Done").click();
  const question = page.getByRole("alertdialog", { name: "Mark 2 items done?" });
  await expect(question).toContainText("There is no Undo.");
  await question.getByRole("button", { name: "Cancel" }).click();
  await expect(bulkButton(page, "Done")).toBeFocused();

  await bulkButton(page, "Done").click();
  await question.getByRole("button", { name: "Mark 2 done", exact: true }).click();
  await expect(toastWith(page, "Marked 2 items done.")).toBeVisible();
  await expect(page.getByRole("link", { name: /^Water the plants/ })).toHaveCount(0);
  await expect(page.getByRole("link", { name: /^Pull the weeds/ })).toHaveCount(0);
  await expect(page.getByRole("link", { name: /^Rotate the certs/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Select", exact: true })).toBeVisible();
});

test("selected items are closed as won't do at once, after asking", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds"]) await addItem(page, title);
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  await bulkButton(page, "Won't do").click();
  const question = page.getByRole("alertdialog", { name: "Close 2 items as won't do?" });
  await question.getByRole("button", { name: "Close 2", exact: true }).click();
  await expect(toastWith(page, "Closed 2 items as won't do.")).toBeVisible();
  await expect(page.getByRole("link", { name: /^Water the plants/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Select", exact: true })).toBeVisible();
});

test("selected items are started and paused at once, and stay selected", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds"]) await addItem(page, title);
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  await bulkButton(page, "Start").click();
  await expect(toastWith(page, "Started 2 items.")).toBeVisible();
  await expect(bulkButton(page, "Pause")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "2 selected" })).toBeVisible();

  await bulkButton(page, "Pause").click();
  await expect(toastWith(page, "Paused 2 items.")).toBeVisible();
  await expect(bulkButton(page, "Start")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "2 selected" })).toBeVisible();
});

test("a blocked item starts with the others, without a question", async ({ page, heapPath }) => {
  await addHeaps(page, ["Garden"]);
  const blocked = await seedItem(page, heapPath, "Water the plants");
  await seedItem(page, heapPath, "Pull the weeds");
  const patched = await page.request.patch(`/api/v1${heapPath}/items/${blocked.key}`, {
    headers: { "x-csrf-protection": "1", "if-match": `"${blocked.version}"` },
    data: { state: "blocked", blocked_reason: "Waiting for the hose" },
  });
  expect(patched.status()).toBe(200);
  await page.goto(heapPath);
  await page.getByRole("button", { name: "Show it" }).click();
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  await bulkButton(page, "Start").click();
  await expect(toastWith(page, "Started 2 items.")).toBeVisible();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
});

test("an item the server refuses is listed and stays selected", async ({ page, heapPath }) => {
  await addHeaps(page, ["Garden"]);
  await seedItem(page, heapPath, "Water the plants");
  const stale = await seedItem(page, heapPath, "Pull the weeds");
  await page.goto(heapPath);
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  // Another edit makes the page's copy of one item stale.
  const edited = await page.request.patch(`/api/v1${heapPath}/items/${stale.key}`, {
    headers: { "x-csrf-protection": "1", "if-match": `"${stale.version}"` },
    data: { effort: "l" },
  });
  expect(edited.status()).toBe(200);

  await bulkButton(page, "Done").click();
  await page
    .getByRole("alertdialog", { name: "Mark 2 items done?" })
    .getByRole("button", { name: "Mark 2 done" })
    .click();
  const panel = page.getByRole("dialog", { name: "Not changed", exact: true });
  await expect(panel.getByRole("heading", { name: "Marked 1 of 2 items done." })).toBeVisible();
  await expect(panel).toContainText("Not marked done:");
  await expect(panel).toContainText("Pull the weeds — ");
  await panel.getByRole("button", { name: "Close" }).filter({ hasText: "Close" }).click();
  await expect(page.getByRole("status").filter({ hasText: "1 selected" })).toBeVisible();
  await expect(page.getByRole("checkbox", { name: /^Pull the weeds/ })).toBeChecked();
});

test("selected items are sent to an agent at once", async ({ page }) => {
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds"]) await addItem(page, title);
  await selectItems(page, ["Water the plants", "Pull the weeds"]);

  await bulkButton(page, "Send to an agent…").click();
  const form = page.getByRole("dialog", { name: "Send to an agent", exact: true });
  await expect(form.getByRole("radio", { name: "One step" })).toBeChecked();
  await form.locator("label").filter({ hasText: "All the way" }).click();
  await form.getByRole("button", { name: "Send", exact: true }).click();
  await expect(toastWith(page, "Sent 2 items to an agent.")).toBeVisible();
  await expect(form).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "2 selected" })).toBeVisible();
});

test("selecting from an open item leaves on the list, not on the item, after a move", async ({ page }) => {
  test.skip(
    page.viewportSize()?.width !== 1280,
    "Select shows beside an open item only where the pane sits beside the list.",
  );
  await addHeaps(page, ["Garden"]);
  for (const title of ["Water the plants", "Pull the weeds"]) await addItem(page, title);
  const picked = (title: string) => page.getByRole("checkbox", { name: new RegExp(`^${title}`) });
  const listUrl = /\/teams\/[^/]+\/heaps\/personal$/;
  await openItem(page, "Water the plants");

  await page.getByRole("button", { name: "Select", exact: true }).click();
  await picked("Water the plants").click();
  await picked("Pull the weeds").click();
  await page.getByRole("button", { name: "Move to…" }).click();
  const picker = page.getByRole("dialog", { name: "Move to", exact: true });
  await picker.getByRole("menuitem", { name: "Garden" }).click();
  await picker.getByRole("button", { name: "Move 2", exact: true }).click();

  await expect(page.getByRole("status").filter({ hasText: "Moved 2 items to Garden." })).toBeVisible();
  await expect(page).toHaveURL(listUrl);
  await expect(itemPane(page)).toHaveCount(0);
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
  await fillEditor(await editContext(page), "Mine");
  await changeElsewhere(page, heapPath, { description: "Theirs", title: "Renamed once" });
  await refetchShowing(page, "Renamed once");
  await pane.getByRole("button", { name: "Save", exact: true }).click();
  await expect(saved).toBeVisible();
  await expect(pane.getByText("Theirs", { exact: true })).toBeVisible();
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveText("Mine");
  await pane.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(context).toContainText("Theirs");
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveCount(0);

  // Save again replaces it with the draft.
  await fillEditor(await editContext(page), "Mine again");
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
  await fillEditor(await editContext(page), "Shipped");
  await changeElsewhere(page, heapPath, { description: "Held back", title: "Renamed" });
  await refetchShowing(page, "Renamed");

  await changeState(page, "Done", "more");
  await expect(pane.getByRole("heading", { name: "Saved meanwhile", exact: true })).toBeVisible();
  await expect(pane.getByRole("textbox", { name: "Context", exact: true })).toHaveText("Shipped");
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
  await fillEditor(await editContext(page), "Original");
  await saveContext(page, "Original");
  await fillEditor(await editContext(page), "Wrap-up that never got saved");
  await changeElsewhere(page, heapPath, { state: "done" });
  await page.evaluate(() => window.dispatchEvent(new Event("visibilitychange")));

  await expect(pane.getByText("This item can't be edited any more; your text wasn't saved.")).toBeVisible();
  const draft = pane.getByRole("textbox", { name: "Context", exact: true });
  await expect(draft).toHaveText("Wrap-up that never got saved");
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
  await fillEditor(await editContext(page), "Mine");
  await expect(sheet.getByRole("button", { name: "Save", exact: true })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Cancel", exact: true })).toBeVisible();

  await changeElsewhere(page, heapPath, { description: "Theirs", title: "Renamed once" });
  await refetchShowing(page, "Renamed once");
  // Dismissing with a changed draft runs the same save, so the conflict keeps the sheet open.
  await sheet.getByRole("button", { name: "Close", exact: true }).click();
  await expect(sheet.getByRole("heading", { name: "Saved meanwhile", exact: true })).toBeVisible();
  await expect(sheet.getByRole("textbox", { name: "Context", exact: true })).toHaveText("Mine");

  await sheet.getByRole("button", { name: "Save again", exact: true }).click();
  await expect(sheet).toHaveCount(0);
  await expect(itemPane(page).getByRole("region", { name: "Context", exact: true })).toContainText("Mine");
});

test("a context typed for the wrap-up is saved before Done closes the item", async ({ page }) => {
  test.skip(isPhone(page), "a phone edits the context in a sheet, which covers the pane's Done");
  await addItem(page, "Ship the release");
  await openItem(page, "Ship the release");
  const pane = itemPane(page);
  await fillEditor(await editContext(page), "Shipped, and the notes are in the wiki");

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
  await fillEditor(await editContext(page), "Mine");
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
  await fillEditor(await editContext(page), "Wrap-up that never got saved");
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
  await fillEditor(field, "Use **bold**\nand <b>raw</b>");
  await field.press("Tab");
  await expect(steps.locator("strong")).toHaveText("bold");
  await expect(field).toHaveCount(0);
  await expect(steps.locator("b")).toHaveCount(0);
  const edit = steps.getByRole("button", { name: "Edit step context" });
  await expect(edit).toBeVisible();

  // Edit shows the source, focused with the caret at the end; leaving it saves the change.
  await edit.click();
  await expect(field).toBeFocused();
  await expect(field).toHaveText("Use **bold**\nand <b>raw</b>", { useInnerText: true });
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
  await fillEditor(field, "New");
  // Escape leaves the draft and the item open: typing goes on in the same field.
  await field.press("Escape");
  await page.keyboard.type(" text");
  await expect(field).toHaveText("New text");
  await expect(page).toHaveURL(/\/items\//);

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
  await expect(field).toHaveText("New text");
  await expect(steps.locator("p", { hasText: "Old text" })).toHaveCount(0);

  // Once it can be saved, leaving the field again saves it and shows it rendered.
  await page.unroute(/\/steps\/\d+$/);
  await field.focus();
  await field.press("Tab");
  await expect(field).toHaveCount(0);
  await expect(steps.getByText("New text")).toBeVisible();
});

test("the list keeps its place while items open, switch, walk and close over it", async ({ page, heapPath }) => {
  const titles = Array.from({ length: 40 }, (_, i) => `Item ${String(i + 1).padStart(2, "0")}`);
  for (const title of titles) {
    const created = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data: { title, need: "expected" },
    });
    expect(created.status()).toBe(201);
  }
  await page.goto(heapPath);
  const rows = page.getByRole("listitem").filter({ has: page.locator("[data-row-link]") });
  await expect(rows).toHaveCount(titles.length);

  // Far down the list, with the row in the middle of the screen.
  const far = rows.nth(30);
  await far.evaluate((element) => element.scrollIntoView({ block: "center" }));
  const scrolled = await page.evaluate(() => window.scrollY);
  expect(scrolled).toBeGreaterThan(200);
  const scrollY = () => page.evaluate(() => window.scrollY);
  const title = (locator: typeof far) =>
    locator.locator("[data-row-link]").evaluate((link) => link.firstChild?.textContent ?? "");

  await openItem(page, await title(far));
  await expect.poll(scrollY).toBe(scrolled);
  await expect(far).toBeInViewport();

  if (!isPhone(page)) {
    // Another row, opened in place of this item. From the keyboard: a click retried while the pane still slides in
    // scrolls the row into view itself.
    const next = rows.nth(31);
    await next.locator("[data-row-link]").evaluate((link: HTMLElement) => link.focus({ preventScroll: true }));
    await page.keyboard.press("Enter");
    await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(await title(next));
    await expect.poll(scrollY).toBe(scrolled);

    // The pane's ↑ walks back to the row before.
    await itemPane(page).getByRole("link", { name: "Previous item" }).click();
    await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(await title(far));
    await expect.poll(scrollY).toBe(scrolled);
  }

  // Scrolled a little while the item is open, closing goes Back to the list where it is now. With Escape, as a click
  // on ✕ retried while the pane still slides in scrolls the window to bring it into view.
  await page.evaluate(() => window.scrollBy(0, -100));
  const moved = scrolled - 100;
  await expect.poll(scrollY).toBe(moved);
  await page.keyboard.press("Escape");
  await expect(itemPane(page)).toHaveCount(0);
  await expect.poll(scrollY).toBe(moved);

  // Opened from a link, the item closes to the list, which stays where it was scrolled to meanwhile.
  await openItem(page, await title(far));
  await page.goto(page.url());
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(await title(far));
  await far.evaluate((element) => element.scrollIntoView({ block: "center" }));
  const reloaded = await scrollY();
  expect(reloaded).toBeGreaterThan(200);
  await page.keyboard.press("Escape");
  await expect(itemPane(page)).toHaveCount(0);
  await expect.poll(scrollY).toBe(reloaded);
});

test("the browser's Back from an item leaves the list where it was scrolled beside it", async ({ page, heapPath }) => {
  for (let i = 1; i <= 40; i++) {
    const created = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data: { title: `Item ${String(i).padStart(2, "0")}`, need: "expected" },
    });
    expect(created.status()).toBe(201);
  }
  await page.goto(heapPath);
  const scrollY = () => page.evaluate(() => window.scrollY);
  await row(page, "Item 30")
    .locator("[data-row-link]")
    .evaluate((link) => link.scrollIntoView({ block: "center" }));
  const opened = await scrollY();
  expect(opened).toBeGreaterThan(200);

  await openItem(page, "Item 30");
  await expect.poll(scrollY).toBe(opened);
  await page.evaluate(() => window.scrollBy(0, -100));
  await expect.poll(scrollY).toBe(opened - 100);
  await page.goBack();
  await expect(itemPane(page)).toHaveCount(0);
  // Beside the item, the list stays where it is now, as ✕ leaves it; on a phone, where the item covers the list, the
  // list goes back to where it was when the item opened.
  await expect.poll(scrollY).toBe(isPhone(page) ? opened : opened - 100);

  // Forward opens the item again, and the browser puts the list back where it was when the item was left.
  await page.goForward();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Item 30");
  await expect.poll(scrollY).toBe(opened - 100);
});

test("the browser's Back and Forward between two items over the list leave it where it was scrolled", async ({
  page,
  heapPath,
}) => {
  const post = async (data: object) => {
    const response = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data,
    });
    expect(response.status()).toBe(201);
    return (await response.json()) as { id: string };
  };
  for (let i = 1; i <= 40; i++) {
    await post({ title: `Item ${String(i).padStart(2, "0")}`, need: "expected" });
  }
  // A note at the bottom of the list, and a task made from it at the top, whose From link opens the note.
  const note = await post({ title: "The note far down", kind: "note", need: "idea" });
  await post({ title: "Made from the note", kind: "task", need: "critical", origin: { item_id: note.id, age: "new" } });
  await page.goto(heapPath);
  const scrollY = () => page.evaluate(() => window.scrollY);
  const title = itemPane(page).getByLabel("Title", { exact: true });

  await openItem(page, "Made from the note");
  await itemPane(page)
    .getByRole("link", { name: /The note far down/ })
    .click();
  await expect(title).toHaveValue("The note far down");
  await expect.poll(scrollY).toBeGreaterThan(200);
  const opened = await scrollY();

  // Scrolled while the second item is open, Back to the first leaves the list where it is now beside it; on a phone,
  // where the item covers the list, the browser puts it back where it was when the first item was left.
  await page.evaluate(() => window.scrollBy(0, -100));
  await expect.poll(scrollY).toBe(opened - 100);
  await page.goBack();
  await expect(title).toHaveValue("Made from the note");
  if (isPhone(page)) {
    await expect.poll(scrollY).toBe(opened);
    return;
  }
  await expect.poll(scrollY).toBe(opened - 100);

  // Scrolled again, Forward to the second item does the same.
  await page.evaluate(() => window.scrollBy(0, -50));
  await expect.poll(scrollY).toBe(opened - 150);
  await page.goForward();
  await expect(title).toHaveValue("The note far down");
  await expect.poll(scrollY).toBe(opened - 150);
});

test("a link in the item to another in its heap brings that row into view, selected; another heap's starts at the top", async ({
  page,
  heapPath,
}) => {
  const post = async (path: string, data: object, headers: Record<string, string> = {}) => {
    const response = await page.request.post(`/api/v1${path}`, {
      headers: { "x-csrf-protection": "1", ...headers },
      data,
    });
    expect(response.ok()).toBe(true);
    return (await response.json()) as { id: string; key: string; slug: string; version: number };
  };
  for (let i = 1; i <= 40; i++) {
    await post(`${heapPath}/items`, { title: `Item ${String(i).padStart(2, "0")}`, need: "expected" });
  }
  // A note at the bottom of the list, and a task made from it at the top.
  const note = await post(`${heapPath}/items`, { title: "The note far down", kind: "note", need: "idea" });
  const task = await post(`${heapPath}/items`, {
    title: "Made from the note",
    kind: "task",
    need: "critical",
    origin: { item_id: note.id, age: "new" },
  });
  await page.goto(heapPath);
  const rowLink = (title: string) => row(page, title).locator("[data-row-link]");
  const scrollY = () => page.evaluate(() => window.scrollY);

  await openItem(page, "Made from the note");
  await expect(rowLink("The note far down")).not.toBeInViewport();

  // From: opens the note over the list, its row brought into view and selected.
  await itemPane(page)
    .getByRole("link", { name: /The note far down/ })
    .click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("The note far down");
  await expect(rowLink("The note far down")).toHaveAttribute("aria-current", "page");
  await expect(rowLink("The note far down")).toBeInViewport();
  await expect.poll(scrollY).toBeGreaterThan(200);

  // Became: back to the task, at the top of the list.
  await itemPane(page).getByRole("link", { name: "Made from the note" }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Made from the note");
  await expect(rowLink("Made from the note")).toHaveAttribute("aria-current", "page");
  await expect(rowLink("Made from the note")).toBeInViewport();
  await expect(rowLink("The note far down")).not.toHaveAttribute("aria-current");

  // A target that isn't a row on screen, as the note once closed, leaves the list where it is.
  const current = await page.request.get(`/api/v1${heapPath}/items/${note.key}`);
  const { version } = (await current.json()) as { version: number };
  const closed = await page.request.patch(`/api/v1${heapPath}/items/${note.key}`, {
    headers: { "x-csrf-protection": "1", "if-match": `"${version}"` },
    data: { state: "done" },
  });
  expect(closed.ok()).toBe(true);
  await page.goto(`${heapPath}/items/${task.key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Made from the note");
  await rowLink("Item 30").evaluate((link) => link.scrollIntoView({ block: "center" }));
  await expect.poll(scrollY).toBeGreaterThan(200);
  const kept = await scrollY();
  await itemPane(page)
    .getByRole("link", { name: /The note far down/ })
    .click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("The note far down");
  await expect.poll(scrollY).toBe(kept);

  // Moved to another heap, the task's link leads to that heap's list, which starts at the top.
  const team = heapPath.split("/")[2];
  const other = await post(`/teams/${team}/heaps`, { name: "Elsewhere" });
  await post(`${heapPath}/items/${task.key}/move`, { heap_id: other.id }, { "if-match": `"${task.version}"` });
  await page.goto(`${heapPath}/items/${note.key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("The note far down");
  await rowLink("Item 30").evaluate((link) => link.scrollIntoView({ block: "center" }));
  await expect.poll(scrollY).toBeGreaterThan(200);
  await itemPane(page).getByRole("link", { name: "Made from the note" }).click();
  await expect(page).toHaveURL(new RegExp(`/heaps/${other.slug}/items/${task.key}`));
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Made from the note");
  await expect.poll(scrollY).toBe(0);
});

/** Makes a task through the API with these steps, in order, and opens it. Returns the Steps region and the item's URL. */
async function openTaskWithSteps(page: Page, heapPath: string, titles: string[]) {
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
  for (const title of titles) {
    const step = await page.request.post(`${item}/steps`, { headers, data: { title } });
    expect(step.status()).toBe(201);
  }
  await page.goto(`${heapPath}/items/${key}`);
  const steps = itemPane(page).getByRole("region", { name: "Steps" });
  await expect(steps.getByRole("textbox", { name: "Step title" })).toHaveCount(titles.length);
  return { steps, item };
}

async function expectStepOrder(steps: Locator, titles: string[]) {
  const fields = steps.getByRole("textbox", { name: "Step title" });
  await expect(fields).toHaveCount(titles.length);
  for (const [index, title] of titles.entries()) await expect(fields.nth(index)).toHaveValue(title);
}

test("a step is dragged by its grip to a new place, and the server keeps it", async ({ page, heapPath }) => {
  const { steps } = await openTaskWithSteps(page, heapPath, ["One", "Two", "Three"]);
  const grips = steps.locator("[data-step-grip]");
  if (test.info().project.name === "phone") {
    // Always shown on a phone, not only on hover.
    await expect(grips.first()).toHaveCSS("opacity", "1");
    const box = await grips.first().boundingBox();
    expect(Math.round(box?.height ?? 0)).toBeGreaterThanOrEqual(44);
  }
  const fields = steps.getByRole("textbox", { name: "Step title" });
  await grips.nth(2).hover();
  const target = await fields.nth(0).boundingBox();
  await page.mouse.down();
  const start = await grips.nth(2).boundingBox();
  const fromY = (start?.y ?? 0) + (start?.height ?? 0) / 2;
  const toY = (target?.y ?? 0) + 2;
  for (let i = 1; i <= 6; i++) await page.mouse.move((start?.x ?? 0) + 4, fromY + ((toY - fromY) * i) / 6);
  const saved = page.waitForResponse((r) => r.request().method() === "PATCH" && /\/steps\/3$/.test(r.url()));
  await page.mouse.up();
  await saved;
  await expectStepOrder(steps, ["Three", "One", "Two"]);
  await page.reload();
  await expectStepOrder(steps, ["Three", "One", "Two"]);
});

test("Escape puts a dragged step back and saves nothing", async ({ page, heapPath }) => {
  const { steps } = await openTaskWithSteps(page, heapPath, ["One", "Two", "Three"]);
  const grips = steps.locator("[data-step-grip]");
  await grips.nth(0).hover();
  const last = await steps.getByRole("textbox", { name: "Step title" }).nth(2).boundingBox();
  await page.mouse.down();
  const start = await grips.nth(0).boundingBox();
  const fromY = (start?.y ?? 0) + (start?.height ?? 0) / 2;
  const toY = (last?.y ?? 0) + (last?.height ?? 0) + 4;
  for (let i = 1; i <= 6; i++) await page.mouse.move((start?.x ?? 0) + 4, fromY + ((toY - fromY) * i) / 6);
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(itemPane(page)).toBeVisible();
  await expectStepOrder(steps, ["One", "Two", "Three"]);
  await page.reload();
  await expectStepOrder(steps, ["One", "Two", "Three"]);
});

test("Move up and Move down in a step's menu move it, and focus stays on its menu button", async ({
  page,
  heapPath,
}) => {
  const { steps } = await openTaskWithSteps(page, heapPath, ["One", "Two", "Three"]);
  const actions = steps.getByRole("button", { name: "Step actions" });
  await actions.nth(0).click();
  await expect(page.getByRole("menuitem", { name: "Move down" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Move up" })).toHaveCount(0);
  const saved = page.waitForResponse((r) => r.request().method() === "PATCH" && /\/steps\/1$/.test(r.url()));
  await page.getByRole("menuitem", { name: "Move down" }).click();
  await saved;
  await expectStepOrder(steps, ["Two", "One", "Three"]);
  await expect(steps.getByRole("status")).toHaveText("Moved to place 2 of 3");
  await expect(actions.nth(1)).toBeFocused();

  await actions.nth(2).click();
  await expect(page.getByRole("menuitem", { name: "Move up" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Move down" })).toHaveCount(0);
});

test("a move the server refuses goes back, and the reason shows under the steps", async ({ page, heapPath }) => {
  const { steps, item } = await openTaskWithSteps(page, heapPath, ["One", "Two", "Three"]);
  // Someone else edits "Two" after the page loaded, so the page's version of it is stale.
  const current = await page.request.get(item);
  const two = (await current.json()).steps.find((step: { number: number }) => step.number === 2);
  const edited = await page.request.patch(`${item}/steps/2`, {
    headers: { "x-csrf-protection": "1", "If-Match": `"${two.version}"` },
    data: { context: "Changed meanwhile" },
  });
  expect(edited.status()).toBe(200);

  await steps.getByRole("button", { name: "Step actions" }).nth(1).click();
  await page.getByRole("menuitem", { name: "Move up" }).click();
  await expect(steps.getByRole("alert")).toHaveText(
    "Someone else changed this item meanwhile. It now shows their version.",
  );
  await expectStepOrder(steps, ["One", "Two", "Three"]);
});
