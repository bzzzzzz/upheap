import { addItem, goTo, dayFromToday, expect, expectOrder, row, test } from "./fixtures";

test("added items are ranked, and an edit moves an item", async ({ page }) => {
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });

  // More value for less effort ranks higher, whatever the order they were added in.
  await expectOrder(page, ["Quick win", "Slog"]);

  // An exact deadline that has passed puts the slog first.
  await row(page, "Slog").getByLabel("Due").fill(dayFromToday(-1));
  // The box shows the saved value, so it ticks once the save is back.
  await row(page, "Slog").getByLabel("Exact").click();
  await expect(row(page, "Slog").getByLabel("Exact")).toBeChecked();
  await expect(row(page, "Slog").getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  await expectOrder(page, ["Slog", "Quick win"]);
});

test("an item is closed from the list and reopened from Closed", async ({ page }) => {
  await addItem(page, "Write release notes");

  await row(page, "Write release notes").getByLabel("State").selectOption("Done");
  await expect(row(page, "Write release notes")).toHaveCount(0);

  await goTo(page, "Closed");
  const closed = page.getByRole("listitem").filter({ hasText: "Write release notes" });
  await expect(closed).toContainText("Done ·");
  await closed.getByRole("button", { name: "Reopen" }).click();
  await expect(page.getByText("Nothing closed yet.")).toBeVisible();

  await goTo(page, "Open");
  await expect(row(page, "Write release notes").getByLabel("State")).toHaveValue("open");
});

test("an item is blocked by another and unblocked", async ({ page }) => {
  await addItem(page, "Deploy", { effort: "S", impact: "XL" });
  await addItem(page, "Get approval");

  // Blocked items are hidden by default.
  await row(page, "Deploy").getByLabel("State").selectOption("Blocked");
  await expect(row(page, "Deploy")).toHaveCount(0);
  await expect(page.getByText("1 item is hidden by the view settings.")).toBeVisible();
  await page.getByRole("button", { name: "Change view" }).click();
  await page.getByRole("radiogroup", { name: "Blocked" }).getByText("Shown", { exact: true }).click();

  await expect(row(page, "Deploy")).toBeVisible();
  await row(page, "Deploy").getByLabel("Waiting for").selectOption({ label: "Get approval" });
  await expect(row(page, "Deploy").getByLabel("Waiting for")).toHaveValue(/.+/);
  // The blocker takes on the priority of what waits for it, and says so.
  await expect(row(page, "Get approval")).toContainText("Blocks “Deploy”");

  // The blocker isn't offered the item it blocks to wait for, which would be a cycle.
  await row(page, "Get approval").getByLabel("State").selectOption("Blocked");
  await expect(row(page, "Get approval").getByLabel("Waiting for")).toBeVisible();
  await expect(row(page, "Get approval").getByLabel("Waiting for").getByRole("option")).toHaveText(["Nothing picked"]);
  await row(page, "Get approval").getByLabel("State").selectOption("Open");

  await row(page, "Deploy").getByLabel("State").selectOption("Open");
  await expect(row(page, "Deploy").getByLabel("Waiting for")).toHaveCount(0);
  await expect(row(page, "Get approval")).not.toContainText("Blocks “Deploy”");
});

test("the Add dialog adds items in a row, with rough or exact due times", async ({ page }) => {
  await page.keyboard.press("n");
  const dialog = page.getByRole("dialog", { name: "Add an item" });
  const due = dialog.getByRole("radiogroup", { name: "Due" });

  // A rough choice reads back with the label it was picked by, which comes from the server.
  await dialog.getByLabel("Title").fill("Book the dentist");
  await due.getByRole("radio", { name: "next week" }).click();
  await dialog.getByLabel("Add another").check();
  await dialog.getByLabel("Title").press("Enter");
  await expect(row(page, "Book the dentist").getByText("next week", { exact: true })).toBeVisible();

  // With Add another, the dialog stays open and clears.
  await expect(dialog.getByLabel("Title")).toHaveValue("");
  await expect(due.getByRole("radio", { name: "next week" })).not.toBeChecked();
  await dialog.getByLabel("Title").fill("Renew the passport");
  await due.getByRole("radio", { name: "Exact date" }).click();
  await dialog.getByLabel("Exact due date").fill(dayFromToday(-1));
  await dialog.getByLabel("Title").press("ControlOrMeta+Enter");
  await expect(row(page, "Renew the passport").getByText("Overdue · yesterday", { exact: true })).toBeVisible();

  // Closing with a title typed asks first.
  await dialog.getByLabel("Title").fill("A stray thought");
  await dialog.getByLabel("Title").press("Escape");
  await dialog.getByRole("button", { name: "Discard" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "A stray thought")).toHaveCount(0);
});
