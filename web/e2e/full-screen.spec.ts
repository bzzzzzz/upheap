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
  await addItem(page, "Water the plants");
  await openItem(page, "Plan the launch");
  await fullButton(page).click();
  await expect(exitButton(page)).toBeVisible();

  // In full screen, the next item keeps full screen.
  await itemPane(page).getByRole("link", { name: "Next item", exact: true }).click();
  await expect(page).toHaveURL(/\?full=1$/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");

  // Walking replaced the full screen entry, so Back is the pane of the first item.
  await page.goBack();
  await expect(page).not.toHaveURL(/full=/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Plan the launch");

  await fullButton(page).click();
  await closeItem(page);
  await expect(page).not.toHaveURL(/items|full=/);
  await expect(row(page, "Plan the launch").getByRole("link").first()).toBeFocused();
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

test("the list keeps its place through full screen", async ({ page, heapPath }) => {
  test.skip(isPhone(page), "a phone has no full screen button");
  for (let i = 0; i < 30; i++) {
    const made = await page.request.post(`/api/v1${heapPath}/items`, {
      headers: { "x-csrf-protection": "1" },
      data: { title: `Task ${String(i).padStart(2, "0")}`, need: "expected" },
    });
    expect(made.status()).toBe(201);
  }
  await page.goto(heapPath);
  await expect(row(page, "Task 05")).toBeVisible();
  await page.evaluate(() => window.scrollTo(0, 300));
  const before = await page.evaluate(() => Math.round(window.scrollY));
  expect(before).toBeGreaterThan(0);
  await row(page, "Task 05").getByRole("link").first().click();
  await expect(itemPane(page)).toBeVisible();

  await fullButton(page).click();
  await expect(exitButton(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(fullButton(page)).toBeVisible();
  expect(await page.evaluate(() => Math.round(window.scrollY))).toBe(before);
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
