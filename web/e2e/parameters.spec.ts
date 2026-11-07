import { addItem, goTo, expect, expectOrder, row, test } from "./fixtures";

test("an admin manages a parameter, and its options rank the items", async ({ page }) => {
  await addItem(page, "Free user's request");
  await addItem(page, "Enterprise request");

  // Define the parameter, highest priority option first.
  await goTo(page, "Parameters");
  await page.getByLabel("New parameter name").fill("Customer");
  await page.getByLabel("Weight of the new parameter").fill("50");
  await page.getByLabel(/^Options of the new parameter/).fill("Enterprise\nFree");
  await page.getByRole("button", { name: "Add parameter" }).click();
  await expect(page.getByLabel("Name of parameter Customer")).toBeVisible();

  // Give each item its customer.
  for (const [title, customer] of [
    ["Free user's request", "Free"],
    ["Enterprise request", "Enterprise"],
  ] as const) {
    await goTo(page, "Open");
    await page.getByRole("link", { name: `Edit ${title}` }).click();
    await page.getByRole("group", { name: "Parameters" }).getByLabel("Customer").selectOption(customer);
    await page.getByRole("button", { name: "Save" }).click();
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
  await enterprise.getByLabel("Open items with Enterprise switch to").selectOption("Free");
  await enterprise.getByRole("button", { name: "Archive" }).click();
  await expect(page.getByLabel("Label of option 2")).toHaveCount(0);
  await goTo(page, "Open");
  await expect(row(page, "Enterprise request")).toContainText("Customer: Free");
});
