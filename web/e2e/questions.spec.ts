// Questions on an item (ux.md, The item §12 to §14): answered in the item's "Waits for you" section, and read back in
// the folded "Questions n". Nothing in the web app asks one, so they are seeded through the API, as another member or
// an agent would. Server build only.
import type { Page } from "@playwright/test";

import { changeState, expect, isPhone, itemPane, test } from "./fixtures";

const headers = { "x-csrf-protection": "1" };

type Seed = {
  kind: "question" | "request";
  text: string;
  details?: string;
  choices?: string[];
  many?: boolean;
  step?: number;
};

/** Adds an item through the API, with these questions in order, and opens it. Returns its address in the API. */
async function itemWithQuestions(page: Page, heapPath: string, title: string, questions: Seed[], task = false) {
  const created = await page.request.post(`/api/v1${heapPath}/items`, { headers, data: { title, need: "expected" } });
  expect(created.status()).toBe(201);
  const { key, version } = await created.json();
  const item = `/api/v1${heapPath}/items/${key}`;
  if (task) {
    const made = await page.request.patch(item, {
      headers: { ...headers, "If-Match": `"${version}"` },
      data: { kind: "task" },
    });
    expect(made.status()).toBe(200);
    const step = await page.request.post(`${item}/steps`, { headers, data: { title: "Pick the CA" } });
    expect(step.status()).toBe(201);
  }
  for (const body of questions) {
    const asked = await page.request.post(`${item}/questions`, { headers, data: body });
    expect(asked.status()).toBe(201);
  }
  await page.goto(`${heapPath}/items/${key}`);
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toHaveValue(title);
  return item;
}

const waits = (page: Page) => itemPane(page).getByRole("region", { name: "Waits for you", exact: true });
const answered = (page: Page) =>
  itemPane(page)
    .locator("details")
    .filter({ hasText: /^Questions \d/ });

/** The item's questions as the API has them, by number. */
async function questionsOf(page: Page, item: string) {
  const response = await page.request.get(item);
  expect(response.ok()).toBe(true);
  return (await response.json()).questions;
}

test("a question without choices is answered with text, and then reads in Questions", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?" },
    { kind: "question", text: "Who signs off?" },
  ]);

  const section = waits(page);
  await expect(section.getByRole("heading", { name: "Waits for you" })).toBeVisible();
  await expect(section.getByRole("listitem")).toHaveCount(2);
  // Above "Why it's #N" and the rest, and tinted: the one outlined block.
  await expect(section).toHaveCSS("border-top-width", "2px");

  const first = section.getByRole("listitem").filter({ hasText: "Which CA do we use?" });
  const answer = first.getByLabel("Answer");
  const reply = first.getByRole("button", { name: "Reply", exact: true });
  await expect(first).toContainText("Question");
  await expect(reply).toBeDisabled();
  // Rounded: layout can land a 44 px control a hair short, such as 43.99997.
  if (isPhone(page)) expect(Math.round((await reply.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
  await answer.fill("   ");
  await expect(reply).toBeDisabled();
  await answer.fill("Let's Encrypt");
  await expect(reply).toBeEnabled();
  await reply.click();

  // The row leaves, the next row's first control has the focus, and a screen reader is told.
  await expect(section.getByRole("listitem")).toHaveCount(1);
  await expect(section.getByLabel("Answer")).toBeFocused();
  await expect(itemPane(page).getByRole("status").filter({ hasText: "Replied to" })).toHaveText(
    "Replied to “Which CA do we use?”.",
  );
  await expect(page.getByRole("status").filter({ hasText: "Replied to" })).toHaveCount(1);

  // It reads in the folded Questions, in number order, with the reply and who made it.
  const folded = answered(page);
  await expect(folded.locator("summary")).toHaveText("Questions 1");
  await folded.locator("summary").click();
  await expect(folded).toContainText("Which CA do we use?");
  await expect(folded).toContainText("→ Let's Encrypt");
  await expect(folded).toContainText("You, today");

  // The last reply takes the section away and puts the focus on the item itself, not its title field.
  await section.getByLabel("Answer").fill("Sam");
  await section.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);
  await expect(itemPane(page)).toBeFocused();
  await expect(folded.locator("summary")).toHaveText("Questions 2");

  // After a reload nothing has come back, and the server has both replies.
  await page.reload();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toBeVisible();
  await expect(section).toHaveCount(0);
  const [one, two] = await questionsOf(page, item);
  expect([one.state, one.reply.comment, two.reply.comment]).toEqual(["answered", "Let's Encrypt", "Sam"]);
});

