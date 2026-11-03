import { expect, test } from "@playwright/test";

import { openApp } from "./helpers.js";

test("D12 the paper task and the extension describe themselves differently", async ({ page }) => {
  await openApp(page);

  await page.selectOption("#preset", "sutton1992");
  const paper = await page.locator("#preset-description").innerText();
  expect(paper).toContain("the same five stay relevant");
  expect(paper).toContain("Every 20 samples");
  expect(paper).toContain("switched in sign");
  expect(paper).toContain("Experiment 1 of the paper");

  await page.selectOption("#preset", "changing-relevance");
  const extension = await page.locator("#preset-description").innerText();
  expect(extension).toContain("new stress test, not a paper reproduction");
  expect(extension).toContain("5,000 samples");

  await page.selectOption("#preset", "drifting");
  expect(await page.locator("#preset-description").innerText()).toContain(
    "not a paper reproduction",
  );
});

test("D12 every view identifies its preset, seed, version and settings", async ({ page }) => {
  await openApp(page);

  // The pair labels are set in small caps by CSS, so match without case.
  const header = await page.locator("#run-header").innerText();
  expect(header).toContain("Sutton 1992 (paper task)");
  expect(header).toContain("Sutton (1992)");
  expect(header).toMatch(/seed\n1/i);
  expect(header).toMatch(/rust core version\n\d+\.\d+\.\d+/i);
  expect(header).toContain("xoshiro256");
  expect(header).toContain("theta 0.01,");
  expect(header).toContain("theta 0.015 had the lowest mean squared error, 1.525");
  expect(header).toContain("Mahmood, Sutton, Degris and Pilarski (2012)");
  expect(header).toContain("100 samples");
  expect(header).toContain("synthetic");

  await page.selectOption("#preset", "changing-relevance");
  const extension = await page.locator("#run-header").innerText();
  expect(extension).toContain("Changing relevance (extension)");
  expect(extension).toContain("extension, not from the paper");
});

test("D12 a new seed starts a new run", async ({ page }) => {
  await openApp(page);
  await page.click("#step");
  await page.fill("#seed", "42");
  await page.click("#apply-settings");

  expect(await page.evaluate(() => window.idbdExplorer.experiment().sample)).toBe(0);
  expect(await page.locator("#run-header").innerText()).toMatch(/seed\n42/i);
});
