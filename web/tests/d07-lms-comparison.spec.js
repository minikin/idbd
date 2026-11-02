import { expect, test } from "@playwright/test";

import { experimentState, openApp, runUntil } from "./helpers.js";

test("D07 both learners' configurations are displayed", async ({ page }) => {
  await openApp(page);

  const header = await page.locator("#run-header").innerText();
  expect(header).toContain("20 inputs, alpha0 0.050000, theta 0.0100, no bounds");
  expect(header).toContain("fixed-step LMS, 20 inputs, alpha 0.0300");
  // The step size is labeled with how it was chosen, never as "tuned" on its own.
  expect(header).toContain("best value in our Sutton 1992 Experiment 1 grid");
  expect(header).toContain("20 seeds");
});

test("D07 the rolling mean squared error is labeled with its window", async ({ page }) => {
  await openApp(page);

  const subtitle = await page.locator("#error-subtitle").innerText();
  expect(subtitle).toContain("min(100, completed)");
  expect(subtitle).toContain("pre-update");

  const legend = await page.locator("#error-legend").innerText();
  expect(legend).toContain("IDBD rolling MSE");
  expect(legend).toContain("LMS rolling MSE");

  // The pair labels are set in small caps by CSS, so compare without case.
  const metrics = (await page.locator("#metrics-pairs").innerText()).toLowerCase();
  expect(metrics).toContain("idbd rolling mse (last 100)");
  expect(metrics).toContain("lms rolling mse (last 100)");
});

test('D07 no "tuned LMS" claim appears anywhere', async ({ page }) => {
  await openApp(page);
  await runUntil(page, 120);

  const text = (await page.locator("body").innerText()).toLowerCase();
  expect(text).not.toContain("tuned lms");
  expect(text).toContain("does not show that either rule is better in general");
});

test("D07 both learners are scored on the same stream", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 150);

  const state = await experimentState(page);
  expect(state.idbdRollingMse).toBeGreaterThan(0);
  expect(state.lmsRollingMse).toBeGreaterThan(0);
  expect(state.idbdWeights.length).toBe(state.lmsWeights.length);
});
