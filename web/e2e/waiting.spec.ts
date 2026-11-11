// Waiting for me (ux.md, Waiting for me): every open question across the person's heaps, answered in its row. Nothing
// in the web app asks a question, so they are seeded through the API, as another member or an agent would. Server
// build only.
import type { Page } from "@playwright/test";

import { expect, isPhone, itemPane, row, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

type Seed = {
  kind: "question" | "request";
  text: string;
  details?: string;
  choices?: string[];
  many?: boolean;
  step?: number;
};

type Entry = {
  title: string;
  need?: "idea" | "nice_to_have" | "expected" | "critical";
  description?: string;
  questions: Seed[];
  task?: boolean;
};

/** Adds an item to the heap at `heapPath` through the API with these questions in order. Returns its API address. */
async function seed(page: Page, heapPath: string, entry: Entry) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, {
    headers,
    data: { title: entry.title, need: entry.need ?? "expected", description: entry.description },
  });
  expect(created.status()).toBe(201);
  const { key, version } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  if (entry.task) {
    const made = await page.request.patch(item, {
      headers: { ...headers, "If-Match": `"${version}"` },
      data: { kind: "task" },
    });
    expect(made.status()).toBe(200);
    const step = await page.request.post(`${item}/steps`, { headers, data: { title: "Pick the CA" } });
    expect(step.status()).toBe(201);
  }
  for (const body of entry.questions) {
    const asked = await page.request.post(`${item}/questions`, { headers, data: body });
    expect(asked.status()).toBe(201);
  }
  return { item, key };
}

/** Makes a heap in the team of `heapPath` and returns its path. */
async function newHeap(page: Page, team: string, name: string, context?: string) {
  const created = await page.request.post(`/api/v1/teams/${team}/heaps`, { headers, data: { name } });
  expect(created.status()).toBe(201);
  const heap = await created.json();
  if (context) {
    const patched = await page.request.patch(`/api/v1/teams/${team}/heaps/${heap.slug}`, {
      headers: { ...headers, "If-Match": `"${heap.version}"` },
      data: { context },
    });
    expect(patched.status()).toBe(200);
  }
  return `/teams/${team}/heaps/${heap.slug}`;
}

const teamOf = (heapPath: string) => heapPath.split("/")[2] ?? "";

/** The navigation entry: "Waiting for me" on a wide screen, "Waiting" on a tablet and a phone, then its count. */
const entry = (page: Page) => page.getByRole("link", { name: /^Waiting( for me)?( \d+)?$/ });

/** The page's rows, each an open question. */
const rows = (page: Page) => page.getByRole("main").getByRole("listitem");

const waitingPath = (page: Page) => new URL(page.url()).pathname;

test("on a phone an open question is found in the Waiting tab and answered without opening the item", async ({
  page,
  heapPath,
  localBuild,
}) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  const other = await newHeap(page, teamOf(heapPath), "Launch");
  const certs = await seed(page, heapPath, {
    title: "Rotate the certs",
    questions: [{ kind: "question", text: "Which CA do we use?" }],
  });
  await seed(page, other, {
    title: "Plan the launch",
    need: "critical",
    questions: [{ kind: "question", text: "Which date?", choices: ["Monday", "Friday"] }],
  });
  await page.reload();

  // No count at zero was the state before; now it shows 2.
  await expect(entry(page)).toHaveAccessibleName(/ 2$/);
  await entry(page).click();
  await expect(page.getByRole("heading", { name: "Waiting for me · 2", level: 1 })).toBeVisible();
  await expect(rows(page)).toHaveCount(2);
  const waiting = waitingPath(page);
  expect(waiting.endsWith("/waiting")).toBe(true);

  // The more important one is first: answer it in its row.
  const first = rows(page).first();
  await expect(first).toContainText("Which date?");
  const reply = first.getByRole("button", { name: "Reply", exact: true });
  await expect(reply).toBeDisabled();
  await first.getByRole("radiogroup", { name: "Which date?" }).locator("label").filter({ hasText: "Friday" }).click();
  if (isPhone(page)) expect(Math.round((await reply.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
  await reply.click();

  // The row leaves, the count drops, the next row's first control has the focus, a screen reader is told, and the
  // item was never opened.
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "Waiting for me · 1", level: 1 })).toBeVisible();
  await expect(entry(page)).toHaveAccessibleName(/ 1$/);
  await expect(page.getByLabel("Answer")).toBeFocused();
  await expect(page.getByRole("status").filter({ hasText: "Replied to" })).toHaveText("Replied to “Which date?”.");
  expect(waitingPath(page)).toBe(waiting);
  await expect(itemPane(page)).toHaveCount(0);

  // The server has the reply, and the other question still waits.
  const asked = await (await page.request.get(certs.item)).json();
  expect(asked.questions[0].state).toBe("open");
  await rows(page).first().getByLabel("Answer").fill("Let's Encrypt");
  await rows(page).first().getByRole("button", { name: "Reply", exact: true }).click();

  // The last reply: the empty state, no count anywhere, and the focus on the heading.
  await expect(page.getByText("Nothing waits for you.")).toBeVisible();
  await expect(page.getByText("Questions from agents and teammates show up here.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Waiting for me", level: 1, exact: true })).toBeFocused();
  await expect(entry(page)).toHaveAccessibleName(/^Waiting( for me)?$/);
  expect((await (await page.request.get(certs.item)).json()).questions[0].reply.comment).toBe("Let's Encrypt");
});

