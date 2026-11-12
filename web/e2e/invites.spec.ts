// The invite page (design/ux.md, Invite page): the golden path of stage 1 on a server with registration closed,
// and a small test for each state of the page.
import type { Browser, BrowserContext, Page, TestInfo } from "@playwright/test";

import { test as closed } from "./closedServer";
import { createTeam, expect, expectFitsWidth, HEAP_URL, isPhone, PASSWORD, test, uniqueUser } from "./fixtures";

const CSRF = { "x-csrf-protection": "1" };

/** The invite links panel of the Team page. */
const inviteLinks = (page: Page) => page.getByRole("region", { name: "Invite links" });

/** Presses Revoke on the team page's first link, and confirms, in the wide or the phone way. */
async function revokeFirstLink(page: Page) {
  const link = inviteLinks(page).getByRole("listitem").first();
  if (isPhone(page)) {
    await link.getByRole("button", { name: /^Actions for the link / }).click();
    await page.getByRole("dialog", { name: "Invite link" }).getByRole("button", { name: "Revoke link…" }).click();
    await page
      .getByRole("alertdialog", { name: "Invite link" })
      .getByRole("button", { name: "Revoke", exact: true })
      .click();
  } else {
    await link.getByRole("button", { name: "Revoke", exact: true }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "Revoke", exact: true }).click();
  }
  await expect(inviteLinks(page).getByRole("listitem")).toHaveCount(0);
}

