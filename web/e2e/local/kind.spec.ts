// Kind and stage in the local build (design/architecture.md, Local build): an item's kind shows, its stage never does.
import { addItem, expect, isPhone, itemPane, openItem, test } from "../fixtures";

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