test("rows follow the score across heaps, with their item, heap, team and step, and Details unfold their context", async ({
  page,
  heapPath,
  localBuild,
}) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  // A second team, so the team names show.
  const made = await page.request.post("/api/v1/teams", {
    headers,
    data: { name: "Acme", slug: `t-${crypto.randomUUID().slice(0, 8)}` },
  });
  expect(made.status()).toBe(201);
  const acme = await newHeap(page, (await made.json()).slug, "Launch", "Ship by June. Reviews happen on Fridays.");
  const certs = await seed(page, heapPath, {
    title: "Rotate the certs",
    description: "The certs expire on the 3rd.",
    task: true,
    questions: [
      { kind: "question", text: "Which CA do we use?", details: "Internal or public.", step: 1 },
      { kind: "request", text: "Put a token in the vault." },
    ],
  });
  await seed(page, acme, {
    title: "Plan the launch",
    need: "critical",
    questions: [{ kind: "question", text: "Which regions?", choices: ["EU", "US"], many: true }],
  });
  await seed(page, acme, { title: "Quiet", need: "idea", questions: [{ kind: "question", text: "Any plans?" }] });
  await page.goto("/waiting");

  await expect(rows(page)).toHaveCount(4);
  // Highest score first; the two questions of one item are next to each other, by number.
  const texts = await rows(page).locator("p[id^='question-text-']").allTextContents();
  expect(texts).toEqual(["Which regions?", "Which CA do we use?", "Put a token in the vault.", "Any plans?"]);

  // The head line: what it is, the item as a link, the heap, the team (the user is in two), the step.
  const regions = rows(page).filter({ hasText: "Which regions?" });
  await expect(regions).toContainText("Question · pick any");
  await expect(regions.getByRole("link", { name: "Plan the launch", exact: true })).toBeVisible();
  await expect(regions).toContainText("Launch · Acme");
  const ca = rows(page).filter({ hasText: "Which CA do we use?" });
  await expect(ca).toContainText(/Question · Rotate the certs · .+ · .+ · step “Pick the CA”/);
  await expect(rows(page).filter({ hasText: "Put a token in the vault." })).toContainText("Request");

  // Details holds the details as written, and Context under it the item's and the heap's, fetched when opened.
  const contextRequests: string[] = [];
  page.on("request", (request) => {
    if (/\/items\/[^/]+$/.test(new URL(request.url()).pathname)) contextRequests.push(request.url());
  });
  await expect(ca.getByText("The certs expire on the 3rd.")).toHaveCount(0);
  await ca.getByRole("button", { name: "Details", exact: true }).click();
  await expect(ca.getByText("Internal or public.")).toBeVisible();
  expect(contextRequests).toEqual([]);
  await ca.getByRole("button", { name: "Context", exact: true }).click();
  await expect(ca.getByText("The certs expire on the 3rd.")).toBeVisible();
  await expect(ca.getByRole("heading", { name: "This item" })).toBeVisible();
  // The personal heap has no context written.
  await expect(ca.getByText("No context")).toBeVisible();

  // Without details, Context stands alone, and the heap's context shows under the heap's name.
  await regions.getByRole("button", { name: "Context", exact: true }).click();
  await expect(regions.getByRole("heading", { name: "Launch", exact: true })).toBeVisible();
  await expect(regions.getByText("Ship by June. Reviews happen on Fridays.")).toBeVisible();
  await expect(regions.getByRole("button", { name: "Details" })).toHaveCount(0);

  // The title opens the item in its heap.
  await ca.getByRole("link", { name: "Rotate the certs", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`${heapPath}/items/${certs.key}$`));
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");
});

