import { expect, test } from "@playwright/test";

import { stepTexts } from "../story/loop.js";

// S08: the four update steps in "What IDBD does" walk through live readings from the
// crate's FamilyDay, one step at a time, without naming a sensor.

async function openStory(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

const loop = (page) => page.evaluate(() => window.idbdStory.loop());
const active = (page) => page.locator('#loop > li[aria-current="step"] h3');

test("S08 each card shows what its step did on the crate's reading", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  const state = await loop(page);
  const replay = await page.evaluate(async (completed) => {
    const wasm = await import("/pkg/idbd_wasm.js");
    const day = new wasm.FamilyDay(1);
    day.run(completed - 1);
    const snap = () => ({ speeds: Array.from(day.idbdSpeeds()), weights: Array.from(day.idbdWeights()) });
    const before = snap();
    day.step();
    return { before, after: { ...snap(), guess: day.idbdPrediction(), target: day.target() } };
  }, state.completed);
  expect(state.texts).toEqual(stepTexts(replay.before, replay.after));
  for (const [i, text] of state.texts.entries()) await expect(page.locator(`#loop-live-${i + 1}`)).toHaveText(text);
  const names = await page.evaluate(() => window.idbdStory.home.sensors.map((s) => s.name));
  for (const text of state.texts) for (const name of names) expect(text).not.toContain(name);
});

test("S08 the step texts count, add up as printed and say when every trust moved", () => {
  const before = { speeds: [1, 1, 1], weights: [0, 0, 0] };
  const [guess, check, tune, correct] = stepTexts(before, {
    speeds: [2, 0.5, 1],
    weights: [0.004, -0.001, 0.002],
    guess: 0.374,
    target: 0.224,
  });
  expect(guess).toBe("IDBD guesses 0.37 kW above the base.");
  expect(check).toBe("The meter reads 0.22 kW above the base, so the guess is 0.15 kW too high.");
  expect(tune).toBe("1 speed went up and 1 went down.");
  expect(correct).toBe("All 3 trusts moved, the largest by 0.004 kW per swing.");
  const still = stepTexts(before, { ...before, guess: 0, target: 0 });
  expect(still[3]).toBe("No trust moved.");
  expect(stepTexts(before, { speeds: [1, 1, 1], weights: [0, 0.5, 0], guess: 0, target: 0 })[3]).toBe(
    "1 trust moved, the largest by 0.500 kW per swing.",
  );
});

test("S08 the highlight walks the four steps and takes a new reading each round", async ({ page }) => {
  await page.clock.install();
  await openStory(page);
  await page.locator("#loop").scrollIntoViewIfNeeded();
  await expect
    .poll(async () => {
      await page.clock.runFor(20);
      return (await loop(page)).running;
    })
    .toBe(true);
  const first = await loop(page);
  await expect(active(page)).toHaveText("Tune the speeds");
  await page.clock.runFor(1800);
  await expect(active(page)).toHaveText("Correct");
  expect((await loop(page)).completed).toBe(first.completed);
  await page.clock.runFor(1800);
  await expect(active(page)).toHaveText("Guess");
  expect((await loop(page)).completed).toBe(first.completed + 1);
  await expect(page.locator("#loop > li.highlight")).toHaveCount(1);
});

test("S08 the steps stop when paused and when off screen", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await openStory(page);
  await page.locator("#loop").scrollIntoViewIfNeeded();
  await expect.poll(async () => (await loop(page)).running).toBe(true);
  await page.click("#loop-pause");
  await expect(page.locator("#loop-pause")).toHaveText("Play");
  const parked = await loop(page);
  expect(parked.running).toBe(false);
  await page.waitForTimeout(2200);
  expect((await loop(page)).active).toBe(parked.active);
  await page.click("#loop-pause");
  await expect.poll(async () => (await loop(page)).running).toBe(true);

  await page.locator("#play").scrollIntoViewIfNeeded();
  await expect.poll(async () => (await loop(page)).running).toBe(false);
});

test("S08 reduced motion holds still on the step that is IDBD", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  await page.locator("#loop").scrollIntoViewIfNeeded();
  await expect(page.locator("#loop-pause")).toHaveText("Play");
  await expect(active(page)).toHaveText("Tune the speeds");
  await page.waitForTimeout(2200);
  const state = await loop(page);
  expect(state.running).toBe(false);
  expect(state.active).toBe(2);
  await expect(page.locator("#loop-live-1")).toContainText("IDBD guesses");
});
