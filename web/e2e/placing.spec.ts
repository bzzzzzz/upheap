import { expect, expectOrder, itemPane, openItem, row, seed, test } from "./fixtures";
import type { Locator, Page } from "@playwright/test";

// Placing an item by hand in the priority list (design/ux.md, The list): the grip, Alt+↑/↓ and a long press on a phone.

const headers = { "x-csrf-protection": "1" };

/** Three items that rank in this order, the oldest first, and the list open. */
async function listOfThree(page: Page, heapPath: string) {
  const first = await seed(page, heapPath, "Rotate the certs");
  const second = await seed(page, heapPath, "Call the bank");
  const third = await seed(page, heapPath, "Water the plants");
  await page.goto(heapPath);
  await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
  return { first, second, third };
}

const grip = (page: Page, title: string): Locator => row(page, title).locator("[data-row-grip]");
const toast = (page: Page, text: string) => page.getByRole("status").filter({ hasText: text });
const placed = (page: Page) => page.waitForResponse((r) => /\/place$/.test(r.url()) && r.request().method() !== "GET");

test.describe("with a mouse", () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(isMobile, "A wide screen drags by the grip.");
  });

  test("a row is dragged by its grip to the top, and the server keeps it", async ({ page, heapPath }) => {
    const { third } = await listOfThree(page, heapPath);
    await grip(page, "Water the plants").hover();
    const start = await grip(page, "Water the plants").boundingBox();
    const top = await row(page, "Rotate the certs").boundingBox();
    const fromY = (start?.y ?? 0) + (start?.height ?? 0) / 2;
    const toY = (top?.y ?? 0) + 2;
    await page.mouse.down();
    for (let i = 1; i <= 6; i++) await page.mouse.move((start?.x ?? 0) + 4, fromY + ((toY - fromY) * i) / 6);
    const saved = placed(page);
    await page.mouse.up();
    await saved;
    await expect(toast(page, "Moved “Water the plants” up.")).toBeVisible();
    await expectOrder(page, ["Water the plants", "Rotate the certs", "Call the bank"]);

    await page.reload();
    await expectOrder(page, ["Water the plants", "Rotate the certs", "Call the bank"]);
    await expect(row(page, "Water the plants").locator("p").first()).toHaveText(/^Moved up/);
    // Only its place changed.
    const item = await (await page.request.get(third.item)).json();
    expect(item).toMatchObject({ title: "Water the plants", need: "expected", placement: "up" });
  });

  test("Escape puts a dragged row back and saves nothing", async ({ page, heapPath }) => {
    await listOfThree(page, heapPath);
    await grip(page, "Water the plants").hover();
    const start = await grip(page, "Water the plants").boundingBox();
    const top = await row(page, "Rotate the certs").boundingBox();
    const fromY = (start?.y ?? 0) + (start?.height ?? 0) / 2;
    const toY = (top?.y ?? 0) + 2;
    await page.mouse.down();
    for (let i = 1; i <= 6; i++) await page.mouse.move((start?.x ?? 0) + 4, fromY + ((toY - fromY) * i) / 6);
    await page.keyboard.press("Escape");
    await page.mouse.up();
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
    await expect(page.getByRole("status").filter({ hasText: "Moved" })).toHaveCount(0);
  });

  test("Alt+↓ moves the focused row one place down, and the focus stays on it", async ({ page, heapPath }) => {
    await listOfThree(page, heapPath);
    const link = row(page, "Rotate the certs").getByRole("link", { name: /^Rotate the certs/ });
    await link.focus();
    await page.keyboard.press("Alt+ArrowDown");
    await expect(toast(page, "Moved “Rotate the certs” down.")).toBeVisible();
    await expectOrder(page, ["Call the bank", "Rotate the certs", "Water the plants"]);
    await expect(link).toBeFocused();

    await page.reload();
    await expectOrder(page, ["Call the bank", "Rotate the certs", "Water the plants"]);
  });

  test("Alt+↑ on the first row does nothing", async ({ page, heapPath }) => {
    await listOfThree(page, heapPath);
    await row(page, "Rotate the certs")
      .getByRole("link", { name: /^Rotate the certs/ })
      .focus();
    await page.keyboard.press("Alt+ArrowUp");
    await expect(page.getByRole("status").filter({ hasText: "Moved" })).toHaveCount(0);
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
  });

  test("Move down in the grip's menu moves the row, and Undo puts it back unplaced", async ({ page, heapPath }) => {
    const { first } = await listOfThree(page, heapPath);
    await grip(page, "Rotate the certs").getByRole("button", { name: "Move “Rotate the certs”" }).click();
    await expect(page.getByRole("menuitem", { name: "Move up" })).toHaveCount(0);
    const saved = placed(page);
    await page.getByRole("menuitem", { name: "Move down" }).click();
    await saved;
    await expect(toast(page, "Moved “Rotate the certs” down.")).toBeVisible();
    await expectOrder(page, ["Call the bank", "Rotate the certs", "Water the plants"]);

    const undone = placed(page);
    await page.getByRole("button", { name: "Undo" }).click();
    await undone;
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
    await page.reload();
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
    expect((await (await page.request.get(first.item)).json()).placement).toBeNull();
  });

  test("the open item says it was moved, and Reset puts it where its fields do", async ({ page, heapPath }) => {
    const { first, third } = await listOfThree(page, heapPath);
    const moved = await page.request.post(`${first.item}/place`, {
      headers: { ...headers, "If-Match": '"1"' },
      data: { anchor: third.id, side: "below" },
    });
    expect(moved.status()).toBe(200);
    await page.reload();
    await openItem(page, "Rotate the certs");
    const pane = itemPane(page);
    await expect(pane.getByText("Moved down in the list")).toBeVisible();

    const reset = placed(page);
    await pane.getByRole("button", { name: "Reset" }).click();
    await reset;
    await expect(toast(page, "Reset its place.")).toBeVisible();
    await expect(pane.getByText("Moved down in the list")).toHaveCount(0);
    await page.reload();
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
  });

  test("with text in the search box or another sort, a row has no grip and Alt+↓ moves nothing", async ({
    page,
    heapPath,
  }) => {
    await listOfThree(page, heapPath);
    await expect(grip(page, "Rotate the certs")).toHaveCount(1);

    await page.keyboard.press("/");
    await page.getByRole("searchbox").fill("bank");
    await expect(row(page, "Call the bank")).toBeVisible();
    await expect(page.locator("[data-row-grip]")).toHaveCount(0);
    await row(page, "Call the bank")
      .getByRole("link", { name: /^Call the bank/ })
      .focus();
    await page.keyboard.press("Alt+ArrowDown");
    await expect(page.getByRole("status").filter({ hasText: "Moved" })).toHaveCount(0);

    await page.goto(`${heapPath}?sort=due`);
    await expect(row(page, "Rotate the certs")).toBeVisible();
    await expect(page.locator("[data-row-grip]")).toHaveCount(0);
  });
});

