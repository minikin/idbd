import { expect, test } from "@playwright/test";

import { experimentSamples, experimentState, openApp, runUntil } from "./helpers.js";
import { fmtAlpha, fmtMse, fmtSigned } from "../src/format.js";

test("D02 every experiment panel identifies the same completed sample", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 150);

  const samples = await experimentSamples(page);
  expect(samples.length).toBeGreaterThanOrEqual(6);
  expect(new Set(samples).size).toBe(1);

  const state = await experimentState(page);
  expect(samples[0]).toBe(String(state.sample));
});

test("D02 the table values equal the state the charts were drawn from", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 150);

  const state = await experimentState(page);
  const rows = await page.$$eval("#state-table tbody tr", (nodes) =>
    nodes.map((row) => Array.from(row.querySelectorAll("th, td")).map((cell) => cell.textContent)),
  );

  expect(rows.length).toBe(state.features.length);
  for (let index = 0; index < rows.length; index += 1) {
    expect(rows[index]).toEqual([
      `x${index}`,
      fmtSigned(state.features[index], 4),
      fmtSigned(state.coefficients[index], 3),
      fmtSigned(state.idbdWeights[index], 5),
      fmtSigned(state.lmsWeights[index], 5),
      fmtAlpha(state.alphas[index]),
      fmtSigned(state.traces[index], 5),
    ]);
  }

  const metrics = await page.locator("#metrics-pairs").innerText();
  expect(metrics).toContain(fmtMse(state.idbdRollingMse));
  expect(metrics).toContain(fmtMse(state.lmsRollingMse));
});

test("D02 the feature inspector agrees with the selected row", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 120);
  await page.locator("#weight-row-3").click();

  const state = await experimentState(page);
  const inspector = await page.locator("#inspector-pairs").innerText();
  expect(await page.locator("#inspector-name").innerText()).toBe("x3");
  expect(inspector).toContain(fmtSigned(state.idbdWeights[3], 5));
  expect(inspector).toContain(fmtSigned(state.lmsWeights[3], 5));
  expect(inspector).toContain(fmtAlpha(state.alphas[3]));
  expect(inspector).toContain(fmtSigned(state.traces[3], 5));
});
