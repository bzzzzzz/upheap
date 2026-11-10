// The Teams and Team pages (design/ux.md, Teams and Team): making a team, sharing it, and managing its heaps
// and members. Teams with others are the server's, so most of these skip the local build.
import type { APIRequestContext, Page } from "@playwright/test";

import { addItem, createTeam, expect, isPhone, PASSWORD, test, uniqueUser } from "./fixtures";

const SERVER_ONLY = "The local build has one user and one team.";

/** Registers someone else, in a request context of their own so the page stays signed in as itself. */
async function registerOther(request: APIRequestContext, display_name: string) {
  const user = uniqueUser();
  const registered = await request.post("/api/v1/auth/register", {
    headers: { "x-csrf-protection": "1" },
    data: { ...user, display_name, password: PASSWORD },
  });
  expect(registered.status()).toBe(201);
  return user;
}

/** The member search, by its exact name: the hint describes it without being part of it. */
const memberSearch = (page: Page) =>
  page.getByRole("combobox", { name: "Add a member by username, name or email", exact: true });

/** Adds the user `username` from the member search: with Enter on a wider screen, a tap on the result on a phone. */
async function addMember(page: Page, username: string) {
  const search = memberSearch(page);
  await search.fill(username);
  if (isPhone(page)) {
    await page.getByRole("option", { name: new RegExp(`@${username}`) }).click();
  } else {
    await expect(page.getByRole("option", { selected: true })).toContainText(`@${username}`);
    await search.press("Enter");
  }
  await expect(search).toHaveValue("");
}

const members = (page: Page) => page.getByRole("region", { name: /^Members/ });
const heaps = (page: Page) => page.getByRole("region", { name: "Heaps" });

