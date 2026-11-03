import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";

// WCAG 2.2 A and AA rules from axe-core, on the page at rest, after a run with a world
// change, and after a numerical failure has been reported.
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function violations(page) {
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  return results.violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact,
    targets: violation.nodes.map((node) => node.target.join(" ")),
  }));
}

test("D15 the page at rest has no WCAG A or AA violations", async ({ page }) => {
  await openApp(page);
  expect(await violations(page)).toEqual([]);
});

test("D15 a running experiment has no WCAG A or AA violations", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 200);
  await page.click("#change-world");
  await page.click("#step");
  await page.locator("#weight-row-4").click();
  expect(await violations(page)).toEqual([]);
});

test("D15 the dark color scheme has no WCAG A or AA violations", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await openApp(page);
  await runUntil(page, 100);
  expect(await violations(page)).toEqual([]);
});
