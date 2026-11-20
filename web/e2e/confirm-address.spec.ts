// The Check your mail card and the confirm page (design/ux.md, Sign in and create account, Invite page and Confirm
// page): the golden path of a person who creates an account through an invite link on a server with mail set up, asks
// for the link again, and confirms their address.
import { test as closed } from "./closedServer";
import { expect, expectFitsWidth, HEAP_URL, PASSWORD, uniqueUser } from "./fixtures";

const CSRF = { "x-csrf-protection": "1" };

closed(
  "someone who creates an account through an invite link confirms their address with the mailed link",
  async ({ browser, closedServer }, testInfo) => {
    const { baseURL, admin, mailedLink } = closedServer;
    const anaContext = await browser.newContext({ ...testInfo.project.use, baseURL });
    const newcomerContext = await browser.newContext({ ...testInfo.project.use, baseURL });
    try {
      // Ana makes a team and a link to it.
      const signedIn = await anaContext.request.post("/api/v1/auth/login", {
        headers: CSRF,
        data: { login: admin.email, password: PASSWORD },
      });
      expect(signedIn.status()).toBe(200);
      const team = await anaContext.request.post("/api/v1/teams", {
        headers: CSRF,
        data: { name: "Confirmed", slug: `t-${crypto.randomUUID().slice(0, 8)}` },
      });
      expect(team.status()).toBe(201);
      const { slug } = (await team.json()) as { slug: string };
      const made = await anaContext.request.post(`/api/v1/teams/${slug}/invite-links`, { headers: CSRF });
      expect(made.status()).toBe(201);
      const { token } = (await made.json()) as { token: string };

      // The newcomer creates an account on the invite page, and is asked to check their mail, signed out.
      const page = await newcomerContext.newPage();
      await page.goto(`/invite/${token}`);
      const { username, email } = uniqueUser();
      await page.getByLabel("Name", { exact: true }).fill("Nia Park");
      await page.getByLabel("Username").fill(username);
      await page.getByLabel("Email").fill(email);
      await page.getByLabel("Password").fill(PASSWORD);
      await page.getByRole("button", { name: "Create account and join" }).click();
      const heading = page.getByRole("heading", { name: "Check your mail" });
      await expect(heading).toBeVisible();
      await expect(heading).toBeFocused();
      await expect(page.getByText(`We sent a link to ${email}. It works for a day.`)).toBeVisible();
      await expect(page.getByText("Wrong address?")).toBeVisible();
      await expectFitsWidth(page, "The Check your mail card");

      // Send it again mails a fresh link, which cancels the first.
      const asked = new Date();
      await page.getByRole("button", { name: "Send it again" }).click();
      await expect(page.getByRole("status")).toHaveText("Sent. It can take a minute to arrive.");

      // Opening the newest link confirms nothing; pressing the button does, and signs the browser in.
      const link = await mailedLink(email, asked, "confirm");
      expect(link).toMatch(new RegExp(`^${baseURL}/confirm/[A-Za-z0-9_-]{43}$`));
      await page.goto(link);
      await expect(page.getByRole("heading", { name: "Confirm your address and sign in?" })).toBeVisible();
      await expectFitsWidth(page, "The confirm page");
      await page.getByRole("button", { name: "Confirm and sign in" }).click();
      await expect(page).toHaveURL(HEAP_URL);
      await expect(page.getByText("Address confirmed. You're signed in.")).toBeVisible();

      // The link works once.
      await page.goto(link);
      await page.getByRole("button", { name: "Confirm and sign in" }).click();
      await expect(page.getByRole("heading", { name: "This link was already used." })).toBeVisible();
    } finally {
      await Promise.all([anaContext.close(), newcomerContext.close()]);
    }
  },
);
