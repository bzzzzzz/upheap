// Questions on an item (ux.md, The item): the open ones are answered in "Waits for you" at the top, under a line that
// says how many wait, and read back in the Questions section's fold, "N answered". Most are seeded through the API, as another member or an agent would; the "Ask a question" form on the item asks
// them from the web app. Server build only.
import type { Page } from "@playwright/test";

import { changeState, closeItem, expect, isPhone, itemPane, test } from "./fixtures";

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

/** The block at the top that holds what waits, the open questions among it. */
const topWaits = (page: Page) => itemPane(page).getByRole("region", { name: "Waits for you", exact: true });
/** The open questions with their reply controls, in Waits for you. */
const waits = (page: Page) => itemPane(page).getByRole("group", { name: "Open questions", exact: true });
const answered = (page: Page) =>
  itemPane(page)
    .locator("details")
    .filter({ hasText: /^\d+ (answered|closed)/ });

/** The item's questions as the API has them, by number. */
async function questionsOf(page: Page, item: string) {
  const response = await page.request.get(item);
  expect(response.ok()).toBe(true);
  return (await response.json()).questions;
}

/** The "Ask a question" button at the foot of Questions. */
const askButton = (page: Page) => itemPane(page).getByRole("button", { name: "Ask a question", exact: true });
/** The form it opens. */
const askForm = (page: Page) => itemPane(page).getByRole("form", { name: "Ask a question", exact: true });

