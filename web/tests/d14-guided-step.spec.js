import { expect, test } from "@playwright/test";

import { openApp } from "./helpers.js";

async function guided(page) {
  return page.evaluate(() => window.idbdExplorer.guided());
}

test("D14 the guided values come from the Rust walkthrough", async ({ page }) => {
  await openApp(page);
  await page.click("#guided-advance");
  await page.click("#guided-advance");

  const shown = await guided(page);
  expect(shown.sample).toBe(3);

  // Replay the same seeded walkthrough through the module and compare bit for bit:
  // the page cannot be computing these numbers itself.
  const replay = await page.evaluate(
    ({ seed, steps, feature }) => {
      const walk = new window.idbdExplorer.wasm.Walkthrough(seed);
      for (let i = 0; i < steps; i += 1) {
        walk.advance();
      }
      const values = {
        sample: walk.sample(),
        prediction: walk.prediction(),
        error: walk.error(),
        target: walk.target(),
        features: Array.from(walk.features()),
        coordinate: JSON.parse(walk.coordinateJson(feature)),
      };
      walk.free();
      return values;
    },
    { seed: shown.seed, steps: shown.sample, feature: shown.selected },
  );

  expect(replay.sample).toBe(shown.sample);
  expect(replay.prediction).toBe(shown.prediction);
  expect(replay.error).toBe(shown.error);
  expect(replay.target).toBe(shown.target);
  expect(replay.features).toEqual(shown.features);
  expect(replay.coordinate).toEqual(shown.coordinate);
});

test("D14 the ladder shows the update in order, with the new alpha before the weight", async ({
  page,
}) => {
  await openApp(page);
  await page.click("#guided-advance");

  const shown = await guided(page);
  const rungs = await page.$$eval("#guided-ladder .rung", (nodes) =>
    nodes.map((node) => node.innerText),
  );

  expect(rungs.length).toBe(7);
  expect(rungs[0]).toContain("Prediction for sample");
  expect(rungs[1]).toContain("target minus prediction");
  expect(rungs[2]).toContain("Trace h of");
  expect(rungs[3]).toContain("theta times error times input times old trace");
  expect(rungs[4]).toContain("New step size alpha");
  expect(rungs[5]).toContain("Weight update");
  expect(rungs[6]).toContain("Trace update");

  const alphaIndex = rungs.findIndex((text) => text.includes("New step size alpha"));
  const weightIndex = rungs.findIndex((text) => text.includes("Weight update"));
  const traceIndex = rungs.findIndex((text) => text.includes("Trace update"));
  expect(alphaIndex).toBeLessThan(weightIndex);
  expect(alphaIndex).toBeLessThan(traceIndex);
  expect(rungs[4]).toContain("used by both updates below");

  // The beta step explains its own sign from error, input and the old trace.
  expect(rungs[3]).toMatch(/so their product is (positive|negative|zero)|old trace is zero/);

  // Old and new values carry the same sample index.
  const label = `sample ${shown.sample}`;
  expect(rungs[0]).toContain(label);
  expect(rungs[5]).toContain(`Before ${label} to after ${label}`);
  expect(rungs[6]).toContain(`Before ${label} to after ${label}`);
  expect(await page.locator("#guided-sample").innerText()).toBe(label);
});

test("D14 the guided step advances one sample and leads into the experiment", async ({ page }) => {
  await openApp(page);
  const first = await guided(page);
  await page.click("#guided-advance");
  expect((await guided(page)).sample).toBe(first.sample + 1);

  await page.click('#guided-feature-choice button[data-feature="1"]');
  expect((await guided(page)).selected).toBe(1);
  expect(await page.locator("#ladder-feature").innerText()).toBe("x1");

  await page.click("#guided-continue");
  expect(await page.evaluate(() => document.activeElement.id)).toBe("run");
});