test("a team is created with or without a link name, and a taken one is said under its field", async ({
  page,
  localBuild,
}) => {
  test.skip(localBuild, SERVER_ONLY);
  const linkName = `t-${crypto.randomUUID().slice(0, 8)}`;

  // Without a link name the server makes one from the name, and the note doesn't guess it.
  await page.goto("/teams");
  await page.getByRole("button", { name: "+ Create team" }).click();
  const form = page.getByRole("form", { name: "Create a team" });
  await expect(form.getByText("You'll be its admin.", { exact: true })).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("Garden Club");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Garden Club", level: 1 })).toBeVisible();
  await expect(page).toHaveURL(/\/teams\/garden-club(-\d+)?$/);
  await expect(page.getByText("Teams · you're an admin")).toBeVisible();

  // With one, the note shows the link.
  await page.goto("/teams");
  await page.getByRole("button", { name: "+ Create team" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Acme");
  await page.getByLabel("Link name").fill(linkName);
  await expect(form.getByText(`/teams/${linkName} · you'll be its admin`)).toBeVisible();
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${linkName}$`));

  // The same link name again is taken: the error is under Link name, which gets focus, and typing clears it.
  await page.goto("/teams");
  await page.getByRole("button", { name: "+ Create team" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Acme again");
  const link = page.getByLabel("Link name");
  await link.fill(linkName);
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(link).toHaveAttribute("aria-invalid", "true");
  await expect(link).toHaveAccessibleDescription("That's taken. Pick another.");
  await expect(link).toBeFocused();
  await link.press("End");
  await link.pressSequentially("-2");
  await expect(link).not.toHaveAttribute("aria-invalid");

  // Escape closes the form, and focus goes back to Create team.
  await link.press("Escape");
  await expect(form).toHaveCount(0);
  await expect(page.getByRole("button", { name: "+ Create team" })).toBeFocused();

  // The rows say the role, members and heaps.
  const acme = page.getByRole("listitem").filter({ has: page.getByRole("link", { name: "Open Acme" }) });
  await expect(acme).toContainText("Admin · 1 member · 0 heaps");
  const personal = page.getByRole("listitem").filter({ hasText: "Only you" });
  await expect(personal).toContainText("Only you · 1 heap");
});

test("the counts come from the server: open items per heap, members per team", async ({
  page,
  request,
  localBuild,
}) => {
  test.skip(localBuild, SERVER_ONLY);
  const grace = await registerOther(request, "Grace Hopper");
  await createTeam(page, "Counting");
  await page.getByRole("button", { name: "+ New heap" }).click();
  await page.getByLabel("New heap name").fill("Ledger");
  await page.getByRole("button", { name: "Add heap" }).click();
  await expect(heaps(page).getByText("0 open")).toBeVisible();

  // Two items in the new heap; the team's page counts them.
  const teamPath = new URL(page.url()).pathname;
  await heaps(page)
    .getByRole("link", { name: /Ledger/ })
    .click();
  await addItem(page, "Balance the books");
  await addItem(page, "File the receipts");
  await page.goto(teamPath);
  await expect(heaps(page).getByText("2 open")).toBeVisible();

  await addMember(page, grace.username);
  await expect(page.getByRole("heading", { name: "Members · 2" })).toBeVisible();
  await page.goto("/teams");
  const row = page.getByRole("listitem").filter({ has: page.getByRole("link", { name: "Open Counting" }) });
  await expect(row).toContainText("Admin · 2 members · 1 heap");
});

test("a member is added from the search with the keyboard, and removing one says what happens", async ({
  page,
  request,
  localBuild,
}) => {
  test.skip(localBuild, SERVER_ONLY);
  const sam = await registerOther(request, "Sam Ortiz");
  await createTeam(page, "Acme");
  await page.getByRole("button", { name: "+ New heap" }).click();
  await page.getByLabel("New heap name").fill("Launch");
  await page.getByRole("button", { name: "Add heap" }).click();
  await expect(heaps(page).getByText("Launch", { exact: true })).toBeVisible();

  // On a wider screen the arrows and Enter pick from the results. On a phone nothing is highlighted and the keyboard's
  // Enter adds no one: a tap does. Someone already in the team can't be added.
  const search = memberSearch(page);
  await expect(search).toHaveAccessibleName("Add a member by username, name or email");
  await expect(search).toHaveAccessibleDescription("They need an account first");
  await search.fill(sam.username);
  const option = page.getByRole("option", { name: new RegExp(`Sam Ortiz @${sam.username}`) });
  if (isPhone(page)) {
    await expect(option).toBeVisible();
    await expect(option).toHaveAttribute("aria-selected", "false");
    await expect(option).not.toContainText("Add as member");
    await search.press("Enter");
    await expect(search).toHaveValue(sam.username);
    await expect(page.getByRole("heading", { name: "Members · 1" })).toBeVisible();
    await option.click();
  } else {
    await expect(option).toHaveAttribute("aria-selected", "true");
    await expect(option).toContainText("Add as member");
    await search.press("ArrowDown");
    await search.press("Enter");
  }
  await expect(search).toHaveValue("");
  await expect(page.getByRole("heading", { name: "Members · 2" })).toBeVisible();
  await search.fill(sam.username);
  await expect(option).toContainText("Already a member");
  await expect(option).toHaveAttribute("aria-disabled", "true");
  await search.fill("zzzzzz-nobody");
  await expect(page.getByText("No one matches.")).toBeVisible();
  await search.fill("");

  const question = "Remove Sam Ortiz from Acme? They lose access to Launch. Their items stay.";
  if (isPhone(page)) {
    // On a phone the row's actions are in a sheet, which asks there.
    await members(page).getByRole("button", { name: "Actions for Sam Ortiz" }).click();
    const sheet = page.getByRole("dialog", { name: "Sam Ortiz" });
    await expect(sheet.getByText(`Sam Ortiz · @${sam.username} · Member`)).toBeVisible();
    await sheet.getByRole("button", { name: "Make admin" }).click();
    await expect(sheet).toHaveCount(0);
    await expect(members(page).getByRole("listitem").filter({ hasText: "Sam Ortiz" })).toContainText("Admin");
    await members(page).getByRole("button", { name: "Actions for Sam Ortiz" }).click();
    const removeRow = page
      .getByRole("dialog", { name: "Sam Ortiz" })
      .getByRole("button", { name: "Remove from Acme…" });
    await removeRow.click();
    const confirm = page.getByRole("alertdialog", { name: "Sam Ortiz" });
    await expect(confirm).toHaveAccessibleDescription(question);
    // Cancel goes back to the row that asked.
    await confirm.getByRole("button", { name: "Cancel" }).click();
    await expect(removeRow).toBeFocused();
    await removeRow.click();
    await confirm.getByRole("button", { name: "Remove", exact: true }).click();
  } else {
    const samRow = members(page).getByRole("listitem").filter({ hasText: "Sam Ortiz" });
    await samRow.getByRole("button", { name: "Make admin" }).click();
    await expect(samRow.getByRole("button", { name: "Make member" })).toBeVisible();
    const remove = samRow.getByRole("button", { name: "Remove" });
    await remove.click();
    const confirm = page.getByRole("alertdialog", { name: question });
    await expect(confirm).toBeFocused();
    // Escape puts focus back on Remove.
    await page.keyboard.press("Escape");
    await expect(confirm).toHaveCount(0);
    await expect(remove).toBeFocused();
    await remove.click();
    await page.getByRole("alertdialog", { name: question }).getByRole("button", { name: "Cancel" }).click();
    await expect(remove).toBeFocused();
    await remove.click();
    await page.getByRole("alertdialog", { name: question }).getByRole("button", { name: "Remove" }).click();
  }
  await expect(page.getByRole("heading", { name: "Members · 1" })).toBeVisible();
  await expect(members(page).getByText("Sam Ortiz")).toHaveCount(0);
});

test("a heap's ⋯ renames it, archives and unarchives it, and shows its history", async ({ page, heapPath }) => {
  // The team's page, in either build: there the user is the team's admin.
  await page.goto(heapPath.replace(/\/heaps\/[^/]+$/, ""));
  await page.getByRole("button", { name: "+ New heap" }).click();
  await page.getByLabel("New heap name").fill("Garden");
  await page.getByRole("button", { name: "Add heap" }).click();

  const choose = async (heap: string, action: string) => {
    await heaps(page)
      .getByRole("button", { name: `Actions for ${heap}` })
      .click();
    // A menu: a popover on a wider screen, a sheet from the bottom on a phone.
    await page.getByRole("menuitem", { name: action, exact: true }).click();
  };

  await choose("Garden", "Rename");
  const name = page.getByLabel("Name of heap Garden");
  await expect(name).toBeFocused();
  await name.fill("Allotment");
  await name.press("Enter");
  await expect(heaps(page).getByText("Allotment", { exact: true })).toBeVisible();
  // Focus goes back to the ⋯ the editor came from, after Save and after Escape.
  await expect(heaps(page).getByRole("button", { name: "Actions for Allotment" })).toBeFocused();
  await choose("Allotment", "Link name");
  const link = page.getByLabel("Link name of heap Allotment");
  await expect(link).toBeFocused();
  await link.press("Escape");
  await expect(link).toHaveCount(0);
  await expect(heaps(page).getByRole("button", { name: "Actions for Allotment" })).toBeFocused();

  // Archiving offers Undo in the toast.
  await choose("Allotment", "Archive");
  await expect(heaps(page).getByText("Archived", { exact: true })).toBeVisible();
  const toast = page.getByRole("status").filter({ hasText: "“Allotment” is archived." });
  await expect(toast).toBeVisible();
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(heaps(page).getByText("Archived", { exact: true })).toHaveCount(0);
  await choose("Allotment", "Archive");
  await expect(heaps(page).getByText("Archived", { exact: true })).toBeVisible();
  await choose("Allotment", "Unarchive");
  await expect(heaps(page).getByText("Archived", { exact: true })).toHaveCount(0);

  await choose("Allotment", "History");
  // The rename, with the name before it struck through.
  await expect(heaps(page).getByRole("deletion").filter({ hasText: "Garden" })).toBeVisible();
});

test("the team's ⋯ renames it, changes its link name and opens its activity", async ({ page, localBuild }) => {
  test.skip(localBuild, SERVER_ONLY);
  const linkName = `t-${crypto.randomUUID().slice(0, 8)}`;
  await createTeam(page, "Orchard");
  const more = page.getByRole("button", { name: /^More for / });

  await more.click();
  await page.getByRole("menuitem", { name: "Rename" }).click();
  const name = page.getByLabel("Team name");
  await expect(name).toBeFocused();
  await name.fill("Orchard Co");
  await name.press("Enter");
  await expect(page.getByRole("heading", { name: "Orchard Co", level: 1 })).toBeVisible();
  await expect(more).toBeFocused();

  await more.click();
  await page.getByRole("menuitem", { name: "Link name" }).click();
  const link = page.getByLabel("Team link name");
  await expect(link).toBeFocused();
  // Escape leaves it as it was.
  await link.press("Escape");
  await expect(link).toHaveCount(0);
  await expect(more).toBeFocused();
  // So does Enter with nothing changed.
  await more.click();
  await page.getByRole("menuitem", { name: "Link name" }).click();
  await expect(link).toBeFocused();
  await link.press("Enter");
  await expect(link).toHaveCount(0);
  await expect(more).toBeFocused();
  await more.click();
  await page.getByRole("menuitem", { name: "Link name" }).click();
  await link.fill(linkName);
  await link.press("Enter");
  await expect(page).toHaveURL(new RegExp(`/teams/${linkName}$`));

  await more.click();
  await page.getByRole("menuitem", { name: "Activity" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${linkName}/activity$`));
});

