import {
  addItem,
  closeItem,
  dayFromToday,
  expect,
  expectOrder,
  goTo,
  itemPane,
  openItem,
  pick,
  row,
  test,
} from "./fixtures";

test("added items are ranked, and an edit moves an item", async ({ page }) => {
  await addItem(page, "Slog", { effort: "XL", impact: "S" });
  await addItem(page, "Quick win", { effort: "S", impact: "XL" });

  // More value for less effort ranks higher, whatever the order they were added in.
  await expectOrder(page, ["Quick win", "Slog"]);

  // An exact deadline that has passed puts the slog first.
  await openItem(page, "Slog");
  await pick(page, "Due", "Exact date");
  await itemPane(page).getByLabel("Exact due date").fill(dayFromToday(-1));
  await expect(itemPane(page).getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  await closeItem(page);
  await expect(row(page, "Slog").getByText("Overdue · yesterday", { exact: true })).toBeVisible();
  await expectOrder(page, ["Slog", "Quick win"]);
});

test("the item opens over the list, saves each change, and Back closes it", async ({ page, heapPath }) => {
  await addItem(page, "Write the rota");
  await openItem(page, "Write the rota");
  const pane = itemPane(page);

  // Each change saves as it's made, one after the other.
  await pick(page, "Effort", "S");
  await pick(page, "Impact", "XL");
  await pane.getByLabel("Title", { exact: true }).fill("Write the team rota");
  await pane.getByLabel("Title", { exact: true }).press("Enter");
  await pane.getByLabel("Description").fill("For the next quarter");
  await pick(page, "State", "In progress");
  await expect(pane.getByRole("radio", { name: "In progress", exact: true })).toBeChecked();
  await expect(row(page, "Write the team rota")).toBeVisible();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  await expect(pane.getByRole("region", { name: "History" })).toContainText("In progress");
  // The user's own changes read as theirs.
  await expect(pane.getByRole("region", { name: "History" })).toContainText("You added item");

  // Back closes the item and leaves the list where it was.
  await page.goBack();
  await expect(pane).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));

  // Opened from a link, it closes to its list.
  await openItem(page, "Write the team rota");
  await page.reload();
  await expect(pane.getByLabel("Description")).toHaveValue("For the next quarter");
  await closeItem(page);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  await expect(row(page, "Write the team rota")).toBeVisible();
});

test("an item is closed from the list, undone, and reopened from Closed", async ({ page }) => {
  await addItem(page, "Write release notes");

  await row(page, "Write release notes").getByRole("button", { name: "Mark Write release notes done" }).click();
  await expect(row(page, "Write release notes")).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "“Write release notes” is done." });
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(row(page, "Write release notes")).toBeVisible();

  await row(page, "Write release notes").getByRole("button", { name: "Mark Write release notes done" }).click();
  await expect(row(page, "Write release notes")).toHaveCount(0);
  await goTo(page, "Closed");
  const closed = page.getByRole("listitem").filter({ hasText: "Write release notes" });
  await expect(closed).toContainText("Done ·");
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

  // Blocked items are hidden by default.
  await openItem(page, "Deploy");
  await pick(page, "State", "Blocked");
  await expect(row(page, "Deploy")).toHaveCount(0);
  await closeItem(page);
  await expect(page.getByText("1 item is hidden by the view settings.")).toBeVisible();
  await page.getByRole("button", { name: "Change view" }).click();
  const viewSettings = page.getByRole("dialog", { name: "View settings" });
  await viewSettings.getByRole("radiogroup", { name: "Blocked" }).getByText("Shown", { exact: true }).click();
  await viewSettings.getByRole("button", { name: "Close" }).click();
  await expect(viewSettings).toHaveCount(0);

  await openItem(page, "Deploy");
  await itemPane(page).getByLabel("Waiting for").selectOption({ label: "Get approval" });
  await expect(itemPane(page).getByLabel("Waiting for")).toHaveValue(/.+/);
  await closeItem(page);
  // The blocker takes on the priority of what waits for it, and says so.
  await expect(row(page, "Get approval")).toContainText("Blocks “Deploy”");

  // The blocker isn't offered the item it blocks to wait for, which would be a cycle.
  await openItem(page, "Get approval");
  await pick(page, "State", "Blocked");
  await expect(itemPane(page).getByLabel("Waiting for")).toBeVisible();
  await expect(itemPane(page).getByLabel("Waiting for").getByRole("option")).toHaveText(["Nothing picked"]);
  await pick(page, "State", "Open");
  await closeItem(page);

  await openItem(page, "Deploy");
  await pick(page, "State", "Open");
  await expect(itemPane(page).getByLabel("Waiting for")).toHaveCount(0);
  await closeItem(page);
  await expect(row(page, "Get approval")).not.toContainText("Blocks “Deploy”");
});

test("the Add dialog adds items in a row, with rough or exact due times", async ({ page }) => {
  // `N` works once the list is there.
  await expect(page.getByRole("button", { name: "Add an item" })).toBeEnabled();
  await page.keyboard.press("n");
  const dialog = page.getByRole("dialog", { name: "Add an item" });
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

  // Closing with a title typed asks first.
  await dialog.getByLabel("Title").fill("A stray thought");
  await dialog.getByLabel("Title").press("Escape");
  await dialog.getByRole("button", { name: "Discard" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, "A stray thought")).toHaveCount(0);
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
