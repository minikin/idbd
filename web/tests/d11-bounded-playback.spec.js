import { expect, test } from "@playwright/test";
import { openApp } from "./helpers.js";

// D11: 100,000 samples of playback keep a bounded chart history, aggregate metrics over
// every sample, and a Pause that takes effect promptly.
test("D11 playback of 100,000 samples stays bounded and responsive", async ({ page }) => {
  test.setTimeout(120000);
  await openApp(page);
  await page.selectOption("#speed", "20000");
  await page.click("#run");
  await page.waitForFunction(
    () => window.idbdExplorer.experiment().sample >= 100000,
    null,
    { timeout: 100000, polling: 250 },
  );

  // The input delay a visitor feels: from the click event's creation to the end of the
  // Pause handler, measured in the page. Playwright's own round trips to a busy page are
  // not part of it.
  await page.evaluate(() => {
    document.getElementById("pause").addEventListener("click", (event) => {
      queueMicrotask(() => {
        window.pauseLatency = performance.now() - event.timeStamp;
        window.runningAfterPause = window.idbdExplorer.isRunning();
      });
    });
  });
  const clicked = Date.now();
  await page.click("#pause");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === false);
  const latency = await page.evaluate(() => window.pauseLatency);
  expect(await page.evaluate(() => window.runningAfterPause)).toBe(false);
  console.log(`pause input delay ${latency.toFixed(1)} ms`);
  expect(latency).toBeGreaterThanOrEqual(0);
  expect(latency).toBeLessThan(250);
  // A hang guard for the whole round trip, which includes Playwright's overhead.
  expect(Date.now() - clicked).toBeLessThan(5000);

  const paused = await page.evaluate(() => window.idbdExplorer.experiment());
  await page.waitForTimeout(400);
  const later = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(later.sample).toBe(paused.sample);

  expect(paused.historyLength).toBe(2000);
  expect(paused.samples.at(-1)).toBe(paused.sample);
  expect(paused.samples[0]).toBeGreaterThanOrEqual(paused.sample - 1999);
  expect(Number.isFinite(paused.idbdMse)).toBe(true);
  expect(Number.isFinite(paused.lmsMse)).toBe(true);
});