test("leaving asks first, and the last admin is told why they can't", async ({ page, request, localBuild }) => {
  test.skip(localBuild, SERVER_ONLY);
  const grace = await registerOther(request, "Grace Hopper");
  await createTeam(page, "Rota");
  await expect(page.getByText("You are the only admin, so you can't leave until someone else is one.")).toBeVisible();
  await addMember(page, grace.username);
  if (isPhone(page)) {
    await members(page).getByRole("button", { name: "Actions for Grace Hopper" }).click();
    await page.getByRole("dialog", { name: "Grace Hopper" }).getByRole("button", { name: "Make admin" }).click();
  } else {
    await members(page)
      .getByRole("listitem")
      .filter({ hasText: "Grace Hopper" })
      .getByRole("button", { name: "Make admin" })
      .click();
  }

  const leave = page.getByRole("button", { name: "Leave team" });
  await leave.click();
  await page.getByRole("group", { name: "Leave team" }).getByRole("button", { name: "Cancel" }).click();
  await expect(leave).toBeFocused();
  await leave.click();
  await expect(page.getByText("Leave Rota? You'll lose access to its heaps.")).toBeVisible();
  await page.getByRole("button", { name: "Leave", exact: true }).click();
  await expect(page).toHaveURL(/\/teams$/);
  await expect(page.getByRole("link", { name: "Open Rota" })).toHaveCount(0);
});

