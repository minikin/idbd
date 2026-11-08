import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { inWords, missInWords } from "../story/slow.js";

// S01 to S07: the story page. Its learning values come from the crate's FamilyWatch and
// FamilyDay, and the page stays usable without WebGL or motion. The game has its own
// file, s10.

const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function openStory(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

const story = (page) =>
  page.evaluate(() => ({
    home: window.idbdStory.home,
    renderer: window.idbdStory.renderer,
    hero: window.idbdStory.hero(),
    desk: window.idbdStory.desk,
    game: window.idbdStory.game(),
    slow: window.idbdStory.slow(),
  }));

test("S01 the hero and the closing faders show speeds computed by the crate", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await openStory(page);
  const s = await story(page);
  expect(s.home.sensors).toHaveLength(12);
  const matters = s.home.sensors.map((sensor) => sensor.hours !== 0);
  expect(matters.filter(Boolean)).toHaveLength(4);
  expect(s.hero.warmup).toBe(7 * 1440);
  expect(s.hero.completed).toBeGreaterThanOrEqual(s.hero.warmup);
  await expect(page.locator("#sensor-grid li")).toHaveCount(12);
  // After the warm-up the hero's four widest rings belong to the sensors that matter.
  const heroMatter = Math.min(...s.hero.speeds.filter((_, i) => matters[i]));
  const heroQuiet = Math.max(...s.hero.speeds.filter((_, i) => !matters[i]));
  expect(heroMatter).toBeGreaterThan(heroQuiet);

  const slowestRelevant = Math.min(...s.desk.filter((_, i) => matters[i]));
  const fastestInert = Math.max(...s.desk.filter((_, i) => !matters[i]));
  expect(slowestRelevant).toBeGreaterThan(fastestInert);
  const labels = await page.$$eval("#desk .fader-label .mono", (nodes) => nodes.map((n) => n.textContent));
  expect(labels).toEqual(s.desk.map((a) => a.toFixed(3)));
  await expect(page.locator("#desk li.relevant")).toHaveCount(4);
  expect(errors).toEqual([]);
});

test("S02 slow motion shows one crate update at a time", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#slow-reading")).toHaveText("0");
  await page.click("#slow-next");
  await expect(page.locator("#slow-reading")).toHaveText("1");
  const one = (await story(page)).slow;
  // An independent replay of the same seed gives the same guess and meter reading.
  const replay = await page.evaluate(async () => {
    const wasm = await import("/pkg/idbd_wasm.js");
    const day = new wasm.FamilyDay(3);
    day.step();
    const out = { guess: day.idbdPrediction(), target: day.target() };
    day.free();
    return out;
  });
  expect(one.last.guess).toBe(replay.guess);
  expect(one.last.target).toBe(replay.target);
  await expect(page.locator("#slow-guess")).toHaveText(inWords(one.last.guess));
  await expect(page.locator("#slow-actual")).toHaveText(inWords(one.last.target));
  await expect(page.locator("#slow-miss")).toHaveText(missInWords(one.last.target, one.last.guess));
  await expect(page.locator("#slow-summary")).toContainText("IDBD guessed");

  await page.click("#slow-run");
  await expect(page.locator("#slow-reading")).toHaveText("4,320");
  await expect(page.locator("#slow-next")).toBeDisabled();
  await expect(page.locator("#slow-summary")).toContainText("The three days are over.");
  const later = (await story(page)).slow;
  const speeds = await page.$$eval("#slow-rows td:nth-child(4)", (cells) => cells.map((c) => c.textContent));
  speeds.forEach((text, row) => expect(text).toContain(`→ ${later.stepSizes[later.order[row]].toFixed(4)}`));
  // The readings and the trust shown are in the same units: their products add up to the guess.
  const sum = later.readings.reduce((acc, x, s) => acc + x * later.trustBefore[s], 0);
  expect(sum).toBeCloseTo(later.last.guess, 9);
  const shown = await page.$$eval("#slow-rows td:nth-child(2)", (cells) => cells.map((c) => Number(c.textContent.replace("−", "-"))));
  shown.forEach((value, row) => expect(value).toBeCloseTo(later.readings[later.order[row]], 2));

  await page.click("#slow-restart");
  await expect(page.locator("#slow-reading")).toHaveText("0");
});

test("S06 with reduced motion the house is the 2D drawing", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  expect((await story(page)).renderer).toBe("2d");
  await expect(page.locator("#house-3d")).toBeHidden();
  await expect(page.locator("#house-2d")).toBeVisible();
  await expect(page.locator("#house-2d-sensors .attention")).toHaveCount(12);
  await expect(page.locator("#house-2d-sensors .label-chip")).toHaveCount(12);
  await page.evaluate(() => document.fonts.ready);
  const outside = await page.locator("#house-2d").evaluate((svg) => {
    const [x0, y0, w, h] = svg.getAttribute("viewBox").split(/\s+/).map(Number);
    return [...svg.querySelectorAll(".label-chip")].flatMap((chip) => {
      // The chip is drawn around its dot and its text, so its box bounds the whole label.
      const b = chip.getBBox();
      return b.x < x0 || b.y < y0 || b.x + b.width > x0 + w || b.y + b.height > y0 + h ? [chip.nextSibling.nextSibling.textContent] : [];
    });
  });
  expect(outside).toEqual([]);
});

test("S07 the story page has no WCAG A or AA violations and fits a phone", async ({ page }) => {
  await openStory(page);
  await page.click("#slow-run");
  await page.click("#family-run");
  await page.waitForFunction(() => window.idbdStory.game().phase === "done", null, { timeout: 30000 });
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  const found = results.violations.map((v) => ({ id: v.id, targets: v.nodes.map((n) => n.target.join(" ")) }));
  expect(found).toEqual([]);

  await page.setViewportSize({ width: 390, height: 844 });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});

test("S07 no section leaves an empty gap between its blocks", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openStory(page);
  const gaps = await page.evaluate(() =>
    [...document.querySelectorAll("main > section")].map((section) => {
      const boxes = [...section.children]
        .map((child) => child.getBoundingClientRect())
        .filter((box) => box.height > 0);
      let largest = 0;
      for (let i = 1; i < boxes.length; i += 1) largest = Math.max(largest, boxes[i].top - boxes[i - 1].bottom);
      return { section: section.id || section.getAttribute("aria-labelledby"), largest };
    }),
  );
  for (const { section, largest } of gaps) expect(largest, section).toBeLessThanOrEqual(64);
});