test("a request is answered with Done or Can't be done, and an answered-first row leaves with a note", async ({
  page,
  heapPath,
  localBuild,
}) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  const { item } = await seed(page, heapPath, {
    title: "Rotate the certs",
    questions: [
      { kind: "request", text: "Put a token in the vault." },
      { kind: "question", text: "Which CA do we use?" },
    ],
  });
  await page.goto("/waiting");
  await expect(rows(page)).toHaveCount(2);

  const request = rows(page).filter({ hasText: "Put a token in the vault." });
  const done = request.getByRole("button", { name: "Done", exact: true });
  const cannot = request.getByRole("button", { name: "Can't be done", exact: true });
  if (isPhone(page)) {
    // Half the width each, 44 px tall.
    const [a, b] = [await done.boundingBox(), await cannot.boundingBox()];
    expect(Math.round(a?.height ?? 0)).toBeGreaterThanOrEqual(44);
    expect(Math.abs((a?.width ?? 0) - (b?.width ?? 1))).toBeLessThan(2);
  }

  // Someone replies to the other question meanwhile: sending our reply gets the note, and the row leaves.
  const current = await (await page.request.get(item)).json();
  const second = current.questions[1];
  const replied = await page.request.patch(`${item}/questions/${second.number}`, {
    headers: { ...headers, "If-Match": `"${second.version}"` },
    data: { reply: { picked: [], comment: "Let's Encrypt" } },
  });
  expect(replied.ok()).toBe(true);
  const ca = rows(page).filter({ hasText: "Which CA do we use?" });
  await ca.getByLabel("Answer").fill("Sam's pick");
  await ca.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(ca).toHaveCount(0);
  await expect(page.getByRole("alert").filter({ hasText: "Someone else replied" })).toHaveText(
    "Someone else replied to “Which CA do we use?” meanwhile.",
  );

  // The request is closed meanwhile: the item closes under it.
  const fresh = await (await page.request.get(item)).json();
  const closed = await page.request.patch(item, {
    headers: { ...headers, "If-Match": `"${fresh.version}"` },
    data: { state: "wont_do" },
  });
  expect(closed.ok()).toBe(true);
  await request.getByLabel("Comment, optional").fill("Done by hand");
  await done.click();
  await expect(request).toHaveCount(0);
  const note = page.getByRole("alert").filter({ hasText: "The item was closed before your reply" });
  await expect(note).toContainText("The item was closed before your reply to “Put a token in the vault.” was sent.");
  await expect(note).toContainText("Your comment: Done by hand");
  await expect(page.getByText("Nothing waits for you.")).toBeVisible();
});

test("a list row says it waits for you until the question is answered", async ({ page, heapPath, localBuild }) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  const { item } = await seed(page, heapPath, {
    title: "Rotate the certs",
    questions: [{ kind: "question", text: "Which CA do we use?" }],
  });
  await seed(page, heapPath, { title: "Write the notes", questions: [] });
  await page.reload();

  // The row's link says it waits, so its name is no longer the bare title.
  const waiting = page.getByRole("listitem").filter({ hasText: "Rotate the certs" });
  await expect(waiting.getByRole("link", { name: /waits for you/ })).toBeVisible();
  await expect(row(page, "Write the notes").getByRole("link", { name: /waits for you/ })).toHaveCount(0);
  if (isPhone(page)) {
    // A dot before the title; the words are for a screen reader only.
    await expect(waiting.getByText("waits for you", { exact: true })).toBeHidden();
  } else {
    await expect(waiting.getByText("waits for you", { exact: true })).toBeVisible();
  }

  await waiting.getByRole("link", { name: /^Rotate the certs/ }).click();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");
  const section = itemPane(page).getByRole("region", { name: "Waits for you", exact: true });
  await section.getByLabel("Answer").fill("Let's Encrypt");
  await section.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);
  await expect(row(page, "Rotate the certs")).toBeVisible();
  expect((await (await page.request.get(item)).json()).waits_for_you).toBe(false);
});

