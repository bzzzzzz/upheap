// The heap's context in the local build (design/ux.md, Heap settings): Settings has no Context card, though the
// backend supports it.
import { expect, goTo, test } from "../fixtures";

test("Settings has no Context card", async ({ page }) => {
  await goTo(page, "Settings");
  await expect(page.getByLabel("Heap name")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Context" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Edit", exact: true })).toHaveCount(0);
});
