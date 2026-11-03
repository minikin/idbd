import { readFile } from "node:fs/promises";

import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";
import { fmtAlpha, fmtSigned } from "../src/format.js";

// D16: Autostep runs untuned on the same stream, and every value the page shows for it
// comes from the Rust session.
test("D16 untuned Autostep is shown beside IDBD and LMS", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 300);
  const state = await page.evaluate(() => window.idbdExplorer.experiment());

  expect(state.autostepConfig).toEqual({
    initialStepSize: 0.005,
    metaStepSize: 0.01,
    timeScale: 10000,
  });
  await expect(page.locator("#run-header")).toContainText("not tuned to this task");
  await expect(page.locator("#error-legend")).toContainText("Autostep rolling MSE, untuned");
  expect(await page.locator("#error-chart path.series-autostep").count()).toBe(1);

  expect(state.autostepWeights).toHaveLength(20);
  expect(state.autostepAlphas.every((alpha) => alpha > 0)).toBe(true);
  expect(Number.isFinite(state.autostepError)).toBe(true);
  expect(state.autostepRollingSeries.at(-1)).toBe(state.autostepRollingMse);

  await page.locator("#weight-row-2").click();
  const inspector = await page.locator("#inspector-pairs").innerText();
  expect(inspector).toContain(fmtSigned(state.autostepWeights[2], 5));
  expect(inspector).toContain(fmtAlpha(state.autostepAlphas[2]));

  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#export")]);
  const run = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(run.settings.autostep.metaStepSize).toBe(0.01);
  expect(run.metrics.autostepWeights).toEqual(state.autostepWeights);
  expect(run.metrics.autostepRollingMse).toBe(state.autostepRollingMse);
});
