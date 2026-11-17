// The All items page (design/ux.md, All items): every open item of the person's heaps in one list, at each width.
// Server build only, as it is the list of the account's heaps.
import type { Page } from "@playwright/test";

import { addItem, closeItem, expect, isPhone, itemPane, markDoneFromList, openItem, row, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

type Need = "idea" | "expected" | "critical";

/** Adds an item to the heap at `heapPath` through the API. */
async function seed(page: Page, heapPath: string, title: string, need: Need = "expected") {
  const created = await page.request.post(`/api/v1${heapPath}/items`, { headers, data: { title, need } });
  expect(created.status()).toBe(201);
}

/** Makes a team "Acme" with a heap "Launch" and returns the heap's path. */
async function makeLaunchHeap(page: Page) {
  const made = await page.request.post("/api/v1/teams", {
    headers,
    data: { name: "Acme", slug: `t-${crypto.randomUUID().slice(0, 8)}` },
  });
  expect(made.status()).toBe(201);
  const team = (await made.json()).slug;
  const created = await page.request.post(`/api/v1/teams/${team}/heaps`, { headers, data: { name: "Launch" } });
  expect(created.status()).toBe(201);
  return `/teams/${team}/heaps/${(await created.json()).slug}`;
}

/** The name of the heap at `heapPath`. */
async function heapName(page: Page, heapPath: string): Promise<string> {
  return (await (await page.request.get(`/api/v1${heapPath}`)).json()).name;
}

/** The label that says which heap a row is in, as the width shows it. */
const heapLabel = (rowOfItem: ReturnType<typeof row>, label: string) =>
  rowOfItem.getByText(label).locator("visible=true");

test("lists the items of every heap, each with its heap and team", async ({ page, heapPath }) => {
  const launch = await makeLaunchHeap(page);
  const personal = await heapName(page, heapPath);
  await seed(page, heapPath, "Rotate the certs", "critical");
  await seed(page, heapPath, "Water the plants", "idea");
  await seed(page, launch, "Plan the launch", "expected");

  await page.goto("/all");
  await expect(page.getByRole("heading", { name: "All items · 3", level: 1 })).toBeVisible();
  await expect(page.getByText("All your heaps, most important first.")).toBeVisible();
  await expect(row(page, "Rotate the certs")).toBeVisible();
  await expect(row(page, "Plan the launch")).toBeVisible();
  await expect(row(page, "Water the plants")).toBeVisible();

  // The team's name shows because the person is in two; a personal heap names none.
  await expect(heapLabel(row(page, "Plan the launch"), "Launch · Acme")).toBeVisible();
  await expect(heapLabel(row(page, "Rotate the certs"), personal)).toBeVisible();
  await expect(row(page, "Rotate the certs")).not.toContainText("Tester's team");
});

test("an item opens over the list from its own heap, in the address, and survives a reload", async ({
  page,
  heapPath,
}) => {
  const launch = await makeLaunchHeap(page);
  await seed(page, heapPath, "Rotate the certs");
  await seed(page, launch, "Plan the launch");
  await page.goto("/all");

  const link = row(page, "Plan the launch").getByRole("link", { name: /^Plan the launch/ });
  await link.click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Plan the launch");
  await expect(page).toHaveURL((url) => {
    const [team, heap, key] = url.searchParams.get("item")?.split("/") ?? [];
    return (
      url.pathname.endsWith("/all") && `/teams/${team}/heaps/${heap}` === launch && /^[0-9a-z]{5}$/.test(key ?? "")
    );
  });
  // The shell stays where it was: nothing switched heap.
  await expect(page.getByRole("heading", { name: /^All items/, level: 1 })).toBeVisible();

  await page.reload();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Plan the launch");

  await closeItem(page);
  await expect(page).toHaveURL((url) => url.pathname.endsWith("/all") && !url.searchParams.has("item"));
  await expect(link).toBeFocused();
});

test("an item value that is not team/heap/key opens nothing", async ({ page, heapPath }) => {
  await seed(page, heapPath, "Rotate the certs");
  await page.goto("/all?item=nonsense");
  await expect(row(page, "Rotate the certs")).toBeVisible();
  await expect(itemPane(page)).toHaveCount(0);
});

test("a need filter narrows the list, shows its chip and goes in the address", async ({ page, heapPath }) => {
  const launch = await makeLaunchHeap(page);
  await seed(page, heapPath, "Rotate the certs", "critical");
  await seed(page, launch, "Plan the launch", "idea");
  await page.goto("/all");
  await expect(row(page, "Plan the launch")).toBeVisible();

  await page.getByRole("button", { name: /^Filters/ }).click();
  const panel = page.getByRole("dialog", { name: "Filters" });
  // No sort and no tags: the list across heaps has neither.
  await expect(panel.getByText("Sort by")).toHaveCount(0);
  await expect(panel.getByRole("group", { name: "Tags (any)" })).toHaveCount(0);
  await panel.getByRole("group", { name: "Need" }).getByRole("button", { name: "Critical", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);

  await expect(page).toHaveURL(/[?&]need=critical/);
  await expect(row(page, "Plan the launch")).toHaveCount(0);
  await expect(row(page, "Rotate the certs")).toBeVisible();
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on.getByRole("button", { name: "Remove filter Need: Critical" })).toBeVisible();

  await on.getByRole("button", { name: "Reset all" }).click();
  await expect(on).toHaveCount(0);
  await expect(page).not.toHaveURL(/need=/);
  await expect(row(page, "Plan the launch")).toBeVisible();
});

test("a team choice limits the list to that team, in the address, and Back returns to it", async ({
  page,
  heapPath,
}) => {
  const launch = await makeLaunchHeap(page);
  await seed(page, heapPath, "Rotate the certs", "critical");
  await seed(page, launch, "Plan the launch", "idea");
  await page.goto("/all");
  await expect(row(page, "Plan the launch")).toBeVisible();
  await expect(row(page, "Rotate the certs")).toBeVisible();

  await page.getByRole("button", { name: /^Filters/ }).click();
  const panel = page.getByRole("dialog", { name: "Filters" });
  await panel.getByRole("radiogroup", { name: "Team" }).getByText("Acme", { exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);

  await expect(page).toHaveURL(/[?&]team=/);
  await expect(row(page, "Rotate the certs")).toHaveCount(0);
  await expect(row(page, "Plan the launch")).toBeVisible();
  await expect(page.getByRole("heading", { name: "All items · 1", level: 1 })).toBeVisible();
  await expect(page.getByText("Acme's heaps, most important first.")).toBeVisible();
  // The heap's name alone: the team is the page's.
  await expect(row(page, "Plan the launch")).not.toContainText("Acme");
  const on = page.getByRole("group", { name: "Filters on" });
  await expect(on.getByRole("button", { name: "Remove team Acme" })).toBeVisible();

  await on.getByRole("button", { name: "Remove team Acme" }).click();
  await expect(page).not.toHaveURL(/team=/);
  await expect(row(page, "Rotate the certs")).toBeVisible();
  await expect(row(page, "Plan the launch")).toBeVisible();

  await page.goBack();
  await expect(page).toHaveURL(/[?&]team=/);
  await expect(row(page, "Rotate the certs")).toHaveCount(0);
  await expect(row(page, "Plan the launch")).toBeVisible();
});

test("the heap switcher's All <team> items row opens the team's list", async ({ page, heapPath }) => {
  const launch = await makeLaunchHeap(page);
  await seed(page, heapPath, "Rotate the certs");
  await seed(page, launch, "Plan the launch");
  await page.goto("/all");
  await expect(row(page, "Rotate the certs")).toBeVisible();

  await page.getByRole("button", { name: /^Switch heap/ }).click();
  await page.getByRole("menuitem", { name: /^All Acme items/ }).click();

  await expect(page).toHaveURL(/\/all\?team=/);
  await expect(row(page, "Rotate the certs")).toHaveCount(0);
  await expect(row(page, "Plan the launch")).toBeVisible();
});

test("a team the person is not in says so", async ({ page, heapPath }) => {
  await seed(page, heapPath, "Rotate the certs");
  await page.goto("/all?team=nobody-here");
  await expect(page.getByRole("alert")).toHaveText("You're not in this team.");
  await expect(row(page, "Rotate the certs")).toHaveCount(0);
});

test("Done on a row leaves the list and offers to undo", async ({ page, heapPath }) => {
  const launch = await makeLaunchHeap(page);
  await seed(page, heapPath, "Rotate the certs");
  await seed(page, launch, "Plan the launch");
  await page.goto("/all");
  await expect(page.getByRole("heading", { name: "All items · 2", level: 1 })).toBeVisible();

  await markDoneFromList(page, "Plan the launch");
  const toast = page.getByRole("status").filter({ hasText: "“Plan the launch” is done." });
  await expect(toast).toBeVisible();
  await expect(row(page, "Plan the launch")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "All items · 1", level: 1 })).toBeVisible();
  // Nothing is offered to start next: this page has no heap's list to say which.
  await expect(page.getByRole("button", { name: /^Start next/ })).toHaveCount(0);

  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(row(page, "Plan the launch")).toBeVisible();
});

