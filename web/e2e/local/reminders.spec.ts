// Reminders in the local build (design/ux.md, The list): one for a later date stays out of the list, and one for
// today is listed and can be made a task.
import { addDialog, expect, itemPane, test } from "../fixtures";

test("a reminder joins the list on its date and can be made a task", async ({ page }) => {
  const reminders: [string, string][] = [
    ["Renew the domain", "2099-03-12"],
    ["Pay the rent", new Date().toISOString().slice(0, 10)],
  ];
  for (const [title, date] of reminders) {
    await page.getByRole("button", { name: "Add item", exact: true }).click();
    const dialog = addDialog(page);
    await dialog
      .getByRole("radiogroup", { name: "Kind" })
      .getByRole("radio", { name: "Reminder", exact: true })
      .click();
    await dialog.getByLabel("Title").fill(title);
    await dialog
      .getByRole("radiogroup", { name: "Need" })
      .getByRole("radio", { name: "Expected", exact: true })
      .click();
    await dialog.getByLabel("Date", { exact: true }).fill(date);
    await dialog.getByLabel("Title").press("Enter");
    await expect(dialog).toHaveCount(0);
  }
  await expect(page.getByRole("status").filter({ hasText: "Added “Pay the rent” at #1." })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Renew the domain/ })).toHaveCount(0);

  await page.getByRole("link", { name: /^Pay the rent ?, reminder$/ }).click();
  const pane = itemPane(page);
  await pane.getByRole("group", { name: "State", exact: true }).getByRole("button", { name: "Make it a task" }).click();
  await page
    .getByRole("alertdialog", { name: "Make “Pay the rent” a task?" })
    .getByRole("button", { name: "Make it a task" })
    .click();
  await expect(pane.getByRole("region", { name: "Steps" })).toBeVisible();
});
