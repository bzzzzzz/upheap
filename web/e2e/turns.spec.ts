// Turns (ux.md, The item and The list): sending an item to an agent and taking it back, who has it on the item and in the
// list, and the heap's setting that agents prepare new notes. Nothing in the web app runs a turn, so the tests that need
// an item an agent already has send it through the API. Server build only.
import type { Locator, Page } from "@playwright/test";

import { addItem, expect, goTo, isPhone, itemPane, PASSWORD, row, test, uniqueUser } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

/** Adds a note through the API and returns its key and its address in the API. */
async function seed(page: Page, heapPath: string, title: string, sendToAgent = false) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, { headers, data: { title, need: "expected" } });
  expect(created.status()).toBe(201);
  const { key, id } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  if (sendToAgent) {
    const sent = await page.request.post(`${item}/turn`, { headers, data: { reach: "one", line: "Use the new CA." } });
    expect(sent.status()).toBe(201);
  }
  return { key, id, item };
}

/** Opens the item with `key` by its address, and waits for it to show. */
async function open(page: Page, heapPath: string, key: string, title: string) {
  await page.goto(`${heapPath}/items/${key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(title);
}

/** The list row of an item an agent has, whose link a screen reader hears with ", with an agent" after the title. */
const heldRow = (page: Page, title: string): Locator =>
  page
    .getByRole("listitem")
    .filter({ has: page.getByRole("link", { name: new RegExp(`^${title}\\s*, with an agent$`) }) });

const who = (page: Page): Locator => itemPane(page).getByRole("region", { name: "Who has it", exact: true });

/** The form or the question the block opens. */
const sendForm = (page: Page) => page.getByRole("dialog", { name: "Send to an agent", exact: true });
const takeBackAsk = (page: Page) => page.getByRole("alertdialog");

/** The history of the open item, as it reads in the pane. */
const history = (page: Page) => itemPane(page).getByRole("region", { name: "History", exact: true });

/** Expects a control to be at least 44 px tall, rounded: layout can land one a hair short, such as 43.99997. */
async function expectTouchSize(control: Locator) {
  const box = await control.boundingBox();
  expect(Math.round(box?.height ?? 0)).toBeGreaterThanOrEqual(44);
}

test("sending from the item with a line shows who has it, in the item and in the list, and Undo takes it back", async ({
  page,
  heapPath,
}) => {
  const { key } = await seed(page, heapPath, "Rotate the certs");
  await open(page, heapPath, key, "Rotate the certs");

  await expect(who(page)).toContainText("With you.");
  await who(page).getByRole("button", { name: "Send to an agent", exact: true }).click();
  const form = sendForm(page);
  await expect(form.getByRole("radio", { name: "To with context" })).toBeChecked();
  await expect(form.getByRole("radio", { name: "All the way" })).not.toBeChecked();
  await form.getByLabel("A line for the agent (optional)").fill("Use the new CA.");
  await form.getByRole("button", { name: "Send", exact: true }).click();

  await expect(who(page)).toContainText("With an agent since");
  await expect(who(page)).toContainText("going to: with context");
  await expect(who(page)).not.toContainText("all the way");
  await expect(who(page)).toContainText("“Use the new CA.”");
  await expect(sendForm(page)).toHaveCount(0);
  // Focus follows the button that replaced Send to an agent.
  await expect(who(page).getByRole("button", { name: "Take back", exact: true })).toBeFocused();
  await expect(page.getByRole("status").filter({ hasText: "Sent to an agent, going to: with context." })).toBeVisible();
  // The row says it too: in words from a tablet up, and in its name for a screen reader at every width.
  await expect(heldRow(page, "Rotate the certs")).toBeVisible();
  if (!isPhone(page)) {
    await expect(heldRow(page, "Rotate the certs").getByText("with an agent", { exact: true })).toBeVisible();
  }

  await page
    .getByRole("status")
    .filter({ hasText: "Sent to an agent" })
    .getByRole("button", { name: "Undo", exact: true })
    .click();
  await expect(who(page)).toContainText("With you.");
  await expect(who(page).getByRole("button", { name: "Send to an agent", exact: true })).toBeVisible();
  await expect(row(page, "Rotate the certs")).toBeVisible();
  await expect(heldRow(page, "Rotate the certs")).toHaveCount(0);
  // Undo is a take back, which the history keeps.
  await expect(history(page)).toContainText("opened a turn");
  await expect(history(page)).toContainText("took back a turn");
});

test("all the way is offered and shown, and a defined task has one way to go", async ({ page, heapPath }) => {
  const { key } = await seed(page, heapPath, "Rotate the certs");
  await open(page, heapPath, key, "Rotate the certs");
  await who(page).getByRole("button", { name: "Send to an agent", exact: true }).click();
  await sendForm(page).locator("label").filter({ hasText: "All the way" }).click();
  await expect(sendForm(page).getByRole("radio", { name: "All the way" })).toBeChecked();
  await sendForm(page).getByRole("button", { name: "Send", exact: true }).click();
  await expect(who(page)).toContainText("going to: with context, all the way");

  const defined = await seed(page, heapPath, "Ship it");
  const current = await (await page.request.get(defined.item)).json();
  const made = await page.request.patch(defined.item, {
    headers: { ...headers, "If-Match": `"${current.version}"` },
    data: { kind: "task", stage: "defined" },
  });
  expect(made.ok()).toBe(true);
  await open(page, heapPath, defined.key, "Ship it");
  await who(page).getByRole("button", { name: "Send to an agent", exact: true }).click();
  await expect(sendForm(page)).toContainText("To done");
  await expect(sendForm(page).getByRole("radio")).toHaveCount(0);
  await sendForm(page).getByRole("button", { name: "Send", exact: true }).click();
  await expect(who(page)).toContainText("going to: done");
});

test("taking back asks first: Cancel keeps it, Take back returns it to you with focus on Send to an agent", async ({
  page,
  heapPath,
}) => {
  const { key } = await seed(page, heapPath, "Rotate the certs", true);
  await open(page, heapPath, key, "Rotate the certs");
  const takeBack = who(page).getByRole("button", { name: "Take back", exact: true });

  await takeBack.click();
  await expect(takeBackAsk(page)).toContainText("Take this back from the agent? It stops, and what it changed stays.");
  await takeBackAsk(page).getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(takeBackAsk(page)).toHaveCount(0);
  await expect(who(page)).toContainText("With an agent since");
  await expect(takeBack).toBeFocused();

  await takeBack.click();
  await takeBackAsk(page).getByRole("button", { name: "Take back", exact: true }).click();
  await expect(who(page)).toContainText("With you.");
  await expect(who(page).getByRole("button", { name: "Send to an agent", exact: true })).toBeFocused();
  // No toast: the person did it, and the history keeps it.
  await expect(page.getByRole("status").filter({ hasText: "agent" })).toHaveCount(0);
});

test("an item an agent has asks before Start and Done in the item, and Blocked doesn't ask", async ({
  page,
  heapPath,
}) => {
  const { key } = await seed(page, heapPath, "Rotate the certs", true);
  await open(page, heapPath, key, "Rotate the certs");
  const state = itemPane(page).getByRole("group", { name: "State", exact: true });

  await state.getByRole("button", { name: "Start", exact: true }).click();
  await expect(takeBackAsk(page)).toContainText(
    "An agent has this. Starting it takes it back. Take it back and start it?",
  );
  await takeBackAsk(page).getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(who(page)).toContainText("With an agent since");
  await expect(state.getByRole("button", { name: "Start", exact: true })).toBeFocused();

  await state.getByRole("button", { name: "Block", exact: true }).click();
  await expect(takeBackAsk(page)).toHaveCount(0);
  await expect(itemPane(page).getByText("Blocked", { exact: true })).toBeVisible();
  await expect(who(page)).toContainText("With an agent since");

  await state.getByRole("button", { name: "Done", exact: true }).click();
  await expect(takeBackAsk(page)).toContainText("Closing it takes it back. Take it back and close it?");
  await takeBackAsk(page).getByRole("button", { name: "Take back and close", exact: true }).click();
  // A closed item offers only Reopen, at every width: "Done" would match the button still there before the close lands.
  await expect(state.getByRole("button")).toHaveText(["Reopen"]);
});

test("marking an item an agent has done from its circle asks first, and closing takes it back", async ({
  page,
  heapPath,
}) => {
  const { key, id } = await seed(page, heapPath, "Rotate the certs", true);
  await page.goto(heapPath);
  const circle = page.getByRole("button", { name: "Mark Rotate the certs done" });
  const ask = page.getByRole("alertdialog");

  await circle.click();
  await expect(ask).toContainText("An agent has “Rotate the certs”. Marking it done takes it back from the agent.");
  await ask.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(ask).toHaveCount(0);
  await expect(heldRow(page, "Rotate the certs")).toBeVisible();
  await expect(heldRow(page, "Rotate the certs").getByRole("link")).toBeFocused();

  await circle.click();
  await ask.getByRole("button", { name: "Take back and mark done", exact: true }).click();
  await expect(heldRow(page, "Rotate the certs")).toHaveCount(0);
  await expect(row(page, "Rotate the certs")).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "“Rotate the certs” is done." })).toBeVisible();

  // The item's history shows the take back, before the close.
  const log = await page.request.get(`/api/v1${heapPath}/audit?entity_id=${id}`);
  const actions = ((await log.json()).entries as { action: string }[]).map((entry) => entry.action);
  expect(actions.slice(0, 3)).toEqual(["item_updated", "turn_taken_back", "turn_opened"]);
  await open(page, heapPath, key, "Rotate the certs");
  await expect(history(page)).toContainText("took back a turn");
});

test("an admin turns on Agents prepare new notes, and a note added from Add shows with an agent", async ({
  page,
  heapPath,
}) => {
  await goTo(page, "Settings");
  const prepare = page.getByRole("switch", { name: "Agents prepare new notes" });
  await expect(page.getByRole("heading", { name: "Agents", exact: true })).toBeVisible();
  await expect(
    page.getByText("Each note added here is sent to an agent to fill in its context, then comes back to you."),
  ).toBeVisible();
  await expect(prepare).not.toBeChecked();
  // The switch shows the new value while it saves, so the save is waited for before the heap is read.
  const saved = page.waitForResponse((response) => response.request().method() === "PATCH" && response.ok());
  await page.locator("label").filter({ hasText: "Agents prepare new notes" }).click();
  await expect(prepare).toBeChecked();
  await saved;
  const heap = await (await page.request.get(`/api/v1${heapPath}`)).json();
  expect(heap.agents_prepare).toBe(true);
  // The setting is the admin's, and the history of the heap keeps it.
  await expect(page.getByText("Agents prepare new notes: on")).toHaveCount(0);

  await page.goto(heapPath);
  await addItem(page, "Fresh note", {}, { listed: false });
  await expect(heldRow(page, "Fresh note")).toBeVisible();
  await heldRow(page, "Fresh note").getByRole("link").click();
  await expect(who(page)).toContainText("Prepared on add");
  await expect(who(page)).toContainText("going to: with context");
});

test("a member who is not an admin reads the prepare setting as text, with no switch", async ({
  page,
  browser,
}, testInfo) => {
  const created = await page.request.post("/api/v1/teams", {
    headers,
    data: { name: "Acme", slug: `t-${crypto.randomUUID().slice(0, 8)}` },
  });
  expect(created.status()).toBe(201);
  const { slug } = (await created.json()) as { slug: string };
  const heap = await page.request.post(`/api/v1/teams/${slug}/heaps`, { headers, data: { name: "Roadmap" } });
  expect(heap.status()).toBe(201);
  const { slug: heapSlug } = (await heap.json()) as { slug: string };

  const context = await browser.newContext(testInfo.project.use);
  try {
    const user = uniqueUser();
    const registered = await context.request.post("/api/v1/auth/register", {
      headers,
      data: { ...user, display_name: "Sam Ortiz", password: PASSWORD },
    });
    expect(registered.status()).toBe(201);
    const { id } = (await registered.json()) as { id: string };
    const added = await page.request.post(`/api/v1/teams/${slug}/members`, { headers, data: { user_id: id } });
    expect(added.ok()).toBe(true);

    const member = await context.newPage();
    await member.goto(`/teams/${slug}/heaps/${heapSlug}/settings`);
    await expect(member.getByRole("heading", { name: "Agents", exact: true })).toBeVisible();
    await expect(member.getByText("Agents prepare new notes: off")).toBeVisible();
    await expect(member.getByRole("switch")).toHaveCount(0);
  } finally {
    await context.close();
  }
});

test("on a phone the row carries the agent mark and the forms open as sheets that a finger can use", async ({
  page,
  heapPath,
}) => {
  test.skip(!isPhone(page), "Phone only.");
  const sent = await seed(page, heapPath, "Rotate the certs", true);
  const plain = await seed(page, heapPath, "Plain note");
  await page.goto(heapPath);
  await expect(heldRow(page, "Rotate the certs").locator("svg.lucide-bot")).toBeVisible();
  await expect(row(page, "Plain note").locator("svg.lucide-bot")).toHaveCount(0);
  // The words are for wider screens.
  await expect(heldRow(page, "Rotate the certs").getByText("with an agent", { exact: true })).toBeHidden();

  await open(page, heapPath, plain.key, "Plain note");
  const send = who(page).getByRole("button", { name: "Send to an agent", exact: true });
  await expectTouchSize(send);
  await send.click();
  const form = sendForm(page);
  // A sheet, from the bottom of the screen: its padding is all that is below the form.
  const sheet = await form.boundingBox();
  const viewport = page.viewportSize();
  expect((viewport?.height ?? 0) - ((sheet?.y ?? 0) + (sheet?.height ?? 0))).toBeLessThanOrEqual(24);
  await expectTouchSize(form.locator("label").filter({ hasText: "All the way" }));
  await expectTouchSize(form.locator("label").filter({ hasText: "To with context" }));
  await expectTouchSize(form.getByRole("button", { name: "Send", exact: true }));
  await expectTouchSize(form.getByRole("button", { name: "Cancel", exact: true }));
  await form.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(who(page).getByRole("button", { name: "Send to an agent", exact: true })).toBeFocused();

  await open(page, heapPath, sent.key, "Rotate the certs");
  const takeBack = who(page).getByRole("button", { name: "Take back", exact: true });
  await expectTouchSize(takeBack);
  await takeBack.click();
  const ask = takeBackAsk(page);
  await expectTouchSize(ask.getByRole("button", { name: "Take back", exact: true }));
  await expectTouchSize(ask.getByRole("button", { name: "Cancel", exact: true }));
  await ask.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(takeBack).toBeFocused();
});

test("the item in an archived heap shows no Who has it block", async ({ page, heapPath }) => {
  const { key } = await seed(page, heapPath, "Rotate the certs", true);
  await open(page, heapPath, key, "Rotate the certs");
  await expect(who(page)).toBeVisible();

  const archived = await page.request.post(`/api/v1${heapPath}/archive`, { headers });
  expect(archived.ok()).toBe(true);
  await page.reload();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue("Rotate the certs");
  await expect(who(page)).toHaveCount(0);
  await expect(itemPane(page).getByRole("button", { name: /Send to an agent|Take back/ })).toHaveCount(0);
  // Archiving took the turn back.
  const item = await (await page.request.get(`/api/v1${heapPath}/items/${key}`)).json();
  expect(item.turn).toBeNull();
});
