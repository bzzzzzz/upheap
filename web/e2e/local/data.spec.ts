// Keeping the local build's data (design/architecture.md, Local build): the storage note, export, import and its
// confirm, and one tab at a time.
import { readFile } from "node:fs/promises";

import { addItem, expect, goTo, row, test } from "../fixtures";

test("a downloaded copy replaces the data after a confirm, and a file that isn't one is refused", async ({ page }) => {
  const note = page.locator("footer").filter({ visible: true });
  await expect(note).toContainText("Stored only in this browser");
  await expect(note).toContainText("No copy downloaded yet.");
  await addItem(page, "Keep me");
  const heapUrl = page.url();

  await goTo(page, "Account");
  const card = page.getByRole("region", { name: "Your data" });
  await expect(card).toContainText("1 heap · 1 item · never downloaded");
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    card.getByRole("button", { name: "Download a copy" }).click(),
  ]);
  expect(download.suggestedFilename()).toMatch(/^upheap-\d{4}-\d{2}-\d{2}\.sqlite3$/);
  const copy = await readFile(await download.path());
  await expect(card).toContainText(/last downloaded \d{1,2} \w{3}$/);
  // A copy downloaded just now is recent, so the note no longer nudges.
  await expect(note).not.toContainText("downloaded");

  // Loaded straight from the address, as a bookmark would on Pages, through 404.html.
  await page.goto(heapUrl);
  await addItem(page, "Added later");

  await page.goto("account");
  const input = page.getByLabel("Open a copy…");
  await input.setInputFiles({ name: "monday.sqlite3", mimeType: "application/vnd.sqlite3", buffer: copy });
  const confirm = page.getByRole("alertdialog", { name: "Replace everything here with monday.sqlite3?" });
  await expect(confirm).toContainText(/The copy has 1 heap and 1 item, from \d{1,2} \w{3}\./);
  await expect(confirm).toContainText("What's in this browser now (2 items) is replaced and can't be brought back.");
  const [current] = await Promise.all([
    page.waitForEvent("download"),
    confirm.getByRole("button", { name: "Download current first" }).click(),
  ]);
  expect(current.suggestedFilename()).toMatch(/\.sqlite3$/);
  await expect(confirm.getByRole("button", { name: "Downloaded" })).toBeVisible();
  await confirm.getByRole("button", { name: "Replace" }).click();
  // The app starts again from the top, on the copy.
  await expect(row(page, "Keep me")).toBeVisible();
  await expect(row(page, "Added later")).toHaveCount(0);

  await page.goto("account");
  await input.setInputFiles({ name: "notes.sqlite3", mimeType: "text/plain", buffer: Buffer.from("not a database") });
  const refused = page.getByRole("alertdialog", { name: "Replace everything here with notes.sqlite3?" });
  await expect(refused.getByRole("alert")).toHaveText("This file isn't an Upheap database.");
  await expect(refused.getByRole("button", { name: "Replace" })).toHaveCount(0);
  await refused.getByRole("button", { name: "Cancel" }).click();
  await expect(refused).toHaveCount(0);
  await expect(input).toBeFocused();
  await input.setInputFiles({ name: "notes.sqlite3", mimeType: "text/plain", buffer: Buffer.from("not a database") });
  await expect(refused.getByRole("alert")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(refused).toHaveCount(0);
  await expect(input).toBeFocused();
  await page.goto(heapUrl);
  await expect(row(page, "Keep me")).toBeVisible();
});

for (const [daysAgo, note] of [
  [15, "Last copy downloaded 2 weeks ago."],
  [13, null],
] as const) {
  test(`the storage note after a copy downloaded ${daysAgo} days ago`, async ({ page }) => {
    await page.addInitScript(
      (at) => localStorage.setItem("upheap.lastDownloadAt", String(at)),
      Date.now() - daysAgo * 864e5,
    );
    await page.reload();
    const footer = page.locator("footer").filter({ visible: true });
    await expect(footer).toContainText("Stored only in this browser");
    if (note) {
      await expect(footer).toContainText(note);
    } else {
      await expect(footer).not.toContainText("downloaded");
    }
  });
}

test("a second tab waits until the first one closes", async ({ page, context }) => {
  await addItem(page, "Only here");
  const heapUrl = page.url();

  const second = await context.newPage();
  await second.goto(heapUrl);
  await expect(second.getByRole("heading", { name: "Upheap is open in another tab" })).toBeVisible();

  await page.close();
  await expect(row(second, "Only here")).toBeVisible();
});