// The golden path of stage 1, from an admin making a link to a newcomer and an existing user using it.
closed(
  "an invite link brings a newcomer and an existing user into a team on a server with registration closed",
  async ({ browser, closedServer }, testInfo) => {
    const { baseURL, admin, member } = closedServer;
    const person = (permissions: string[] = []) =>
      browser.newContext({ ...testInfo.project.use, baseURL, permissions });
    const ana = await person(["clipboard-read", "clipboard-write"]);
    const newcomerContext = await person();
    const samContext = await person();
    try {
      // Ana signs in, makes the team Acme with its one heap, and copies a link from the Team page.
      const page = await ana.newPage();
      await page.goto(`${baseURL}/login`);
      await page.getByLabel("Email or username").fill(admin.email);
      await page.getByLabel("Password").fill(PASSWORD);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await expect(page).toHaveURL(HEAP_URL);
      await createTeam(page, "Acme");
      const teamPath = new URL(page.url()).pathname;
      const team = teamPath.split("/").pop();
      const heap = await page.request.post(`/api/v1/teams/${team}/heaps`, { headers: CSRF, data: { name: "Launch" } });
      expect(heap.status()).toBe(201);
      await page.goto(teamPath);
      await page.getByRole("button", { name: "Invite link", exact: true }).click();
      await inviteLinks(page).getByRole("listitem").first().getByRole("button", { name: "Copy", exact: true }).click();
      await expect(inviteLinks(page).getByRole("status")).toHaveText("Link copied.");
      const link = await page.evaluate(() => navigator.clipboard.readText());
      expect(link).toMatch(new RegExp(`^${baseURL}/invite/[A-Za-z0-9_-]{43}$`));

      // A newcomer with no account opens it and sees who invited them, with Create account selected.
      const newcomer = await newcomerContext.newPage();
      await newcomer.goto(link);
      await expect(newcomer.getByRole("heading", { name: "Join Acme", exact: true })).toBeVisible();
      await expect(newcomer.getByText("Ana Ruiz invited you.")).toBeVisible();
      await expect(newcomer.getByRole("tab", { name: "Create account" })).toHaveAttribute("aria-selected", "true");
      await expect(newcomer.getByText("You'll join Acme as a member, and get a heap of your own too.")).toBeVisible();
      await expectFitsWidth(newcomer, "The invite page");

      // They create an account, and land on the team's only heap, with a line saying they joined.
      const { username, email } = uniqueUser();
      await newcomer.getByLabel("Name", { exact: true }).fill("Nia Park");
      await newcomer.getByLabel("Username").fill(username);
      await newcomer.getByLabel("Email").fill(email);
      await newcomer.getByLabel("Password").fill(PASSWORD);
      await newcomer.getByRole("button", { name: "Create account and join" }).click();
      await expect(newcomer).toHaveURL(`${baseURL}/teams/${team}/heaps/launch`);
      await expect(newcomer.getByRole("heading", { name: "Launch" })).toBeVisible();
      await expect(newcomer.getByText("You joined Acme.")).toBeVisible();

      // Reloading doesn't bring the line back.
      await newcomer.reload();
      await expect(newcomer.getByRole("heading", { name: "Launch" })).toBeVisible();
      await expect(newcomer.getByText("You joined Acme.")).toHaveCount(0);

      // Plain registration on this server is still closed.
      const plain = await newcomerContext.request.post(`${baseURL}/api/v1/auth/register`, {
        headers: CSRF,
        data: { ...uniqueUser(), display_name: "Eve", password: PASSWORD },
      });
      expect(plain.status()).toBe(403);
      expect(((await plain.json()) as { code: string }).code).toBe("registration_closed");

      // Sam, who has an account, opens the link signed out, signs in, confirms, and lands the same way.
      const sam = await samContext.newPage();
      await sam.goto(link);
      await sam.getByRole("tab", { name: "Sign in" }).click();
      await sam.getByLabel("Email or username").fill(member.email);
      await sam.getByLabel("Password").fill(PASSWORD);
      await sam.getByRole("button", { name: "Sign in", exact: true }).click();
      await expect(sam.getByRole("heading", { name: "Join Acme?" })).toBeVisible();
      await expect(sam.getByText(`You're signed in as Sam Ortiz (@${member.username}).`)).toBeVisible();
      await sam.getByRole("button", { name: "Join Acme", exact: true }).click();
      await expect(sam).toHaveURL(`${baseURL}/teams/${team}/heaps/launch`);
      await expect(sam.getByText("You joined Acme.")).toBeVisible();

      // Ana revokes the link, and it says so to whoever opens it after.
      await page.goto(teamPath);
      await revokeFirstLink(page);
      // A member who opens it lands on the team whatever the link's state.
      await sam.goto(link);
      await expect(sam).toHaveURL(`${baseURL}/teams/${team}/heaps/launch`);
      await expect(sam.getByText("You're already in Acme.")).toBeVisible();
      // Anyone else is told it was revoked; a fresh context is signed out.
      const visitorContext = await person();
      try {
        const visitor = await visitorContext.newPage();
        await visitor.goto(link);
        await expect(visitor.getByRole("heading", { name: "This invite link was revoked." })).toBeVisible();
        await expect(visitor.getByText("It was for Acme, from Ana Ruiz.")).toBeVisible();
      } finally {
        await visitorContext.close();
      }
    } finally {
      await Promise.all([ana.close(), newcomerContext.close(), samContext.close()]);
    }
  },
);

// Each test below makes its own admin (the page), team and people, on the suite's main server.

type Team = { slug: string };
type Person = {
  context: BrowserContext;
  page: Page;
  display_name: string;
  email: string;
  username: string;
  id: string;
};

/** Makes the page's user the admin of a team called Acme, with `heaps` heaps, and a link to it. */
async function acmeWithLink(page: Page, heaps: number) {
  const created = await page.request.post("/api/v1/teams", {
    headers: CSRF,
    data: { name: "Acme", slug: `t-${crypto.randomUUID().slice(0, 8)}` },
  });
  expect(created.status()).toBe(201);
  const { slug } = (await created.json()) as Team;
  for (let index = 1; index <= heaps; index++) {
    const heap = await page.request.post(`/api/v1/teams/${slug}/heaps`, {
      headers: CSRF,
      data: { name: `Heap ${index}` },
    });
    expect(heap.status()).toBe(201);
  }
  const made = await page.request.post(`/api/v1/teams/${slug}/invite-links`, { headers: CSRF });
  expect(made.status()).toBe(201);
  const invite = (await made.json()) as { id: string; token: string };
  return { slug, id: invite.id, token: invite.token, path: `/invite/${invite.token}` };
}