test("the phone's tabs are Open, Waiting, Add, Closed and More, with Settings in More", async ({
  page,
  localBuild,
}) => {
  test.skip(localBuild, "The static build has its own tabs.");
  test.skip(!isPhone(page), "Tabs are the phone's.");
  const tabs = page.getByRole("navigation", { name: "Heap views" });
  const names = await tabs
    .locator("a, button")
    .evaluateAll((all) =>
      all.map((element) => element.getAttribute("aria-label") ?? (element as HTMLElement).innerText.trim()),
    );
  expect(names).toEqual(["Open", "Waiting", "Add item", "Closed", "More"]);
  await expect(tabs.getByRole("link", { name: "Settings" })).toHaveCount(0);

  await tabs.getByRole("button", { name: "More" }).click();
  const more = page.getByRole("navigation", { name: "More" });
  const links = await more.getByRole("link").allTextContents();
  expect(links[0]).toBe("Settings");
  expect(links[1]).toBe("Activity");
  await more.getByRole("link", { name: "Settings" }).click();
  await expect(page).toHaveURL(/\/settings/);
  // More shows as the page's tab.
  await expect(tabs.getByRole("button", { name: "More" })).toHaveAttribute("aria-current", "true");
});

test("on the Waiting page the shell keeps the heap opened last, and marks no view", async ({
  page,
  heapPath,
  localBuild,
}) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  const other = await newHeap(page, teamOf(heapPath), "Launch");
  await page.goto(other);
  await expect(page.getByRole("button", { name: /^Switch heap: Launch/ })).toBeVisible();
  await entry(page).click();
  await expect(page.getByRole("heading", { name: "Waiting for me", level: 1 })).toBeVisible();
  expect(waitingPath(page).endsWith("/waiting")).toBe(true);

  // The switcher shows it, the views link to it, and none of them is current.
  await expect(page.getByRole("button", { name: /^Switch heap: Launch/ })).toBeVisible();
  const views = page.getByRole("navigation", { name: "Heap views" });
  const open = views.getByRole("link", { name: "Open", exact: true });
  await expect(open).toHaveAttribute("href", new RegExp(`${other}$`));
  await expect(open).not.toHaveAttribute("aria-current", "page");
  await expect(views.getByRole("link", { name: "Closed", exact: true })).not.toHaveAttribute("aria-current", "page");
  await expect(entry(page)).toHaveAttribute("aria-current", "page");

  // On a phone Add adds to that heap.
  if (isPhone(page)) {
    await views.getByRole("button", { name: "Add item" }).click();
    await expect(page.getByRole("dialog", { name: /^Add an item to Launch/ })).toBeVisible();
  }
});

test("the navigation entry has the width's form: with the count on the right, or badged under its label", async ({
  page,
  heapPath,
  localBuild,
}) => {
  test.skip(localBuild, "Nothing asks a question in the static build.");
  const width = page.viewportSize()?.width ?? 0;
  // No count at zero; the entry stays.
  await expect(entry(page)).toBeVisible();
  await expect(entry(page)).toHaveAccessibleName(/^Waiting( for me)?$/);
  if (width >= 640) {
    // Above the heap switcher.
    const wait = await entry(page).boundingBox();
    const heap = await page.getByRole("button", { name: /^Switch heap/ }).boundingBox();
    expect((wait?.y ?? 0) + (wait?.height ?? 0)).toBeLessThanOrEqual(heap?.y ?? 0);
  }

  await seed(page, heapPath, { title: "Rotate the certs", questions: [{ kind: "question", text: "Which CA?" }] });
  await page.reload();
  if (width >= 1024) await expect(entry(page)).toHaveAccessibleName(/^Waiting for me 1$/);
  else await expect(entry(page)).toHaveAccessibleName(/^Waiting 1$/);
});
