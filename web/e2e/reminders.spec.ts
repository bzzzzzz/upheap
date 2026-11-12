// Reminders (design/ux.md: The list, The item, Adding an item), at each width: added with a date, out of the list
// until it, found by Filters › Show and by search, and made a task once it has arrived.
import { addDialog, closeField, expect, field, isPhone, itemPane, test, type Need } from "./fixtures";
import type { Page } from "@playwright/test";

/** Today's date in the tests' time zone (UTC), as the Add dialog's date field takes it. */
const today = () => new Date().toISOString().slice(0, 10);

/** A reminder's row: its link is named for the title and the reminder mark a screen reader hears. */
const reminderRow = (page: Page, title: string) =>
  page.getByRole("listitem").filter({ has: page.getByRole("link", { name: new RegExp(`^${title} ?, reminder$`) }) });

/** Adds a reminder through the Add dialog: the Reminder choice, a title, a need and a date. */
async function addReminder(page: Page, title: string, date: string | null, need: Need = "Expected") {
  await page.getByRole("button", { name: "Add item", exact: true }).click();
  const dialog = addDialog(page);
  await dialog.getByRole("radiogroup", { name: "Kind" }).getByRole("radio", { name: "Reminder", exact: true }).click();
  await dialog.getByLabel("Title").fill(title);
  await dialog.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: need, exact: true }).click();
  if (date) {
    await dialog.getByLabel("Date", { exact: true }).fill(date);
    await dialog.getByLabel("Title").press("Enter");
    await expect(dialog).toHaveCount(0);
  }
}

/** Sets Filters › Show › Reminders to come, and closes the panel. */
async function showReminders(page: Page, mode: "Shown" | "Hidden") {
  await page.getByRole("button", { name: /^Filters/ }).click();
  const panel = page.getByRole("dialog", { name: isPhone(page) ? "Filters & sort" : "Filters" });
  await panel.getByRole("radiogroup", { name: "Reminders to come" }).getByText(mode, { exact: true }).click();
  await panel.getByRole("button", { name: isPhone(page) ? /^Show / : "Done" }).click();
  await expect(panel).toHaveCount(0);
}

test("a reminder for a later date says when it joins the list, and stays out of it until then", async ({ page }) => {
  await page.getByRole("button", { name: "Add item", exact: true }).click();
  const dialog = addDialog(page);
  await dialog.getByRole("radiogroup", { name: "Kind" }).getByRole("radio", { name: "Reminder", exact: true }).click();
  await dialog.getByLabel("Title").fill("Renew the domain");
  await dialog.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: "Expected", exact: true }).click();

  // A reminder's date is its deadline: no Due field, and Add waits for the date.
  await expect(dialog.getByLabel("Date", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Due", { exact: true })).toHaveCount(0);
  await expect(dialog.getByRole("button", { name: "Add", exact: true })).toBeDisabled();
  await dialog.getByLabel("Date", { exact: true }).fill("2099-03-12");
  await expect(dialog.getByRole("button", { name: "Add", exact: true })).toBeEnabled();

  // Back to Note keeps the rest and brings the Due field back.
  await dialog.getByRole("radiogroup", { name: "Kind" }).getByRole("radio", { name: "Note", exact: true }).click();
  await expect(dialog.getByLabel("Date", { exact: true })).toHaveCount(0);
  await expect(dialog.getByLabel("Title")).toHaveValue("Renew the domain");
  await dialog.getByRole("radiogroup", { name: "Kind" }).getByRole("radio", { name: "Reminder", exact: true }).click();
  await expect(dialog.getByLabel("Date", { exact: true })).toHaveValue("2099-03-12");

  await dialog.getByLabel("Title").press("Enter");
  await expect(dialog).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "Added. It joins the list on 12 March 2099." });
  await expect(toast).toBeVisible();

  // It isn't in the list, and nothing says how many are hidden.
  await expect(reminderRow(page, "Renew the domain")).toHaveCount(0);
  await expect(page.getByText("hidden", { exact: false })).toHaveCount(0);

  // Opened from the toast, it shows its date and when it joins, and has no Send to an agent.
  await toast.getByRole("button", { name: "Open it" }).click();
  const pane = itemPane(page);
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Renew the domain");
  await expect(pane).toContainText("Reminder · 12 March 2099");
  await expect(pane).toContainText("Joins the list on 12 March 2099");
  await expect(pane.getByRole("button", { name: "Send to an agent" })).toHaveCount(0);

  // The date moves where due is edited, and can't be cleared.
  const dateField = await field(page, "Date");
  await dateField.getByLabel("Date", { exact: true }).fill("2099-04-02");
  await expect(pane).toContainText("Joins the list on 2 April 2099");
  await expect(dateField.getByRole("button", { name: /clear/i })).toHaveCount(0);
  await closeField(page);
});

