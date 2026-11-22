import { addItem, expect, markDoneFromList, row, test } from "./fixtures";

// Not-found pages, the connection banner and the request ID on server errors (ux.md, Conflicts and errors).

test("an unknown address shows Page not found, and its button goes to Waiting for me", async ({ page }) => {
  await page.goto("/no-such-page");
  await expect(page.getByRole("heading", { name: "Page not found", level: 1 })).toBeVisible();
  await page.getByRole("link", { name: "Go to Waiting for me" }).click();
  await expect(page).toHaveURL(/\/waiting$/);
});

test("a missing team shows its own page with a link to the teams", async ({ page }) => {
  await page.goto("/teams/no-such-team");
  await expect(page.getByRole("heading", { name: "This team isn't here", level: 1 })).toBeVisible();
  await page.getByRole("link", { name: "See your teams" }).click();
  await expect(page).toHaveURL(/\/teams$/);
});

test("a missing heap shows its own page", async ({ page, heapPath }) => {
  const team = /\/teams\/([^/]+)\//.exec(heapPath)?.[1];
  await page.goto(`/teams/${team}/heaps/no-such-heap`);
  await expect(page.getByRole("heading", { name: "This heap isn't here", level: 1 })).toBeVisible();
});

test("a missing item stays in the pane, and its button closes it", async ({ page, heapPath }) => {
  await page.goto(`${heapPath}/items/zz9zz`);
  const pane = page.locator("[data-item-pane]");
  await expect(pane.getByText("This item isn't here")).toBeVisible();
  // On a heap's list the button is named for the heap, not "the list".
  await expect(pane.getByRole("button", { name: /^Back to (?!the list$)/ })).toBeVisible();
  await pane.getByRole("button", { name: /^Back to / }).click();
  await expect(page).toHaveURL(new RegExp(`${heapPath}$`));
  await expect(pane).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add item", exact: true }).first()).toBeVisible();
});

test("a server error toast shows the request ID, stays until closed, and copies it", async ({
  page,
  context,
  browserName,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await addItem(page, "Water the plants");
  await page.clock.install();
  const id = crypto.randomUUID();
  await page.route("**/api/v1/teams/*/heaps/*/items/*", (route) => {
    if (route.request().method() !== "PATCH") return route.fallback();
    return route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      headers: { "x-request-id": id },
      body: JSON.stringify({
        status: 500,
        code: "internal",
        title: "Something went wrong on the server",
        request_id: id,
      }),
    });
  });

  await markDoneFromList(page, "Water the plants");
  const toast = page.getByRole("alert").filter({ hasText: "Request ID" });
  await expect(toast).toContainText(id);

  if (browserName === "chromium") {
    await toast.getByRole("button", { name: "Copy" }).click();
    await expect(toast.getByRole("button", { name: "Copied" })).toBeVisible();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(id);
  }

  await page.clock.runFor(10_000);
  await expect(toast).toBeVisible();
  await toast.getByRole("button", { name: "Dismiss" }).click();
  await expect(toast).toHaveCount(0);
});

test("offline shows the banner and an edit fails at once", async ({ page, context }) => {
  await addItem(page, "Water the plants");
  await context.setOffline(true);
  const banner = page.getByRole("status").filter({ hasText: "You're offline." });
  await expect(banner).toBeVisible();

  await markDoneFromList(page, "Water the plants");
  await expect(page.getByRole("alert").filter({ hasText: "You're offline, so this wasn't saved." })).toBeVisible();
  await expect(row(page, "Water the plants")).toBeVisible();

  await context.setOffline(false);
  await expect(banner).toHaveCount(0);
});

test("when the server can't be reached the banner offers Try now", async ({ page }) => {
  await addItem(page, "Water the plants");
  await page.route("**/api/v1/**", (route) => route.abort());

  await markDoneFromList(page, "Water the plants");
  const banner = page.getByRole("status").filter({ hasText: "Can't reach the server." });
  await expect(banner).toBeVisible();
  await expect(
    page.getByRole("alert").filter({ hasText: "Couldn't reach the server, so this wasn't saved." }),
  ).toBeVisible();

  await page.unroute("**/api/v1/**");
  await banner.getByRole("button", { name: "Try now" }).click();
  await expect(banner).toHaveCount(0);
});

test("a proxy's bare 502 reads as the server being out of reach", async ({ page }) => {
  await addItem(page, "Water the plants");
  await page.route("**/api/v1/**", (route) =>
    route.fulfill({ status: 502, contentType: "text/plain", body: "Bad Gateway" }),
  );

  await markDoneFromList(page, "Water the plants");
  await expect(page.getByRole("status").filter({ hasText: "Can't reach the server." })).toBeVisible();
});
