// The app shell at each width (design/app-ux.md, Layout): the navigation the width calls for, and no page wider
// than the screen.
import { addItem, expect, goTo, HEAP_URL, row, test, type AppPage } from "./fixtures";

// Account last: it belongs to no heap, so it has no heap views to go on from.
const pages: AppPage[] = ["Closed", "Activity", "Tags", "Parameters", "Open", "Account"];

test("every page fits the screen, with the navigation for its width", async ({ page }) => {
  await addItem(page, "Write up the findings from the customer interviews for the whole team", { effort: "L" });
  const width = page.viewportSize()?.width ?? 0;
  const views = page.getByRole("navigation", { name: "Heap views" });
  await expect(views.getByRole("link", { name: "Open", exact: true })).toBeVisible();

  const box = await views.boundingBox();
  if (width < 640) {
    // The tabs at the bottom, with the heap name at the top to switch heap.
    expect(box && box.y + box.height).toBeCloseTo(page.viewportSize()?.height ?? 0, 0);
    await expect(page.getByRole("button", { name: /^Switch heap/ })).toBeVisible();
  } else if (width < 1024) {
    // The rail.
    expect(box?.width).toBeLessThanOrEqual(80);
    await expect(page.getByRole("button", { name: /^Switch heap/ })).toBeVisible();
  } else {
    // The sidebar, with the heap and its team on the button that switches heap.
    await expect(page.getByRole("button", { name: /^Switch heap: .+ · .+$/ })).toBeVisible();
    // Open says how many items the heap has open.
    await expect(views.getByRole("link", { name: "Open", exact: true })).toContainText("1");
    // The content sits next to the sidebar, not across the screen from it.
    const sidebar = await page.getByRole("complementary", { name: "Heaps and views" }).boundingBox();
    const main = await page.getByRole("main").boundingBox();
    if (!sidebar || !main) throw new Error("the sidebar or the content is not laid out");
    expect(main.x - (sidebar.x + sidebar.width)).toBeLessThanOrEqual(64);
  }

  for (const name of pages) {
    await goTo(page, name);
    await expect(page.getByRole("main")).toBeVisible();
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(overflow, `${name} scrolls sideways`).toBeLessThanOrEqual(0);
  }
});

test("the sidebar's Open count follows adding an item and closing one", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) < 1024, "Only the sidebar shows the count.");

  const open = page.getByRole("navigation", { name: "Heap views" }).getByRole("link", { name: "Open", exact: true });
  await expect(open).toHaveText(/^Open\s*0$/);
  await addItem(page, "Water the plants");
  await addItem(page, "Feed the cat");
  await expect(open).toHaveText(/^Open\s*2$/);
  await row(page, "Feed the cat").getByRole("button", { name: "Mark Feed the cat done" }).click();
  await expect(row(page, "Feed the cat")).toHaveCount(0);
  await expect(open).toHaveText(/^Open\s*1$/);
});

test("the heap switcher opens another heap", async ({ page }) => {
  await page.getByRole("button", { name: /^Switch heap/ }).click();
  const heaps = page.getByRole("dialog", { name: "Heaps" });
  // The heap on screen is checked.
  await expect(heaps.locator("a[aria-current=page]")).toHaveCount(1);
  await heaps.getByRole("link").first().click();
  await expect(page).toHaveURL(HEAP_URL);
  await expect(heaps).toHaveCount(0);
});

test("the heap switcher starts a new heap in the heap's team", async ({ page }) => {
  await page.getByRole("button", { name: /^Switch heap/ }).click();
  const heaps = page.getByRole("dialog", { name: "Heaps" });
  await heaps.getByRole("button", { name: /^New heap in / }).click();
  await heaps.getByLabel(/^Name of the new heap in /).fill("Garden");
  await heaps.getByRole("button", { name: "Add heap" }).click();
  await expect(heaps).toHaveCount(0);
  await expect(page).toHaveURL(/\/heaps\/garden$/);
  await expect(page.getByRole("button", { name: "Switch heap: Garden" })).toBeVisible();
});

test("on a phone, More opens above the tabs, and + adds an item from any of the heap's pages", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) >= 640, "Only a phone has the tabs.");

  const tabs = page.getByRole("navigation", { name: "Heap views" });
  await tabs.getByRole("button", { name: "More" }).click();
  const more = page.getByRole("dialog", { name: "More" });
  await expect(more.getByRole("link", { name: "Tags", exact: true })).toBeVisible();
  const tabsBox = await tabs.boundingBox();
  const sheet = await more.boundingBox();
  if (!tabsBox || !sheet) throw new Error("the tabs or More are not laid out");
  expect(sheet.y + sheet.height).toBeLessThanOrEqual(tabsBox.y);
  await page.keyboard.press("Escape");
  await expect(more).toHaveCount(0);

  await goTo(page, "Closed");
  await addItem(page, "Water the plants", {}, { listed: false });
  await goTo(page, "Open");
  await expect(row(page, "Water the plants")).toBeVisible();
});
