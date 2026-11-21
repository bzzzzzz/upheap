// Team parameters (design/ux.md, Heap settings and Teams and Team): made on the Team page, chosen by each heap on its
// Parameters tab, weighted on its Ranking page, and set on items like any parameter.
import type { Page } from "@playwright/test";
import {
  addHeaps,
  addItem,
  closeItem,
  expect,
  expectOrder,
  goTo,
  itemPane,
  openItem,
  pick,
  row,
  test,
} from "./fixtures";

test("a team parameter is made on the Team page, used and weighted in one heap, and set on an item", async ({
  page,
  heapPath,
}) => {
  const team = /\/teams\/([^/]+)\//.exec(heapPath)?.[1] ?? "";
  await addHeaps(page, ["Beta"]);
  const heapB = `/teams/${team}/heaps/beta`;

  // The team parameter is made once, on the Team page. Its card only exists once the server has answered.
  await page.goto(`/teams/${team}`);
  const parameters = page.getByRole("region", { name: "Parameters", exact: true });
  await parameters.getByRole("button", { name: "New parameter" }).click();
  await parameters.getByLabel("New parameter name").fill("Tier");
  await parameters.getByLabel(/^Options of the new parameter/).fill("Gold\nSilver");
  await parameters.getByRole("button", { name: "Add parameter" }).click();
  await expect(parameters.getByRole("heading", { name: "Tier", exact: true })).toBeVisible();
  await expect(parameters).toContainText("Not used in any heap yet");
  await expect(parameters.getByLabel(/^Weight of parameter/)).toHaveCount(0);

  // The heap's Parameters tab offers it in the team's card, and Use puts it among the heap's own with a weight of 10.
  await page.goto(heapPath);
  await addItem(page, "Gold request");
  await addItem(page, "Silver request");
  await goTo(page, "Parameters");
  const card = page.getByRole("region", { name: /^1 parameter of the team / });
  await expect(card).toContainText("Tier");
  await card.getByRole("button", { name: "Use Tier" }).click();
  const used = page.getByRole("listitem").filter({ has: page.getByRole("heading", { name: /^Tier/ }) });
  await expect(used).toContainText("Weight 10");
  await expect(used.getByRole("heading", { name: /^Tier\s?, team parameter$/ })).toBeVisible();
  await expect(page.getByRole("region", { name: /^0 parameters of the team / })).toContainText(
    "has no other team parameters",
  );

  // Its weight is set with the heap's other weights, on the Ranking page, where it carries the team mark.
  await used.getByRole("link", { name: "Ranking page" }).click();
  await expect(page.getByText(", team parameter")).toHaveCount(1);
  await page.getByLabel("Tier weight, 0 to 100").fill("100");
  await page.getByRole("button", { name: "Save weights" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Weights saved." })).toBeVisible();

  // An item takes a value from it, like any parameter, and the weight ranks the heap's items.
  await goTo(page, "Open");
  await openItem(page, "Silver request");
  await pick(page, "Tier", "Silver");
  await expect(itemPane(page).getByRole("button", { name: "Tier: Silver", exact: true })).toBeVisible();
  await closeItem(page);
  await openItem(page, "Gold request");
  await pick(page, "Tier", "Gold");
  await expect(itemPane(page).getByRole("button", { name: "Tier: Gold", exact: true })).toBeVisible();
  await closeItem(page);
  await expect(row(page, "Gold request")).toContainText("Tier: Gold");
  await expectOrder(page, ["Gold request", "Silver request"]);

  // Another heap of the team doesn't use it until it chooses to.
  await page.goto(heapB);
  await goTo(page, "Parameters");
  await expect(page.getByText("No parameters yet.")).toBeVisible();
  await expect(page.getByRole("region", { name: /^1 parameter of the team / })).toContainText("Tier");

  // The Team page names the heap that uses it.
  await page.goto(`/teams/${team}`);
  await expect(parameters).toContainText(/Used in /);
});

/** In the heap at `path`, makes a parameter `name` with `options`, one per line. */
async function newParameter(page: Page, path: string, name: string, options: string) {
  await page.goto(path);
  await expect(page.getByRole("navigation", { name: "Heap views" })).toBeVisible();
  await goTo(page, "Parameters");
  await page.getByRole("button", { name: "New parameter" }).click();
  await page.getByLabel("New parameter name").fill(name);
  await page.getByLabel(/^Options of the new parameter/).fill(options);
  await page.getByRole("button", { name: "Add parameter" }).click();
  await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
}

test("a heap parameter is made a team parameter after the merge is confirmed, and both heaps' items keep their values", async ({
  page,
  heapPath,
}) => {
  const heapA = heapPath;
  const team = /\/teams\/([^/]+)\//.exec(heapA)?.[1] ?? "";
  await addHeaps(page, ["Beta"]);
  const heapB = `/teams/${team}/heaps/beta`;
  await newParameter(page, heapA, "Customer tier", "Gold\nSilver");
  await newParameter(page, heapB, "customer tier", "silver\nPlatinum");
  await goTo(page, "Open");
  await addItem(page, "Beta request");
  await openItem(page, "Beta request");
  await pick(page, "customer tier", "silver");
  await closeItem(page);

  // The ⋯ on the heap parameter opens the confirm, which lists both heaps' parameters and marks this heap.
  await page.goto(heapA);
  await expect(page.getByRole("navigation", { name: "Heap views" })).toBeVisible();
  await goTo(page, "Parameters");
  await page.getByRole("button", { name: "More for Customer tier" }).click();
  await page.getByRole("menuitem", { name: "Make a team parameter" }).click();
  const confirm = page.getByRole("alertdialog", { name: "Make “Customer tier” a team parameter?" });
  const rows = confirm.getByRole("listitem");
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: "this heap" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "adds Platinum" })).toHaveCount(1);
  await expect(confirm).toContainText("1 item in 2 heaps keep their values.");

  // The toast exists only once the server has merged them.
  await confirm.getByRole("button", { name: "Make team parameter" }).click();
  await expect(page.getByRole("status").filter({ hasText: "is a team parameter now" })).toContainText(
    "“Customer tier” is a team parameter now: 1 item in 2 heaps keep their values.",
  );
  await expect(page.getByRole("heading", { name: /^Customer tier\s?, team parameter$/ })).toBeVisible();

  // Beta's item has the team's option, and Beta lists the parameter as a team one.
  await page.goto(heapB);
  await expect(row(page, "Beta request")).toContainText("Customer tier: Silver");
  await goTo(page, "Parameters");
  await expect(page.getByRole("heading", { name: /^Customer tier\s?, team parameter$/ })).toBeVisible();
});
