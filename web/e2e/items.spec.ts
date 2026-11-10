import {
  addDialog,
  addItem,
  changeState,
  closeField,
  closeItem,
  dayFromToday,
  expect,
  expectOrder,
  field,
  goTo,
  isPhone,
  itemPane,
  makeTask,
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
  // The link sign beside the title copies the item's short link.
  await expect(pane.getByRole("button", { name: /^Copy the link to [0-9a-z]{5}$/ })).toBeVisible();

  // Each change saves as it's made, one after the other.
  await pick(page, "Effort", "S");
  await pick(page, "Impact", "XL");
  await pane.getByLabel("Title", { exact: true }).fill("Write the team rota");
  await pane.getByLabel("Title", { exact: true }).press("Enter");
  await (await field(page, "Context")).getByLabel("Context").fill("For the next quarter");
  await closeField(page);
  // The likeliest next state is the first button: Start, for an open item, then Done once it's started.
  await changeState(page, "Start");
  await expect(pane.getByRole("group", { name: "State" }).getByRole("button").first()).toHaveText("Done");
  await expect(row(page, "Write the team rota")).toBeVisible();
  await expect(page.getByRole("heading", { name: "In progress" })).toBeVisible();
  await expect(pane.getByRole("region", { name: "History" })).toContainText("In progress");
  // The user's own changes read as theirs.
  await expect(pane.getByRole("region", { name: "History" })).toContainText("You added item");
  await expect(pane.getByRole("region", { name: "History" })).toContainText("Added by you today");

  // Done leaves the item open in the pane, which offers to reopen it.
  await changeState(page, "Done");
  await expect(row(page, "Write the team rota")).toHaveCount(0);
  await changeState(page, "Reopen");
  await expect(pane.getByRole("group", { name: "State" }).getByRole("button").first()).toHaveText("Start");
  await expect(row(page, "Write the team rota")).toBeVisible();

  // Back closes the item and leaves the list where it was.
  await page.goBack();
  await expect(pane).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));

  // Opened from a link, it closes to its list.
  await openItem(page, "Write the team rota");
  await page.reload();
  await expect((await field(page, "Context")).getByLabel("Context")).toHaveValue("For the next quarter");
  await closeField(page);
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

  // Blocked items are hidden by default.
  await openItem(page, "Deploy");
  await changeState(page, "Block");
  await expect(row(page, "Deploy")).toHaveCount(0);
  await closeItem(page);
  // The note under the list says what's hidden, and brings it back.
  await expect(page.getByText("1 blocked item is hidden.")).toBeVisible();
  await page.getByRole("button", { name: "Show it" }).click();
  await expect(row(page, "Deploy")).toBeVisible();

  await openItem(page, "Deploy");
  await itemPane(page).getByLabel("Waiting for").selectOption({ label: "Get approval" });
  await expect(itemPane(page).getByLabel("Waiting for")).toHaveValue(/.+/);
  await closeItem(page);
  // The blocker takes on the priority of what waits for it, and says so.
  await expect(row(page, "Get approval")).toContainText("Blocks “Deploy”");

  // The blocker isn't offered the item it blocks to wait for, which would be a cycle.
  await openItem(page, "Get approval");
  await changeState(page, "Block");
  await expect(itemPane(page).getByLabel("Waiting for")).toBeVisible();
  await expect(itemPane(page).getByLabel("Waiting for").getByRole("option")).toHaveText(["Nothing picked"]);
  await changeState(page, "Unblock");
  await closeItem(page);

  await openItem(page, "Deploy");
  await changeState(page, "Unblock");
  await expect(itemPane(page).getByLabel("Waiting for")).toHaveCount(0);
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
  await row(page, "Slog").getByRole("button", { name: "Mark Slog done" }).click();
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

test("on a phone, the item's fields open sheets, and the state buttons sit at the bottom", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) >= 640, "Only a phone shows the fields as rows.");
  await addItem(page, "Plan the offsite");
  await openItem(page, "Plan the offsite");
  const pane = itemPane(page);

  // The toast from adding it stays under the sheet, which keeps its bar of states usable.
  await expect(page.getByRole("status").filter({ hasText: "Added “Plan the offsite”" })).toBeVisible();
  await pane.getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "More", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "More", exact: true })).toHaveCount(0);

  // The context saves however its sheet closes: here with Escape, then with a tap on the dimmed page.
  await (await field(page, "Context")).getByLabel("Context").fill("Venue first");
  await page.keyboard.press("Escape");
  await expect(pane.getByRole("button", { name: "Context Venue first" })).toBeVisible();
  await (await field(page, "Context")).getByLabel("Context").fill("Venue, then dates");
  await page.mouse.click(200, 20);
  await expect(pane.getByRole("button", { name: "Context Venue, then dates" })).toBeVisible();

  // A new tag from the Tags sheet, its offer in view under the field.
  const tags = await field(page, "Tags");
  await tags.getByLabel("Add a tag").fill("travel");
  await tags.getByRole("option", { name: "Create tag “travel”" }).click();
  await closeField(page);
  await expect(pane.getByRole("button", { name: "Tags travel" })).toBeVisible();

  // Won't do is under ⋯ at the end of the bar; a closed item shows all its fields, unchangeable.
  await pane.getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true }).click();
  await page.getByRole("dialog", { name: "More", exact: true }).getByRole("button", { name: "Won't do" }).click();
  await expect(pane.getByText("Venue, then dates", { exact: true })).toBeVisible();
  await expect(pane.getByRole("button", { name: /^Context/ })).toHaveCount(0);
  await expect(pane.getByRole("group", { name: "State" }).getByRole("button")).toHaveText(["Reopen"]);
});