test("the navigation leads to All items", async ({ page, heapPath }) => {
  await seed(page, heapPath, "Rotate the certs");
  await seed(page, heapPath, "Water the plants");
  await page.reload();

  if (isPhone(page)) {
    await page.getByRole("button", { name: /^Switch heap/ }).click();
    const entry = page.getByRole("menuitem", { name: /^All heaps/ });
    await expect(entry).toContainText("2");
    await entry.click();
  } else {
    const wide = (page.viewportSize()?.width ?? 0) >= 1024;
    const entry = page.getByRole("complementary", { name: "Heaps and views" }).getByRole("link", {
      name: wide ? "All items" : "All",
      exact: true,
    });
    // Only the sidebar shows the count.
    if (wide) await expect(entry).toContainText("2");
    await entry.click();
    await expect(entry).toHaveAttribute("data-status", "active");
  }
  await expect(page).toHaveURL(/\/all$/);
  await expect(page.getByRole("heading", { name: "All items · 2", level: 1 })).toBeVisible();
  // The heap's views stay: the shell keeps the heap opened last.
  await expect(
    page.getByRole("navigation", { name: "Heap views" }).getByRole("link", { name: "Open", exact: true }),
  ).toBeVisible();
});

test("with nothing open there is a line saying so", async ({ page }) => {
  await page.goto("/all");
  await expect(page.getByRole("heading", { name: "All items", level: 1, exact: true })).toBeVisible();
  await expect(page.getByText("Nothing open in your heaps.")).toBeVisible();
});