test("a question asked in the web app waits on the item and in Waiting for me, and is answered as usual", async ({
  page,
  heapPath,
}) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", []);

  // Without questions the section is its heading and the button.
  const section = itemPane(page).getByRole("region", { name: "Questions", exact: true });
  await expect(section).toBeVisible();
  await expect(askButton(page)).toBeVisible();
  await askButton(page).click();

  const form = askForm(page);
  const field = form.getByRole("textbox", { name: "Question", exact: true });
  await expect(field).toBeFocused();
  const ask = form.getByRole("button", { name: "Ask", exact: true });
  const cancel = form.getByRole("button", { name: "Cancel", exact: true });
  await expect(ask).toBeDisabled();
  if (isPhone(page)) {
    expect(Math.round((await ask.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
    expect(Math.round((await cancel.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
  }
  await form.getByLabel("Details, optional").fill("Internal or public.");
  await field.fill("Which CA do we use?");
  await expect(ask).toBeEnabled();
  await field.press("Enter");

  // The row shows, the item waits, the form is gone and focus is back on the button.
  await expect(waits(page).getByRole("listitem")).toContainText("Which CA do we use?");
  await expect(topWaits(page)).toContainText("1 question waits for you");
  await expect(form).toHaveCount(0);
  await expect(askButton(page)).toBeFocused();
  await expect(itemPane(page).getByRole("status").filter({ hasText: "Asked" })).toHaveText(
    "Asked “Which CA do we use?”.",
  );
  const [asked] = await questionsOf(page, item);
  expect(asked).toMatchObject({
    kind: "question",
    text: "Which CA do we use?",
    details: "Internal or public.",
    choices: [],
    state: "open",
  });

  // It waits in Waiting for me too.
  await closeItem(page);
  const entry = page.getByRole("link", { name: /^Waiting( for me)?( \d+)?$/ });
  await expect(entry).toHaveAccessibleName(/ 1$/);
  await entry.click();
  await expect(page.getByRole("heading", { name: "Waiting for me · 1", level: 1 })).toBeVisible();
  await expect(page.getByRole("main").getByRole("listitem")).toContainText("Which CA do we use?");
});

test("a choice question and a request are asked with their own fields", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [], true);
  await askButton(page).click();
  const form = askForm(page);

  await form.getByRole("textbox", { name: "Question", exact: true }).fill("Which regions?");
  await form.getByRole("radio", { name: "Pick any" }).click({ force: true });
  await form.getByLabel("Choice 1", { exact: true }).fill("EU");
  await form.getByLabel("Choice 2", { exact: true }).fill("US");
  // Enter adds a field after the one it is in, and doesn't submit.
  await form.getByLabel("Choice 2", { exact: true }).press("Enter");
  await expect(form.getByLabel("Choice 3", { exact: true })).toBeFocused();
  await form.getByLabel("Choice 3", { exact: true }).fill("Asia");
  await form.getByLabel("About a step").selectOption({ label: "1. Pick the CA" });
  await form.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(waits(page).getByRole("listitem")).toContainText("Which regions?");
  await expect(askButton(page)).toBeFocused();
  const [many] = await questionsOf(page, item);
  expect(many).toMatchObject({ kind: "question", choices: ["EU", "US", "Asia"], many: true, step: 1 });

  // A request keeps the text typed and has no answer or choices.
  await askButton(page).click();
  await form.getByRole("textbox", { name: "Question", exact: true }).fill("Put a token in the vault");
  await form.getByRole("radio", { name: "Request" }).click({ force: true });
  await expect(form.getByLabel("What to do")).toHaveValue("Put a token in the vault");
  await expect(form.getByRole("radiogroup", { name: "Answer" })).toHaveCount(0);
  await expect(form.getByLabel("Choice 1", { exact: true })).toHaveCount(0);
  await form.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(waits(page).getByRole("listitem")).toHaveCount(2);
  const [, request] = await questionsOf(page, item);
  expect(request).toMatchObject({
    kind: "request",
    text: "Put a token in the vault",
    choices: ["Done", "Can't be done"],
  });
});

test("the Ask form keeps its draft: Ask is off until it is valid, Escape keeps a typed draft, and a failed ask says why", async ({
  page,
  heapPath,
}) => {
  await itemWithQuestions(page, heapPath, "Rotate the certs", []);
  const form = askForm(page);
  const ask = form.getByRole("button", { name: "Ask", exact: true });

  // Escape with nothing typed closes the form.
  await askButton(page).click();
  await expect(form.getByRole("textbox", { name: "Question", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(form).toHaveCount(0);
  await expect(askButton(page)).toBeFocused();

  await askButton(page).click();
  await expect(ask).toBeDisabled();
  await form.getByRole("textbox", { name: "Question", exact: true }).fill("   ");
  await expect(ask).toBeDisabled();
  await form.getByRole("textbox", { name: "Question", exact: true }).fill("Which CA?");
  await form.getByRole("radio", { name: "Pick one" }).click({ force: true });
  await form.getByLabel("Choice 1", { exact: true }).fill("Internal");
  await form.getByLabel("Choice 2", { exact: true }).fill("INTERNAL");
  await expect(form.getByText("Two choices are the same.")).toBeVisible();
  await expect(ask).toBeDisabled();
  await form.getByLabel("Choice 2", { exact: true }).fill("Public");
  await expect(ask).toBeEnabled();

  // Escape with a draft keeps the form, and the item pane.
  await page.keyboard.press("Escape");
  await expect(form).toBeVisible();
  await expect(form.getByRole("textbox", { name: "Question", exact: true })).toHaveValue("Which CA?");
  await expect(itemPane(page)).toBeVisible();
  // So does Escape in the details editor.
  await form.getByLabel("Details, optional").press("Escape");
  await expect(form).toBeVisible();
  await expect(itemPane(page)).toBeVisible();

  await page.route("**/questions", (route) =>
    route.request().method() === "POST"
      ? route.fulfill({
          status: 409,
          contentType: "application/problem+json",
          body: JSON.stringify({ status: 409, code: "too_many_questions", title: "Too many" }),
        })
      : route.fallback(),
  );
  await ask.click();
  await expect(form.getByRole("alert")).toHaveText("This item has 100 questions, the most it can have.");
  await expect(form.getByRole("textbox", { name: "Question", exact: true })).toHaveValue("Which CA?");
  await expect(form.getByLabel("Choice 2", { exact: true })).toHaveValue("Public");

  await form.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(form).toHaveCount(0);
  await expect(askButton(page)).toBeFocused();
});

test("a question without choices is answered with text, and then reads in Questions", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?" },
    { kind: "question", text: "Who signs off?" },
  ]);

  const section = waits(page);
  // At the top, in Waits for you, it says how many wait, over the questions themselves.
  await expect(topWaits(page)).toContainText("2 questions wait for you");
  await expect(topWaits(page)).toHaveCSS("border-top-width", "2px");
  await expect(topWaits(page).getByRole("group", { name: "Open questions" })).toHaveCount(1);
  await expect(section.getByRole("listitem")).toHaveCount(2);

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
  await expect(folded.locator("summary")).toHaveText("1 answered");
  await folded.locator("summary").click();
  await expect(folded).toContainText("Which CA do we use?");
  // A reply without picks shows its comment alone, without the arrow.
  await expect(folded).toContainText("Let's Encrypt");
  await expect(folded).not.toContainText("→");
  await expect(folded).toContainText("You, today");

  // The last reply takes the section away and puts the focus on the item itself, not its title field.
  await section.getByLabel("Answer").fill("Sam");
  await section.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);
  await expect(itemPane(page)).toBeFocused();
  await expect(folded.locator("summary")).toHaveText("2 answered");

  // After a reload nothing has come back, and the server has both replies.
  await page.reload();
  await expect(itemPane(page).getByLabel("Title", { exact: true })).toBeVisible();
  await expect(section).toHaveCount(0);
  const [one, two] = await questionsOf(page, item);
  expect([one.state, one.reply.comment, two.reply.comment]).toEqual(["answered", "Let's Encrypt", "Sam"]);
});

test("Skip to details goes past the open questions to the fields, and the link in Questions comes back up", async ({
  page,
  heapPath,
}) => {
  await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA do we use?" },
    { kind: "question", text: "Who signs off?" },
  ]);
  const fields = itemPane(page).getByRole("region", { name: "Fields", exact: true });
  const top = topWaits(page);
  // The questions come before the fields.
  const topBox = await top.boundingBox();
  const fieldsBox = await fields.boundingBox();
  expect(Math.round(topBox?.y ?? 0)).toBeLessThan(Math.round(fieldsBox?.y ?? 0));
  await expect(top.getByRole("button", { name: "Answer ↓" })).toHaveCount(0);

  const skip = top.getByRole("button", { name: "Skip to details ↓", exact: true });
  if (isPhone(page)) expect(Math.round((await skip.boundingBox())?.height ?? 0)).toBeGreaterThanOrEqual(44);
  await skip.click();
  // Focus is on the first field, and the questions stay as they were.
  await expect(fields.getByRole("button").first()).toBeFocused();
  await expect(fields).toBeInViewport();
  await expect(waits(page).getByRole("listitem")).toHaveCount(2);

  // From the bottom, "2 open, at the top" goes back to the first question's first control.
  const back = itemPane(page)
    .getByRole("region", { name: "Questions", exact: true })
    .getByRole("button", { name: "2 open, at the top ↑", exact: true });
  await back.click();
  await expect(waits(page).getByLabel("Answer").first()).toBeFocused();
  await expect(waits(page).getByRole("listitem").first()).toBeInViewport();

  // With one question the link counts one, and with none it is gone, as is Skip.
  const first = waits(page).getByRole("listitem").first();
  await first.getByLabel("Answer").fill("Let's Encrypt");
  await first.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(top).toContainText("1 question waits for you");
  await expect(itemPane(page).getByRole("button", { name: "1 open, at the top ↑", exact: true })).toBeVisible();
  await expect(skip).toBeVisible();
  await waits(page).getByLabel("Answer").fill("Sam");
  await waits(page).getByRole("button", { name: "Reply", exact: true }).click();
  await expect(top).toHaveCount(0);
  await expect(itemPane(page).getByRole("button", { name: /open, at the top/ })).toHaveCount(0);
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
  await expect(folded).toContainText("→ Let's Encrypt");
  await expect(folded).toContainText("Free and automatic");
  await expect(folded).toContainText("→ EU, Asia");

  const [first, second] = await questionsOf(page, item);
  expect([first.reply.picked, first.reply.comment]).toEqual([[1], "Free and automatic"]);
  expect([second.reply.picked, second.reply.comment]).toEqual([[0, 2], null]);
});

