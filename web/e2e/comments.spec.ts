// Comments on an item (ux.md, The item §3): the thread under the context, and the field that adds to it. Added here as
// a person would, through the item's own field. Server build only.
import { expect, isPhone, itemPane, openItem, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

test("a comment is added from the item's field and reads under Comments with its author", async ({
  page,
  heapPath,
}) => {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: "Rotate the certs", need: "expected" },
  });
  expect(created.status()).toBe(201);
  await page.goto(heapPath);
  await openItem(page, "Rotate the certs");

  const pane = itemPane(page);
  const comments = pane.getByRole("region", { name: "Comments", exact: true });
  // Without comments the heading is just "Comments", and the field is there to add the first.
  await expect(comments.getByRole("heading", { name: "Comments", exact: true })).toBeVisible();
  const field = comments.getByLabel("Add a comment");
  const send = comments.getByRole("button", { name: "Comment", exact: true });
  await expect(send).toBeDisabled();
  await field.fill("   ");
  await expect(send).toBeDisabled();
  await field.fill("Use the internal CA\nfor staging.");
  await expect(send).toBeEnabled();
  // Rounded: layout can land a 44 px control a hair short, such as 43.99997.
  if (isPhone(page)) {
    expect(Math.round((await send.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
    expect(Math.round((await send.boundingBox())?.width ?? 0)).toBeGreaterThan(200);
  }
  await send.click();

  // What only the server's answer shows: the count, the author, the text with its line break, the cleared field.
  await expect(comments.getByRole("heading", { name: "Comments 1", exact: true })).toBeVisible();
  const first = comments.getByRole("listitem");
  await expect(first).toHaveCount(1);
  await expect(first).toContainText("Tester ·");
  await expect(first).toContainText("Use the internal CA");
  await expect(first).toContainText("for staging.");
  await expect(first.locator("br")).toHaveCount(1);
  await expect(field).toHaveValue("");
  await expect(field).toBeFocused();
  await expect(comments.getByRole("status")).toHaveText("Comment added");

  // A second one follows the first, and the item's own fields are untouched.
  await field.fill("Installed it in staging.");
  await send.click();
  await expect(comments.getByRole("heading", { name: "Comments 2", exact: true })).toBeVisible();
  await expect(comments.getByRole("listitem").last()).toContainText("Installed it in staging.");
  await expect(pane.getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");

  // A reload shows them from the server, in order.
  await page.reload();
  const reloaded = itemPane(page).getByRole("region", { name: "Comments", exact: true });
  await expect(reloaded.getByRole("listitem")).toHaveCount(2);
  await expect(reloaded.getByRole("listitem").first()).toContainText("Use the internal CA");
});

test("a comment renders as Markdown and never runs raw HTML", async ({ page, heapPath }) => {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: "Rotate the certs", need: "expected" },
  });
  expect(created.status()).toBe(201);
  const { key } = await created.json();
  const added = await page.request.post(`/api/v1${heapPath}/items/${key}/comments`, {
    headers,
    data: { body: "Do **both**:\n\n- staging\n- prod\n\nthen tell <b>us</b>\nsoon" },
  });
  expect(added.ok()).toBe(true);
  await page.goto(`${heapPath}/items/${key}`);
  const comment = itemPane(page).getByRole("region", { name: "Comments", exact: true }).locator("ul > li").first();
  await expect(comment.locator("strong")).toHaveText("both");
  await expect(comment.getByRole("listitem")).toHaveText(["staging", "prod"]);
  await expect(comment.locator("br")).toHaveCount(1);
  await expect(comment.locator("b")).toHaveCount(0);
  await expect(comment).not.toContainText("<b>");
});