test("giving up your own admin role asks first", async ({ page, request, localBuild }) => {
  test.skip(localBuild, SERVER_ONLY);
  const sam = await registerOther(request, "Sam Ortiz");
  await createTeam(page, "Acme");
  await addMember(page, sam.username);
  const question =
    "Give up your admin role in Acme? You'll no longer manage its members and heaps, and only another admin can make you an admin again.";
  const myRow = members(page).getByRole("listitem").filter({ hasText: "(you)" });

  // Making someone else an admin acts at once.
  if (isPhone(page)) {
    await members(page).getByRole("button", { name: "Actions for Sam Ortiz" }).click();
    await page.getByRole("dialog", { name: "Sam Ortiz" }).getByRole("button", { name: "Make admin" }).click();
    await expect(members(page).getByRole("listitem").filter({ hasText: "Sam Ortiz" })).toContainText("Admin");

    await myRow.getByRole("button", { name: /^Actions for / }).click();
    const sheet = page.getByRole("dialog", { name: "Tester" });
    const makeMember = sheet.getByRole("button", { name: "Make member…" });
    await makeMember.click();
    const confirm = page.getByRole("alertdialog", { name: "Tester" });
    await expect(confirm).toHaveAccessibleDescription(question);
    // Cancel goes back to the row that asked.
    await confirm.getByRole("button", { name: "Cancel" }).click();
    await expect(makeMember).toBeFocused();
    await makeMember.click();
    await confirm.getByRole("button", { name: "Give up admin" }).click();
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(myRow).toContainText("Member");
  } else {
    await members(page)
      .getByRole("listitem")
      .filter({ hasText: "Sam Ortiz" })
      .getByRole("button", { name: "Make admin" })
      .click();
    await expect(members(page).getByRole("listitem").filter({ hasText: "Sam Ortiz" })).toContainText("Admin");

    const makeMember = myRow.getByRole("button", { name: "Make member" });
    await makeMember.click();
    const confirm = page.getByRole("alertdialog", { name: question });
    await expect(confirm).toBeFocused();
    await expect(makeMember).toHaveAttribute("aria-expanded", "true");
    // Escape and Cancel put focus back on Make member.
    await page.keyboard.press("Escape");
    await expect(confirm).toHaveCount(0);
    await expect(makeMember).toBeFocused();
    await makeMember.click();
    await confirm.getByRole("button", { name: "Cancel" }).click();
    await expect(makeMember).toBeFocused();
    await makeMember.click();
    await confirm.getByRole("button", { name: "Give up admin" }).click();
    await expect(confirm).toHaveCount(0);
    await expect(myRow).toContainText("Member");
    await expect(myRow.getByRole("button", { name: /^Make (member|admin)$/ })).toHaveCount(0);
  }
});

