import { expect, test } from "@playwright/test";

import { experimentState, openApp } from "./helpers.js";

test("D08 a numerical failure freezes the view and requires Reset", async ({ page }) => {
  await openApp(page);

  // A step size this large makes the LMS baseline diverge within a few hundred samples.
  await page.fill("#lms-step", "5");
  await page.click("#apply-settings");
  await page.selectOption("#speed", "1000");
  await page.click("#run");
  await page.waitForFunction(() => window.idbdExplorer.experiment().failure !== null, null, {
    timeout: 30000,
  });

  const failed = await experimentState(page);
  expect(failed.failure.learner).toBe("LMS");
  expect(failed.failure.sample).toBe(failed.sample + 1);

  const banner = await page.locator("#failure").innerText();
  expect(banner).toContain("LMS failed on sample");
  expect(banner).toContain("Choose Reset");

  expect(await page.evaluate(() => window.idbdExplorer.isRunning())).toBe(false);
  for (const id of ["run", "step", "change-world"]) {
    await expect(page.locator(`#${id}`)).toBeDisabled();
  }

  // Nothing moves and nothing is replaced with zero.
  await page.waitForTimeout(400);
  const after = await experimentState(page);
  expect(after.sample).toBe(failed.sample);
  expect(after.idbdWeights).toEqual(failed.idbdWeights);
  expect(after.lmsWeights.every((value) => Number.isFinite(value))).toBe(true);

  await page.click("#reset");
  const fresh = await experimentState(page);
  expect(fresh.failure).toBe(null);
  expect(fresh.sample).toBe(0);
  await expect(page.locator("#failure")).toBeHidden();
  await expect(page.locator("#run")).toBeEnabled();
});