test.describe("with a finger", () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(!isMobile, "A touch screen lifts a row by a long press.");
  });

  test("a long press lifts a row, and dragging it up places it", async ({ page, heapPath, context }) => {
    await listOfThree(page, heapPath);
    await expect(page.locator("[data-row-grip]")).toHaveCount(0);
    const target = row(page, "Water the plants");
    const box = await target.boundingBox();
    const top = await row(page, "Rotate the certs").boundingBox();
    const x = (box?.x ?? 0) + 40;
    const y = (box?.y ?? 0) + (box?.height ?? 0) / 2;
    const cdp = await context.newCDPSession(page);
    const touch = (type: "touchStart" | "touchMove" | "touchEnd", at: number) =>
      cdp.send("Input.dispatchTouchEvent", {
        type,
        touchPoints: type === "touchEnd" ? [] : [{ x, y: at }],
      });
    await touch("touchStart", y);
    // Lifted once the finger has stayed put for the press.
    await expect(target).toHaveCSS("z-index", "10");
    const toY = (top?.y ?? 0) + 2;
    for (let i = 1; i <= 6; i++) await touch("touchMove", y + ((toY - y) * i) / 6);
    const saved = placed(page);
    await touch("touchEnd", toY);
    await saved;
    await expect(toast(page, "Moved “Water the plants” up.")).toBeVisible();
    await expectOrder(page, ["Water the plants", "Rotate the certs", "Call the bank"]);
  });

  test("a vertical move before the press is over leaves the row where it is", async ({ page, heapPath, context }) => {
    await listOfThree(page, heapPath);
    const target = row(page, "Call the bank");
    const box = await target.boundingBox();
    const x = (box?.x ?? 0) + 40;
    const y = (box?.y ?? 0) + (box?.height ?? 0) / 2;
    const cdp = await context.newCDPSession(page);
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y }] });
    await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x, y: y - 30 }] });
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await expect(target).not.toHaveCSS("z-index", "10");
    await expectOrder(page, ["Rotate the certs", "Call the bank", "Water the plants"]);
    await expect(page.getByRole("status").filter({ hasText: "Moved" })).toHaveCount(0);
  });
});
