import { expect, test } from "@playwright/test";

import { HEAP_URL, signOut, uniqueUser } from "./fixtures";

test("a new user registers, signs out and signs back in", async ({ page }) => {
  const { email, username } = uniqueUser();
  const password = "a long test password";

  await page.goto("/");
  await expect(page).toHaveURL(/\/login$/);
  await page.getByRole("button", { name: "New here? Create an account" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Ada");
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  // Registering creates the user's personal team, named by the username, with a first heap, and opens it.
  await expect(page).toHaveURL(new RegExp(`/teams/${username}/heaps/personal$`));
  await expect(page.getByRole("heading", { name: "Personal" })).toBeVisible();
  await expect(page.getByText("Nothing here yet. Add the first thing on your plate.")).toBeVisible();

  await signOut(page);
  await expect(page).toHaveURL(/\/login$/);
  await page.getByLabel("Password").fill("not the password");
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("alert")).toBeVisible();

  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(HEAP_URL);
  await expect(page.getByRole("heading", { name: "Personal" })).toBeVisible();
});
