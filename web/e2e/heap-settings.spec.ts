// The heap's settings (design/ux.md, Heap settings): General, Context, Tags, Parameters and Ranking as tabs, each at its own path.
import { addItem, expect, expectOrder, goTo, test } from "./fixtures";

test("General renames the heap, changes its link name and archives it", async ({ page, heapPath }) => {
  await goTo(page, "Settings");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings$`));
  const tabs = page.getByRole("navigation", { name: "Settings" });
  await expect(tabs.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");

  await page.getByLabel("Heap name").fill("Allotment");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /^Switch heap: Allotment/ }).first()).toBeAttached();

  // A new link name moves the page to the heap's new path, with the heap
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

  // Archiving offers Undo in the toast; Unarchive is the other way back.
  await page.getByRole("button", { name: "Archive heap" }).click();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toBeVisible();
  const toast = page.getByRole("status").filter({ hasText: "is archived." }).filter({ hasText: "Undo" });
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(page.getByRole("button", { name: "Archive heap" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toHaveCount(0);
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

test("Ranking edits the heap's weights and saves them together", async ({ page, heapPath }) => {
  await goTo(page, "Ranking");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/ranking$`));
  const tabs = page.getByRole("navigation", { name: "Settings" });
  await expect(tabs.getByRole("link", { name: "Ranking" })).toHaveAttribute("aria-current", "page");

  // Four built-in factors at 10, so each has a quarter of the total.
  const need = page.getByLabel("Need weight, 0 to 100");
  for (const name of ["Need", "Age", "Deadline", "Value per effort"]) {
    await expect(page.getByLabel(`${name} weight, 0 to 100`)).toHaveValue("10");
  }
  await expect(page.getByText("25%", { exact: true })).toHaveCount(4);
  await expect(page.getByRole("button", { name: "Save weights" })).toHaveCount(0);

  await need.fill("40");
  await expect(page.getByText("was 10", { exact: true })).toBeVisible();
  await expect(page.getByText("1 unsaved")).toBeVisible();
  await expect(page.getByText("57%", { exact: true })).toBeVisible();
  await expect(page.getByText("14%", { exact: true })).toHaveCount(3);

  // Enter in a weight saves, as the button does; the status takes the focus the button had.
  await need.press("Enter");
  const saved = page.getByRole("status").filter({ hasText: "Weights saved." });
  await expect(saved).toBeVisible();
  await expect(saved).toBeFocused();
  await expect(page.getByRole("button", { name: "Save weights" })).toHaveCount(0);
  await expect(page.getByText("was 10", { exact: true })).toHaveCount(0);

  await page.reload();
  await expect(need).toHaveValue("40");

  await goTo(page, "Settings");
  await page.locator("summary", { hasText: "History" }).click();
  await expect(page.getByText("Need weight")).toBeVisible();
});

test("Cancel puts the weights back", async ({ page }) => {
  await goTo(page, "Ranking");
  const need = page.getByLabel("Need weight, 0 to 100");
  const age = page.getByLabel("Age weight, 0 to 100");
  await need.fill("40");
  await age.fill("0");
  await expect(page.getByText("2 unsaved")).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(need).toHaveValue("10");
  await expect(age).toHaveValue("10");
  await expect(page.getByRole("button", { name: "Save weights" })).toHaveCount(0);
  await expect(need).toBeFocused();

  // Escape in a weight does the same, and Reset all to 10 only draws weights back to the default.
  await age.fill("30");
  await age.press("Escape");
  await expect(age).toHaveValue("10");
  await expect(page.getByRole("button", { name: "Reset all to 10" })).toBeDisabled();
  await age.fill("30");
  await page.getByRole("button", { name: "Reset all to 10" }).click();
  await expect(age).toHaveValue("10");
  await expect(page.getByRole("button", { name: "Reset all to 10" })).toBeDisabled();
  await expect(page.getByText("1 unsaved")).toHaveCount(0);
});