test("Filters › Show lists the reminders to come in a group of their own, soonest first", async ({ page }) => {
  await addReminder(page, "Later one", "2099-06-01");
  await addReminder(page, "Sooner one", "2099-02-01");
  await expect(reminderRow(page, "Sooner one")).toHaveCount(0);

  await showReminders(page, "Shown");
  const group = page.getByRole("region", { name: "Reminders to come" });
  await expect(group.getByRole("listitem")).toHaveCount(2);
  await expect(group.getByRole("listitem").nth(0)).toContainText("Sooner one");
  await expect(group.getByRole("listitem").nth(0)).toContainText("1 February 2099");
  await expect(group.getByRole("listitem").nth(1)).toContainText("Later one");
  // No position on a reminder to come.
  await expect(group.getByLabel(/^Position/)).toHaveCount(0);

  // The item pane's ↑ ↓ walk the group too (a phone's sheet has none).
  if (!isPhone(page)) {
    await group.getByRole("link", { name: /^Sooner one/ }).click();
    const pane = itemPane(page);
    await pane.getByRole("link", { name: "Next item" }).click();
    await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Later one");
    await expect(pane.getByRole("link", { name: "Next item" })).toHaveAttribute("aria-disabled", "true");
    await expect(pane.getByRole("link", { name: "Previous item" })).not.toHaveAttribute("aria-disabled", "true");
    await pane.getByRole("button", { name: "Close" }).click();
  }

  // The setting is a chip, and taking it off hides the group.
  await page
    .getByRole("group", { name: "Filters on" })
    .getByRole("button", { name: "Remove view setting Reminders to come shown" })
    .click();
  await expect(page.getByRole("region", { name: "Reminders to come" })).toHaveCount(0);
});

test("search finds a reminder to come while the setting is Hidden", async ({ page }) => {
  await addReminder(page, "Renew the domain", "2099-03-12");
  await expect(page.getByRole("region", { name: "Reminders to come" })).toHaveCount(0);

  await page.getByLabel("Search items").fill("domain");
  const group = page.getByRole("region", { name: "Reminders to come" });
  await expect(reminderRow(page, "Renew the domain")).toBeVisible();
  await expect(group).toContainText("12 March 2099");

  // Text that matches nothing leaves no group; clearing the search puts the list back as it was.
  await page.getByLabel("Search items").fill("nothing like it");
  await expect(group).toHaveCount(0);
  await page.getByLabel("Search items").fill("");
  await expect(reminderRow(page, "Renew the domain")).toHaveCount(0);
});

test("a reminder for today is in the list, and Make it a task is its main action", async ({ page }) => {
  await addReminder(page, "Pay the rent", today());
  // The usual toast: it landed in the list.
  await expect(page.getByRole("status").filter({ hasText: "Added “Pay the rent” at #1." })).toBeVisible();
  await expect(reminderRow(page, "Pay the rent")).toBeVisible();

  await reminderRow(page, "Pay the rent").getByRole("link").click();
  const pane = itemPane(page);
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Pay the rent");
  await expect(pane).not.toContainText("Joins the list");

  // A button of its own in the state buttons, with the question that asks first.
  const state = pane.getByRole("group", { name: "State", exact: true });
  await state.getByRole("button", { name: "Make it a task", exact: true }).click();
  const confirm = page.getByRole("alertdialog", { name: "Make “Pay the rent” a task?" });
  await expect(confirm).toContainText("A task can't go back to a reminder.");
  await confirm.getByRole("button", { name: "Make it a task" }).click();

  // It is a task in the same place, with steps to add.
  await expect(pane.getByRole("region", { name: "Steps" })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Pay the rent ?, task$/ })).toBeVisible();
  await expect(reminderRow(page, "Pay the rent")).toHaveCount(0);
});
