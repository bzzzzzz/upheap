// The heap's settings (design/app-ux.md, Heap settings): General, Tags and Parameters as tabs, each at its own path.
import { expect, goTo, test } from "./fixtures";

test("General renames the heap, changes its link name and archives it", async ({ page, heapPath }) => {
  await goTo(page, "Settings");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings$`));
  const tabs = page.getByRole("navigation", { name: "Settings" });
  await expect(tabs.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");

  await page.getByLabel("Heap name").fill("Allotment");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /^Switch heap: Allotment/ }).first()).toBeAttached();

  // A new link name moves the page to the heap's new path.
  await page.getByLabel("Heap link name").fill("allotment");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page).toHaveURL(/\/heaps\/allotment\/settings$/);

  await page.getByRole("button", { name: "Archive heap" }).click();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toBeVisible();
  await page.getByRole("button", { name: "Unarchive heap" }).click();
  await expect(page.getByRole("button", { name: "Archive heap" })).toBeVisible();
});

test("Tags and Parameters are tabs of Settings, and their old paths lead there", async ({ page, heapPath }) => {
  await goTo(page, "Tags");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/tags$`));
  await expect(page.getByLabel("New tag name")).toBeVisible();
  await goTo(page, "Parameters");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/parameters$`));
  await expect(page.getByLabel("New parameter name")).toBeVisible();

  // `heapPath` holds the local build's base path too.
  await page.goto(`${heapPath}/tags`);
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/tags$`));
});
