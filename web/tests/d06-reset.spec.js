import { expect, test } from "@playwright/test";
import { openApp, runUntil } from "./helpers.js";

// D06 (partial): reset clears charts, the pending-event queue and the metric windows,
// and the same seed reproduces the same stream from sample 0.
test("D06 reset clears history, pending events and metrics", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 400);
  const first = await page.evaluate(() => window.idbdExplorer.experiment());
  await page.click("#change-world");
  expect((await page.evaluate(() => window.idbdExplorer.experiment())).changePending).toBe(true);

  await page.click("#reset");
  const reset = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(reset.sample).toBe(0);
  expect(reset.changePending).toBe(false);
  expect(reset.historyLength).toBe(0);
  expect(reset.events).toEqual([]);
  expect(reset.idbdRollingMse).toBeNull();
  expect(reset.lmsRollingMse).toBeNull();
  expect(reset.seed).toBe(first.seed);

  await page.click("#step");
  const replayed = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(replayed.sample).toBe(1);
  expect(replayed.features).toHaveLength(20);
  expect(Number.isFinite(replayed.idbdError)).toBe(true);
});
