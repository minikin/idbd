import { readFile } from "node:fs/promises";

import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";

/** The learner state and sample-indexed metrics that a replay must reproduce. */
function replayed(state) {
  return {
    sample: state.sample,
    idbdWeights: state.idbdWeights,
    lmsWeights: state.lmsWeights,
    alphas: state.alphas,
    traces: state.traces,
    idbdRollingMse: state.idbdRollingMse,
    lmsRollingMse: state.lmsRollingMse,
    idbdMse: state.idbdMse,
    lmsMse: state.lmsMse,
    samples: state.samples,
    idbdRollingSeries: state.idbdRollingSeries,
    lmsRollingSeries: state.lmsRollingSeries,
    events: state.events,
    requestedChanges: state.requestedChanges,
  };
}

async function exportRun(page) {
  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#export")]);
  return { path: await download.path(), name: download.suggestedFilename() };
}

async function playReplay(page, speed) {
  await page.selectOption("#speed", speed);
  await page.click("#run");
  await page.waitForFunction(() => window.idbdExplorer.experiment().replay?.reached === true, null, {
    timeout: 30000,
  });
  expect(await page.evaluate(() => window.idbdExplorer.isRunning())).toBe(false);
  return page.evaluate(() => window.idbdExplorer.experiment());
}

test("D06 a reloaded run replays to the same state at two speeds", async ({ page }) => {
  test.setTimeout(90000);
  await openApp(page);
  await runUntil(page, 300);
  await page.click("#change-world");
  await runUntil(page, 1000);
  await page.click("#change-world");
  await page.click("#step");
  const original = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(original.requestedChanges).toHaveLength(2);
  expect(original.requestedChanges[1]).toBe(original.sample);

  const file = await exportRun(page);
  const exported = JSON.parse(await readFile(file.path, "utf8"));
  expect(exported.settings.requestedChanges).toEqual(original.requestedChanges);

  await page.click("#reset");
  await page.setInputFiles("#load-run", file.path);
  await page.waitForFunction(() => window.idbdExplorer.experiment().replay !== null);
  const loaded = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(loaded.sample).toBe(0);
  expect(loaded.historyLength).toBe(0);
  expect(loaded.scheduledChanges).toEqual(original.requestedChanges);
  expect(loaded.changePending).toBe(false);
  await expect(page.locator("#replay-note")).toContainText(
    `pauses at sample ${original.sample.toLocaleString("en-US")}`,
  );

  const slow = await playReplay(page, "1000");
  expect(replayed(slow)).toEqual(replayed(original));
  expect(slow.idbdWeights).toEqual(exported.metrics.idbdWeights);
  await expect(page.locator("#replay-note")).toContainText("Paused at sample");

  await page.click("#replay");
  const rebuilt = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(rebuilt.sample).toBe(0);
  expect(rebuilt.scheduledChanges).toEqual(original.requestedChanges);
  const fast = await playReplay(page, "20000");
  expect(replayed(fast)).toEqual(replayed(original));

  await page.click("#step");
  expect(await page.evaluate(() => window.idbdExplorer.experiment().sample)).toBe(
    original.sample + 1,
  );
});

test("D06 an invalid run file leaves the current run unchanged", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 50);
  const before = await page.evaluate(() => window.idbdExplorer.experiment());

  await page.setInputFiles("#load-run", {
    name: "not-a-run.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ kind: "something-else" })),
  });
  const note = page.locator("#replay-note");
  await expect(note).toBeVisible();
  await expect(note).toHaveText(
    "Could not load the run: the file is not an IDBD Explorer run export.",
  );
  const after = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(after.sample).toBe(before.sample);
  expect(after.idbdWeights).toEqual(before.idbdWeights);
  expect(after.replay).toBe(null);
});

test("D06 changing a hyperparameter starts a new labeled run", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 50);
  await page.click("#replay");
  await expect(page.locator("#replay-note")).toBeVisible();

  await page.fill("#theta", "0.02");
  await page.click("#apply-settings");
  const state = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(state.sample).toBe(0);
  expect(state.theta).toBe(0.02);
  expect(state.replay).toBe(null);
  await expect(page.locator("#replay-note")).toBeHidden();
  await expect(page.locator("#run-header")).toContainText("theta 0.0200");
});
