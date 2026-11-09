import {
  addItem,
  addParameter,
  closeItem,
  expect,
  expectOrder,
  goTo,
  itemPane,
  openItem,
  pick,
  row,
  test,
} from "./fixtures";

test("an admin manages a parameter, and its options rank the items", async ({ page }) => {
  await addItem(page, "Free user's request");
  await addItem(page, "Enterprise request");

  // Define the parameter, highest priority option first.
  await addParameter(page, "Customer", 50, ["Enterprise", "Free"]);

  // Give each item its customer.
  for (const [title, customer] of [
    ["Free user's request", "Free"],
    ["Enterprise request", "Enterprise"],
  ] as const) {
    await goTo(page, "Open");
    await openItem(page, title);
    await pick(page, "Customer", customer);
    // Checked in the pane, or on a phone shown on the field's row once its sheet closes.
    const phone = (page.viewportSize()?.width ?? 0) < 640;
    await expect(
      phone
        ? itemPane(page).getByRole("button", { name: `Customer ${customer}`, exact: true })
        : itemPane(page).getByRole("radio", { name: customer, exact: true }),
    ).toBeVisible();
    if (!phone) await expect(itemPane(page).getByRole("radio", { name: customer, exact: true })).toBeChecked();
    await closeItem(page);
    await expect(row(page, title)).toContainText(`Customer: ${customer}`);
  }
  await page.reload();
  await expectOrder(page, ["Enterprise request", "Free user's request"]);

  // Putting Free first turns the order around.
  await goTo(page, "Parameters");
  await page.getByRole("button", { name: "Move Free up" }).click();
  await page.getByRole("button", { name: "Save options" }).click();
  await expect(page.getByLabel("Label of option 1")).toHaveValue("Free");
  await goTo(page, "Open");
  await expectOrder(page, ["Free user's request", "Enterprise request"]);

  // Archiving an option moves its open items to the replacement.
  await goTo(page, "Parameters");
  const enterprise = page
    .getByRole("listitem")
    .filter({ has: page.getByRole("button", { name: "Move Enterprise up" }) })
    // The parameter's card is a list item too.
    .filter({ hasNot: page.getByRole("button", { name: "Archive parameter" }) });
  await enterprise.getByRole("button", { name: "Archive" }).click();
  // The row turns into the confirm.
  const confirm = page
    .getByRole("listitem")
    .filter({ hasText: "Archive Enterprise?" })
    .filter({ hasNot: page.getByRole("button", { name: "Archive parameter" }) });
  await confirm.getByLabel("Open items with Enterprise switch to").selectOption("Free");
  await confirm.getByRole("button", { name: "Archive", exact: true }).click();
  await expect(page.getByLabel("Label of option 2")).toHaveCount(0);
  await goTo(page, "Open");
  await expect(row(page, "Enterprise request")).toContainText("Customer: Free");
});

test("New parameter opens a form that Cancel closes, and Rename renames the parameter", async ({ page }) => {
  await goTo(page, "Parameters");
  const newParameter = page.getByRole("button", { name: "New parameter" });
  await newParameter.click();
  await expect(page.getByLabel("New parameter name")).toBeFocused();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByLabel("New parameter name")).toHaveCount(0);
  await expect(newParameter).toBeFocused();

  await addParameter(page, "Customer", 20, ["Enterprise", "Team", "Free"]);
  await expect(page.getByLabel("New parameter name")).toHaveCount(0);
  await expect(page.getByLabel("Label of option 3")).toHaveValue("Free");
  await expect(page.getByRole("button", { name: "Move Enterprise up" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Move Free down" })).toBeDisabled();

  await page.getByRole("button", { name: "Rename" }).click();
  const name = page.getByLabel("Name of parameter Customer");
  await expect(name).toBeFocused();
  await name.fill("Plan");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Plan", exact: true })).toBeVisible();
  await expect(page.getByLabel("Name of parameter Plan")).toHaveCount(0);
  await expect(
    page
      .getByText("Adding one of these tags to an item suggests Plan.")
      .or(page.getByText("No tags suggest Plan yet.")),
  ).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Tags that suggest Plan" })).toBeVisible();
});