test("Add on All items names the heap, not a place, and Open it opens the item over All items", async ({
  page,
  heapPath,
}) => {
  const personal = await heapName(page, heapPath);
  await seed(page, heapPath, "Rotate the certs");
  await page.goto("/all");
  await expect(row(page, "Rotate the certs")).toBeVisible();

  await addItem(page, "Water the plants");
  // The item's place in its heap's list isn't its place here, so the toast has none.
  const toast = page.getByRole("status").filter({ hasText: `Added “Water the plants” to ${personal}.` });
  await expect(toast).toBeVisible();
  await toast.getByRole("button", { name: "Open it" }).click();

  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Water the plants");
  await expect(page).toHaveURL((url) => url.pathname.endsWith("/all") && url.searchParams.has("item"));
  await expect(page.getByRole("heading", { name: /^All items/, level: 1 })).toBeVisible();
});

test("the browser's Back from an item leaves the list where it was scrolled beside it", async ({ page, heapPath }) => {
  for (let i = 1; i <= 40; i++) await seed(page, heapPath, `Item ${String(i).padStart(2, "0")}`);
  await page.goto("/all");
  const scrollY = () => page.evaluate(() => window.scrollY);
  await row(page, "Item 30")
    .locator("[data-row-link]")
    .evaluate((link) => link.scrollIntoView({ block: "center" }));
  const opened = await scrollY();
  expect(opened).toBeGreaterThan(200);

  await openItem(page, "Item 30");
  await expect.poll(scrollY).toBe(opened);
  await page.evaluate(() => window.scrollBy(0, -100));
  await expect.poll(scrollY).toBe(opened - 100);
  await page.goBack();
  await expect(itemPane(page)).toHaveCount(0);
  // Beside the item, the list stays where it is now, as ✕ leaves it; on a phone, where the item covers the list, the
  // list goes back to where it was when the item opened.
  await expect.poll(scrollY).toBe(isPhone(page) ? opened : opened - 100);

  // Forward opens the item again, and the browser puts the list back where it was when the item was left.
  await page.goForward();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Item 30");
  await expect.poll(scrollY).toBe(opened - 100);
});

test("a view change with an item open starts the list at the top", async ({ page, heapPath }) => {
  for (let i = 1; i <= 40; i++) await seed(page, heapPath, `Item ${String(i).padStart(2, "0")}`);
  await page.goto("/all?need=expected");
  const scrollY = () => page.evaluate(() => window.scrollY);
  await row(page, "Item 30")
    .locator("[data-row-link]")
    .evaluate((link) => link.scrollIntoView({ block: "center" }));
  expect(await scrollY()).toBeGreaterThan(200);
  await openItem(page, "Item 30");

  // Without scrolling to the chip first, as a click would.
  await page
    .getByRole("group", { name: "Filters on" })
    .getByRole("button", { name: "Remove filter Need: Expected" })
    .dispatchEvent("click");
  await expect(page).not.toHaveURL(/need=/);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Item 30");
  await expect.poll(scrollY).toBe(0);
});
