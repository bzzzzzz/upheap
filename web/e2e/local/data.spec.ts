// Keeping the local build's data (design/local-build.md, Keeping the data): export and import, and one tab at a time.
import { addItem, expect, goTo, row, test } from "../fixtures";

test("a downloaded copy replaces the data, and a file that isn't one is refused", async ({ page }) => {
  await expect(page.locator("footer").filter({ visible: true })).toContainText(
    "Your heaps are stored only in this browser",
  );
  await addItem(page, "Keep me");
  const heapUrl = page.url();

  await goTo(page, "Account");
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Download a copy" }).click(),
  ]);
  const copy = await download.path();

  // Loaded straight from the address, as a bookmark would on Pages, through 404.html.
  await page.goto(heapUrl);
  await addItem(page, "Added later");

  await page.goto("account");
  await page.getByLabel("Open a copy…").setInputFiles(copy);
  await page.getByRole("button", { name: "Replace" }).click();
  // The app starts again from the top, on the copy.
  await expect(row(page, "Keep me")).toBeVisible();
  await expect(row(page, "Added later")).toHaveCount(0);

  await page.goto("account");
  await page
    .getByLabel("Open a copy…")
    .setInputFiles({ name: "notes.sqlite3", mimeType: "text/plain", buffer: Buffer.from("not a database") });
  await page.getByRole("button", { name: "Replace" }).click();
  await expect(page.getByRole("alert")).toHaveText("This file isn't an Upheap database.");
  await page.goto(heapUrl);
  await expect(row(page, "Keep me")).toBeVisible();
});

test("a second tab waits until the first one closes", async ({ page, context }) => {
  await addItem(page, "Only here");
  const heapUrl = page.url();

  const second = await context.newPage();
  await second.goto(heapUrl);
  await expect(second.getByRole("heading", { name: "Upheap is open in another tab" })).toBeVisible();

  await page.close();
  await expect(row(second, "Only here")).toBeVisible();
});