test("an answered question's reply is changed from the fold, and Cancel drops the draft", async ({
  page,
  heapPath,
}) => {
  const item = await itemWithQuestions(page, heapPath, "Rotate the certs", [
    { kind: "question", text: "Which CA?", choices: ["Internal", "Let's Encrypt"] },
  ]);
  const one = waits(page).getByRole("listitem");
  await one.getByRole("radiogroup", { name: "Which CA?" }).locator("label").filter({ hasText: "Internal" }).click();
  await one.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(waits(page)).toHaveCount(0);

  const folded = answered(page);
  await folded.locator("summary").click();
  await expect(folded).toContainText("→ Internal");
  const change = folded.getByRole("button", { name: "Change reply", exact: true });

  // Cancel closes the controls and drops the draft, and focus returns to Change reply.
  await change.click();
  const radios = folded.getByRole("radiogroup", { name: "Which CA?" });
  await expect(radios.getByRole("radio", { name: "Internal" })).toBeChecked();
  await expect(radios.getByRole("radio").first()).toBeFocused();
  await radios.locator("label").filter({ hasText: "Let's Encrypt" }).click();
  await folded.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(change).toBeFocused();
  await expect(folded).toContainText("→ Internal");
  expect((await questionsOf(page, item))[0].reply.picked).toEqual([0]);

  // Sending shows the new reply, says so, and returns focus to Change reply.
  await change.click();
  await folded
    .getByRole("radiogroup", { name: "Which CA?" })
    .locator("label")
    .filter({ hasText: "Let's Encrypt" })
    .click();
  await folded.getByLabel("Comment, optional").fill("Free and automatic");
  await folded.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(itemPane(page).getByRole("status").filter({ hasText: "Changed the reply to" })).toHaveText(
    "Changed the reply to “Which CA?”.",
  );
  await expect(folded).toContainText("→ Let's Encrypt");
  await expect(folded).toContainText("Free and automatic");
  await expect(change).toBeFocused();
  const [question] = await questionsOf(page, item);
  expect([question.reply.picked, question.reply.comment]).toEqual([[1], "Free and automatic"]);
});