test("a choice question is answered by one pick, a many-choice one by any number", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Plan the move", [
    { kind: "question", text: "Which CA?", choices: ["Internal", "Let's Encrypt"] },
    { kind: "question", text: "Which regions?", choices: ["EU", "US", "Asia"], many: true },
  ]);
  const section = waits(page);

  const one = section.getByRole("listitem").filter({ hasText: "Which CA?" });
  await expect(one).toContainText("Question");
  await expect(one).not.toContainText("pick any");
  const reply = one.getByRole("button", { name: "Reply", exact: true });
  await expect(reply).toBeDisabled();
  const radios = one.getByRole("radiogroup", { name: "Which CA?" });
  await radios.locator("label").filter({ hasText: "Let's Encrypt" }).click();
  await expect(reply).toBeEnabled();
  await one.getByLabel("Comment, optional").fill("Free and automatic");
  await reply.click();
  await expect(section.getByRole("listitem")).toHaveCount(1);

  const many = section.getByRole("listitem").filter({ hasText: "Which regions?" });
  await expect(many).toContainText("Question · pick any");
  const regions = many.getByRole("group", { name: "Which regions?" });
  await expect(many.getByRole("button", { name: "Reply", exact: true })).toBeDisabled();
  await regions.locator("label").filter({ hasText: "EU" }).click();
  await regions.locator("label").filter({ hasText: "Asia" }).click();
  await many.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);

  const folded = answered(page);
  await folded.locator("summary").click();
  await expect(folded).toContainText("→ Let's Encrypt. Free and automatic");
  await expect(folded).toContainText("→ EU, Asia");

  const [first, second] = await questionsOf(page, item);
  expect([first.reply.picked, first.reply.comment]).toEqual([[1], "Free and automatic"]);
  expect([second.reply.picked, second.reply.comment]).toEqual([[0, 2], null]);
});

test("a request is replied to at once with Done or Can't be done, and a comment", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Stage the token", [
    { kind: "request", text: "Put a staging token in the vault." },
    { kind: "request", text: "Open the firewall." },
  ]);
  const section = waits(page);
  const vault = section.getByRole("listitem").filter({ hasText: "Put a staging token in the vault." });
  await expect(vault).toContainText("Request");
  // There is no Reply button: the two buttons are the reply.
  await expect(vault.getByRole("button", { name: "Reply", exact: true })).toHaveCount(0);
  await vault.getByLabel("Comment, optional").fill("Token is in the vault.");
  // Each control says which request it answers.
  await expect(vault.getByRole("button", { name: "Done", exact: true })).toHaveAccessibleDescription(
    "Put a staging token in the vault.",
  );
  await expect(vault.getByLabel("Comment, optional")).toHaveAccessibleDescription("Put a staging token in the vault.");
  await vault.getByRole("button", { name: "Done", exact: true }).click();
  await expect(section.getByRole("listitem")).toHaveCount(1);

  const firewall = section.getByRole("listitem").filter({ hasText: "Open the firewall." });
  await firewall.getByRole("button", { name: "Can't be done", exact: true }).click();
  await expect(section).toHaveCount(0);

  const folded = answered(page);
  await folded.locator("summary").click();
  await expect(folded).toContainText("→ Done. Token is in the vault.");
  await expect(folded).toContainText("→ Can't be done");

  const [first, second] = await questionsOf(page, item);
  expect([first.reply.picked, first.reply.comment]).toEqual([[0], "Token is in the vault."]);
  expect([second.reply.picked, second.reply.comment]).toEqual([[1], null]);
});

test("details open in place, and the row names its step", async ({ page, heapPath }) => {
  await itemWithQuestions(
    page,
    heapPath,
    "Choose a CA",
    [
      { kind: "question", text: "Which CA?", details: "Internal is cheaper.\nPublic is trusted.", step: 1 },
      { kind: "question", text: "Any other thoughts?" },
    ],
    true,
  );
  const section = waits(page);
  const row = section.getByRole("listitem").filter({ hasText: "Which CA?" });
  await expect(row).toContainText("Question · step “Pick the CA”");
  const details = row.getByRole("button", { name: "Details", exact: true });
  await expect(details).toHaveAttribute("aria-expanded", "false");
  await expect(row.getByText("Internal is cheaper.")).toHaveCount(0);
  await details.click();
  await expect(details).toHaveAttribute("aria-expanded", "true");
  await expect(row.getByText("Public is trusted.")).toBeVisible();
  // A question without details has no disclosure.
  await expect(
    section.getByRole("listitem").filter({ hasText: "Any other thoughts?" }).getByRole("button", { name: "Details" }),
  ).toHaveCount(0);
});

test("someone else's reply meanwhile takes the row away with a note", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?" },
    { kind: "question", text: "Who signs off?" },
  ]);
  const section = waits(page);
  const row = section.getByRole("listitem").filter({ hasText: "Which CA do we use?" });
  await row.getByLabel("Answer").fill("My answer");

  // Another member answers it while the row is open.
  const other = await page.request.patch(`${item}/questions/1`, {
    headers: { ...headers, "If-Match": '"1"' },
    data: { reply: { picked: [], comment: "Theirs" } },
  });
  expect(other.ok()).toBe(true);

  await row.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section.getByRole("listitem")).toHaveCount(1);
  await expect(section.getByRole("listitem").filter({ hasText: "Which CA do we use?" })).toHaveCount(0);
  await expect(itemPane(page).getByRole("alert")).toHaveText(
    "Someone else replied to “Which CA do we use?” meanwhile.",
  );
  // The reply that stands is theirs.
  expect((await questionsOf(page, item))[0].reply.comment).toBe("Theirs");

  // The note stays until the next reply.
  await section.getByLabel("Answer").fill("Sam");
  await section.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);
  await expect(itemPane(page).getByText("meanwhile.")).toHaveCount(0);
});