test("a note is made a task and its stage is set from the item", async ({ page, localBuild }) => {
  test.skip(localBuild, "The static build has no stage; kind.spec.ts in e2e/local covers it.");
  await addItem(page, "Rotate the certs");
  await openItem(page, "Rotate the certs");
  const pane = itemPane(page);
  const phone = isPhone(page);
  await expect(pane.getByText("Note · As written", { exact: true })).toBeVisible();

  // Making a note a task asks first, naming it: a task can't go back.
  const more = pane.getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true });
  await more.click();
  if (phone)
    await page
      .getByRole("dialog", { name: "More", exact: true })
      .getByRole("button", { name: "Make it a task…" })
      .click();
  else await page.getByRole("menu", { name: "More" }).getByRole("menuitem", { name: "Make it a task…" }).click();
  const confirm = page.getByRole("alertdialog", { name: "Make “Rotate the certs” a task?" });
  await expect(confirm).toContainText("A task can't go back to a note.");
  await confirm.getByRole("button", { name: "Make it a task" }).click();
  await expect(pane.getByText("Task · As written", { exact: true })).toBeVisible();

  // The stage saves at once. A task can be defined, and offers no second Make it a task.
  await more.click();
  if (phone) {
    const sheet = page.getByRole("dialog", { name: "More", exact: true });
    await expect(sheet.getByRole("button", { name: "Make it a task…" })).toHaveCount(0);
    await sheet.getByRole("radio", { name: "With context" }).click();
  } else {
    const menu = page.getByRole("menu", { name: "More" });
    await expect(menu.getByRole("menuitem", { name: "Make it a task…" })).toHaveCount(0);
    await menu.getByRole("menuitemradio", { name: "With context" }).click();
  }
  await expect(pane.getByText("Task · With context", { exact: true })).toBeVisible();

  await closeItem(page);
  await expect(page.getByRole("link", { name: /^Rotate the certs\s?, task, with context$/ })).toBeVisible();
});

test("a task is worked through its steps from the item", async ({ page, localBuild }) => {
  test.skip(localBuild, "The static build's steps are covered by e2e/local/steps.spec.ts.");
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

test("the item says what holds it back, and each entry takes you to its field", async ({ page }) => {
  await addItem(page, "Renew the certificate", { need: "Critical", effort: "S", impact: "XL" });
  await addItem(page, "Dark mode", { need: "Idea" });
  await openItem(page, "Dark mode");
  const pane = itemPane(page);
  const heldBack = pane.getByRole("list", { name: "Held back by" });
  await expect(heldBack.getByRole("listitem")).toContainText([
    "Need: Idea",
    "No due date",
    "Effort and impact not set",
  ]);

  // Sizes: the Effort buttons come first, and picking one changes the entry once the server has ranked again.
  await pane.getByRole("button", { name: /Set sizes/ }).click();
  const effort = pane.getByRole("radiogroup", { name: "Effort", exact: true });
  await expect(effort.getByRole("radio", { name: "S", exact: true })).toBeFocused();
  await effort.getByRole("radio", { name: "M", exact: true }).click();
  await expect(heldBack).toContainText("Impact not set, counts as S");
  await expect(heldBack).not.toContainText("Effort and impact not set");

  // Need: the picked step has the focus, and the entry goes once the need isn't low.
  await pane.getByRole("button", { name: /Change need/ }).click();
  const need = pane.getByRole("radiogroup", { name: "Need", exact: true });
  await expect(need.getByRole("radio", { name: "Idea", exact: true })).toBeFocused();
  await need.getByRole("radio", { name: "Expected", exact: true }).click();
  await expect(heldBack).not.toContainText("Need: Idea");

  // Due: the picker on a wide screen, the Due sheet on a phone.
  await pane.getByRole("button", { name: /Set due date/ }).click();
  if (isPhone(page)) {
    const sheet = page.getByRole("dialog", { name: "Due", exact: true });
    await sheet.getByRole("radio", { name: "Exact date", exact: true }).click();
    await sheet.getByLabel("Exact due date").fill(dayFromToday(5));
    await closeField(page);
  } else {
    const due = pane.getByRole("radiogroup", { name: "Due", exact: true });
    await expect(due.locator(":focus")).toHaveCount(1);
    await due.getByRole("radio", { name: "Exact date", exact: true }).click();
    await pane.getByLabel("Exact due date").fill(dayFromToday(5));
  }
  await expect(heldBack).not.toContainText("No due date");
});

test("the item that ranks first says nothing holds it back", async ({ page }) => {
  await addItem(page, "Dark mode", { need: "Idea", effort: "S", impact: "XL" });
  await openItem(page, "Dark mode");

  await expect(itemPane(page).getByRole("heading", { name: "Why it's #1" })).toBeVisible();
  await expect(itemPane(page).getByText("Held back by")).toHaveCount(0);
});
