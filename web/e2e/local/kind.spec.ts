// Kind and stage in the local build (design/architecture.md, Local build): an item's kind shows, its stage never does.
import { addItem, expect, goTo, isPhone, itemPane, openItem, test } from "../fixtures";

test("an item shows its kind and no stage, and More has no Stage section", async ({ page }) => {
  await addItem(page, "Keep me");
  await openItem(page, "Keep me");
  const pane = itemPane(page);

  await expect(pane.getByText("Note", { exact: true })).toBeVisible();
  await expect(pane).not.toContainText("·");
  await expect(pane).not.toContainText("As written");

  await pane.getByRole("group", { name: "State" }).getByRole("button", { name: "More", exact: true }).click();
  const more = isPhone(page)
    ? page.getByRole("dialog", { name: "More", exact: true })
    : page.getByRole("menu", { name: "More" });
  await expect(more.getByText("Make it a task…")).toBeVisible();
  await expect(more.getByText("Stage", { exact: true })).toHaveCount(0);
});

test("an item has no Who has it block or Send to an agent, and Heap settings has no Agents card", async ({ page }) => {
  await addItem(page, "Keep me");
  await openItem(page, "Keep me");
  await expect(itemPane(page).getByRole("region", { name: "Who has it" })).toHaveCount(0);
  await expect(itemPane(page).getByRole("button", { name: "Send to an agent" })).toHaveCount(0);
  await expect(page.getByText("with an agent")).toHaveCount(0);

  await goTo(page, "Settings");
  await expect(page.getByLabel("Heap name")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Agents", exact: true })).toHaveCount(0);
  await expect(page.getByRole("switch")).toHaveCount(0);
});
