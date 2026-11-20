// An item open full screen (design/ux.md, The item): `full=1` in the address, a header button, and Escape, ✕ and Back
// out of it. A phone has no button, only the full-height sheet a full link opens.
import type { Page } from "@playwright/test";

import { addItem, closeItem, expect, isPhone, itemPane, openItem, row, test } from "./fixtures";

const fullButton = (page: Page) => itemPane(page).getByRole("button", { name: "Full screen", exact: true });
const exitButton = (page: Page) => itemPane(page).getByRole("button", { name: "Exit full screen", exact: true });
const widthOf = async (page: Page) => Math.round((await itemPane(page).boundingBox())?.width ?? 0);

test("Escape leaves full screen for the pane, and Back then closes the item", async ({ page }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  await addItem(page, "Plan the launch");
  await openItem(page, "Plan the launch");
  const paneWidth = await widthOf(page);

  await fullButton(page).click();
  await expect(page).toHaveURL(/\/items\/[^/?]+\?full=1$/);
  await expect.poll(() => widthOf(page)).toBeGreaterThan(600);

  await page.keyboard.press("Escape");
  await expect(page).not.toHaveURL(/full=/);
  await expect(itemPane(page)).toBeVisible();
  await expect.poll(() => widthOf(page)).toBe(paneWidth);

  await page.goBack();
  await expect(itemPane(page)).toHaveCount(0);
});

test("✕ in full screen goes to the list, and Back goes to the pane", async ({ page }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  await addItem(page, "Plan the launch");
  await openItem(page, "Plan the launch");
  await fullButton(page).click();
  await expect(exitButton(page)).toBeVisible();
  await page.goBack();
  await expect(page).not.toHaveURL(/full=/);
  await expect(fullButton(page)).toBeVisible();

  await fullButton(page).click();
  await closeItem(page);
  await expect(page).not.toHaveURL(/items|full=/);
  await expect(row(page, "Plan the launch").getByRole("link").first()).toBeFocused();
});

test("walking in full screen keeps it, and Escape leaves for the pane of the item walked to", async ({ page }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  await addItem(page, "Plan the launch");
  await addItem(page, "Water the plants");
  await openItem(page, "Plan the launch");
  await fullButton(page).click();
  await expect(exitButton(page)).toBeVisible();

  await itemPane(page).getByRole("link", { name: "Next item", exact: true }).click();
  await expect(page).toHaveURL(/\?full=1$/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");

  await page.keyboard.press("Escape");
  await expect(fullButton(page)).toBeVisible();
  await expect(page).not.toHaveURL(/full=/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");

  // The walk ended in the pane as a walk there does, so Back closes the item.
  await page.goBack();
  await expect(itemPane(page)).toHaveCount(0);
});

test("a full link opens full screen, and leaves for the pane, then the list", async ({ page }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  await addItem(page, "Plan the launch");
  await openItem(page, "Plan the launch");
  await page.goto(`${page.url()}?full=1`);
  await expect(exitButton(page)).toBeVisible();

  await exitButton(page).click();
  await expect(page).not.toHaveURL(/full=/);
  await expect(fullButton(page)).toBeVisible();

  await closeItem(page);
  await expect(page).not.toHaveURL(/items|full=/);
});

test("✕ from a full link leaves full screen behind, so the next item opens in the pane", async ({ page }) => {
  await addItem(page, "Plan the launch");
  await addItem(page, "Water the plants");
  await openItem(page, "Plan the launch");
  await page.goto(`${page.url()}?full=1`);
  await expect(itemPane(page)).toBeVisible();

  await closeItem(page);
  await expect(page).not.toHaveURL(/items|full=/);
  await openItem(page, "Water the plants");
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");
  await expect(page).not.toHaveURL(/full=/);
});

test("the list keeps its place through full screen", async ({ page, heapPath }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  // Titles that wrap beside the pane, so a list that widened under full screen would get shorter.
  const title = (i: number) =>
    `Task ${String(i).padStart(2, "0")} ${"with a title long enough to wrap ".repeat(3).trim()}`;
  for (let i = 0; i < 30; i++) {
    const made = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data: { title: title(i), need: "expected" },
    });
    expect(made.status()).toBe(201);
  }
  await page.goto(heapPath);
  await row(page, title(5)).getByRole("link").first().click();
  await expect(itemPane(page)).toBeVisible();
  // At the bottom, where a shorter list would pull the window up.
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBeGreaterThan(0);
  const before = await page.evaluate(() => Math.round(window.scrollY));

  // At the bottom the sticky pane's header sits above the window, and a click would scroll it into view first.
  await fullButton(page).dispatchEvent("click");
  await expect(exitButton(page)).toBeVisible();
  await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBe(before);
  await page.keyboard.press("Escape");
  await expect(fullButton(page)).toBeVisible();
  await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBe(before);
});

test("All items goes full screen with the item kept, and Escape leaves it", async ({ page }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  await addItem(page, "Plan the launch");
  await page.goto("/all");
  await openItem(page, "Plan the launch");
  await fullButton(page).click();
  await expect(page).toHaveURL(/item=[^&]+&full=1$/);
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/item=[^&]+$/);
  await expect(itemPane(page)).toBeVisible();
});

test("on a phone, the sheet has no full screen button, and a full link opens it full height", async ({ page }) => {
  test.skip(!isPhone(page), "only a phone has the sheet");
  await addItem(page, "Plan the launch");
  await openItem(page, "Plan the launch");
  await expect(fullButton(page)).toHaveCount(0);
  const link = page.url();

  await page.goto(`${link}?full=1`);
  await expect(itemPane(page)).toBeVisible();
  await expect.poll(async () => Math.round((await itemPane(page).boundingBox())?.y ?? 1)).toBe(0);
  await expect(itemPane(page)).toHaveCSS("border-top-left-radius", "0px");
});
