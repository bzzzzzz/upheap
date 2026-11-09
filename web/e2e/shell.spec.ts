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
  const heaps = page.getByRole("dialog", { name: "Heaps" });
  // The heap on screen is checked.
  await expect(heaps.locator("a[aria-current=page]")).toHaveCount(1);
  await heaps.getByRole("link").first().click();
  await expect(page).toHaveURL(HEAP_URL);
  await expect(heaps).toHaveCount(0);
});

test("the heap switcher starts a new heap in the heap's team", async ({ page }) => {
  const width = page.viewportSize()?.width ?? 0;
  test.skip(width >= 1024, "A wide screen lists the heaps in the sidebar instead.");

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

test("the theme picked on Account holds over a reload and in other tabs, and System follows the device", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  const html = page.locator("html");
  // The canvas token, as the page's background shows it.
  const canvasOf = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const light = "rgb(250, 250, 249)";
  const dark = "rgb(12, 10, 9)";
  await goTo(page, "Account");
  const theme = page.getByRole("radiogroup", { name: "Theme" });
  await expect(theme.getByRole("radio", { name: "System" })).toBeChecked();
  await expect(html).not.toHaveAttribute("data-theme");
  expect(await canvasOf()).toBe(light);
  const other = await page.context().newPage();
  await other.emulateMedia({ colorScheme: "light" });
  await other.goto(page.url());

  await theme.getByRole("radio", { name: "Dark" }).click();
  await expect(html).toHaveAttribute("data-theme", "dark");
  expect(await canvasOf()).toBe(dark);
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme)).toBe("dark");
  // Pressing the picked theme again keeps it: there's always one.
  await theme.getByRole("radio", { name: "Dark" }).click();
  await expect(theme.getByRole("radio", { name: "Dark" })).toBeChecked();
  await expect(other.locator("html")).toHaveAttribute("data-theme", "dark");
  await other.close();

  // Dark before the app runs, so a reload doesn't flash light first.
  await page.route("**/assets/*.js", (route) => route.abort());
  await page.reload();
  await expect(html).toHaveAttribute("data-theme", "dark");
  expect(await canvasOf()).toBe(dark);
  await page.unrouteAll();
  await page.reload();
  await expect(theme.getByRole("radio", { name: "Dark" })).toBeChecked();

  // Light holds on a dark device, and System follows the device again.
  await page.emulateMedia({ colorScheme: "dark" });
  await theme.getByRole("radio", { name: "Light" }).click();
  await expect(html).toHaveAttribute("data-theme", "light");
  expect(await canvasOf()).toBe(light);
  await theme.getByRole("radio", { name: "System" }).click();
  await expect(html).not.toHaveAttribute("data-theme");
  expect(await canvasOf()).toBe(dark);
  await page.emulateMedia({ colorScheme: "light" });
  expect(await canvasOf()).toBe(light);
});
