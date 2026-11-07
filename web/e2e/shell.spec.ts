// The app shell at each width (design/app-ux.md, Layout): the navigation the width calls for, and no page wider
// than the screen.
import { addItem, expect, goTo, test, type AppPage } from "./fixtures";

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
    // The sidebar, listing the heaps.
    await expect(page.getByRole("navigation", { name: "Heaps" }).getByRole("link")).not.toHaveCount(0);
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

test("the heap switcher opens another heap", async ({ page }) => {
  const width = page.viewportSize()?.width ?? 0;
  test.skip(width >= 1024, "A wide screen lists the heaps in the sidebar instead.");

  await page.getByRole("button", { name: /^Switch heap/ }).click();
  // The menu takes its name from the button.
  const heaps = page.getByRole("menu");
  await expect(heaps.getByRole("menuitem")).not.toHaveCount(0);
  await heaps.getByRole("menuitem").first().click();
  await expect(page).toHaveURL(/\/heaps\/[^/]+$/);
  await expect(heaps).toHaveCount(0);
});