test("the preview shows how the top of the list would move", async ({ page }) => {
  await addItem(page, "Urgent", { need: "Critical" });
  await addItem(page, "Quick win", { effort: "S", impact: "L" });
  await expectOrder(page, ["Urgent", "Quick win"]);

  await goTo(page, "Ranking");
  const preview = page.getByRole("complementary", { name: "Top of the list" });
  await expect(preview.getByText("Move a weight to see how the top of the list would change.")).toBeVisible();
  await expect(preview.getByRole("listitem")).toHaveText([/Urgent/, /Quick win/]);

  await page.getByLabel("Value per effort weight, 0 to 100").fill("100");
  await expect(preview.getByText("With your changes. Nothing moves until you save.")).toBeVisible();
  await expect(preview.getByRole("listitem")).toHaveText([/Quick win.*↑1/, /Urgent.*↓1/]);
  await expect(preview.getByRole("img", { name: "up 1" })).toBeVisible();
  await expect(preview.getByRole("img", { name: "down 1" })).toBeVisible();

  // Nothing moved in the list itself, until it is saved.
  await goTo(page, "Open");
  await expectOrder(page, ["Urgent", "Quick win"]);
  await goTo(page, "Ranking");
  await page.getByLabel("Value per effort weight, 0 to 100").fill("100");
  await page.getByRole("button", { name: "Save weights" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Weights saved." })).toBeVisible();
  await expect(preview.getByRole("listitem")).toHaveText([/Quick win/, /Urgent/]);
  await expect(preview.getByRole("img")).toHaveCount(0);
  await goTo(page, "Open");
  await expectOrder(page, ["Quick win", "Urgent"]);
});

test("General has no Context card, and Context is a tab between General and Tags", async ({ page, heapPath }) => {
  await goTo(page, "Settings");
  await expect(page.getByRole("region", { name: "Context", exact: true })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Agents" })).toBeVisible();
  const tabs = page.getByRole("navigation", { name: "Settings" });
  await expect(tabs.getByRole("link")).toHaveText(["General", "Context", "Tags", "Parameters", "Ranking"]);
  // Every tab fits the width, on a phone too, down to a 375 px one: none hides past the edge of the bar.
  if (test.info().project.name === "phone") await page.setViewportSize({ width: 375, height: 667 });
  expect(await tabs.evaluate((nav) => nav.scrollWidth <= nav.clientWidth)).toBe(true);
  await goTo(page, "Context");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/context$`));
  await expect(tabs.getByRole("link", { name: "Context" })).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("region", { name: "Context", exact: true })).toBeVisible();
});

test("the context renders as Markdown, and never runs what it holds", async ({ page }) => {
  await goTo(page, "Context");
  const card = page.getByRole("region", { name: "Context", exact: true });
  const edit = card.getByRole("button", { name: "Edit" });
  await edit.click();
  await page
    .getByLabel("Heap context")
    .fill(
      "# Docs\n\nSee **the wiki**.\n\n- one\n- two\n\n[site](https://example.com) <b>raw</b> [x](javascript:alert(1))",
    );
  await card.getByRole("button", { name: "Save" }).click();
  await expect(edit).toBeFocused();
  await expect(card.getByRole("heading", { name: "Docs" })).toBeVisible();
  await expect(card.locator("strong")).toHaveText("the wiki");
  await expect(card.getByRole("listitem")).toHaveText(["one", "two"]);
  await expect(card.getByRole("link", { name: "site" })).toHaveAttribute("target", "_blank");
  await expect(card.locator("b")).toHaveCount(0);
  await expect(card).not.toContainText("<b>");
  await expect(card.getByRole("link", { name: "x" })).toHaveCount(0);
  await expect(card).toContainText("x");
});

test("an archived heap shows the context rendered, without Edit", async ({ page, heapPath }) => {
  await goTo(page, "Context");
  const card = page.getByRole("region", { name: "Context", exact: true });
  await card.getByRole("button", { name: "Edit" }).click();
  await page.getByLabel("Heap context").fill("Read **me**");
  await card.getByRole("button", { name: "Save" }).click();
  await expect(card.locator("strong")).toHaveText("me");
  await goTo(page, "Settings");
  await page.getByRole("button", { name: "Archive heap" }).click();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toBeVisible();
  await goTo(page, "Context");
  await expect(page).toHaveURL(new RegExp(`${heapPath}/settings/context$`));
  await expect(card.locator("strong")).toHaveText("me");
  await expect(card.getByRole("button", { name: "Edit" })).toHaveCount(0);
});

test("the heap's context is written on the Context tab: Edit, Save, Cancel and clearing it", async ({ page }) => {
  await goTo(page, "Context");
  const card = page.getByRole("region", { name: "Context", exact: true });
  const empty = card.getByText(
    "What the project is, where its repository and docs are, where its work is reviewed and what finishing a piece means.",
  );
  const edit = card.getByRole("button", { name: "Edit" });
  const field = page.getByLabel("Heap context");
  await expect(empty).toBeVisible();

  // The text keeps its line breaks (as a break), and focus goes back to Edit once it is saved.
  await edit.click();
  await expect(field).toBeFocused();
  await field.fill("Docs are in the wiki.\nWork is reviewed on GitHub.");
  await card.getByRole("button", { name: "Save" }).click();
  await expect(field).toHaveCount(0);
  await expect(edit).toBeFocused();
  const text = card.getByText("Docs are in the wiki.");
  await expect(text).toContainText("Work is reviewed on GitHub.");
  await expect(text.locator("br")).toHaveCount(1);
  await page.reload();
  await expect(card.getByText("Docs are in the wiki.")).toBeVisible();

  // Cancel drops the draft, and Escape does nothing to it.
  await edit.click();
  await expect(field).toHaveValue("Docs are in the wiki.\nWork is reviewed on GitHub.");
  await field.fill("A draft");
  await field.press("Escape");
  await expect(field).toHaveValue("A draft");
  await card.getByRole("button", { name: "Cancel" }).click();
  await expect(field).toHaveCount(0);
  await expect(edit).toBeFocused();
  await expect(card.getByText("Docs are in the wiki.")).toBeVisible();

  // Clearing all the text empties it.
  await edit.click();
  await field.fill("");
  await card.getByRole("button", { name: "Save" }).click();
  await expect(empty).toBeVisible();
  await expect(edit).toBeFocused();
});

test("a context saved by someone else meanwhile is shown above the draft, which a second Save then saves", async ({
  page,
  heapPath,
}) => {
  await goTo(page, "Context");
  const card = page.getByRole("region", { name: "Context", exact: true });
  const field = page.getByLabel("Heap context");
  await card.getByRole("button", { name: "Edit" }).click();
  await field.fill("My version");

  // Another member saves a context while the form is open, through the API with the page's session.
  const url = `/api/v1${heapPath}`;
  const current = await page.request.get(url);
  expect(current.ok()).toBe(true);
  const { version } = await current.json();
  const patched = await page.request.patch(url, {
    headers: { "x-csrf-protection": "1", "If-Match": `"${version}"` },
    data: { context: "Their version" },
  });
  expect(patched.ok()).toBe(true);

  await card.getByRole("button", { name: "Save" }).click();
  await expect(card.getByRole("heading", { name: "Saved meanwhile" })).toBeVisible();
  await expect(card.getByText("Their version", { exact: true })).toBeVisible();
  await expect(field).toHaveValue("My version");
  await expect(card.getByRole("alert")).toContainText("Someone else changed the context while you were editing.");

  // The second Save replaces theirs with the draft.
  await card.getByRole("button", { name: "Save" }).click();
  await expect(field).toHaveCount(0);
  await expect(card.getByText("My version", { exact: true })).toBeVisible();
  await expect(card.getByText("Their version")).toHaveCount(0);
});
