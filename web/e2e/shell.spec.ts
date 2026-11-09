// The app shell at each width (design/ux.md, Layout): the navigation the width calls for, and no page wider
// than the screen.
import type { Locator, Page } from "@playwright/test";

import { addHeaps, addItem, createTeam, expect, goTo, HEAP_URL, row, test, type AppPage } from "./fixtures";

// Account last: it belongs to no heap, so it has no heap views to go on from.
const pages: AppPage[] = ["Closed", "Activity", "Settings", "Tags", "Parameters", "Open", "Account"];

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

/** Opens the heap switcher from whichever button the width shows, and returns its panel. */
async function openSwitcher(page: Page) {
  await page.getByRole("button", { name: /^Switch heap/ }).click();
  const heaps = page.getByRole("dialog", { name: "Heaps" });
  await expect(heaps).toBeVisible();
  return heaps;
}

/** Heaps that, with the one the test starts on, make the list long enough (8) for search and Recent. */
const MORE_HEAPS = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf"];

const isNarrow = (page: Page) => (page.viewportSize()?.width ?? 0) < 640;

test("the heap switcher opens another heap", async ({ page, localBuild }) => {
  await addHeaps(page, ["Garden"], localBuild);
  const startUrl = page.url();
  const heaps = await openSwitcher(page);
  // The heap on screen is checked.
  await expect(heaps.getByRole("menuitem", { name: /, current$/ })).toHaveCount(1);
  await heaps.getByRole("menuitem", { name: "Garden" }).click();
  await expect(page).toHaveURL(/\/heaps\/garden$/);
  await expect(page).not.toHaveURL(startUrl);
  await expect(heaps).toHaveCount(0);
});

test("the heap switcher starts a new heap in the heap's team", async ({ page }) => {
  const heaps = await openSwitcher(page);
  await heaps.getByRole("button", { name: /^New heap in / }).click();
  await heaps.getByLabel(/^Name of the new heap in /).fill("Garden");
  await heaps.getByRole("button", { name: "Add heap" }).click();
  await expect(heaps).toHaveCount(0);
  await expect(page).toHaveURL(/\/heaps\/garden$/);
  await expect(page.getByRole("button", { name: "Switch heap: Garden" })).toBeVisible();
});

test("a short list of heaps has no search and no Recent", async ({ page, localBuild }) => {
  await addHeaps(page, MORE_HEAPS.slice(0, 5), localBuild);
  const heaps = await openSwitcher(page);
  await expect(heaps.getByRole("menuitem", { name: "Alpha" })).toBeVisible();
  await expect(heaps.getByLabel("Find a heap")).toHaveCount(0);
  await expect(heaps.getByText("Recent", { exact: true })).toHaveCount(0);
});

test("a long list of heaps is searched as you type, and Enter opens the first match", async ({ page, localBuild }) => {
  await addHeaps(page, ["Q4 planning", "Meal planning", ...MORE_HEAPS.slice(0, 5)], localBuild);
  const heaps = await openSwitcher(page);
  const field = heaps.getByLabel("Find a heap");
  await expect(field).toBeVisible();
  await field.fill("plan");
  await expect(heaps.getByRole("menuitem")).toHaveText([/Meal planning/, /Q4 planning/, /^New heap “plan” in /]);
  await expect(heaps.getByRole("status")).toHaveText("2 heaps");
  await expect(heaps.locator("mark").first()).toHaveText("plan");
  await field.press("Enter");
  await expect(page).toHaveURL(/\/heaps\/meal-planning$/);
  await expect(heaps).toHaveCount(0);
});

test("searching for a heap that isn't there offers to make it, named as typed", async ({ page, localBuild }) => {
  await addHeaps(page, MORE_HEAPS, localBuild);
  const heaps = await openSwitcher(page);
  await heaps.getByLabel("Find a heap").fill("Roadmap");
  await expect(heaps.getByText("No heap matches “Roadmap”")).toBeVisible();
  await expect(heaps.getByText(/^Searched all 8 heaps/)).toBeVisible();
  await heaps.getByRole("menuitem", { name: /^New heap “Roadmap” in / }).click();
  const name = heaps.getByLabel(/^Name of the new heap in /);
  await expect(name).toHaveValue("Roadmap");
  await expect(name).toBeFocused();
  await heaps.getByRole("button", { name: "Add heap" }).click();
  await expect(page).toHaveURL(/\/heaps\/roadmap$/);
  await expect(heaps).toHaveCount(0);
});

