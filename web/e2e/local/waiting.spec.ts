// The local build has nothing to wait for (design/ux.md, Waiting for me): no entry, today's phone tabs, and no request
// for what waits.
import { expect, isPhone, test } from "../fixtures";

test("no Waiting entry, today's phone tabs, and no request for what waits", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/me/waiting")) requests.push(request.url());
  });
  await page.reload();
  await expect(page.getByRole("link", { name: /^Waiting/ })).toHaveCount(0);

  if (isPhone(page)) {
    const tabs = page.getByRole("navigation", { name: "Heap views" });
    await expect(tabs.getByRole("link", { name: "Open", exact: true })).toBeVisible();
    const names = await tabs
      .locator("a, button")
      .evaluateAll((all) =>
        all.map((element) => element.getAttribute("aria-label") ?? (element as HTMLElement).innerText.trim()),
      );
    expect(names).toEqual(["Open", "Closed", "Add item", "Settings", "More"]);
  }

  // The page's address leads back to the list.
  await page.goto("./waiting");
  await expect(page).toHaveURL(/\/teams\/[^/]+\/heaps\/[^/]+$/);
  expect(requests).toEqual([]);
});
