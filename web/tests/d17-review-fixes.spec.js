import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";

const snapshot = (page) => page.evaluate(() => window.idbdExplorer.experiment());

test("D17 an invalid setting is refused inline and Reset keeps working", async ({ page }) => {
  await openApp(page);
  await page.fill("#seed", "9007199254740992");
  await page.click("#apply-settings");

  await expect(page.locator("#settings-error")).toBeVisible();
  await expect(page.locator("#settings-error")).toContainText("seed");
  expect((await snapshot(page)).seed).toBe(1);

  await page.click("#step");
  await page.click("#reset");
  const fresh = await snapshot(page);
  expect(fresh.sample).toBe(0);
  expect(fresh.seed).toBe(1);

  await page.fill("#seed", "4");
  await page.click("#apply-settings");
  await expect(page.locator("#settings-error")).toBeHidden();
  expect((await snapshot(page)).seed).toBe(4);
});

test("D17 a diverged learner is drawn as an overflow, not as zero error", async ({ page }) => {
  await openApp(page);
  await page.fill("#lms-step", "5");
  await page.click("#apply-settings");
  await page.click("#run");
  await page.waitForFunction(() => window.idbdExplorer.experiment().failure !== null, null, {
    timeout: 30000,
  });

  const state = await snapshot(page);
  expect(state.lmsRollingSeries.some((value) => !Number.isFinite(value))).toBe(true);
  await expect(page.locator("#error-chart")).toContainText("overflow");
});

test("D17 small settings keep their digits and a tiny alpha0 keeps its axis", async ({ page }) => {
  await openApp(page);
  await page.fill("#theta", "0.000001");
  await page.fill("#alpha0", "1e-12");
  await page.click("#apply-settings");
  await expect(page.locator("#run-header")).toContainText("theta 0.000001,");
  await expect(page.locator("#run-header")).toContainText("alpha0 1e-12,");

  await page.click("#step");
  await page.click("#step");
  const ticks = await page.locator("#alpha-chart .tick-label").allTextContents();
  expect(ticks.some((text) => text.includes("e-12") || text.includes("e-13"))).toBe(true);
});

test("D17 empty states tell no samples from one sample", async ({ page }) => {
  await openApp(page);
  const metrics = (await page.locator("#metrics-pairs").innerText()).toLowerCase();
  expect(metrics).toMatch(/idbd mse, all samples\s*no sample yet/);
  await expect(page.locator("#error-chart")).toContainText("No completed samples yet");

  await page.click("#step");
  await expect(page.locator("#error-chart")).toContainText("One sample so far");
});

test("D17 Change the world shows the queued change while paused", async ({ page }) => {
  await openApp(page);
  await page.click("#change-world");
  await expect(page.locator("#event-list .pending")).toContainText("you changed the world");

  await page.click("#step");
  await expect(page.locator("#event-list .pending")).toHaveCount(0);
  await expect(page.locator("#event-list")).toContainText("(you changed the world)");
});

test("D17 the event summary counts only the samples the charts show", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 4000, "20000");
  await page.click("#pause");

  const state = await snapshot(page);
  const first = state.samples[0];
  expect(first).toBeGreaterThan(1);
  const flips = state.events.filter(
    (event) => event.kind === "signFlip" && event.sample >= first && event.sample <= state.sample,
  ).length;
  const summary = await page.locator("#event-summary").innerText();
  expect(summary).toContain(`${flips.toLocaleString("en-US")} sign flips`);
  expect(summary).toContain(`samples ${first.toLocaleString("en-US")} to`);
});

test("D17 the charts can be read from the keyboard", async ({ page }) => {
  await openApp(page);
  for (let i = 0; i < 10; i += 1) {
    await page.click("#step");
  }
  await page.focus("#error-chart");
  await page.keyboard.press("End");
  await expect(page.locator("#error-readout")).toContainText("sample 10");
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator("#error-readout")).toContainText("sample 9");
  await page.keyboard.press("Escape");
  await expect(page.locator("#error-readout")).toBeHidden();
});

test("D17 the guided input picker keeps focus and moves with arrow keys", async ({ page }) => {
  await openApp(page);
  await page.focus('#guided-feature-choice button[data-feature="0"]');
  await page.keyboard.press("ArrowRight");

  expect((await page.evaluate(() => window.idbdExplorer.guided())).selected).toBe(1);
  expect(await page.evaluate(() => document.activeElement.dataset.feature)).toBe("1");
  await page.keyboard.press("Space");
  expect(await page.evaluate(() => document.activeElement.dataset.feature)).toBe("1");
});
