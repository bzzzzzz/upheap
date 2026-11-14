// Runs (ux.md, Waiting for me and The item): a turn whose runs keep failing is stuck, and waits for the person with
// Retry and Take back, on the Waiting for me page and in the item. Nothing in the web app runs an agent, so a stuck turn
// is made through the API, as a loop would: claim, then finish with a failure, three times. Server build only.
import type { Locator, Page } from "@playwright/test";

import { expect, isPhone, itemPane, openHistory, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

/** `team/heap`, as the queue names a heap. */
const refOf = (heapPath: string) =>
  heapPath
    .split("/")
    .filter((part) => part !== "" && part !== "teams" && part !== "heaps")
    .join("/");

/** Adds a note and sends it to an agent through the API. Returns its key, its ID and its API address. */
async function seed(page: Page, heapPath: string, title: string, need = "expected") {
  const created = await page.request.post(`/api/v1${heapPath}/items`, { headers, data: { title, need } });
  expect(created.status()).toBe(201);
  const { key, id } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  const sent = await page.request.post(`${item}/turn`, { headers, data: { reach: "one" } });
  expect(sent.status()).toBe(201);
  return { key, id, item };
}

/** Claims the next item of the heap and finishes the run as failed with `failure`. */
async function failOnce(page: Page, heapPath: string, failure: string) {
  const claimed = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  expect(claimed.status()).toBe(201);
  const { run } = await claimed.json();
  const finished = await page.request.post(`/api/v1/work/runs/${run.id}/finish`, { headers, data: { failure } });
  expect(finished.status()).toBe(200);
}

/** Adds a note, sends it to an agent, and fails three runs on it, so its turn is stuck. */
async function seedStuck(page: Page, heapPath: string, title: string, failure = "The tool crashed", need = "expected") {
  const item = await seed(page, heapPath, title, need);
  for (let run = 0; run < 3; run++) await failOnce(page, heapPath, failure);
  return item;
}

/** The navigation entry: "Waiting for me" on a wide screen, "Waiting" on a tablet and a phone, then its count. */
const entry = (page: Page) => page.getByRole("link", { name: /^Waiting( for me)?( \d+)?$/ });

/** The page's rows. */
const rows = (page: Page) => page.getByRole("main").getByRole("listitem");

/** Opens Waiting for me and waits for its heading. */
async function openWaiting(page: Page, count: number) {
  await page.reload();
  await entry(page).click();
  await expect(page.getByRole("heading", { name: `Waiting for me · ${count}`, level: 1 })).toBeVisible();
}

/** Expects a control to be at least 44 px tall, rounded: layout can land one a hair short, such as 43.99997. */
async function expectTouchSize(control: Locator) {
  const box = await control.boundingBox();
  expect(Math.round(box?.height ?? 0)).toBeGreaterThanOrEqual(44);
}

/** The agent chip in the status line, and the popover it opens. */
const chip = (page: Page): Locator => itemPane(page).getByRole("button", { name: /^Who has it: / });
const who = (page: Page): Locator => page.getByRole("dialog", { name: "Who has it", exact: true });
const waitsForYou = (page: Page): Locator => itemPane(page).getByRole("region", { name: "Waits for you", exact: true });

test("a stuck turn is a row in Waiting for me with its reason, and Retry puts it back at once", async ({
  page,
  heapPath,
}) => {
  const certs = await seedStuck(page, heapPath, "Rotate the certs", "The tool crashed");

  await openWaiting(page, 1);

  // The line, the reason and the two ways out; Retry is the primary one.
  const stuck = rows(page).first();
  await expect(stuck).toContainText("Stuck · Rotate the certs");
  await expect(stuck.getByRole("link", { name: "Rotate the certs" })).toBeVisible();
  await expect(stuck).toContainText("3 runs in a row failed: The tool crashed");
  const retry = stuck.getByRole("button", { name: /^Retry/ });
  const takeBack = stuck.getByRole("button", { name: /^Take back/ });
  await expect(retry).toBeVisible();
  await expect(takeBack).toBeVisible();
  if (isPhone(page)) {
    await expectTouchSize(retry);
    await expectTouchSize(takeBack);
  }
  await expect(entry(page)).toHaveAccessibleName(/ 1$/);

  await retry.click();

  // The row leaves, the count goes, a screen reader is told, and the focus lands on the heading (no next row).
  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByText("Nothing waits for you.")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Retried" })).toHaveText("Retried “Rotate the certs”.");
  await expect(page.getByRole("heading", { name: "Waiting for me", level: 1, exact: true })).toBeFocused();
  await expect(entry(page)).toHaveAccessibleName(/^Waiting( for me)?$/);

  // The server has it back in the queue, with no failed runs counted.
  const fetched = await (await page.request.get(certs.item)).json();
  expect(fetched.turn.stuck).toBe(false);
  expect(fetched.turn.failed_runs).toBe(0);
  const next = await (await page.request.get(`/api/v1/work/next?heaps=${refOf(heapPath)}`)).json();
  expect(next.item.key).toBe(certs.key);
});

test("Take back on a stuck row asks first, and the item is the person's after", async ({ page, heapPath }) => {
  const certs = await seedStuck(page, heapPath, "Rotate the certs");
  await openWaiting(page, 1);
  const stuck = rows(page).first();

  await stuck.getByRole("button", { name: /^Take back/ }).click();

  // It asks, in the row; Cancel gives the button back with the focus.
  const ask = stuck.getByRole("alertdialog");
  await expect(ask).toContainText("Take this back from the agent? It stops, and what it changed stays.");
  await expect(rows(page)).toHaveCount(1);
  await ask.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(ask).toHaveCount(0);
  await expect(stuck.getByRole("button", { name: /^Take back/ })).toBeFocused();

  await stuck.getByRole("button", { name: /^Take back/ }).click();
  await stuck.getByRole("alertdialog").getByRole("button", { name: "Take back", exact: true }).click();

  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByText("Nothing waits for you.")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Took back" })).toHaveText("Took back “Rotate the certs”.");
  const fetched = await (await page.request.get(certs.item)).json();
  expect(fetched.turn).toBeNull();
  expect(fetched.with_agent).toBe(false);
});

test("a long reason is cut to two lines, with Show all", async ({ page, heapPath }) => {
  const reason = Array.from({ length: 40 }, (_, line) => `Line ${line + 1} of the tool's output.`).join("\n");
  await seedStuck(page, heapPath, "Rotate the certs", reason);
  await openWaiting(page, 1);
  const stuck = rows(page).first();
  const text = stuck.locator("[data-stuck-reason]");

  const cut = (await text.boundingBox())?.height ?? 0;
  await stuck.getByRole("button", { name: "Show all", exact: true }).click();
  const full = (await text.boundingBox())?.height ?? 0;

  expect(full).toBeGreaterThan(cut * 3);
  await expect(text).toContainText("Line 40 of the tool's output.");
  await stuck.getByRole("button", { name: "Show less", exact: true }).click();
  expect((await text.boundingBox())?.height ?? 0).toBeLessThan(full);
});

test("a short reason has no Show all", async ({ page, heapPath }) => {
  await seedStuck(page, heapPath, "Rotate the certs", "Out of memory");
  await openWaiting(page, 1);

  await expect(rows(page).first()).toContainText("3 runs in a row failed: Out of memory");
  await expect(rows(page).first().getByRole("button", { name: "Show all" })).toHaveCount(0);
});

test("a reply draft in another row survives a stuck row leaving, and the focus goes to that row", async ({
  page,
  heapPath,
}) => {
  // The stuck item is the more important one, so its row is first.
  await seedStuck(page, heapPath, "Rotate the certs", "The tool crashed", "critical");
  const plan = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: "Plan the launch", need: "idea" },
  });
  const { key } = await plan.json();
  const asked = await page.request.post(`/api/v1${heapPath}/items/${key}/questions`, {
    headers,
    data: { kind: "question", text: "Which date?" },
  });
  expect(asked.status()).toBe(201);
  await openWaiting(page, 2);
  await expect(rows(page).first()).toContainText("Stuck · Rotate the certs");
  const draft = rows(page).nth(1).getByLabel("Answer");
  await draft.fill("A draft I haven't sent");

  await rows(page)
    .first()
    .getByRole("button", { name: /^Retry/ })
    .click();

  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "Waiting for me · 1", level: 1 })).toBeVisible();
  await expect(rows(page).first().getByLabel("Answer")).toHaveValue("A draft I haven't sent");
  await expect(rows(page).first().getByLabel("Answer")).toBeFocused();
});

