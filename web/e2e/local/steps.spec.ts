// Steps in the local build (design/architecture.md, Local build): the same backend, so a task is worked through its
// steps here too, with no stage anywhere.
import { addItem, expect, itemPane, makeTask, openItem, test } from "../fixtures";

test("a task takes steps, which tick and count on its row", async ({ page }) => {
  await addItem(page, "Rotate the certs");
  await openItem(page, "Rotate the certs");
  await makeTask(page, "Rotate the certs");
  const steps = itemPane(page).getByRole("region", { name: "Steps" });

  await steps.getByRole("button", { name: "Add a step" }).click();
  const field = steps.getByRole("textbox", { name: "New step" });
  await field.fill("Inventory the certs");
  await field.press("Enter");
  await field.fill("Run in staging");
  await field.press("Enter");
  await expect(steps.getByRole("heading", { name: "Steps 0/2" })).toBeVisible();

  await steps.getByRole("checkbox", { name: "Inventory the certs" }).click({ force: true });
  await expect(steps.getByRole("heading", { name: "Steps 1/2" })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Rotate the certs\s?, task, 1 of 2 steps ticked$/ })).toBeVisible();
  await expect(page.getByRole("main").getByRole("listitem").filter({ hasText: "Rotate the certs" })).toContainText(
    "1/2",
  );
});
