import { addItem, closeItem, expect, itemPane, makeTask, moreEntry, openItem, openMore, row, test } from "./fixtures";
import type { Locator, Page } from "@playwright/test";

/** Opens ⋯ → Split… on the open note and returns the dialog. */
async function openSplit(page: Page, title: string) {
  await moreEntry(page, await openMore(page), "Split…").click();
  const dialog = page.getByRole("dialog", { name: `Split “${title}”` });
  await expect(dialog).toBeVisible();
  return dialog;
}

/** Fills the row `number` of the dialog: its title, a need and an age. */
async function fillRow(dialog: Locator, number: number, title: string, need: string, age: string) {
  const fields = dialog.getByRole("group", { name: `Row ${number}` });
  await fields.getByLabel("Title", { exact: true }).fill(title);
  await fields.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: need, exact: true }).click();
  await fields.getByRole("radiogroup", { name: "Age" }).getByRole("radio", { name: age, exact: true }).click();
  return fields;
}

test("a note splits into tasks", async ({ page }) => {
  await addItem(page, "Plan the launch", { need: "Expected" });
  await openItem(page, "Plan the launch");
  const dialog = await openSplit(page, "Plan the launch");

  const first = dialog.getByRole("group", { name: "Row 1" });
  await expect(first.getByRole("radiogroup", { name: "Need" }).getByRole("radio", { name: "Critical" })).toBeDisabled();
  await fillRow(dialog, 1, "Book the venue", "Expected", "Keep the note's age");
  await fillRow(dialog, 2, "Send the invites", "Idea", "Start new");

  await dialog.getByRole("button", { name: "Make 2 tasks" }).click();

  await expect(page.getByText("“Plan the launch” became 2 tasks.")).toBeVisible();
  const became = itemPane(page).getByRole("region", { name: "Became" });
  await expect(became).toContainText("2 tasks");
  await expect(became.getByRole("link", { name: "Book the venue" })).toBeVisible();
  await expect(became.getByRole("link", { name: "Send the invites" })).toBeVisible();
  await expect(
    itemPane(page).getByRole("group", { name: "State", exact: true }).getByRole("button", { name: "Reopen" }),
  ).toBeFocused();
});

test("a note splits into a step of a task", async ({ page }) => {
  await addItem(page, "Write the report");
  await openItem(page, "Write the report");
  await makeTask(page, "Write the report");
  await closeItem(page);
  await addItem(page, "Check the figures");
  await openItem(page, "Check the figures");
  const dialog = await openSplit(page, "Check the figures");

  await dialog.getByRole("radio", { name: "A step", exact: true }).click();
  await dialog.getByLabel("Filter by title or key").fill("report");
  await dialog.getByRole("option", { name: /Write the report/ }).click();
  await dialog.getByRole("button", { name: "Make it a step" }).click();

  await expect(page.getByText("“Check the figures” became a step.")).toBeVisible();
  const became = itemPane(page).getByRole("region", { name: "Became" });
  await expect(became).toContainText("a step");
  await became.getByRole("link", { name: /Write the report/ }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Write the report");
  await expect(itemPane(page).getByRole("region", { name: "Steps" })).toContainText("Check the figures");
});

test("a note splits into a heap", async ({ page }) => {
  await addItem(page, "Start a side project");
  await openItem(page, "Start a side project");
  const dialog = await openSplit(page, "Start a side project");

  await dialog.getByRole("radio", { name: "A heap", exact: true }).click();
  const name = dialog.getByLabel("Name", { exact: true });
  await expect(name).toHaveValue("Start a side project");
  await name.fill("Side project");
  const fields = await fillRow(dialog, 1, "Think about names", "Idea", "Start new");
  await fields.getByRole("radiogroup", { name: "Kind" }).getByRole("radio", { name: "Note", exact: true }).click();

  await dialog.getByRole("button", { name: "Make the heap and 1 item" }).click();

  await expect(page.getByText("“Start a side project” became a heap.")).toBeVisible();
  const became = itemPane(page).getByRole("region", { name: "Became" });
  await became.getByRole("link", { name: "“Side project”" }).click();
  await expect(row(page, "Think about names")).toBeVisible();
});
