import { expect, test } from "@playwright/test";

import { openApp, withinTolerance } from "./helpers.js";
import { REFERENCE_FIXTURE } from "../src/presets.js";

const { expected } = REFERENCE_FIXTURE;

test("D01 the C06 and C07 reference samples match through the built WebAssembly module", async ({
  page,
}) => {
  await openApp(page);

  const got = await page.evaluate(() => {
    const learner = new window.idbdExplorer.wasm.Learner(2, 0.1, 0.01);
    learner.update(new Float64Array([1, 2]), 1);
    learner.update(new Float64Array([2, -1]), 0.5);
    const values = {
      prediction: learner.prediction(),
      error: learner.error(),
      weights: Array.from(learner.weights()),
      betas: Array.from(learner.logStepSizes()),
      traces: Array.from(learner.traces()),
      boundedFeatures: learner.boundedFeatures(),
    };
    learner.free();
    return values;
  });

  expect(withinTolerance(got.prediction, expected.prediction)).toBe(true);
  expect(withinTolerance(got.error, expected.error)).toBe(true);
  expect(got.boundedFeatures).toBe(0);
  for (let i = 0; i < 2; i += 1) {
    expect(withinTolerance(got.weights[i], expected.weights[i])).toBe(true);
    expect(withinTolerance(got.betas[i], expected.betas[i])).toBe(true);
    expect(withinTolerance(got.traces[i], expected.traces[i])).toBe(true);
  }
});

test("D01 the page reports the fixture check and the core version it was built from", async ({
  page,
}) => {
  await openApp(page);

  const fixture = await page.evaluate(() => window.idbdExplorer.fixture);
  expect(fixture.matches).toBe(true);

  const buildInfo = await page.locator("#build-info").innerText();
  expect(buildInfo).toContain("match the crate's values in this browser");
  expect(buildInfo).toMatch(/\d+\.\d+\.\d+/);
  expect(buildInfo).toContain("xoshiro256");
});