test("the arrow keys move through the heaps from the search field", async ({ page, localBuild }) => {
  test.skip(isNarrow(page), "A phone has no keyboard.");
  await addHeaps(page, MORE_HEAPS, localBuild);
  const start = page.url();
  const heaps = await openSwitcher(page);
  await expect(heaps.getByLabel("Find a heap")).toBeFocused();
  // The first row is the first heap by name, the one to start on is somewhere after it.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(heaps).toHaveCount(0);
  await expect(page).toHaveURL(HEAP_URL);
  expect(page.url()).not.toBe(start);
});

test("Escape clears the search first and then closes the switcher, with focus back on its button", async ({
  page,
  localBuild,
}) => {
  test.skip(isNarrow(page), "A phone has no keyboard.");
  await addHeaps(page, MORE_HEAPS, localBuild);
  const opener = page.getByRole("button", { name: /^Switch heap/ });
  const heaps = await openSwitcher(page);
  const field = heaps.getByLabel("Find a heap");
  await field.fill("alp");
  await page.keyboard.press("Escape");
  await expect(heaps).toBeVisible();
  await expect(field).toHaveValue("");
  await page.keyboard.press("Escape");
  await expect(heaps).toHaveCount(0);
  await expect(opener).toBeFocused();
});

test("archived heaps are folded away, and found by search", async ({ page, localBuild }) => {
  await addHeaps(page, ["Old news"], localBuild);
  const start = new URL(page.url()).pathname;
  await page.goto(start.replace(/[^/]+$/, "old-news"));
  await expect(page.getByRole("button", { name: /^Switch heap: Old news/ })).toBeVisible();
  await goTo(page, "Settings");
  await page.getByRole("button", { name: "Archive heap" }).click();
  await expect(page.getByRole("status").filter({ hasText: "This heap is archived and read-only." })).toBeVisible();
  await page.goto(start);

  let heaps = await openSwitcher(page);
  await expect(heaps.getByRole("menuitem", { name: "Old news" })).toHaveCount(0);
  await heaps.getByRole("menuitem", { name: "1 archived" }).click();
  await expect(heaps.getByRole("menuitem", { name: "Old news (archived)" })).toBeVisible();
  await expect(heaps.getByRole("menuitem", { name: "1 archived" })).toHaveCount(0);

  // Searching finds it, tagged, once the list is long.
  await page.keyboard.press("Escape");
  await addHeaps(page, MORE_HEAPS, localBuild);
  heaps = await openSwitcher(page);
  await heaps.getByLabel("Find a heap").fill("old");
  const found = heaps.getByRole("menuitem", { name: /Old news/ });
  await expect(found).toContainText("archived");
  await expect(heaps.getByRole("status")).toHaveText("1 heap");
});

test("Recent lists the heaps opened last, without the one on screen", async ({ page, localBuild }) => {
  await addHeaps(page, MORE_HEAPS, localBuild);
  const start = new URL(page.url()).pathname;
  await page.goto(start.replace(/[^/]+$/, "alpha"));
  await expect(page.getByRole("button", { name: /^Switch heap: Alpha/ })).toBeVisible();
  await page.goto(start.replace(/[^/]+$/, "bravo"));
  await expect(page.getByRole("button", { name: /^Switch heap: Bravo/ })).toBeVisible();

  const heaps = await openSwitcher(page);
  const recent = heaps.getByRole("group", { name: "Recent" });
  await expect(recent.getByRole("menuitem", { name: "Alpha" })).toBeVisible();
  await expect(recent.getByRole("menuitem", { name: "Bravo" })).toHaveCount(0);
  if (isNarrow(page)) {
    // Chips at least 44 px tall.
    const box = await recent.getByRole("menuitem", { name: "Alpha" }).boundingBox();
    expect(box?.height).toBeGreaterThanOrEqual(44);
  }
});

