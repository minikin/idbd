import { expect, test } from "@playwright/test";

import { tally } from "../story/family.js";

// S10: the Family Assistant game. The visitor marks hours per sensor, IDBD and a tuned
// fixed-speed learner watch three days through the crate's FamilyDay, and every bar,
// found hour and score on the reveal comes from the crate.

async function openStory(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
  await page.locator("#play").scrollIntoViewIfNeeded();
}

const game = (page) => page.evaluate(() => window.idbdStory.game());

const span = (bits) => {
  const hours = [...Array(24).keys()].filter((h) => bits & (1 << h));
  return hours.length ? [hours[0], hours[hours.length - 1] + 1] : [0, 0];
};

async function guessTruth(page) {
  const { truth } = await game(page);
  await page.evaluate((spans) => {
    spans.forEach(([from, until], i) => window.idbdStory.family.guess(i, from, until));
  }, truth.map(span));
}

async function runToEnd(page) {
  await page.click("#family-run");
  await page.waitForFunction(() => window.idbdStory.game().phase === "done", null, { timeout: 30000 });
}

test("S10 twelve lanes start empty and four sensors matter", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#family-lanes .lane")).toHaveCount(12);
  await expect(page.locator("#family-lanes .lane.empty")).toHaveCount(12);
  const s = await game(page);
  expect(s.phase).toBe("guess");
  expect(s.guesses).toEqual(Array(12).fill(0));
  expect(s.truth.filter(Boolean)).toHaveLength(4);
  await expect(page.locator("#family-view")).toBeHidden();
  await expect(page.locator("#family-result")).toBeHidden();
});

test("S10 the lane handles work from the keyboard and announce the hour", async ({ page }) => {
  await openStory(page);
  const until = page.locator('#family-lanes .lane[data-sensor="5"] input.until');
  await until.focus();
  for (let i = 0; i < 3; i += 1) await page.keyboard.press("ArrowRight");
  await expect(until).toHaveAttribute("aria-valuetext", "03:00");
  expect((await game(page)).guesses[5]).toBe(0b111);
  await expect(page.locator('#family-lanes .lane[data-sensor="5"]')).not.toHaveClass(/empty/);
  await page.click("#family-reset");
  expect((await game(page)).guesses[5]).toBe(0);
});

test("S10 dragging a handle with the mouse marks hours", async ({ page }) => {
  await openStory(page);
  const track = page.locator('#family-lanes .lane[data-sensor="5"] .lane-track');
  const box = await track.boundingBox();
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 7, y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, y, { steps: 8 });
  await page.mouse.up();
  const bits = (await game(page)).guesses[5];
  const hours = [...Array(24).keys()].filter((h) => bits & (1 << h));
  expect(hours[0]).toBe(0);
  expect(hours.length).toBeGreaterThanOrEqual(10);
  expect(hours.length).toBeLessThanOrEqual(14);
});

test("S10 the true hours score 100% and the reveal shows the crate's bars and scores", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  await guessTruth(page);
  await runToEnd(page);
  const s = await game(page);
  expect(s.completed).toBe(4320);
  expect(s.scores.you).toBe(1);
  expect(s.scores.idbd).toBeGreaterThan(s.scores.lms);
  expect(s.idbdFound).toHaveLength(12);
  await expect(page.locator("#family-scores .score")).toHaveCount(3);
  await expect(page.locator("#family-scores .value").first()).toHaveText("100%");
  await expect(page.locator("#family-scores .value").nth(1)).toHaveText(`${Math.round(s.scores.idbd * 100)}%`);
  const idbd = tally(s.idbdFound, s.truth);
  await expect(page.locator("#family-scores .note").nth(1)).toContainText(`${idbd.sensorsFound} of 4 sensors found`);
  await expect(page.locator("#family-mechanism")).toContainText("times faster than about the eight that never do");
  await expect(page.locator(".lane-bars i.on")).toHaveCount(s.idbdFound.reduce((n, b) => n + span(b)[1] - span(b)[0], 0));
  await expect(page.locator('#family-lanes .lane[data-sensor="0"] .lane-note')).toHaveText("never matters");
  await expect(page.locator("#family-key")).toBeVisible();
  await expect(page.locator("#family-reset")).toBeHidden();
});

test("S10 the reveal switches to the fixed speed's bars and back", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  await runToEnd(page);
  await page.click('#family-view [data-view="lms"]');
  await expect(page.locator("#family")).toHaveAttribute("data-view", "lms");
  await expect(page.locator('#family-view [data-view="lms"]')).toHaveAttribute("aria-pressed", "true");
  const { lmsFound } = await game(page);
  const lmsHours = lmsFound.reduce((n, bits) => n + [...Array(24).keys()].filter((h) => bits & (1 << h)).length, 0);
  await expect(page.locator(".lane-bars i.on")).toHaveCount(lmsHours);
  await page.click('#family-view [data-view="idbd"]');
  await expect(page.locator("#family")).toHaveAttribute("data-view", "idbd");
});

test("S10 the three days play out over time, then Play again starts the next run", async ({ page }) => {
  test.setTimeout(60_000);
  await page.clock.install();
  await openStory(page);
  await page.click("#family-run");
  await page.clock.runFor(2_000);
  const midway = await game(page);
  expect(midway.phase).toBe("running");
  expect(midway.completed).toBeGreaterThan(0);
  expect(midway.completed).toBeLessThan(4320);
  await expect(page.locator("#family-clock")).toHaveText(/^Day [1-3], \d\d:\d\d$/);
  await expect(page.locator(".lane-bars").first()).toBeHidden();
  await page.clock.runFor(10_000);
  expect((await game(page)).phase).toBe("done");
  await expect(page.locator("#family-clock")).toHaveText("Three days watched, 4,320 readings");

  const seed = (await game(page)).seed;
  await page.click("#family-again");
  const next = await game(page);
  expect(next.phase).toBe("guess");
  expect(next.seed).not.toBe(seed);
  await expect(page.locator("#family-result")).toBeHidden();
  await expect(page.locator("#family-run")).toBeFocused();
});

test("S10 tally counts sensors found, right hours and wrong hours", () => {
  const truth = [0, 0b1100];
  expect(tally([0b0110, 0b0100], truth)).toEqual({ sensorsFound: 1, right: 1, wrong: 2, missed: 1 });
  expect(tally([0, 0], truth)).toEqual({ sensorsFound: 0, right: 0, wrong: 0, missed: 2 });
});
