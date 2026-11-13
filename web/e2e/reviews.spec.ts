// Reviews, done proposals and Changes asked… (ux.md, Waiting for me and The item): an agent asks for a review with a
// link or proposes closing a task, and the person replies in a row; an item an agent brought to a stable state short of
// done can be sent back. Nothing in the web app runs an agent, so the agent's side goes through the API, as a loop
// would. Server build only.
import type { Locator, Page } from "@playwright/test";

import { expect, isPhone, itemPane, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };
const PR = "https://example.test/acme/fence/pull/7";

/** `team/heap`, as the queue names a heap. */
const refOf = (heapPath: string) =>
  heapPath
    .split("/")
    .filter((part) => part !== "" && part !== "teams" && part !== "heaps")
    .join("/");

async function patch(page: Page, url: string, version: number, data: object) {
  const done = await page.request.patch(url, { headers: { ...headers, "if-match": `"${version}"` }, data });
  expect(done.status()).toBe(200);
  return done.json();
}

/** A defined task with `steps` steps, sent to an agent and claimed by a run. */
async function taskInRun(page: Page, heapPath: string, title: string, steps: number) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title, need: "expected", description: "Why and how" },
  });
  expect(created.status()).toBe(201);
  const { key, version } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  await patch(page, item, version, { kind: "task" });
  for (let n = 1; n <= steps; n++) {
    const step = await page.request.post(`${item}/steps`, { headers, data: { title: `Step ${n}` } });
    expect(step.status()).toBe(201);
  }
  const current = await (await page.request.get(item)).json();
  await patch(page, item, current.version, { stage: "defined" });
  const sent = await page.request.post(`${item}/turn`, { headers, data: { reach: "one" } });
  expect(sent.status()).toBe(201);
  const claimed = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  expect(claimed.status()).toBe(201);
  const { run } = await claimed.json();
  return { key, item, run: run.id as string };
}

/** The agent asks, in its run. */
async function ask(page: Page, task: { item: string; run: string }, body: object) {
  const asked = await page.request.post(`${task.item}/questions`, {
    headers: { ...headers, "upheap-run": task.run },
    data: { text: "The change", ...body },
  });
  expect(asked.status()).toBe(201);
}

/** The person ticks every step and replies to question `number` picking `pick`, through the API. */
async function reply(page: Page, item: string, number: number, pick: number) {
  const current = await (await page.request.get(item)).json();
  const question = current.questions.find((q: { number: number }) => q.number === number);
  await patch(page, `${item}/questions/${number}`, question.version, { reply: { picked: [pick] } });
}

async function tickAll(page: Page, item: string) {
  const current = await (await page.request.get(item)).json();
  for (const step of current.steps) {
    await patch(page, `${item}/steps/${step.number}`, step.version, { state: "ticked" });
  }
}

const entry = (page: Page) => page.getByRole("link", { name: /^Waiting( for me)?( \d+)?$/ });
const rows = (page: Page) => page.getByRole("main").getByRole("listitem");

async function openWaiting(page: Page, count: number) {
  // From the heap's list: an item's sheet on a phone covers the navigation.
  await page.goto(page.url().replace(/\/items\/.*$/, ""));
  await entry(page).click();
  await expect(page.getByRole("heading", { name: `Waiting for me · ${count}`, level: 1 })).toBeVisible();
}

/** Opens an item by its address, over the list. */
async function openByKey(page: Page, heapPath: string, key: string) {
  await page.goto(`${heapPath}/items/${key}`);
  await expect(itemPane(page)).toBeVisible();
}

test("a review is a row with its link, and the reply does not close the task; the next round is round 2", async ({
  page,
  heapPath,
}) => {
  const task = await taskInRun(page, heapPath, "Paint the fence", 1);
  await ask(page, task, { kind: "review", link: PR });

  await openWaiting(page, 1);
  const review = rows(page).first();
  await expect(review).toContainText("Review, round 1 · Paint the fence");
  await expect(review).toContainText("The change");
  const link = review.getByRole("link", { name: `Open the work: ${PR}` });
  await expect(link).toHaveText("↗ example.test/acme/fence/pull/7");
  await expect(link).toHaveAttribute("href", PR);
  await expect(link).toHaveAttribute("target", "_blank");
  await expect(review.getByText(/^Piece:/)).toHaveCount(0);
  const approved = review.getByRole("button", { name: "Approved", exact: true });
  const changes = review.getByRole("button", { name: "Changes asked", exact: true });
  await expect(approved).toBeVisible();
  if (isPhone(page)) {
    const a = await approved.boundingBox();
    const c = await changes.boundingBox();
    expect(Math.round(a?.y ?? 0)).toBe(Math.round(c?.y ?? -1));
    expect(Math.round(a?.height ?? 0)).toBeGreaterThanOrEqual(44);
  }

  await review.getByRole("textbox", { name: "Comment, optional" }).fill("Rename it.");
  await changes.click();
  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "Replied to" })).toHaveText("Replied to “The change”.");

  // The item's answered list.
  await openByKey(page, heapPath, task.key);
  await itemPane(page)
    .getByText(/^1 answered/)
    .click();
  const answered = itemPane(page).getByRole("region", { name: "Questions" });
  await expect(answered).toContainText("Review, round 1");
  await expect(answered).toContainText("→ Changes asked");
  await expect(answered).toContainText("Rename it.");
  await expect(answered.getByRole("link", { name: `Open the work: ${PR}` })).toBeVisible();

  // The agent asks again, in the same run: round 2.
  await ask(page, task, { kind: "review", link: PR });
  await openWaiting(page, 1);
  const second = rows(page).first();
  await expect(second).toContainText("Review, round 2");
  await second.getByRole("button", { name: "Approved", exact: true }).click();
  await expect(rows(page)).toHaveCount(0);
  const fetched = await (await page.request.get(task.item)).json();
  expect(fetched.state).not.toBe("done");
});

