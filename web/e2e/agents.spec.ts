// The Agents page (ux.md, Agents): Your decision, the items an agent brought to a stable state that wait for the person,
// and the history of the runs that ended as reached. Nothing in the web app runs an agent, so the runs are made through
// the API, as a loop would: add, send, claim, reach. Server build only.
import type { Page } from "@playwright/test";

import { expect, isPhone, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

/** `team/heap`, as the queue names a heap. */
const refOf = (heapPath: string) =>
  heapPath
    .split("/")
    .filter((part) => part !== "" && part !== "teams" && part !== "heaps")
    .join("/");

/** Adds a note with a context, sends it to an agent and claims it. Returns the item's address and the run's ID. */
async function claimed(page: Page, heapPath: string, title: string) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title, need: "expected", description: "Why and how" },
  });
  expect(created.status()).toBe(201);
  const { key } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  expect((await page.request.post(`${item}/turn`, { headers, data: { reach: "one" } })).status()).toBe(201);
  const claim = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  expect(claim.status()).toBe(201);
  const { run } = await claim.json();
  return { item, run: run.id as string };
}

/** Has the agent reach the goal of the run's turn. */
async function reach(page: Page, run: string, result: string, links: string[] = [], stage?: string) {
  const reached = await page.request.post(`/api/v1/work/runs/${run}/reached`, {
    headers,
    data: { result, links, stage },
  });
  expect(reached.status()).toBe(200);
}

/** A note an agent brought to with context. */
async function seedReached(
  page: Page,
  heapPath: string,
  title: string,
  result = "Added the context",
  links?: string[],
) {
  const { run } = await claimed(page, heapPath, title);
  await reach(page, run, result, links);
}

/** The navigation entry, which counts nothing. */
const entry = (page: Page) => page.getByRole("link", { name: "Agents", exact: true });

async function openAgents(page: Page) {
  await page.reload();
  await entry(page).click();
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
}

test("with nothing finished, the page says how to start an agent and has no Your decision", async ({ page }) => {
  await openAgents(page);

  await expect(page.getByText("No agent has finished anything yet.")).toBeVisible();
  await expect(page.getByText("Start one in a terminal:")).toBeVisible();
  await expect(page.getByText("upheap work run", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Your decision/ })).toHaveCount(0);
});