test("web addresses and Markdown links in the text and choices are links, and the rest stays as written", async ({
  page,
  heapPath,
}) => {
  // The links lead outside; the tests use no network, so those pages are answered here.
  await page.context().route("https://example.com/**", (route) => route.fulfill({ body: "elsewhere" }));
  await itemWithQuestions(page, heapPath, "Ship the page", [
    {
      kind: "question",
      text: "Is **this** ready? See https://example.com/pr/1. Or [the mockup](https://example.com/mock) (javascript:x)",
      choices: [
        "Yes, as in [the mockup](https://example.com/mock)",
        "No: [[web] Fix](https://example.com/pr/2), **https://example.com/pr/3**",
      ],
    },
  ]);
  const row = waits(page).getByRole("listitem").filter({ hasText: "Is **this** ready?" });
  const text = row.locator('[id^="question-text-"]');

  const bare = row.getByRole("link", { name: "https://example.com/pr/1", exact: true });
  await expect(bare).toHaveAttribute("href", "https://example.com/pr/1");
  await expect(bare).toHaveAttribute("target", "_blank");
  const mockup = text.getByRole("link", { name: "the mockup", exact: true });
  await expect(mockup).toHaveAttribute("href", "https://example.com/mock");
  // The rest reads as written: no bold, the full stop outside the link, and no link but to a web address.
  await expect(text).toContainText("Is **this** ready? See https://example.com/pr/1. Or");
  await expect(row.locator("strong")).toHaveCount(0);
  await expect(text.getByRole("link")).toHaveCount(2);

  // A link in a choice opens its page and leaves the choice unpicked.
  const radios = row.getByRole("radiogroup");
  const inChoice = radios.getByRole("link", { name: "the mockup", exact: true });
  const [tab] = await Promise.all([page.context().waitForEvent("page"), inChoice.click()]);
  await expect(tab).toHaveURL("https://example.com/mock");
  await tab.close();
  await expect(radios.getByRole("radio", { checked: true })).toHaveCount(0);
  await expect(row.getByRole("button", { name: "Reply", exact: true })).toBeDisabled();
  // A label with brackets, and emphasis around a bare address, stay out of the link's text and address.
  await expect(radios.getByRole("link", { name: "[web] Fix", exact: true })).toHaveAttribute(
    "href",
    "https://example.com/pr/2",
  );
  await expect(radios.getByRole("link", { name: "https://example.com/pr/3", exact: true })).toHaveAttribute(
    "href",
    "https://example.com/pr/3",
  );

  // The words around the link pick the choice.
  await radios.getByText("Yes, as in", { exact: false }).click({ position: { x: 4, y: 4 } });
  await row.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(waits(page)).toHaveCount(0);
  // Picked, the choice reads back with its link.
  const folded = answered(page);
  await folded.locator("summary").click();
  await expect(folded).toContainText("→ Yes, as in the mockup");
  await expect(
    folded.locator("p", { hasText: "→" }).getByRole("link", { name: "the mockup", exact: true }),
  ).toHaveAttribute("href", "https://example.com/mock");
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
  await expect(folded).toContainText("→ Done");
  await expect(folded).toContainText("Token is in the vault.");
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

test("details render as Markdown, and a reply's comment renders below its picks", async ({ page, heapPath }) => {
  const item = await itemWithQuestions(page, heapPath, "Choose a CA", [
    {
      kind: "question",
      text: "Which CA?",
      details: "Consider:\n\n- **cost**\n- trust <b>raw</b>",
      choices: ["A", "B"],
    },
    { kind: "question", text: "Anything else?" },
  ]);
  const section = waits(page);
  const row = section.getByRole("listitem").filter({ hasText: "Which CA?" });
  await row.getByRole("button", { name: "Details", exact: true }).click();
  await expect(row.getByRole("listitem")).toHaveCount(2);
  await expect(row.locator("strong")).toHaveText("cost");
  await expect(row.locator("b")).toHaveCount(0);

  await row.locator("label").filter({ hasText: "B" }).click();
  await row.getByLabel("Comment").fill("Because **trust** matters");
  await row.getByRole("button", { name: "Reply", exact: true }).click();
  const other = section.getByRole("listitem").filter({ hasText: "Anything else?" });
  await other.getByLabel("Answer").fill("Just *this*");
  await other.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(section).toHaveCount(0);

  const folded = answered(page);
  await folded.locator("summary").click();
  const picked = folded.getByRole("listitem").filter({ hasText: "Which CA?" });
  await expect(picked.getByText("→ B", { exact: true })).toBeVisible();
  await expect(picked.locator("strong").last()).toHaveText("trust");
  // A reply with only a comment shows it without the arrow.
  const only = folded.getByRole("listitem").filter({ hasText: "Anything else?" });
  await expect(only.locator("em")).toHaveText("this");
  await expect(only).not.toContainText("→");
  expect((await questionsOf(page, item))[1].reply.comment).toBe("Just *this*");
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

test("without questions the section is only Ask a question, and closing the item withdraws them, as Questions reads", async ({
  page,
  heapPath,
}) => {
  await itemWithQuestions(page, heapPath, "Plain item", []);
  await expect(waits(page)).toHaveCount(0);
  await expect(answered(page)).toHaveCount(0);
  // An open item the person may ask on shows the section with only its button.
  await expect(askButton(page)).toBeVisible();

  await itemWithQuestions(page, heapPath, "Rotate the certs", [{ kind: "question", text: "Which CA do we use?" }]);
  await expect(waits(page)).toBeVisible();
  await expect(answered(page)).toHaveCount(0);

  // Closing the item withdraws the question: it leaves Waits for you, and it reads, withdrawn, in Questions.
  await changeState(page, "Start", "main");
  await changeState(page, "Done", "main");
  await expect(itemPane(page).getByRole("group", { name: "State" }).getByRole("button").first()).toHaveText("Reopen");
  await expect(waits(page)).toHaveCount(0);
  await expect(askButton(page)).toHaveCount(0);
  const folded = answered(page);
  await expect(folded.locator("summary")).toHaveText("1 closed");
  await folded.locator("summary").click();
  await expect(folded).toContainText("Which CA do we use?");
  await expect(folded).toContainText("Withdrawn");
  await expect(folded.getByRole("button", { name: "Reply" })).toHaveCount(0);

  // Reopening doesn't bring it back: it stays withdrawn, and the item waits for nobody.
  await changeState(page, "Reopen", "main");
  await expect(waits(page)).toHaveCount(0);
  await expect(answered(page).locator("summary")).toHaveText("1 closed");
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
  await expect(askButton(page)).toHaveCount(0);

  // Skip still shows; with no field to edit it focuses the fields, and the link back focuses the questions.
  await section.getByRole("button", { name: "Skip to details ↓", exact: true }).click();
  await expect(itemPane(page).getByRole("region", { name: "Fields", exact: true })).toBeFocused();
  await itemPane(page).getByRole("button", { name: "2 open, at the top ↑", exact: true }).click();
  await expect(section).toBeFocused();
});