async function newPerson(browser: Browser, testInfo: TestInfo, display_name: string, signedUp = true): Promise<Person> {
  const context = await browser.newContext(testInfo.project.use);
  const user = uniqueUser();
  let id = "";
  if (signedUp) {
    const registered = await context.request.post("/api/v1/auth/register", {
      headers: CSRF,
      data: { ...user, display_name, password: PASSWORD },
    });
    expect(registered.status()).toBe(201);
    id = ((await registered.json()) as { id: string }).id;
  }
  return { context, page: await context.newPage(), display_name, ...user, id };
}

test("a link that isn't one says so, and Open Upheap leads on", async ({ page }) => {
  await page.goto("/invite/nonsense");
  await expect(page.getByRole("heading", { name: "This isn't an invite link." })).toBeVisible();
  await expect(page.getByText("Check that the whole link was copied, or ask the team for a new one.")).toBeVisible();
  await expect(page.getByRole("link", { name: "Open Upheap" })).toHaveAttribute("href", "/");
  // The link names no team. (An ended link has no test here: the API has no way to end one early, and the server's
  // own tests cover it.)
  await expect(page.getByRole("heading", { name: /Acme/ })).toHaveCount(0);
});

test("someone removed from the team is told this link can't add them back", async ({ page, browser }, testInfo) => {
  const acme = await acmeWithLink(page, 1);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  try {
    const added = await page.request.post(`/api/v1/teams/${acme.slug}/members`, {
      headers: CSRF,
      data: { user_id: sam.id },
    });
    expect(added.ok()).toBe(true);
    const removed = await page.request.delete(`/api/v1/teams/${acme.slug}/members/${sam.username}`, { headers: CSRF });
    expect(removed.ok()).toBe(true);

    await sam.page.goto(acme.path);
    await expect(sam.page.getByRole("heading", { name: "You were removed from Acme." })).toBeVisible();
    await expect(sam.page.getByText("This link can't add you back. Ask one of the team's admins.")).toBeVisible();
    await expect(sam.page.getByRole("link", { name: "Open Upheap" })).toBeVisible();
  } finally {
    await sam.context.close();
  }
});