test("Enter waits for the results of what was typed, not the ones still on screen", async ({
  page,
  request,
  localBuild,
}) => {
  test.skip(localBuild, SERVER_ONLY);
  test.skip(isPhone(page), "A phone's Enter never adds anyone; the member test covers that.");
  const sam = await registerOther(request, "Sam Ortiz");
  await createTeam(page, "Waiting");
  const search = memberSearch(page);
  await search.fill(sam.username);
  await expect(page.getByRole("option", { selected: true })).toContainText(`@${sam.username}`);

  // The next query's answer is held back, so Sam's results stay on screen for text that no longer names him.
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/v1/users?**", async (route) => {
    await held;
    await route.continue();
  });
  await search.fill(`${sam.username}-nobody`);
  await expect(page.getByRole("listbox", { name: "Matching users" })).toHaveAttribute("aria-busy", "true");
  await expect(page.getByRole("option", { selected: true })).toHaveCount(0);
  await search.press("Enter");
  release();
  await expect(page.getByText("No one matches.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Members · 1" })).toBeVisible();
  await expect(members(page).getByText("Sam Ortiz")).toHaveCount(0);
});

const inviteLinks = (page: Page) => page.getByRole("region", { name: "Invite links" });
const inviteLinkButton = (page: Page) => page.getByRole("button", { name: "Invite link", exact: true });
const REVOKE_QUESTION = "Revoke this link? It stops working at once. People who joined through it stay.";

test("an admin makes, copies and revokes an invite link", async ({ page, context, localBuild }) => {
  test.skip(localBuild, SERVER_ONLY);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await createTeam(page, "Gate");
  const links = inviteLinks(page);
  await expect(links.getByRole("heading", { name: "Invite links" })).toBeVisible();
  await expect(links.getByText("Anyone with a link joins as a member.")).toBeVisible();
  await expect(links.getByRole("listitem")).toHaveCount(0);

  // The new link comes first, marked New, with focus on its Copy.
  await inviteLinkButton(page).click();
  const link = links.getByRole("listitem");
  await expect(link).toHaveCount(1);
  await expect(link).toContainText("Made by you");
  await expect(link).toContainText("New");
  await expect(link).toContainText(/ends .+/i);
  const copy = link.getByRole("button", { name: "Copy", exact: true });
  await expect(copy).toBeFocused();

  await copy.click();
  await expect(links.getByRole("status")).toHaveText("Link copied.");
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toMatch(new RegExp(`^${new URL(page.url()).origin}/invite/[A-Za-z0-9_-]{43}$`));

  // Revoke asks first; Cancel goes back to Revoke.
  if (isPhone(page)) {
    await link.getByRole("button", { name: /^Actions for the link / }).click();
    const sheet = page.getByRole("dialog", { name: "Invite link" });
    const revokeRow = sheet.getByRole("button", { name: "Revoke link…" });
    await revokeRow.click();
    const confirm = page.getByRole("alertdialog", { name: "Invite link" });
    await expect(confirm).toHaveAccessibleDescription(REVOKE_QUESTION);
    await confirm.getByRole("button", { name: "Cancel" }).click();
    await expect(revokeRow).toBeFocused();
    await revokeRow.click();
    await confirm.getByRole("button", { name: "Revoke", exact: true }).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
  } else {
    const revoke = link.getByRole("button", { name: "Revoke", exact: true });
    await revoke.click();
    const confirm = page.getByRole("alertdialog", { name: REVOKE_QUESTION });
    await expect(confirm).toBeFocused();
    await confirm.getByRole("button", { name: "Cancel" }).click();
    await expect(revoke).toBeFocused();
    await revoke.click();
    await page.keyboard.press("Escape");
    await expect(revoke).toBeFocused();
    await revoke.click();
    await confirm.getByRole("button", { name: "Revoke", exact: true }).click();
  }
  await expect(links.getByRole("listitem")).toHaveCount(0);
  await expect(links.getByRole("heading", { name: "Invite links" })).toBeFocused();

  // The team's Activity words both.
  await page.goto(`${new URL(page.url()).pathname}/activity`);
  await expect(page.getByText("You made an invite link")).toBeVisible();
  await expect(page.getByText("You revoked Tester's invite link")).toBeVisible();
});

