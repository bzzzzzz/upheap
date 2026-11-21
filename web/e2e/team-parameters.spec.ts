// Team parameters (design/ux.md, Heap settings and Teams and Team): made on the Team page, chosen by each heap on its
// Parameters tab, weighted on its Ranking page, and set on items like any parameter.
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