test("a member who opens the link lands on the team with a line saying they're in", async ({
  page,
  browser,
}, testInfo) => {
  const acme = await acmeWithLink(page, 0);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  try {
    const joined = await sam.context.request.post(`/api/v1/invites/${acme.token}/join`, { headers: CSRF });
    expect(joined.status()).toBe(201);

    await sam.page.goto(acme.path);
    // No heaps, so the Team page; and Back doesn't return to the invite page.
    await expect(sam.page).toHaveURL(`/teams/${acme.slug}`);
    await expect(sam.page.getByText("You're already in Acme.")).toBeVisible();
    await sam.page.goBack();
    await expect(sam.page).not.toHaveURL(/\/invite\//);
  } finally {
    await sam.context.close();
  }
});

test("a team with two heaps lands a new member on its Team page", async ({ page, browser }, testInfo) => {
  const acme = await acmeWithLink(page, 2);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  try {
    await sam.page.goto(acme.path);
    await sam.page.getByRole("button", { name: "Join Acme", exact: true }).click();
    await expect(sam.page).toHaveURL(`/teams/${acme.slug}`);
    await expect(sam.page.getByText("You joined Acme.")).toBeVisible();
  } finally {
    await sam.context.close();
  }
});

test("a taken email shows under Email, and Sign in instead keeps it", async ({ page, browser }, testInfo) => {
  const acme = await acmeWithLink(page, 1);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  const visitor = await newPerson(browser, testInfo, "Visitor", false);
  try {
    await visitor.page.goto(acme.path);
    await visitor.page.getByLabel("Name", { exact: true }).fill("Not Sam");
    await visitor.page.getByLabel("Username").fill(uniqueUser().username);
    await visitor.page.getByLabel("Email").fill(sam.email);
    await visitor.page.getByLabel("Password").fill(PASSWORD);
    await visitor.page.getByRole("button", { name: "Create account and join" }).click();

    const email = visitor.page.getByLabel("Email");
    await expect(email).toHaveAttribute("aria-invalid", "true");
    await expect(email).toHaveAccessibleDescription("There's already an account with this email. Sign in instead.");
    await expect(email).toBeFocused();
    await visitor.page.getByRole("button", { name: "Sign in instead" }).click();
    await expect(visitor.page.getByRole("tab", { name: "Sign in" })).toHaveAttribute("aria-selected", "true");
    await expect(visitor.page.getByLabel("Email or username")).toHaveValue(sam.email);
  } finally {
    await visitor.context.close();
    await sam.context.close();
  }
});

test("a link revoked while the form is open turns the page into 'was revoked'", async ({ page, browser }, testInfo) => {
  const acme = await acmeWithLink(page, 1);
  const visitor = await newPerson(browser, testInfo, "Visitor", false);
  try {
    await visitor.page.goto(acme.path);
    await visitor.page.getByLabel("Name", { exact: true }).fill("Nia Park");
    await visitor.page.getByLabel("Username").fill(uniqueUser().username);
    await visitor.page.getByLabel("Email").fill(uniqueUser().email);
    await visitor.page.getByLabel("Password").fill(PASSWORD);

    const revoked = await page.request.post(`/api/v1/teams/${acme.slug}/invite-links/${acme.id}/revoke`, {
      headers: CSRF,
    });
    expect(revoked.ok()).toBe(true);

    await visitor.page.getByRole("button", { name: "Create account and join" }).click();
    const heading = visitor.page.getByRole("heading", { name: "This invite link was revoked." });
    await expect(heading).toBeVisible();
    await expect(heading).toBeFocused();
    await expect(visitor.page.getByText("It was for Acme, from Tester.")).toBeVisible();
  } finally {
    await visitor.context.close();
  }
});

test("Not you? Sign out leaves the same page, signed out", async ({ page, browser }, testInfo) => {
  const acme = await acmeWithLink(page, 1);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  try {
    await sam.page.goto(acme.path);
    await expect(sam.page.getByRole("heading", { name: "Join Acme?" })).toBeVisible();
    await expect(sam.page.getByText(`You're signed in as Sam Ortiz (@${sam.username}).`)).toBeVisible();
    await sam.page.getByRole("button", { name: "Sign out", exact: true }).click();
    await expect(sam.page).toHaveURL(acme.path);
    await expect(sam.page.getByRole("heading", { name: "Join Acme", exact: true })).toBeVisible();
    await expect(sam.page.getByRole("tab", { name: "Create account" })).toHaveAttribute("aria-selected", "true");
  } finally {
    await sam.context.close();
  }
});

test("a session that ends while the confirm is open leaves the user on the page, signed out", async ({
  page,
  browser,
}, testInfo) => {
  const acme = await acmeWithLink(page, 1);
  const sam = await newPerson(browser, testInfo, "Sam Ortiz");
  try {
    await sam.page.goto(acme.path);
    await expect(sam.page.getByRole("heading", { name: "Join Acme?" })).toBeVisible();
    // The session ends elsewhere, so joining answers 401: the page doesn't go to /login, it shows the signed-out state.
    const loggedOut = await sam.context.request.post("/api/v1/auth/logout", { headers: CSRF });
    expect(loggedOut.ok()).toBe(true);
    await sam.page.getByRole("button", { name: "Join Acme", exact: true }).click();
    await expect(sam.page.getByRole("heading", { name: "Join Acme", exact: true })).toBeVisible();
    await expect(sam.page).toHaveURL(acme.path);
  } finally {
    await sam.context.close();
  }
});