test("a review shows its piece when it differs from the link", async ({ page, heapPath }) => {
  const task = await taskInRun(page, heapPath, "Paint the fence", 1);
  await ask(page, task, { kind: "review", link: PR, piece: "the fence" });
  await openByKey(page, heapPath, task.key);
  const waits = itemPane(page).getByRole("group", { name: "Open questions" });
  await expect(waits).toContainText("Review, round 1");
  await expect(waits.getByText("Piece: the fence")).toBeVisible();
  await expect(waits.getByRole("link", { name: `Open the work: ${PR}` })).toBeVisible();
});

test("a done proposal summarises steps and reviews, and Close as done shows a toast that opens the item", async ({
  page,
  heapPath,
}) => {
  const task = await taskInRun(page, heapPath, "Paint the fence", 2);
  await ask(page, task, { kind: "review", link: PR });
  await reply(page, task.item, 1, 0);
  await tickAll(page, task.item);
  await ask(page, task, { kind: "done_proposal", text: "Nothing is left" });

  await openWaiting(page, 1);
  const proposal = rows(page).first();
  await expect(proposal).toContainText("Ready to close · Paint the fence");
  await expect(proposal).toContainText("Steps 2/2 · review approved");
  await proposal.getByRole("textbox", { name: "Comment, optional: what is left, if not yet" }).fill("Thanks");
  await proposal.getByRole("button", { name: "Close as done", exact: true }).click();

  await expect(rows(page)).toHaveCount(0);
  const toast = page.getByRole("status").filter({ hasText: "“Paint the fence” is done." });
  await expect(toast).toBeVisible();
  await toast.getByRole("button", { name: "Open", exact: true }).click();
  await expect(itemPane(page).getByRole("button", { name: "Reopen" })).toBeVisible();
  const fetched = await (await page.request.get(task.item)).json();
  expect(fetched.state).toBe("done");
});

test("Not yet on a proposal with no reviews keeps the task open", async ({ page, heapPath }) => {
  const task = await taskInRun(page, heapPath, "Paint the fence", 0);
  await ask(page, task, { kind: "done_proposal", text: "Nothing is left" });
  await openByKey(page, heapPath, task.key);
  const open = itemPane(page).getByRole("group", { name: "Open questions" });
  await expect(open).toContainText("Ready to close");
  await expect(open).toContainText("no reviews");

  await open.getByRole("textbox", { name: "Comment, optional: what is left, if not yet" }).fill("Update the README.");
  await open.getByRole("button", { name: "Not yet", exact: true }).click();

  await expect(open).toHaveCount(0);
  await itemPane(page)
    .getByText(/^1 answered/)
    .click();
  const questions = itemPane(page).getByRole("region", { name: "Questions" });
  await expect(questions).toContainText("Ready to close");
  await expect(questions).toContainText("→ Not yet");
  await expect(questions).toContainText("Update the README.");
  const fetched = await (await page.request.get(task.item)).json();
  expect(fetched.state).not.toBe("done");
});

test("Changes asked… sends an item an agent brought to a stable state back", async ({ page, heapPath }) => {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: "Rotate the certs", need: "expected", description: "Why" },
  });
  const { key } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  expect((await page.request.post(`${item}/turn`, { headers, data: { reach: "one" } })).status()).toBe(201);
  const claimed = await page.request.post("/api/v1/work/runs", { headers, data: { heaps: [refOf(heapPath)] } });
  const { run } = await claimed.json();
  const reached = await page.request.post(`/api/v1/work/runs/${run.id}/reached`, {
    headers,
    data: { result: "Added the context" },
  });
  expect(reached.status()).toBe(200);

  await openByKey(page, heapPath, key);
  const button: Locator = itemPane(page).getByRole("button", { name: "Changes asked…" });
  await button.click();
  const form = page.getByRole("dialog", { name: "Changes asked" });
  await expect(form).toContainText("An agent brought this to: With context.");
  const send = form.getByRole("button", { name: "Send back", exact: true });
  await expect(send).toBeDisabled();
  await form.getByRole("textbox", { name: "What's wrong" }).fill("Add the rollback steps.");
  await expect(send).toBeEnabled();
  await send.click();

  await expect(page.getByRole("status").filter({ hasText: "Sent back to the agent." })).toBeVisible();
  await expect(itemPane(page).getByRole("button", { name: /^Who has it: / })).toBeVisible();
  await expect(button).toHaveCount(0);
});
