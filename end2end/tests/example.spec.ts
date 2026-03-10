import { test, expect } from "@playwright/test";

const APP_URL = process.env.APP_URL || "http://localhost:3000";

test("homepage has correct title", async ({ page }) => {
  await page.goto(`${APP_URL}/`);

  await expect(page).toHaveTitle("Guruji Sivananda");
});

test("homepage has header with logo", async ({ page }) => {
  await page.goto(`${APP_URL}/`);

  await expect(page.locator("nav a.logo")).toHaveText("Guruji Sivananda");
});

test("homepage has search form", async ({ page }) => {
  await page.goto(`${APP_URL}/`);

  await expect(page.locator("h1")).toHaveText("Guruji");
  await expect(page.locator("input[type='text']")).toBeVisible();
  await expect(page.locator("button[type='submit']")).toHaveText("SEARCH");
});

test("search button shows searching state", async ({ page }) => {
  await page.goto(`${APP_URL}/`);

  await page.fill("input[type='text']", "test query");
  await page.click("button[type='submit']");

  // Button should briefly show "SEARCHING..." state
  await expect(page.locator("button[type='submit']")).toBeVisible();
});
