import { expect, test } from "@playwright/test";

import { completedSample, experimentState, openApp, runUntil } from "./helpers.js";

test("D04 Change the world marks the next sample and resets nothing", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 200);

  const before = await experimentState(page);
  expect(before.sample).toBeGreaterThanOrEqual(200);

  await page.click("#change-world");
  const pending = await experimentState(page);
  expect(pending.sample).toBe(before.sample);
  expect(pending.idbdWeights).toEqual(before.idbdWeights);
  expect(pending.lmsWeights).toEqual(before.lmsWeights);
  expect((await page.locator("#playback-state").innerText()).toLowerCase()).toContain(
    "world change applies to the next sample",
  );

  await page.click("#step");
  const after = await experimentState(page);
  expect(after.sample).toBe(before.sample + 1);

  const marker = after.events.find(
    (event) => event.kind === "relevanceShift" && event.sample === after.sample,
  );
  expect(marker).toBeTruthy();
  expect(marker.origin).toBe("requested");

  const eventList = await page.locator("#event-list").innerText();
  expect(eventList).toContain("you changed the world");
  expect(eventList).toContain(`sample ${after.sample.toLocaleString("en-US")}`);

  // Learning continued from the state after sample t: nothing was cleared.
  expect(after.idbdWeights.some((value) => value !== 0)).toBe(true);
  expect(after.idbdWeights).not.toEqual(before.idbdWeights);
  expect(await completedSample(page)).toBeGreaterThan(200);
});
