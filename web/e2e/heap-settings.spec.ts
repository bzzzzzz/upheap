// The heap's settings (design/ux.md, Heap settings): General, Tags and Parameters as tabs, each at its own path.
import { expect, goTo, test } from "./fixtures";

test("General renames the heap, changes its link name and archives it", async ({ page, heapPath }) => {
  await goTo(page, "Settings");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings$`));
  const tabs = page.getByRole("navigation", { name: "Settings" });
  await expect(tabs.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");

  await page.getByLabel("Heap name").fill("Allotment");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /^Switch heap: Allotment/ }).first()).toBeAttached();

  // A new link name moves the page to the heap's new path (under the local build's base path too), with the heap
  // shown at once and focus still in the field. To a finger, focus isn't moved back to the field, which would bring
  // the on-screen keyboard back up.
  const linkName = page.getByLabel("Heap link name");
  await linkName.fill("allotment");
  await expect(page.getByText(`New link: ${heapPath.replace(/[^/]+$/, "allotment")}.`)).toBeVisible();
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page).toHaveURL(/\/heaps\/allotment\/settings$/);
  if (await page.evaluate(() => window.matchMedia("(pointer: coarse)").matches)) {
    await expect(linkName).not.toBeFocused();
  } else {
    await expect(linkName).toBeFocused();
  }
  await expect(linkName).toHaveValue("allotment");
  await expect(page.getByText("Loading…")).toHaveCount(0);

  await page.getByRole("button", { name: "Archive heap" }).click();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toBeVisible();
  await page.getByRole("button", { name: "Unarchive heap" }).click();
  await expect(page.getByRole("button", { name: "Archive heap" })).toBeVisible();
});

test("saving the link name keeps a name being typed beside it", async ({ page }) => {
  await goTo(page, "Settings");
  const name = page.getByLabel("Heap name");
  await name.fill("Allotment");
  const linkName = page.getByLabel("Heap link name");
  await linkName.fill("allotment");
  await page.locator("form").filter({ has: linkName }).getByRole("button", { name: "Save" }).click();
  await expect(page).toHaveURL(/\/heaps\/allotment\/settings$/);
  await expect(linkName).toHaveValue("allotment");
  await expect(name).toHaveValue("Allotment");
});

test("Tags and Parameters are tabs of Settings, and their old paths lead there", async ({ page, heapPath }) => {
  await goTo(page, "Tags");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/tags$`));
  await expect(page.getByLabel("New tag name")).toBeVisible();
  await goTo(page, "Parameters");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/parameters$`));
  await expect(page.getByRole("button", { name: "New parameter" })).toBeVisible();

  // `heapPath` holds the local build's base path too.
  await page.goto(`${heapPath}/tags`);
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/tags$`));
  await page.goto(`${heapPath}/parameters`);
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/parameters$`));
});

test("a tag reads as a row, and its edit button turns it into a form", async ({ page }) => {
  await goTo(page, "Tags");
  await page.getByLabel("New tag name").fill("billing");
  await page.getByLabel("New tag description").fill("Invoices");
  await page.getByRole("button", { name: "Add tag" }).click();
  await expect(page.getByRole("heading", { name: "1 tag" })).toBeVisible();
  await page.getByLabel("New tag name").fill("frontend");
  await page.getByRole("button", { name: "Add tag" }).click();
  await expect(page.getByRole("heading", { name: "2 tags" })).toBeVisible();
  await expect(page.getByText("No description")).toBeVisible();
  await expect(page.getByLabel("Name of tag billing")).toHaveCount(0);

  // Cancel leaves the tag as it was, with focus back on its edit button.
  const edit = page.getByRole("button", { name: "Edit billing" });
  await edit.click();
  await expect(page.getByLabel("Name of tag billing")).toBeFocused();
  await page.getByLabel("Name of tag billing").fill("plans");
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByLabel("Name of tag billing")).toHaveCount(0);
  await expect(edit).toBeFocused();

  await edit.click();
  await page.getByLabel("Name of tag billing").fill("billing & plans");
  await page.getByLabel("Description of tag billing").fill("Invoices and plans");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: "Edit billing & plans" })).toBeVisible();
  await expect(page.getByText("Invoices and plans")).toBeVisible();

  // A name another tag has is refused, and the form stays open.
  await page.getByRole("button", { name: "Edit frontend" }).click();
  await page.getByLabel("Name of tag frontend").fill("Billing & Plans");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("alert")).toHaveText("This heap already has a tag with that name.");
  await expect(page.getByLabel("Name of tag frontend")).toBeVisible();
});