test("an item an agent reached is under Your decision and in the history, with its result and pull request", async ({
  page,
  heapPath,
}) => {
  await seedReached(page, heapPath, "Rotate the certs", "Opened the change", [
    "https://github.com/acme/app/pull/7",
    "https://example.com/notes/rotation",
  ]);

  await openAgents(page);

  // Your decision: the count on the heading and not on the entry, the item, where it is and when it arrived.
  await expect(page.getByRole("button", { name: "Your decision · 1" })).toBeVisible();
  await expect(entry(page)).toHaveText("Agents");
  const decision = page.getByRole("region", { name: "Your decision" }).getByRole("listitem");
  await expect(decision).toHaveCount(1);
  await expect(decision).toContainText("Rotate the certs");
  await expect(decision).toContainText("note, with context");
  await expect(decision).toContainText("today");

  // The history: the day, the run, what it reached, the result and the links.
  const history = page.getByRole("region", { name: "History" });
  await expect(history.getByRole("heading", { name: "Today", level: 3 })).toBeVisible();
  const run = history.getByRole("listitem");
  await expect(run).toContainText("Rotate the certs");
  await expect(run).toContainText("reached: with context");
  await expect(run).toContainText("Opened the change");
  const pull = run.getByRole("link", { name: "↗ pull request" });
  await expect(pull).toHaveAttribute("href", "https://github.com/acme/app/pull/7");
  await expect(pull).toHaveAttribute("target", "_blank");
  await expect(run.getByRole("link", { name: "example.com/notes/rotation" })).toBeVisible();
  await expect(page.getByText("That's the last 30 days. Older runs are on their items.")).toBeVisible();

  // A title opens the item over the page.
  await decision.getByRole("link", { name: "Rotate the certs" }).click();
  await expect(page).toHaveURL(/\/items\//);
});

test("a run that went past its goal says it skipped the goal", async ({ page, heapPath }) => {
  const { item, run } = await claimed(page, heapPath, "Rotate the keys");
  const version = (await (await page.request.get(item)).json()).version;
  const inRun = { ...headers, "upheap-run": run, "if-match": String(version) };
  expect((await page.request.patch(item, { headers: inRun, data: { kind: "task" } })).status()).toBe(200);
  const step = await page.request.post(`${item}/steps`, {
    headers: { ...headers, "upheap-run": run },
    data: { title: "Order the keys" },
  });
  expect(step.status()).toBe(201);
  await reach(page, run, "Went straight on", [], "defined");

  await openAgents(page);

  await expect(page.getByRole("region", { name: "History" }).getByRole("listitem")).toContainText(
    "reached: defined (skipped with context)",
  );
  await expect(page.getByRole("region", { name: "Your decision" }).getByRole("listitem")).toContainText(
    "task, defined",
  );
});

test("more than five decisions show five, then Show N more and Show less", async ({ page, heapPath }) => {
  for (let count = 1; count <= 7; count++) await seedReached(page, heapPath, `Note ${count}`);
  await openAgents(page);
  const decisions = page.getByRole("region", { name: "Your decision" }).getByRole("listitem");

  await expect(page.getByRole("button", { name: "Your decision · 7" })).toBeVisible();
  await expect(decisions).toHaveCount(5);
  await page.getByRole("button", { name: "Show 2 more" }).click();
  await expect(decisions).toHaveCount(7);
  await page.getByRole("button", { name: "Show less" }).click();
  await expect(decisions).toHaveCount(5);
});

test("folding Your decision hides its rows, keeps the count and survives a reload", async ({ page, heapPath }) => {
  await seedReached(page, heapPath, "Rotate the certs");
  await openAgents(page);
  const heading = page.getByRole("button", { name: "Your decision · 1" });
  const decisions = page.getByRole("region", { name: "Your decision" }).getByRole("listitem");
  await expect(decisions).toHaveCount(1);

  await heading.click();

  await expect(decisions).toHaveCount(0);
  await expect(heading).toHaveAttribute("aria-expanded", "false");
  await page.reload();
  await expect(heading).toHaveAttribute("aria-expanded", "false");
  await expect(decisions).toHaveCount(0);
  await heading.click();
  await expect(decisions).toHaveCount(1);
});

test("on a phone the Agents tab is there, and Closed is in More", async ({ page }) => {
  test.skip(!isPhone(page), "Tabs are the phone's.");
  const tabs = page.getByRole("navigation", { name: "Heap views" });

  await expect(tabs.getByRole("link", { name: "Agents", exact: true })).toBeVisible();
  await expect(tabs.getByRole("link", { name: "Closed", exact: true })).toHaveCount(0);
  await tabs.getByRole("button", { name: "More" }).click();
  await expect(
    page.getByRole("navigation", { name: "More" }).getByRole("link", { name: "Closed", exact: true }),
  ).toBeVisible();
});

test("a phone's rows and buttons are at least 44 px tall", async ({ page, heapPath }) => {
  test.skip(!isPhone(page), "Touch sizes are the phone's.");
  for (let count = 1; count <= 6; count++) await seedReached(page, heapPath, `Note ${count}`);
  await openAgents(page);

  for (const control of [
    page.getByRole("button", { name: "Your decision · 6" }),
    page.getByRole("button", { name: "Show 1 more" }),
    page.getByRole("region", { name: "Your decision" }).getByRole("link").first(),
  ]) {
    const box = await control.boundingBox();
    expect(Math.round(box?.height ?? 0)).toBeGreaterThanOrEqual(44);
  }
});