test("a retry of a turn someone took back meanwhile says so and the row leaves", async ({ page, heapPath }) => {
  const certs = await seedStuck(page, heapPath, "Rotate the certs");
  await openWaiting(page, 1);
  const taken = await page.request.delete(`${certs.item}/turn`, { headers });
  expect(taken.status()).toBe(200);

  await rows(page)
    .first()
    .getByRole("button", { name: /^Retry/ })
    .click();

  await expect(page.getByRole("alert").filter({ hasText: "Someone else changed this meanwhile." })).toBeVisible();
  await expect(rows(page)).toHaveCount(0);
});

test("the item shows its stuck turn in Waits for you, and Retry hands it back to the agents", async ({
  page,
  heapPath,
}) => {
  const certs = await seedStuck(page, heapPath, "Rotate the certs", "The tool crashed");
  await page.goto(`${heapPath}/items/${certs.key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");

  // The stuck row is in Waits for you, with the reason and both ways out; the agent chip only says so.
  await expect(waitsForYou(page)).toContainText("Stuck · going to: with context");
  await expect(waitsForYou(page)).toContainText("3 runs in a row failed: The tool crashed");
  await expect(chip(page)).toHaveText("Agent stuck");
  await chip(page).click();
  await expect(who(page)).toContainText("An agent is stuck, going to: with context.");
  await expect(who(page).getByRole("button")).toHaveCount(1);
  await who(page).getByRole("button", { name: "Close", exact: true }).click();
  const retry = waitsForYou(page).getByRole("button", { name: /^Retry/ });
  if (isPhone(page)) {
    await expectTouchSize(retry);
    await expectTouchSize(waitsForYou(page).getByRole("button", { name: /^Take back/ }));
  }

  await retry.click();

  await expect(waitsForYou(page)).toHaveCount(0);
  await expect(chip(page)).toHaveText("Agent · to with context");
  await expect(itemPane(page)).toBeFocused();
  await chip(page).click();
  await expect(who(page)).toContainText("With an agent since");
  await expect(who(page).getByRole("button", { name: "Take back", exact: true })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Retried" })).toHaveText("Retried “Rotate the certs”.");
});

test("Take back from the item's stuck row asks first and leaves the item with the person", async ({
  page,
  heapPath,
}) => {
  const certs = await seedStuck(page, heapPath, "Rotate the certs");
  await page.goto(`${heapPath}/items/${certs.key}`);
  await expect(waitsForYou(page)).toBeVisible();

  await waitsForYou(page)
    .getByRole("button", { name: /^Take back/ })
    .click();
  await expect(waitsForYou(page).getByRole("alertdialog")).toContainText("Take this back from the agent?");
  await waitsForYou(page).getByRole("alertdialog").getByRole("button", { name: "Take back", exact: true }).click();

  await expect(waitsForYou(page)).toHaveCount(0);
  await expect(chip(page)).toHaveCount(0);
  // No next row, so focus goes to the item.
  await expect(itemPane(page)).toBeFocused();
});

test("the item list says a stuck item waits for you", async ({ page, heapPath }) => {
  await seedStuck(page, heapPath, "Rotate the certs");
  await page.reload();

  const listed = page.getByRole("listitem").filter({ has: page.getByRole("link", { name: /^Rotate the certs/ }) });
  await expect(listed).toBeVisible();
  await expect(listed.getByRole("link", { name: /waits for you/ })).toBeVisible();
  await expect(listed.getByRole("link", { name: /with an agent/ })).toHaveCount(0);
});

test("the item's Runs section shows a run with its cost, how it ended and why", async ({ page, heapPath }) => {
  const certs = await seed(page, heapPath, "Rotate the certs");
  const claimed = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  expect(claimed.status()).toBe(201);
  const { run } = await claimed.json();
  const finished = await page.request.post(`/api/v1/work/runs/${run.id}/finish`, {
    headers,
    data: { failure: "The tool crashed", cost_usd: 0.31 },
  });
  expect(finished.status()).toBe(200);

  await page.goto(`${heapPath}/items/${certs.key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");

  const runs = itemPane(page).getByRole("region", { name: "Runs", exact: true });
  await runs.locator("summary", { hasText: "Runs 1 · $0.31" }).click();
  const row = runs.getByRole("listitem");
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("failed");
  await expect(row).toContainText("$0.31");
  await expect(row).toContainText("The tool crashed");
});

test("History marks an agent's change with its run, and the link opens Runs at that run", async ({
  page,
  heapPath,
}) => {
  const certs = await seed(page, heapPath, "Rotate the certs");
  const claimed = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  expect(claimed.status()).toBe(201);
  const { run } = await claimed.json();
  const current = await (await page.request.get(certs.item)).json();
  const patched = await page.request.patch(certs.item, {
    headers: { ...headers, "upheap-run": run.id, "if-match": String(current.version) },
    data: { description: "Came up while fixing the CA" },
  });
  expect(patched.status()).toBe(200);

  await page.goto(`${heapPath}/items/${certs.key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");
  const history = await openHistory(page);

  // The agent's edit has the mark and the link; the person's own creation reads as before.
  const agent = history.getByRole("link", { name: "Your agent, see the run" });
  await expect(agent).toHaveCount(1);
  await expect(history.getByRole("listitem").filter({ hasText: "You added" })).toBeVisible();
  if (isPhone(page)) await expectTouchSize(agent);

  await agent.click();

  const runs = itemPane(page).getByRole("region", { name: "Runs", exact: true });
  await expect(runs.locator("details")).toHaveAttribute("open");
  await expect(runs.locator(`#run-${run.id}`)).toBeVisible();
});