test("a member sees no invite links, and Activity says who joined through one", async ({
  page,
  browser,
  localBuild,
}, testInfo) => {
  test.skip(localBuild, SERVER_ONLY);
  await createTeam(page, "Door");
  const teamPath = new URL(page.url()).pathname;
  await inviteLinkButton(page).click();
  await expect(inviteLinks(page).getByRole("listitem")).toHaveCount(1);

  // Someone else joins through the link, in a context of their own.
  const other = await browser.newContext(testInfo.project.use);
  try {
    const user = uniqueUser();
    const registered = await other.request.post("/api/v1/auth/register", {
      headers: { "x-csrf-protection": "1" },
      data: { ...user, display_name: "Sam Ortiz", password: PASSWORD },
    });
    expect(registered.status()).toBe(201);
    const listed = await page.request.get(`/api/v1${teamPath}/invite-links`);
    const token = ((await listed.json()) as { links: { token: string }[] }).links[0]?.token;
    expect(token).toBeTruthy();
    const joined = await other.request.post(`/api/v1/invites/${token}/join`, { headers: { "x-csrf-protection": "1" } });
    expect(joined.status()).toBe(201);

    // A member sees the team, and none of the invite links.
    const theirs = await other.newPage();
    await theirs.goto(teamPath);
    await expect(theirs.getByRole("heading", { name: "Door", level: 1 })).toBeVisible();
    await expect(theirs.getByRole("heading", { name: /^Members/ })).toBeVisible();
    await expect(inviteLinkButton(theirs)).toHaveCount(0);
    await expect(theirs.getByRole("heading", { name: "Invite links" })).toHaveCount(0);
  } finally {
    await other.close();
  }

  await page.goto(`${teamPath}/activity`);
  await expect(page.getByText(/Sam Ortiz joined through Tester.s invite link/)).toBeVisible();
});

test("a copy the browser refuses shows the link to copy by hand", async ({ page, localBuild }) => {
  test.skip(localBuild, SERVER_ONLY);
  test.skip(page.viewportSize()?.width !== 1280, "One width is enough for the fallback.");
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("denied")) },
      configurable: true,
    });
  });
  await createTeam(page, "Plain");
  await inviteLinkButton(page).click();
  const link = inviteLinks(page).getByRole("listitem");
  await link.getByRole("button", { name: "Copy", exact: true }).click();
  const field = link.getByRole("textbox", { name: "Invite link" });
  await expect(field).toHaveValue(new RegExp(`^${new URL(page.url()).origin}/invite/[A-Za-z0-9_-]{43}$`));
  await expect(field).toBeFocused();
  await expect(link.getByText("Copy it from here.")).toBeVisible();
  await expect(inviteLinks(page).getByRole("status")).toBeEmpty();
  await field.press("Escape");
  await expect(field).toHaveCount(0);
});
