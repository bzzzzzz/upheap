// The Email card on Account and the change-email page (ux.md, Email and Change-email page): the golden path of
// a person who changes their address, confirmed by a link mailed to the new one, on a server with mail set up.
import { test as closed } from "./closedServer";
import { expect, expectFitsWidth, goTo, HEAP_URL, PASSWORD, uniqueUser } from "./fixtures";

closed(
  "a user changes their address with a link mailed to the new one",
  async ({ browser, closedServer }, testInfo) => {
    const { baseURL, mover, mailedLink } = closedServer;
    const context = await browser.newContext({ ...testInfo.project.use, baseURL });
    try {
      // Signed in by username, which a change of address leaves as it is.
      const page = await context.newPage();
      await page.goto("/login");
      await page.getByLabel("Email or username").fill(mover.username);
      await page.getByLabel("Password").fill(PASSWORD);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await expect(page).toHaveURL(HEAP_URL);

      await goTo(page, "Account");
      const card = page.getByRole("region", { name: "Email", exact: true });
      await expect(card.getByRole("button", { name: "Change" })).toBeVisible();
      // The address is in the card, and no longer in Profile.
      await expect(page.getByRole("region", { name: "Profile", exact: true })).not.toContainText("Email");

      await card.getByRole("button", { name: "Change" }).click();
      await expect(card.getByLabel("New address")).toBeFocused();
      await expectFitsWidth(page, "The Email card, open");

      const { email: newEmail } = uniqueUser();
      const asked = new Date();
      await card.getByLabel("New address").fill(newEmail);
      await card.getByLabel("Current password").fill(PASSWORD);
      await card.getByRole("button", { name: "Send link" }).click();
      await expect(card.getByRole("status")).toHaveText(
        `Check ${newEmail}. Your address changes when you open the link. It works for a day.`,
      );
      await expect(card.getByRole("button", { name: "Change" })).toBeFocused();

      // Opening the link shows a button and changes nothing; pressing it does.
      const link = await mailedLink(newEmail, asked, "change-email");
      expect(link).toMatch(new RegExp(`^${baseURL}/change-email/[A-Za-z0-9_-]{43}$`));
      await page.goto(link);
      await expect(
        page.getByRole("heading", { name: "Use the address this link was mailed to for your Upheap account?" }),
      ).toBeVisible();
      await expectFitsWidth(page, "The change-email page");
      await page.getByRole("button", { name: "Use this address" }).click();
      await expect(page.getByRole("heading", { name: `Your address is now ${newEmail}.` })).toBeVisible();

      // The link works once.
      await page.reload();
      await page.getByRole("button", { name: "Use this address" }).click();
      await expect(page.getByRole("heading", { name: "This link was already used." })).toBeVisible();

      await page.getByRole("link", { name: "Go to your account" }).click();
      await expect(page).toHaveURL(/\/account$/);
      await expect(page.getByRole("region", { name: "Email", exact: true })).toContainText(newEmail);
    } finally {
      await context.close();
    }
  },
);