test("a team folds in the switcher, and stays folded", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build has one team, so there is nothing to fold.");
  await addHeaps(page, MORE_HEAPS);
  await createTeam(page, "Acme");

  let heaps = await openSwitcher(page);
  const personal = heaps.getByRole("menuitem", { name: /^Tester's team/ });
  await expect(heaps.getByRole("menuitem", { name: "Alpha" })).toBeVisible();
  await personal.click();
  await expect(heaps.getByRole("menuitem", { name: "Alpha" })).toHaveCount(0);

  await page.keyboard.press("Escape");
  await expect(heaps).toHaveCount(0);
  heaps = await openSwitcher(page);
  await expect(heaps.getByRole("menuitem", { name: "Alpha" })).toHaveCount(0);
  await heaps.getByRole("menuitem", { name: /^Tester's team/ }).click();
  await expect(heaps.getByRole("menuitem", { name: "Alpha" })).toBeVisible();
});

test("on a phone, More opens above the tabs, and + adds an item from any of the heap's pages", async ({ page }) => {
  test.skip((page.viewportSize()?.width ?? 0) >= 640, "Only a phone has the tabs.");

  const tabs = page.getByRole("navigation", { name: "Heap views" });
  await tabs.getByRole("button", { name: "More" }).click();
  const more = page.getByRole("dialog", { name: "More" });
  await expect(more.getByRole("link", { name: "Activity", exact: true })).toBeVisible();
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

test("Sign out is on the Account page, not in the navigation, and only on the server", async ({ page, localBuild }) => {
  const signOut = page.getByRole("button", { name: "Sign out", exact: true });
  await expect(page.getByRole("navigation", { name: "Heap views" })).toBeVisible();
  await expect(signOut).toHaveCount(0);
  if ((page.viewportSize()?.width ?? 0) < 640) {
    await page.getByRole("navigation", { name: "Heap views" }).getByRole("button", { name: "More" }).click();
    const more = page.getByRole("navigation", { name: "More" });
    await expect(more.getByRole("link").last()).toHaveText(/^Account/);
    await expect(signOut).toHaveCount(0);
    await page.keyboard.press("Escape");
  }

  await goTo(page, "Account");
  await expect(page.getByRole("heading", { name: "Account", exact: true })).toBeVisible();
  await expect(signOut).toHaveCount(localBuild ? 0 : 1);
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
  // On <html> itself too, so the browser's own canvas is dark before the CSS loads.
  await expect(html).toHaveCSS("color-scheme", "dark");
  expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe("dark");
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
  await expect(html).not.toHaveAttribute("style", /color-scheme/);
  expect(await canvasOf()).toBe(dark);
  await page.emulateMedia({ colorScheme: "light" });
  expect(await canvasOf()).toBe(light);
});

/** The bar the heap's views are in at this width: the phone's tabs, or the sidebar or rail. */
function viewsBar(page: Page) {
  return isPhone(page)
    ? page.getByRole("navigation", { name: "Heap views" })
    : page.getByRole("complementary", { name: "Heaps and views" });
}

function isPhone(page: Page) {
  return (page.viewportSize()?.width ?? 0) < 640;
}

/** `entry` is the active one, and its fill shows against the bar it's in. */
async function expectToStandOut(entry: Locator, bar: Locator) {
  const backgroundOf = (target: Locator) => target.evaluate((element) => getComputedStyle(element).backgroundColor);
  await expect(entry).toHaveAttribute("data-status", "active");
  expect(await backgroundOf(entry)).not.toBe(await backgroundOf(bar));
}

test("the active navigation entry stands out from its bar in either theme", async ({ page }) => {
  const bar = viewsBar(page);
  for (const colorScheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme });
    await expectToStandOut(bar.getByRole("link", { name: "Open", exact: true }), bar);
  }
});

test("on a phone, More shows as active on the pages it holds", async ({ page }) => {
  test.skip(!isPhone(page), "Only the phone's tabs have More.");
  const bar = viewsBar(page);
  await goTo(page, "Activity");
  const more = bar.getByRole("button", { name: "More" });
  await expect(more).toHaveAttribute("aria-current", "true");
  await expectToStandOut(more, bar);
  await goTo(page, "Closed");
  await expect(more).not.toHaveAttribute("data-status");
});

test("the heap's team shows in its title and links from the navigation", async ({ page, localBuild }) => {
  test.skip(localBuild, "The local build has one personal team.");
  await createTeam(page, "Acme");
  await page.getByRole("button", { name: "+ New heap" }).click();
  await page.getByLabel("New heap name").fill("Launch");
  await page.getByRole("button", { name: "Add heap" }).click();
  await page
    .getByRole("region", { name: "Heaps" })
    .getByRole("link", { name: /Launch/ })
    .click();
  await expect(page).toHaveURL(/\/heaps\/launch$/);

  // The title names the team; on a phone the heap switcher at the top carries it and the heading is for readers.
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Launch · Acme");
  if (isPhone(page)) {
    await expect(page.getByRole("button", { name: "Switch heap: Launch · Acme" })).toBeVisible();
    await page.getByRole("navigation", { name: "Heap views" }).getByRole("button", { name: "More" }).click();
    await page.getByRole("navigation", { name: "More" }).getByRole("link", { name: "Acme Team" }).click();
  } else {
    // Above Teams in the bottom group, and active only on the team's own page.
    const bar = page.getByRole("complementary", { name: "Heaps and views" });
    const team = bar.getByRole("link", { name: "Acme", exact: true });
    const teams = bar.getByRole("link", { name: "Teams", exact: true });
    const [teamBox, teamsBox] = [await team.boundingBox(), await teams.boundingBox()];
    if (!teamBox || !teamsBox) throw new Error("the team or Teams entry is not laid out");
    expect(teamBox.y).toBeLessThan(teamsBox.y);
    await expect(team).not.toHaveAttribute("aria-current", "page");
    await team.click();
    await expect(team).toHaveAttribute("aria-current", "page");
  }
  await expect(page.getByRole("heading", { name: "Acme", level: 1 })).toBeVisible();
  await expect(page).toHaveURL(/\/teams\/acme(-\d+)?$/);
});

test("the personal team reads as the user's team, and its heap's title is the heap name alone", async ({
  page,
  localBuild,
}) => {
  test.skip(localBuild, "The local build has no Teams list.");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(/^[^·]+$/);
  // The sidebar's button names the team on every heap, being the switcher; the phone's top bar doesn't for this one.
  if (isPhone(page)) await expect(page.getByRole("button", { name: /^Switch heap/ })).toHaveAccessibleName(/^[^·]+$/);

  // Everywhere the app names a team, the personal one is "Tester's team"; the sidebar's entry opens it.
  if (isPhone(page)) {
    await page.goto("/teams");
    await page.getByRole("link", { name: "Open Tester's team" }).click();
  } else {
    await page
      .getByRole("complementary", { name: "Heaps and views" })
      .getByRole("link", { name: "Tester's team", exact: true })
      .click();
  }
  await expect(page.getByRole("heading", { name: "Tester's team", level: 1 })).toBeVisible();
  // Its stored name isn't shown, so there's nothing to rename.
  await page.getByRole("button", { name: "More for Tester's team" }).click();
  await expect(page.getByRole("menuitem", { name: "Rename" })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "Activity" })).toBeVisible();
  await page.keyboard.press("Escape");

  await page.goto("/teams");
  await expect(page.getByRole("link", { name: "Open Tester's team" })).toBeVisible();
});

test("alone, as in the local build, the personal team keeps its own name", async ({ page, localBuild }) => {
  test.skip(!localBuild, "Only the local build has one team and no Teams list.");
  // There is no list of teams to tell it from, so "<name>'s team" would only read as "Me's team".
  if (isPhone(page)) {
    await page.getByRole("button", { name: "More" }).click();
    await page.getByRole("link", { name: "My heaps" }).click();
  } else {
    await page
      .getByRole("complementary", { name: "Heaps and views" })
      .getByRole("link", { name: "My heaps", exact: true })
      .click();
  }
  await expect(page.getByRole("heading", { name: "My heaps", level: 1 })).toBeVisible();
});
