import { expect, test } from "@playwright/test";

import { completedSample, experimentSamples, openApp } from "./helpers.js";

test("D03 Step processes exactly one shared sample", async ({ page }) => {
  await openApp(page);

  for (let expected = 1; expected <= 3; expected += 1) {
    await page.click("#step");
    expect(await completedSample(page)).toBe(expected);
    const samples = await experimentSamples(page);
    expect(new Set(samples)).toEqual(new Set([String(expected)]));
  }
});

test("D03 Run then Pause stops at a sample boundary and stays fixed", async ({ page }) => {
  await openApp(page);
  await page.selectOption("#speed", "100");
  await page.click("#run");
  await page.waitForFunction(() => window.idbdExplorer.experiment().sample >= 10, null, {
    timeout: 15000,
  });
  await page.click("#pause");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === false);

  const stopped = await completedSample(page);
  expect(stopped).toBeGreaterThanOrEqual(10);
  expect(Number.isInteger(stopped)).toBe(true);

  await page.waitForTimeout(700);
  expect(await completedSample(page)).toBe(stopped);
  expect(await experimentSamples(page)).toEqual(
    new Array((await experimentSamples(page)).length).fill(String(stopped)),
  );
});