test("the section is absent without open questions, and closing the item withdraws them, as Questions reads", async ({
  page,
  heapPath,
}) => {
  await itemWithQuestions(page, heapPath, "Plain item", []);
  await expect(waits(page)).toHaveCount(0);
  await expect(answered(page)).toHaveCount(0);

  await itemWithQuestions(page, heapPath, "Rotate the certs", [{ kind: "question", text: "Which CA do we use?" }]);
  await expect(waits(page)).toBeVisible();
  await expect(answered(page)).toHaveCount(0);

  // Closing the item withdraws the question: it leaves Waits for you, and it reads, withdrawn, in Questions.
  await changeState(page, "Start");
  await changeState(page, "Done");
  await expect(itemPane(page).getByRole("group", { name: "State" }).getByRole("button").first()).toHaveText("Reopen");
  await expect(waits(page)).toHaveCount(0);
  const folded = answered(page);
  await expect(folded.locator("summary")).toHaveText("Questions 1");
  await folded.locator("summary").click();
  await expect(folded).toContainText("Which CA do we use?");
  await expect(folded).toContainText("Withdrawn");
  await expect(folded.getByRole("button", { name: "Reply" })).toHaveCount(0);

  // Reopening doesn't bring it back: it stays withdrawn, and the item waits for nobody.
  await changeState(page, "Reopen");
  await expect(waits(page)).toHaveCount(0);
  await expect(answered(page).locator("summary")).toHaveText("Questions 1");
  await answered(page).locator("summary").click();
  await expect(answered(page)).toContainText("Withdrawn");
});

test("a failed reply keeps the row with its picks and comment, and says why", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Plan the move", [
    { kind: "question", text: "Which regions?", choices: ["EU", "US", "Asia"], many: true },
  ]);
  await page.route("**/questions/1", (route) =>
    route.request().method() === "PATCH"
      ? route.fulfill({
          status: 422,
          contentType: "application/problem+json",
          body: JSON.stringify({ status: 422, code: "pick_invalid", title: "Bad pick", field: "picked" }),
        })
      : route.fallback(),
  );
  const row = waits(page).getByRole("listitem");
  await row.locator("label").filter({ hasText: "US" }).click();
  await row.getByLabel("Comment, optional").fill("Mostly US");
  await row.getByRole("button", { name: "Reply", exact: true }).click();

  await expect(row.getByRole("alert")).toHaveText("That pick doesn't fit this question. Pick again.");
  await expect(row.getByRole("checkbox", { name: "US" })).toBeChecked();
  await expect(row.getByLabel("Comment, optional")).toHaveValue("Mostly US");
  await expect(row.getByRole("button", { name: "Reply", exact: true })).toBeEnabled();
  expect((await questionsOf(page, item))[0].state).toBe("open");
});

test("closing the item before a reply lands keeps a note with the comment", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?" },
  ]);
  const row = waits(page).getByRole("listitem");
  await row.getByLabel("Answer").fill("Let's Encrypt");

  // Another member closes the item while the row is open.
  const current = await (await page.request.get(item)).json();
  const closed = await page.request.patch(item, {
    headers: { ...headers, "If-Match": `"${current.version}"` },
    data: { state: "wont_do" },
  });
  expect(closed.ok()).toBe(true);

  await row.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(waits(page)).toHaveCount(0);
  const note = itemPane(page).getByRole("alert");
  await expect(note).toContainText("The item was closed before your reply to “Which CA do we use?” was sent.");
  await expect(note).toContainText("Your comment: Let's Encrypt");
  expect((await questionsOf(page, item))[0].state).toBe("withdrawn");
});

test("in an archived heap a question shows its text and details, with no controls", async ({ page, heapPath }) => {
  await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?", details: "Internal or public." },
    { kind: "request", text: "Put a token in the vault." },
  ]);
  const archived = await page.request.post(`/api/v1${heapPath}/archive`, { headers });
  expect(archived.ok()).toBe(true);
  await page.reload();

  const section = waits(page);
  await expect(section.getByRole("listitem")).toHaveCount(2);
  await section.getByRole("button", { name: "Details", exact: true }).click();
  await expect(section.getByText("Internal or public.")).toBeVisible();
  await expect(section.getByRole("button", { name: /^(Reply|Done|Can't be done)$/ })).toHaveCount(0);
  await expect(section.getByRole("textbox")).toHaveCount(0);
  await expect(section.getByRole("radio")).toHaveCount(0);
});
